//! Non-authoritative replay of a block's creation size and operation log.
//!
//! The stored [`BlockRecord`] remains the solid. This module builds a candidate beside it
//! and reports whether they agree. It does not write the candidate back, and it does not
//! read the stored body's topology while constructing that candidate.
//!
//! Tolerances, used only after a bit-identical compare fails: vertex position `1e-6` m,
//! volume `1e-8` m³, body bounds `1e-6` m, and the shadow bounds against the stored size
//! `1e-3` m. Analytic parameters compare at `1e-9`. A body hash matches only when the
//! ids and the raw `f64` bits match.

use crate::{
    feature_limit, swap_insets, BlockOp, BlockRecord, ConcreteElement, SolidBody, TopologyEdit, BLOCK_HISTORY_LIMIT, BLOCK_MAX_EXTENT_M, BLOCK_MIN_EXTENT_M,
};

const POSITION_TOL_M: f64 = 1.0e-6;
const VOLUME_TOL_M3: f64 = 1.0e-8;
const BOUNDS_TOL_M: f64 = 1.0e-6;
const STORED_SIZE_TOL_M: f64 = 1.0e-3;
const PARAMETER_TOL: f64 = 1.0e-9;
const NORMAL_TOL: f64 = 1.0e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentShadowStatus {
    Pass,
    Mismatch,
    Incomplete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentShadowReport {
    pub status: IntentShadowStatus,
    pub text: String,
    pub authoritative_faces: usize,
    pub shadow_faces: usize,
    pub authoritative_edges: usize,
    pub shadow_edges: usize,
    pub authoritative_vertices: usize,
    pub shadow_vertices: usize,
}

struct Candidate {
    size_m: [f64; 3],
    inset_m: [f64; 6],
    bevel_m: f64,
    body: Option<SolidBody>,
}

/// Rebuild a candidate from `record`'s creation size and log, then compare it with the stored body.
///
/// The record is borrowed and is not modified. The candidate is dropped with the report.
pub fn evaluate_intent_shadow(record: &BlockRecord) -> IntentShadowReport {
    let seed = match creation_seed(record) {
        Ok(seed) => seed,
        Err(reason) => return incomplete(record, &reason),
    };
    match replay(seed, &record.history) {
        Ok(candidate) => compare(record, &candidate),
        Err((index, detail)) => mismatch_replay(record, index, &detail),
    }
}

fn creation_seed(record: &BlockRecord) -> Result<[f64; 3], String> {
    if record.history.is_empty() {
        return Ok(record.seed_size_m.unwrap_or(record.size_m));
    }
    if !record.steps.is_empty() && record.steps.len() != record.history.len() {
        return Err("the step tape does not line up with the log".into());
    }
    if record.steps.len() == record.history.len() && record.steps.first().is_some_and(|step| step.id.number() != 1) {
        return Err(format!("the intent record dropped its prefix; the first semantic step is {}", record.steps[0].id.number()));
    }
    if record.steps.is_empty() && record.history.len() == BLOCK_HISTORY_LIMIT {
        return Err("a full log without a step tape may have dropped its prefix".into());
    }
    match record.seed_size_m {
        Some(seed) if seed.iter().all(|axis| axis.is_finite()) => Ok(seed),
        Some(_) => Err("creation size is not finite".into()),
        None => Err("creation size is not in the intent record".into()),
    }
}

fn replay(seed: [f64; 3], history: &[BlockOp]) -> Result<Candidate, (usize, String)> {
    let mut candidate = Candidate { size_m: seed, inset_m: [0.0; 6], bevel_m: 0.0, body: None };
    for (index, op) in history.iter().enumerate() {
        candidate.apply(op).map_err(|detail| (index, detail))?;
    }
    Ok(candidate)
}

impl Candidate {
    fn apply(&mut self, op: &BlockOp) -> Result<(), String> {
        match op {
            BlockOp::Size { size_m } => self.apply_size(*size_m),
            BlockOp::ExtrudeFace { face, distance_m } => self.apply_extrude_face(*face, *distance_m),
            BlockOp::InsetFace { face, distance_m } => self.apply_inset(*face, *distance_m),
            BlockOp::Bevel { distance_m } => self.apply_analytic_bevel(*distance_m),
            BlockOp::Mirror { axis } => self.apply_mirror(*axis),
            BlockOp::MoveEdge { .. }
            | BlockOp::ExtrudeEdge { .. }
            | BlockOp::SplitEdge { .. }
            | BlockOp::SubdivideFace { .. }
            | BlockOp::MoveVertex { .. }
            | BlockOp::ExtrudeFaces { .. }
            | BlockOp::BevelEdges { .. } => self.apply_topology(op),
        }
    }

    fn apply_size(&mut self, size_m: [f64; 3]) -> Result<(), String> {
        let size_m = finite_extent(size_m, "logged size")?;
        if let Some(body) = &self.body {
            let edit = body.scale_to(size_m).map_err(|error| format!("scale refused: {error}"))?;
            self.body = Some(edit.body);
            self.size_m = edit.size_m;
        } else {
            self.size_m = size_m;
        }
        self.clamp_features();
        Ok(())
    }

    fn apply_extrude_face(&mut self, face: u8, distance_m: f64) -> Result<(), String> {
        if self.body.is_some() {
            return Err("analytic extrude after the body was stored".into());
        }
        if face > 5 || !distance_m.is_finite() {
            return Err("analytic extrude is not a usable face push".into());
        }
        let axis = (face / 2) as usize;
        self.size_m[axis] = (self.size_m[axis] + distance_m).clamp(BLOCK_MIN_EXTENT_M, BLOCK_MAX_EXTENT_M);
        self.clamp_features();
        Ok(())
    }

    fn apply_inset(&mut self, face: u8, distance_m: f64) -> Result<(), String> {
        if self.body.is_some() {
            return Err("analytic inset after the body was stored".into());
        }
        if face > 5 || !distance_m.is_finite() {
            return Err("analytic inset is not a usable face".into());
        }
        self.inset_m[face as usize] = distance_m;
        self.clamp_features();
        Ok(())
    }

    fn apply_analytic_bevel(&mut self, distance_m: f64) -> Result<(), String> {
        if self.body.is_some() {
            return Err("analytic bevel after the body was stored".into());
        }
        if !distance_m.is_finite() {
            return Err("analytic bevel is not finite".into());
        }
        self.bevel_m = distance_m;
        self.clamp_features();
        Ok(())
    }

    fn apply_mirror(&mut self, axis: u8) -> Result<(), String> {
        if axis > 2 {
            return Err("mirror axis is outside 0..2".into());
        }
        if let Some(body) = &self.body {
            let edit = body.mirrored(axis as usize).map_err(|error| format!("mirror refused: {error}"))?;
            self.body = Some(edit.body);
            self.size_m = edit.size_m;
        } else {
            self.inset_m = swap_insets(self.inset_m, axis as usize).ok_or_else(|| "mirror axis is outside 0..2".to_string())?;
        }
        self.clamp_features();
        Ok(())
    }

    fn apply_topology(&mut self, op: &BlockOp) -> Result<(), String> {
        if has_features(&self.inset_m, self.bevel_m) {
            return Err("analytic inset or bevel is still set, so this topology edit is refused".into());
        }
        let source = match &self.body {
            Some(body) => body.clone(),
            None => SolidBody::from_box(self.size_m).map_err(|error| format!("box seed refused: {error}"))?,
        };
        let edit = topology_edit(&source, op)?;
        self.body = Some(edit.body);
        self.size_m = edit.size_m;
        self.clamp_features();
        Ok(())
    }

    fn clamp_features(&mut self) {
        let limit = feature_limit(self.size_m);
        self.bevel_m = if self.bevel_m.is_finite() { self.bevel_m.clamp(0.0, limit) } else { 0.0 };
        for inset in &mut self.inset_m {
            *inset = if inset.is_finite() { inset.clamp(0.0, limit) } else { 0.0 };
        }
    }
}

fn topology_edit(body: &SolidBody, op: &BlockOp) -> Result<TopologyEdit, String> {
    let edit = match op {
        BlockOp::SplitEdge { edge } => body.split_edge(*edge).map_err(|error| format!("split refused: {error}"))?,
        BlockOp::SubdivideFace { face, u, v } => body.subdivide_face(*face, *u, *v).map_err(|error| format!("subdivide refused: {error}"))?,
        BlockOp::ExtrudeFaces { faces, delta_m } => body.extrude_faces(faces, *delta_m).map_err(|error| format!("extrude refused: {error}"))?,
        BlockOp::MoveEdge { edge, delta_m } => body.move_edge(*edge, *delta_m).map_err(|error| format!("move edge refused: {error}"))?,
        BlockOp::ExtrudeEdge { edge, delta_m } => body.extrude_edge(*edge, *delta_m).map_err(|error| format!("extrude edge refused: {error}"))?,
        BlockOp::MoveVertex { vertex, delta_m } => body.move_vertex(*vertex, *delta_m).map_err(|error| format!("move vertex refused: {error}"))?,
        BlockOp::BevelEdges { edges, distance_m } => {
            body.bevel_edges(edges, *distance_m).map(|cut| cut.edit).map_err(|error| format!("bevel refused: {error}"))?
        }
        _ => return Err("the operation is not a topology edit".into()),
    };
    Ok(edit)
}

fn finite_extent(size_m: [f64; 3], label: &str) -> Result<[f64; 3], String> {
    if size_m.iter().any(|axis| !axis.is_finite()) {
        return Err(format!("{label} is not finite"));
    }
    if size_m.iter().any(|axis| *axis < BLOCK_MIN_EXTENT_M - 1.0e-9 || *axis > BLOCK_MAX_EXTENT_M + 1.0e-9) {
        return Err(format!("{label} is outside 0.05 m to 1000 m"));
    }
    Ok(size_m)
}

fn has_features(inset_m: &[f64; 6], bevel_m: f64) -> bool {
    bevel_m.abs() > 1.0e-9 || inset_m.iter().any(|value| value.abs() > 1.0e-9)
}

fn compare(record: &BlockRecord, candidate: &Candidate) -> IntentShadowReport {
    match (&record.body, &candidate.body) {
        (None, None) if !record.analytic_features() && !has_features(&candidate.inset_m, candidate.bevel_m) => compare_boxes(record, candidate),
        (None, None) => compare_parameters(record, candidate),
        (Some(authority), Some(shadow)) => compare_bodies(record, authority, shadow),
        (authority, shadow) => {
            let detail = if authority.is_some() { "shadow built no body" } else { "shadow built a body the record does not store" };
            mismatch_compare(record, detail, counts_of(authority.as_ref()), counts_of(shadow.as_ref()))
        }
    }
}

fn compare_boxes(record: &BlockRecord, candidate: &Candidate) -> IntentShadowReport {
    let authority = match SolidBody::from_box(record.size_m) {
        Ok(body) => body,
        Err(error) => return mismatch_compare(record, &format!("stored size is not a box: {error}"), (0, 0, 0), (0, 0, 0)),
    };
    let shadow = match SolidBody::from_box(candidate.size_m) {
        Ok(body) => body,
        Err(error) => return mismatch_compare(record, &format!("shadow size is not a box: {error}"), counts_of(Some(&authority)), (0, 0, 0)),
    };
    let mut report = compare_bodies(record, &authority, &shadow);
    if report.status == IntentShadowStatus::Pass {
        report.text.push_str("\nnote: analytic box, no stored topology");
    }
    report
}

fn compare_parameters(record: &BlockRecord, candidate: &Candidate) -> IntentShadowReport {
    let size_gap = max_abs(&record.size_m, &candidate.size_m);
    let inset_gap = max_abs(&record.inset_m, &candidate.inset_m);
    let bevel_gap = (record.bevel_m - candidate.bevel_m).abs();
    if size_gap > PARAMETER_TOL || inset_gap > PARAMETER_TOL || bevel_gap > PARAMETER_TOL {
        return mismatch_compare(
            record,
            &format!("analytic parameters differ: size {size_gap:.3e} m, inset {inset_gap:.3e} m, bevel {bevel_gap:.3e} m"),
            (0, 0, 0),
            (0, 0, 0),
        );
    }
    let bits = record.size_m == candidate.size_m && record.inset_m == candidate.inset_m && record.bevel_m == candidate.bevel_m;
    let volume_gap = (box_volume(record.size_m) - box_volume(candidate.size_m)).abs();
    pass_report((0, 0, 0), (0, 0, 0), bits, true, volume_gap, size_gap.max(bevel_gap).max(inset_gap), 0, Some("analytic inset or bevel, no stored topology"))
}

fn compare_bodies(record: &BlockRecord, authority: &SolidBody, shadow: &SolidBody) -> IntentShadowReport {
    let bits = hash_body(authority) == hash_body(shadow);
    let problem = if bits { None } else { geometry_problem(authority, shadow) };
    let size_gap = max_abs(&extent(shadow), &record.size_m);
    let problem = problem.or_else(|| (size_gap > STORED_SIZE_TOL_M).then(|| format!("shadow bounds differ from the stored size by {size_gap:.3e} m")));
    let auth_counts = counts_of(Some(authority));
    let shadow_counts = counts_of(Some(shadow));
    match problem {
        Some(detail) => mismatch_compare(record, &detail, auth_counts, shadow_counts),
        None => {
            let volume_gap = (volume(authority) - volume(shadow)).abs();
            let bounds_gap = bounds_delta(authority, shadow);
            let unmatched = unmatched_elements(record, Some(authority), Some(shadow));
            pass_report(auth_counts, shadow_counts, bits, false, volume_gap, bounds_gap, unmatched, None)
        }
    }
}

fn geometry_problem(authority: &SolidBody, shadow: &SolidBody) -> Option<String> {
    let vertices = id_gap("vertex", &sorted_ids(authority.vertices.iter().map(|vertex| vertex.id)), &sorted_ids(shadow.vertices.iter().map(|vertex| vertex.id)))?;
    if !vertices.is_empty() {
        return Some(vertices);
    }
    for id in sorted_ids(authority.vertices.iter().map(|vertex| vertex.id)) {
        let left = authority.vertex_position(id)?;
        let right = shadow.vertex_position(id)?;
        let gap = distance(left, right);
        if gap > POSITION_TOL_M {
            return Some(format!("vertex {id} position differs by {gap:.3e} m"));
        }
    }
    let edges = id_gap("edge", &sorted_ids(authority.edges.iter().map(|edge| edge.id)), &sorted_ids(shadow.edges.iter().map(|edge| edge.id)))?;
    if !edges.is_empty() {
        return Some(edges);
    }
    for id in sorted_ids(authority.edges.iter().map(|edge| edge.id)) {
        let left = endpoint_ids(authority, id)?;
        let right = endpoint_ids(shadow, id)?;
        if left != right {
            return Some(format!("edge {id} connects {}-{} on the authoritative body and {}-{} on the shadow", left.0, left.1, right.0, right.1));
        }
    }
    let faces = id_gap("face", &sorted_ids(authority.faces.iter().map(|face| face.id)), &sorted_ids(shadow.faces.iter().map(|face| face.id)))?;
    if !faces.is_empty() {
        return Some(faces);
    }
    for id in sorted_ids(authority.faces.iter().map(|face| face.id)) {
        let left = authority.face_loop(id)?;
        let right = shadow.face_loop(id)?;
        if !same_cycle(left, right) {
            if reversed_cycle(left, right) {
                return Some(format!("face {id} orientation differs"));
            }
            return Some(format!("face {id} loop differs"));
        }
        let left_normal = authority.unit_normal(id)?;
        let right_normal = shadow.unit_normal(id)?;
        if dot(left_normal, right_normal) < 1.0 - NORMAL_TOL {
            return Some(format!("normal of face {id} diverges"));
        }
    }
    let volume_gap = (volume(authority) - volume(shadow)).abs();
    if !volume_gap.is_finite() || volume_gap > VOLUME_TOL_M3 {
        return Some(format!("volume differs by {volume_gap:.3e} m³"));
    }
    let bounds_gap = bounds_delta(authority, shadow);
    if !bounds_gap.is_finite() || bounds_gap > BOUNDS_TOL_M {
        return Some(format!("bounds differ by {bounds_gap:.3e} m"));
    }
    None
}

fn id_gap(kind: &str, left: &[u32], right: &[u32]) -> Option<String> {
    if left == right {
        return Some(String::new());
    }
    let mut index = 0;
    let detail = loop {
        match (left.get(index), right.get(index)) {
            (Some(id), Some(other)) if id == other => index += 1,
            (Some(id), Some(other)) => break format!("{kind} {id} vs {other}"),
            (Some(id), None) => break format!("{kind} {id} is on the authoritative body and missing from the shadow"),
            (None, Some(id)) => break format!("{kind} {id} is on the shadow and missing from the authoritative body"),
            (None, None) => break format!("{kind} ids differ"),
        }
    };
    Some(detail)
}

fn sorted_ids(ids: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut ids: Vec<u32> = ids.collect();
    ids.sort_unstable();
    ids
}

fn endpoint_ids(body: &SolidBody, id: u32) -> Option<(u32, u32)> {
    let edge = body.edges.iter().find(|edge| edge.id == id)?;
    Some((edge.a.min(edge.b), edge.a.max(edge.b)))
}

fn same_cycle(left: &[u32], right: &[u32]) -> bool {
    if left.len() != right.len() || left.is_empty() {
        return false;
    }
    let count = left.len();
    (0..count).any(|rotation| (0..count).all(|index| left[(rotation + index) % count] == right[index]))
}

fn reversed_cycle(left: &[u32], right: &[u32]) -> bool {
    let reversed: Vec<u32> = right.iter().rev().copied().collect();
    same_cycle(left, &reversed)
}

fn unmatched_elements(record: &BlockRecord, authority: Option<&SolidBody>, shadow: Option<&SolidBody>) -> usize {
    let mut named = Vec::new();
    let listed = if record.steps.len() == record.history.len() && !record.steps.is_empty() {
        record.steps.iter().flat_map(|step| step.concrete.iter().copied()).collect::<Vec<_>>()
    } else {
        record.history.iter().flat_map(BlockOp::concrete_elements).collect::<Vec<_>>()
    };
    for element in listed {
        if !named.contains(&element) {
            named.push(element);
        }
    }
    named.into_iter().filter(|element| element_present(authority, *element) != element_present(shadow, *element)).count()
}

fn element_present(body: Option<&SolidBody>, element: ConcreteElement) -> bool {
    body.is_some_and(|body| match element {
        ConcreteElement::Face(id) => body.face_loop(id).is_some(),
        ConcreteElement::Edge(id) => body.edge_endpoints(id).is_some(),
        ConcreteElement::Vertex(id) => body.vertex_position(id).is_some(),
    })
}

fn pass_report(
    authority: (usize, usize, usize),
    shadow: (usize, usize, usize),
    bits: bool,
    parameters: bool,
    volume_gap: f64,
    bounds_gap: f64,
    unmatched: usize,
    note: Option<&str>,
) -> IntentShadowReport {
    let (hash_line, topology_line) = if parameters {
        ("analytic parameters", "analytic parameters")
    } else if bits {
        ("match", "match")
    } else {
        ("differ", "within tolerance")
    };
    let mut text = format!(
        "Shadow: PASS\nbody hash: {hash_line}\ntopology: {topology_line}\nvolume Δ: {}\nbounds Δ: {}\nfaces: authoritative {} / shadow {}\nedges: authoritative {} / shadow {}\nvertices: authoritative {} / shadow {}\nunmatched semantic elements: {unmatched}",
        gap_text(volume_gap),
        gap_text(bounds_gap),
        authority.0,
        shadow.0,
        authority.1,
        shadow.1,
        authority.2,
        shadow.2,
    );
    if let Some(note) = note {
        text.push('\n');
        text.push_str("note: ");
        text.push_str(note);
    }
    IntentShadowReport {
        status: IntentShadowStatus::Pass,
        text,
        authoritative_faces: authority.0,
        shadow_faces: shadow.0,
        authoritative_edges: authority.1,
        shadow_edges: shadow.1,
        authoritative_vertices: authority.2,
        shadow_vertices: shadow.2,
    }
}

fn mismatch_replay(record: &BlockRecord, index: usize, detail: &str) -> IntentShadowReport {
    let shadow_counts = match creation_seed(record) {
        Ok(seed) => replay(seed, &record.history[..index]).ok().and_then(|candidate| candidate.body).as_ref().map(|body| counts_of(Some(body))).unwrap_or((0, 0, 0)),
        Err(_) => (0, 0, 0),
    };
    mismatch(record, Some(index), detail, counts_of(record.body.as_ref()), shadow_counts, false)
}

fn mismatch_compare(record: &BlockRecord, detail: &str, authority: (usize, usize, usize), shadow: (usize, usize, usize)) -> IntentShadowReport {
    let index = record.history.len().checked_sub(1);
    mismatch(record, index, detail, authority, shadow, true)
}

fn mismatch(
    record: &BlockRecord,
    index: Option<usize>,
    detail: &str,
    authority: (usize, usize, usize),
    shadow: (usize, usize, usize),
    checkpoint: bool,
) -> IntentShadowReport {
    let (operation, step, target) = match index {
        Some(index) => match record.history.get(index) {
            Some(op) => (operation_name(op), step_number(record, index), op.summary()),
            None => ("none", 0, "Block".to_string()),
        },
        None => ("none", 0, "Block".to_string()),
    };
    let mut text = format!(
        "INTENT_SHADOW_MISMATCH\noperation: {operation}\nstep: {step}\nsemantic target: {target}\nauthoritative faces: {}\nshadow faces: {}\nfirst topology divergence: {detail}",
        authority.0, shadow.0
    );
    if checkpoint {
        text.push_str("\nthe stored checkpoint has no intermediate body, so this is the first visible disagreement");
    }
    IntentShadowReport {
        status: IntentShadowStatus::Mismatch,
        text,
        authoritative_faces: authority.0,
        shadow_faces: shadow.0,
        authoritative_edges: authority.1,
        shadow_edges: shadow.1,
        authoritative_vertices: authority.2,
        shadow_vertices: shadow.2,
    }
}

fn incomplete(record: &BlockRecord, reason: &str) -> IntentShadowReport {
    let (faces, edges, vertices) = counts_of(record.body.as_ref());
    IntentShadowReport {
        status: IntentShadowStatus::Incomplete,
        text: format!("Shadow: INCOMPLETE\n{reason}"),
        authoritative_faces: faces,
        shadow_faces: 0,
        authoritative_edges: edges,
        shadow_edges: 0,
        authoritative_vertices: vertices,
        shadow_vertices: 0,
    }
}

fn operation_name(op: &BlockOp) -> &'static str {
    match op {
        BlockOp::Size { .. } => "Size",
        BlockOp::ExtrudeFace { .. } | BlockOp::ExtrudeFaces { .. } => "Extrude",
        BlockOp::InsetFace { .. } => "Inset",
        BlockOp::Bevel { .. } | BlockOp::BevelEdges { .. } => "Bevel",
        BlockOp::Mirror { .. } => "Mirror",
        BlockOp::MoveEdge { .. } => "Move Edge",
        BlockOp::ExtrudeEdge { .. } => "Extrude Edge",
        BlockOp::SplitEdge { .. } => "Split",
        BlockOp::SubdivideFace { .. } => "Subdivide",
        BlockOp::MoveVertex { .. } => "Move Vertex",
    }
}

fn step_number(record: &BlockRecord, index: usize) -> u64 {
    if record.steps.len() == record.history.len() {
        if let Some(step) = record.steps.get(index) {
            return step.id.number();
        }
    }
    index as u64 + 1
}

fn counts_of(body: Option<&SolidBody>) -> (usize, usize, usize) {
    match body {
        Some(body) => (body.faces.len(), body.edges.len(), body.vertices.len()),
        None => (0, 0, 0),
    }
}

fn gap_text(value: f64) -> String {
    if value == 0.0 { "0".to_string() } else { format!("{value:.3e}") }
}

fn hash_body(body: &SolidBody) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mix = |hash: &mut u64, bytes: &[u8]| {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    mix(&mut hash, &body.next_id.to_le_bytes());
    mix(&mut hash, &(body.vertices.len() as u64).to_le_bytes());
    for vertex in &body.vertices {
        mix(&mut hash, &vertex.id.to_le_bytes());
        for axis in vertex.position {
            mix(&mut hash, &axis.to_bits().to_le_bytes());
        }
    }
    mix(&mut hash, &(body.edges.len() as u64).to_le_bytes());
    for edge in &body.edges {
        mix(&mut hash, &edge.id.to_le_bytes());
        mix(&mut hash, &edge.a.to_le_bytes());
        mix(&mut hash, &edge.b.to_le_bytes());
    }
    mix(&mut hash, &(body.faces.len() as u64).to_le_bytes());
    for face in &body.faces {
        mix(&mut hash, &face.id.to_le_bytes());
        mix(&mut hash, &(face.vertices.len() as u64).to_le_bytes());
        for vertex in &face.vertices {
            mix(&mut hash, &vertex.to_le_bytes());
        }
    }
    hash
}

fn volume(body: &SolidBody) -> f64 {
    let mut sum = 0.0;
    for face in &body.faces {
        let Some(positions) = body.face_positions(face.id) else { continue };
        if positions.len() < 3 {
            continue;
        }
        for index in 1..positions.len() - 1 {
            sum += triple(positions[0], positions[index], positions[index + 1]);
        }
    }
    sum / 6.0
}

fn box_volume(size_m: [f64; 3]) -> f64 {
    size_m[0] * size_m[1] * size_m[2]
}

fn extent(body: &SolidBody) -> [f64; 3] {
    let (min, max) = aabb(body);
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}

fn bounds_delta(left: &SolidBody, right: &SolidBody) -> f64 {
    let (left_min, left_max) = aabb(left);
    let (right_min, right_max) = aabb(right);
    max_abs(&left_min, &right_min).max(max_abs(&left_max, &right_max))
}

fn aabb(body: &SolidBody) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    for vertex in &body.vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex.position[axis]);
            max[axis] = max[axis].max(vertex.position[axis]);
        }
    }
    (min, max)
}

fn max_abs(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right).map(|(left, right)| (left - right).abs()).fold(0.0, f64::max)
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let gap = [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
    (gap[0] * gap[0] + gap[1] * gap[1] + gap[2] * gap[2]).sqrt()
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[1] * right[2] - left[2] * right[1], left[2] * right[0] - left[0] * right[2], left[0] * right[1] - left[1] * right[0]]
}

fn triple(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    dot(cross(a, b), c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{empty_world_level, parse_level, AuthoringResult, BlockOp, ComponentRecord, LevelDocument, Vec3, BLOCK_HISTORY_LIMIT};

    fn blank() -> (crate::LevelDocument, crate::SceneWorld, crate::EntityId) {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        (document, world, id)
    }

    fn assert_unchanged(world: &crate::SceneWorld, id: crate::EntityId, before: &BlockRecord) {
        assert_eq!(&world.authored_block(id).unwrap(), before);
    }

    #[test]
    fn intent_shadow_torture_sequence_matches_after_save_and_reload() {
        let (document, mut world, id) = blank();
        assert_eq!(world.set_block_extent(id, 0, 3.0).unwrap(), AuthoringResult::Applied);
        let resized = world.authored_block(id).unwrap();
        assert_eq!(resized.seed_size_m, Some([2.0, 2.0, 2.0]));
        assert_eq!(resized.size_m[0], 3.0);
        assert!(resized.body.is_none());
        assert_eq!(world.subdivide_block_face(id, 5, 2, 2).unwrap(), AuthoringResult::Applied);
        let divided = world.authored_block(id).unwrap().body.unwrap();
        let cell = divided
            .faces
            .iter()
            .find(|face| face.id != 5 && divided.unit_normal(face.id).is_some_and(|normal| normal[2] > 0.9))
            .map(|face| face.id)
            .expect("subdivide created a +Z cell");
        let before_ids: Vec<u32> = divided.faces.iter().map(|face| face.id).collect();
        assert_eq!(world.extrude_block_faces(id, &[cell], [0.0, 0.0, 0.4]).unwrap(), AuthoringResult::Applied);
        let raised = world.authored_block(id).unwrap().body.unwrap();
        let side = raised
            .faces
            .iter()
            .find(|face| !before_ids.contains(&face.id) && raised.unit_normal(face.id).is_some_and(|normal| normal[2].abs() < 0.5))
            .expect("the first extrude created a side face");
        let normal = raised.unit_normal(side.id).unwrap();
        assert_eq!(
            world.extrude_block_faces(id, &[side.id], [normal[0] * 0.4, normal[1] * 0.4, normal[2] * 0.4]).unwrap(),
            AuthoringResult::Applied
        );
        let edges: Vec<u32> = world.authored_block(id).unwrap().body.unwrap().edges.iter().map(|edge| edge.id).collect();
        assert!(edges.iter().any(|edge| world.split_block_edge(id, *edge).is_ok()), "no edge accepted a split");
        let body = world.authored_block(id).unwrap().body.unwrap();
        let mut chosen = None;
        for width in [0.05, 0.1, 0.2] {
            for edge in &body.edges {
                if let Ok(cut) = body.bevel_edges(&[edge.id], width) {
                    chosen = Some(cut);
                    break;
                }
            }
            if chosen.is_some() {
                break;
            }
        }
        let cut = chosen.expect("the torture solid has no bevelable edge");
        let local = world.entity_local_pose(id).unwrap();
        let translation = Vec3::new(local.translation.x + cut.edit.shift[0], local.translation.y + cut.edit.shift[1], local.translation.z + cut.edit.shift[2]);
        assert_eq!(world.preview_block_body(id, cut.edit.body.clone(), translation).unwrap(), AuthoringResult::Applied);
        assert_eq!(
            world.commit_block_topology(id, BlockOp::BevelEdges { edges: cut.edges.clone(), distance_m: cut.width_m }).unwrap(),
            AuthoringResult::Applied
        );
        let record = world.authored_block(id).unwrap();
        assert_eq!(record.seed_size_m, Some([2.0, 2.0, 2.0]));
        assert!(record.body.is_some());
        let report = evaluate_intent_shadow(&record);
        assert_unchanged(&world, id, &record);
        assert_eq!(report.status, IntentShadowStatus::Pass, "{}", report.text);
        assert!(report.text.contains("Shadow: PASS"), "{}", report.text);
        assert!(report.text.contains("body hash: match"), "{}", report.text);
        assert!(report.text.contains("topology: match"), "{}", report.text);
        assert_eq!(report.authoritative_faces, report.shadow_faces);
        assert_eq!(unmatched_line(&report), 0, "{}", report.text);
        let json = LevelDocument::capture(&world, document.level_uuid, "Shadow").unwrap().to_json();
        assert!(json.contains("seed_size_m") && json.contains("\"body\"") && json.contains("\"steps\""));
        for forbidden in ["meshlet", "einstein", "Einstein", "indices", "triangle", "hierarchy"] {
            assert!(!json.contains(forbidden), "{forbidden} leaked into the block level");
        }
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let again = loaded.authored_block(id).unwrap();
        assert_eq!(again.body, record.body);
        assert_eq!(again.seed_size_m, Some([2.0, 2.0, 2.0]));
        let reloaded = evaluate_intent_shadow(&again);
        assert_eq!(reloaded.status, IntentShadowStatus::Pass, "{}", reloaded.text);
        assert!(reloaded.text.contains("body hash: match"), "{}", reloaded.text);
        let mut tampered = again.clone();
        tampered.body = SolidBody::from_box(tampered.size_m).ok();
        let lied = evaluate_intent_shadow(&tampered);
        assert_eq!(lied.status, IntentShadowStatus::Mismatch, "{}", lied.text);
        assert!(lied.text.contains("INTENT_SHADOW_MISMATCH"), "{}", lied.text);
        assert_eq!(again, loaded.authored_block(id).unwrap());
    }

    #[test]
    fn intent_shadow_names_a_corrupt_bevel_step() {
        let (_document, mut world, id) = blank();
        assert_eq!(world.subdivide_block_face(id, 5, 2, 2).unwrap(), AuthoringResult::Applied);
        let body = world.authored_block(id).unwrap().body.unwrap();
        let cut = body.edges.iter().find_map(|edge| body.bevel_edges(&[edge.id], 0.1).ok()).expect("a fresh subdivided box has a bevelable edge");
        let local = world.entity_local_pose(id).unwrap();
        let translation = Vec3::new(local.translation.x + cut.edit.shift[0], local.translation.y + cut.edit.shift[1], local.translation.z + cut.edit.shift[2]);
        world.preview_block_body(id, cut.edit.body, translation).unwrap();
        world.commit_block_topology(id, BlockOp::BevelEdges { edges: cut.edges, distance_m: cut.width_m }).unwrap();
        let record = world.authored_block(id).unwrap();
        let step = record.steps.last().unwrap().id.number();
        let mut missing = record.clone();
        let renamed = if let Some(BlockOp::BevelEdges { edges, .. }) = missing.history.last_mut() {
            edges[0] = 9_999_999;
            edges.clone()
        } else {
            panic!("bevel was not the last operation");
        };
        missing.steps.last_mut().unwrap().concrete = renamed.into_iter().map(ConcreteElement::Edge).collect();
        let report = evaluate_intent_shadow(&missing);
        assert_eq!(report.status, IntentShadowStatus::Mismatch, "{}", report.text);
        assert!(report.text.contains("operation: Bevel"), "{}", report.text);
        assert!(report.text.contains(&format!("step: {step}")), "{}", report.text);
        assert!(report.text.contains("that element is not on the solid"), "{}", report.text);
        assert!(!report.text.contains("Shadow: PASS"), "{}", report.text);
        let mut wider = record.clone();
        if let Some(BlockOp::BevelEdges { distance_m, .. }) = wider.history.last_mut() {
            *distance_m += 0.2;
        }
        let drifted = evaluate_intent_shadow(&wider);
        assert_eq!(drifted.status, IntentShadowStatus::Mismatch, "{}", drifted.text);
        assert!(drifted.text.contains("operation: Bevel"), "{}", drifted.text);
        assert!(drifted.text.contains(&format!("step: {step}")), "{}", drifted.text);
        assert!(drifted.text.contains("the stored checkpoint has no intermediate body") || drifted.text.contains("bevel refused"), "{}", drifted.text);
    }

    #[test]
    fn intent_shadow_incomplete_when_the_prefix_or_the_seed_is_missing() {
        let mut truncated = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        for _ in 0..BLOCK_HISTORY_LIMIT + 1 {
            truncated.push_op(BlockOp::SplitEdge { edge: 12 });
        }
        assert_eq!(truncated.steps[0].id.number(), 2);
        let report = evaluate_intent_shadow(&truncated);
        assert_eq!(report.status, IntentShadowStatus::Incomplete, "{}", report.text);
        assert!(report.text.contains("dropped its prefix"), "{}", report.text);
        assert!(!report.text.contains("INTENT_SHADOW_MISMATCH"), "{}", report.text);
        let mut unseeded = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        unseeded.seed_size_m = None;
        unseeded.push_op(BlockOp::Size { size_m: [3.0, 2.0, 2.0] });
        let missing = evaluate_intent_shadow(&unseeded);
        assert_eq!(missing.status, IntentShadowStatus::Incomplete, "{}", missing.text);
        assert!(missing.text.contains("creation size is not in the intent record"), "{}", missing.text);
        let (document, mut world, id) = blank();
        world.subdivide_block_face(id, 3, 2, 2).unwrap();
        let mut captured = LevelDocument::capture(&world, document.level_uuid, "Legacy").unwrap();
        for entity in &mut captured.entities {
            for component in &mut entity.components {
                if let ComponentRecord::ParametricBlock(block) = component {
                    block.seed_size_m = None;
                }
            }
        }
        let json = captured.to_json();
        assert!(!json.contains("seed_size_m"));
        assert!(json.contains("\"body\""));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let old = evaluate_intent_shadow(&loaded.authored_block(id).unwrap());
        assert_eq!(old.status, IntentShadowStatus::Incomplete, "{}", old.text);
        assert!(old.text.contains("creation size is not in the intent record"), "{}", old.text);
    }

    #[test]
    fn intent_shadow_replays_a_short_legacy_log_with_a_seed() {
        let mut record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        let edit = SolidBody::from_box([2.0, 2.0, 2.0]).unwrap().subdivide_face(5, 2, 2).unwrap();
        record.history.push(BlockOp::SubdivideFace { face: 5, u: 2, v: 2 });
        record.body = Some(edit.body.clone());
        record.size_m = edit.size_m;
        assert!(record.steps.is_empty());
        record.validate().unwrap();
        let report = evaluate_intent_shadow(&record);
        assert_eq!(report.status, IntentShadowStatus::Pass, "{}", report.text);
        assert!(report.text.contains("body hash: match"), "{}", report.text);
        assert_eq!(record.body.as_ref(), Some(&edit.body));
    }

    #[test]
    fn intent_shadow_analytic_box_and_features_do_not_invent_a_body() {
        let (document, mut world, id) = blank();
        let plain = evaluate_intent_shadow(&world.authored_block(id).unwrap());
        assert_eq!(plain.status, IntentShadowStatus::Pass, "{}", plain.text);
        assert!(plain.text.contains("analytic box, no stored topology"), "{}", plain.text);
        assert_eq!(plain.authoritative_faces, 6);
        assert_eq!(world.set_block_extent(id, 1, 2.5).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.push_block_face(id, 2, [2.0, 2.5, 2.0], Vec3::new(0.0, 1.0, -4.0), 0.5).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.commit_block_face(id, 2, [2.0, 2.5, 2.0]).unwrap(), AuthoringResult::Applied);
        let pushed = world.authored_block(id).unwrap();
        assert!(pushed.body.is_none());
        assert_eq!(pushed.seed_size_m, Some([2.0, 2.0, 2.0]));
        let report = evaluate_intent_shadow(&pushed);
        assert_eq!(report.status, IntentShadowStatus::Pass, "{}", report.text);
        assert!(report.text.contains("body hash: match"), "{}", report.text);
        assert_eq!(world.set_block_bevel(id, 0.2).unwrap(), AuthoringResult::Applied);
        let featured = world.authored_block(id).unwrap();
        assert!(featured.body.is_none());
        let analytic = evaluate_intent_shadow(&featured);
        assert_eq!(analytic.status, IntentShadowStatus::Pass, "{}", analytic.text);
        assert!(analytic.text.contains("analytic inset or bevel, no stored topology"), "{}", analytic.text);
        assert_eq!(analytic.authoritative_faces, 0);
        let json = LevelDocument::capture(&world, document.level_uuid, "Analytic").unwrap().to_json();
        assert!(json.contains("seed_size_m"));
        assert!(!json.contains("\"body\""));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let again = evaluate_intent_shadow(&loaded.authored_block(id).unwrap());
        assert_eq!(again.status, IntentShadowStatus::Pass, "{}", again.text);
    }

    #[derive(Clone, Debug)]
    enum Action {
        Extent { axis: usize, meters: f64 },
        Subdivide { face: u32, u: u32, v: u32 },
        Extrude { face: u32, delta: [f64; 3] },
        Split { edge: u32 },
        Bevel { edges: Vec<u32>, distance: f64 },
    }

    struct Lcg(u64);

    impl Lcg {
        fn next_u32(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 32) as u32
        }

        fn below(&mut self, count: usize) -> usize {
            self.next_u32() as usize % count
        }
    }

    fn try_action(world: &mut crate::SceneWorld, id: crate::EntityId, rng: &mut Lcg) -> Option<Action> {
        let record = world.authored_block(id)?;
        let kind = rng.below(5);
        if kind == 0 || record.analytic_features() {
            let axis = rng.below(3);
            let choices = [1.0, 1.5, 2.5, 3.0, 4.0, 1.25];
            let meters = choices[rng.below(choices.len())];
            if (record.size_m[axis] - meters).abs() < 1.0e-9 {
                return None;
            }
            if world.set_block_extent(id, axis, meters).ok()? != AuthoringResult::Applied {
                return None;
            }
            return Some(Action::Extent { axis, meters });
        }
        let body = record.body.clone().or_else(|| SolidBody::from_box(record.size_m).ok())?;
        if body.faces.is_empty() || body.edges.is_empty() {
            return None;
        }
        match kind {
            1 => {
                let face = body.faces[rng.below(body.faces.len())].id;
                let u = 2 + rng.below(2) as u32;
                let v = 2 + rng.below(2) as u32;
                world.subdivide_block_face(id, face, u, v).ok()?;
                Some(Action::Subdivide { face, u, v })
            }
            2 => {
                let face = body.faces[rng.below(body.faces.len())].id;
                let normal = body.unit_normal(face)?;
                let distance = [0.25, 0.4, 0.5][rng.below(3)];
                let delta = [normal[0] * distance, normal[1] * distance, normal[2] * distance];
                world.extrude_block_faces(id, &[face], delta).ok()?;
                Some(Action::Extrude { face, delta })
            }
            3 => {
                let edge = body.edges[rng.below(body.edges.len())].id;
                world.split_block_edge(id, edge).ok()?;
                Some(Action::Split { edge })
            }
            _ => {
                let edge = body.edges[rng.below(body.edges.len())].id;
                let distance = [0.05, 0.1, 0.2][rng.below(3)];
                let cut = body.bevel_edges(&[edge], distance).ok()?;
                let local = world.entity_local_pose(id).ok()?;
                let translation = Vec3::new(local.translation.x + cut.edit.shift[0], local.translation.y + cut.edit.shift[1], local.translation.z + cut.edit.shift[2]);
                world.preview_block_body(id, cut.edit.body, translation).ok()?;
                world
                    .commit_block_topology(id, BlockOp::BevelEdges { edges: cut.edges.clone(), distance_m: cut.width_m })
                    .expect("bevel preview stored a body");
                Some(Action::Bevel { edges: cut.edges, distance: cut.width_m })
            }
        }
    }

    fn generate(index: u64) -> Vec<Action> {
        let mut rng = Lcg(0x00C0_FFEE ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let target = 1 + rng.below(6);
        let mut actions = Vec::new();
        let (_document, mut world, id) = blank();
        let mut attempts = 0;
        while actions.len() < target && attempts < 48 {
            attempts += 1;
            if let Some(action) = try_action(&mut world, id, &mut rng) {
                actions.push(action);
            }
        }
        assert!(!actions.is_empty(), "sequence {index} produced no edit");
        actions
    }

    fn apply_actions(actions: &[Action]) -> (crate::LevelDocument, crate::SceneWorld, crate::EntityId) {
        let (document, mut world, id) = blank();
        for action in actions {
            match action {
                Action::Extent { axis, meters } => {
                    world.set_block_extent(id, *axis, *meters).unwrap();
                }
                Action::Subdivide { face, u, v } => {
                    world.subdivide_block_face(id, *face, *u, *v).unwrap();
                }
                Action::Extrude { face, delta } => {
                    world.extrude_block_faces(id, &[*face], *delta).unwrap();
                }
                Action::Split { edge } => {
                    world.split_block_edge(id, *edge).unwrap();
                }
                Action::Bevel { edges, distance } => {
                    let record = world.authored_block(id).unwrap();
                    let body = record.body.clone().unwrap_or_else(|| SolidBody::from_box(record.size_m).unwrap());
                    let cut = body.bevel_edges(edges, *distance).unwrap();
                    let local = world.entity_local_pose(id).unwrap();
                    let translation = Vec3::new(local.translation.x + cut.edit.shift[0], local.translation.y + cut.edit.shift[1], local.translation.z + cut.edit.shift[2]);
                    world.preview_block_body(id, cut.edit.body, translation).unwrap();
                    world.commit_block_topology(id, BlockOp::BevelEdges { edges: edges.clone(), distance_m: *distance }).unwrap();
                }
            }
        }
        (document, world, id)
    }

    fn unmatched_line(report: &IntentShadowReport) -> usize {
        report
            .text
            .lines()
            .find_map(|line| line.strip_prefix("unmatched semantic elements: "))
            .and_then(|value| value.parse().ok())
            .unwrap_or(usize::MAX)
    }

    fn shortest_failure(actions: &[Action]) -> (usize, IntentShadowReport) {
        for len in 1..=actions.len() {
            let (_document, world, id) = apply_actions(&actions[..len]);
            let report = evaluate_intent_shadow(&world.authored_block(id).unwrap());
            if report.status != IntentShadowStatus::Pass {
                return (len, report);
            }
        }
        (actions.len(), evaluate_intent_shadow(&apply_actions(actions).1.authored_block(apply_actions(actions).2).unwrap()))
    }

    #[test]
    fn intent_shadow_property_sequences_match_after_save_and_reload() {
        for index in 0..32u64 {
            let actions = generate(index);
            let (document, world, id) = apply_actions(&actions);
            let record = world.authored_block(id).unwrap();
            let report = evaluate_intent_shadow(&record);
            assert_unchanged(&world, id, &record);
            if report.status != IntentShadowStatus::Pass {
                let (len, failed) = shortest_failure(&actions);
                panic!("sequence {index} diverged\nshortest prefix {len}/{}\n{:?}\n{}", actions.len(), &actions[..len], failed.text);
            }
            let json = LevelDocument::capture(&world, document.level_uuid, "Property").unwrap().to_json();
            assert!(json.contains("seed_size_m"), "sequence {index} omitted the creation size");
            assert!(!json.contains("meshlet") && !json.contains("einstein") && !json.contains("hierarchy"), "sequence {index} leaked derived data");
            let loaded = parse_level(&json).unwrap().instantiate().unwrap();
            let again = loaded.authored_block(id).unwrap();
            assert_eq!(counts_of(again.body.as_ref()), counts_of(record.body.as_ref()), "sequence {index} changed its topology counts across save/reload");
            let reloaded = evaluate_intent_shadow(&again);
            if reloaded.status != IntentShadowStatus::Pass {
                panic!("sequence {index} diverged after reload\n{}", reloaded.text);
            }
        }
    }
}
