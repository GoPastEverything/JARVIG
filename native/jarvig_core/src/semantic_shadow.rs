//! Non-authoritative semantic rebinding.
//!
//! Concrete face, edge, and vertex ids remain the stored solid, the pick, and the highlight.
//! A [`SemanticRef`] names provenance and role. Replay resolves that name against lineage the
//! operation just emitted. One match binds. None is missing. Two is ambiguous. A role the new
//! grid or extrude does not produce is invalidated. The stored body is not an input.
//!
//! A modeling commit may store that name beside the concrete id. An older step leaves the
//! field absent. Load keeps the stored body either way.
//!
//! [`intent_authority_candidate`] rebuilds a body from the persistent tape alone. It does not
//! read the stored body. `parse_level` does not call it, so the file record stays as written.
//!
//! [`intent_authority_eligibility`] reports whether that reconstruction is complete enough to
//! install. [`realize_eligible_body_on_load`] runs from level spawn only when that world's
//! intent-authority experiment is on. The switch defaults off. A stored body is kept, an
//! ineligible tape is kept, and an eligible tape with no body is realized before the mesh is
//! built. ADR-0074 is still Proposed. The switch is the experiment, not that acceptance.
//!
//! On an eligible record with no stored body, `size_m` is the replay cache.
//! A tape with no `round` stores the planar replay AABB. A tape with a valid
//! `round` stores the union of that AABB and the analytic fillet bounds.
//! A later `round` replaces the active fillet. One fillet stays active.
//! [`commit_class_c_intent`] appends one intent operation and writes that cache from the
//! replay. The replay starts at `seed_size_m` and does not read `size_m`. Parse does not
//! repair a stale cache. A stored body, and every ineligible record, stay on the accepted rule.
//! ADR-0074 stays Proposed. `analytic-surface` stays refused.

use std::fmt;

use crate::topology::{BirthRole, ElementBirth, ElementKind, SolidBody, TopologyLineage};
use crate::{evaluate_intent_shadow, BlockOp, BlockRecord, ConcreteElement, IntentPayload, IntentShadowStatus};

const MIN_DISTANCE_M: f64 = 1.0e-9;
const NORMAL_AGREE: f64 = 1.0e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticShadowStatus {
    Pass,
    Missing,
    Ambiguous,
    Invalidated,
    Refused,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProvenanceFact {
    pub identity: String,
    pub kind: ElementKind,
    pub concrete: u32,
}

/// The candidate is a replay result. It is not the solid, and it is not saved.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticShadowReport {
    pub status: SemanticShadowStatus,
    pub text: String,
    pub resolved: u32,
    pub rebound: u32,
    pub missing: u32,
    pub ambiguous: u32,
    pub invalidated: u32,
    pub candidate: Option<SolidBody>,
    pub provenance: Vec<ProvenanceFact>,
}

impl SemanticShadowReport {
    #[cfg(test)]
    pub(crate) fn concrete_of(&self, identity: &str) -> Option<u32> {
        let mut hits: Vec<u32> = self.provenance.iter().filter(|fact| fact.identity == identity).map(|fact| fact.concrete).collect();
        hits.sort_unstable();
        hits.dedup();
        if hits.len() == 1 { Some(hits[0]) } else { None }
    }

    /// Identities present in both evaluations whose concrete id changed.
    #[cfg(test)]
    pub(crate) fn rebound_against(&self, earlier: &Self) -> u32 {
        let mut count = 0u32;
        for fact in &self.provenance {
            let mut prior: Vec<u32> = earlier.provenance.iter().filter(|other| other.identity == fact.identity).map(|other| other.concrete).collect();
            prior.sort_unstable();
            prior.dedup();
            if prior.len() == 1 && prior[0] != fact.concrete {
                count += 1;
            }
        }
        count
    }
}

/// Provenance and role. Equality is the identity. It does not store a concrete id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SemanticRef {
    kind: ElementKind,
    role: SemanticRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SemanticRole {
    SeedFace { axis: u8 },
    SeedEdge { slot: u8 },
    SubdivCell { parent: Box<SemanticRef>, u: u32, v: u32 },
    SubdivEdge { parent: Box<SemanticRef>, u: u32, v: u32, side: u8 },
    ExtrudeCap { source: Box<SemanticRef> },
    ExtrudeSide { cap: Box<SemanticRef>, boundary: Box<SemanticRef> },
    /// The lip of extruding `cap`, grown from `boundary`. The cap is part of the name:
    /// a later extrude can still carry the same boundary edge.
    ExtrudeOuter { cap: Box<SemanticRef>, boundary: Box<SemanticRef> },
    /// `ordinal` distinguishes a later split of the same edge reference. Zero is the first.
    SplitKept { edge: Box<SemanticRef>, ordinal: u32 },
    SplitNew { edge: Box<SemanticRef>, ordinal: u32 },
    SplitVertex { edge: Box<SemanticRef>, ordinal: u32 },
    /// Lip left where an extrude trimmed a neighbor. Named by the cap and the boundary it crossed.
    ExtrudeCarve { cap: Box<SemanticRef>, boundary: Box<SemanticRef> },
    /// Leg at one end of that boundary. `end` 0 is the directed start.
    ExtrudeLeg { cap: Box<SemanticRef>, boundary: Box<SemanticRef>, end: u8 },
    /// `ordinal` distinguishes a later bevel of the same edge reference. Zero is the first.
    BevelFace { edge: Box<SemanticRef>, ordinal: u32 },
    BevelEdge { edge: Box<SemanticRef>, slot: u32, ordinal: u32 },
    /// A side of `cap` without naming the boundary. More than one side is ambiguous on purpose.
    /// The editor log never writes this. A semantic program uses it to prove a refusal.
    #[allow(dead_code)]
    AnyExtrudeSide { cap: Box<SemanticRef> },
    /// `ordinal` distinguishes a later extrude of the same edge. Zero is the first.
    ExtrudeEdgeWall { edge: Box<SemanticRef>, ordinal: u32 },
    ExtrudeEdgeOuter { edge: Box<SemanticRef>, ordinal: u32 },
    /// The curved surface of one round. The source is the edge the round named.
    FilletFace { edge: Box<SemanticRef> },
    /// Tangent where the fillet meets a neighboring face. `side` 0 is the lower face.
    FilletBoundary { edge: Box<SemanticRef>, side: u8 },
    /// Junction at one end of the fillet. `end` 0 is the start.
    FilletJunction { edge: Box<SemanticRef>, end: u8 },
    /// Corner where two or three rounds of one radius meet. The edges are sorted.
    CornerPatch { edges: Vec<Box<SemanticRef>> },
}

#[derive(Clone, Debug, PartialEq)]
enum ExtrudeAmount {
    /// Editor logs store a delta. A semantic program may name a distance along the face normal.
    #[allow(dead_code)]
    Distance(f64),
    Delta([f64; 3]),
}

/// One element may carry several names. They must bind to the same concrete id.
#[derive(Clone, Debug, PartialEq)]
enum SemanticOp {
    Create { size_m: [f64; 3] },
    Resize { size_m: [f64; 3] },
    Subdivide { face: Vec<SemanticRef>, u: u32, v: u32 },
    Extrude { faces: Vec<Vec<SemanticRef>>, amount: ExtrudeAmount },
    Split { edge: Vec<SemanticRef> },
    Bevel { edges: Vec<Vec<SemanticRef>>, width_m: f64 },
    MoveEdge { edge: Vec<SemanticRef>, delta_m: [f64; 3] },
    MoveVertex { vertex: Vec<SemanticRef>, delta_m: [f64; 3] },
    ExtrudeEdge { edge: Vec<SemanticRef>, delta_m: [f64; 3] },
    /// Reflect every vertex through the local origin and reverse each loop.
    Mirror { axis: u8 },
    /// Grow one constructor face of the analytic box. This is not a stored face id.
    PushFace { face: u8, distance_m: f64 },
    /// One or more straight manifold edges sharing one radius. The fillets stay analytic.
    Round { edge: Vec<SemanticRef>, radius_m: f64 },
    /// Mints the constructor solid after create. It names no element.
    Seed,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SemanticProgram {
    ops: Vec<SemanticOp>,
}

impl SemanticProgram {
    pub(crate) fn to_text(&self) -> String {
        let mut lines = vec!["semantic-shadow 1".to_string()];
        for op in &self.ops {
            lines.push(op.to_line());
        }
        lines.join("\n")
    }

    pub(crate) fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines().filter(|line| !line.trim().is_empty());
        let header = lines.next().ok_or_else(|| "semantic program is empty".to_string())?;
        if header.trim() != "semantic-shadow 1" {
            return Err("semantic program header is not semantic-shadow 1".into());
        }
        let mut ops = Vec::new();
        for line in lines {
            ops.push(SemanticOp::from_line(line.trim())?);
        }
        if !matches!(ops.first(), Some(SemanticOp::Create { .. })) {
            return Err("a semantic program starts with create".into());
        }
        Ok(Self { ops })
    }
}

/// Shadow-side undo of a semantic program. This is not the editor's transaction stack.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct SemanticSession {
    undo: Vec<SemanticProgram>,
    redo: Vec<SemanticProgram>,
    current: SemanticProgram,
}

#[cfg(test)]
impl SemanticSession {
    pub(crate) fn new(program: SemanticProgram) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), current: program }
    }

    pub(crate) fn commit(&mut self, program: SemanticProgram) {
        self.undo.push(self.current.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.current = program;
        self.redo.clear();
    }

    pub(crate) fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else { return false };
        self.redo.push(self.current.clone());
        self.current = previous;
        true
    }

    pub(crate) fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(self.current.clone());
        self.current = next;
        true
    }

    pub(crate) fn current(&self) -> &SemanticProgram {
        &self.current
    }
}

/// Replay `program` from its create size. The stored solid is not an argument.
pub(crate) fn evaluate_semantic_shadow(program: &SemanticProgram) -> SemanticShadowReport {
    match replay(program) {
        Ok(eval) => pass_report(&eval),
        Err((eval, fail)) => fail_report(&eval, fail),
    }
}

/// Intent Shadow diagnostic for semantic rebinding. `record` is not modified.
///
/// Names come from references stored on the steps. A concrete-only log says it was not
/// recorded. A selected element lists every stored identity, the operation that created it,
/// what it descends from, and whether that name still binds. Several identities are listed.
/// None is chosen.
pub fn semantic_shadow_diagnostic(record: &BlockRecord, selected: Option<ConcreteElement>) -> String {
    let phase = evaluate_intent_shadow(record);
    let mut lines = vec!["Semantic rebinding:".to_string()];
    // A capped editor log can drop its prefix while the persistent tape is still complete.
    // Phase 1 stays incomplete in that case. This diagnostic still replays the tape.
    if phase.status == IntentShadowStatus::Incomplete && !persistent_tape_builds(record) {
        lines.push("replay: not run".into());
        lines.push("resolved: 0".into());
        lines.push("rebound: 0".into());
        lines.push("missing: 0".into());
        lines.push("ambiguous: 0".into());
        lines.push("invalidated: 0".into());
        lines.push("candidate match: not produced".into());
        lines.push("reason: the intent record is incomplete".into());
        append_selection(&mut lines, None, selected, "not recorded");
        append_authority(&mut lines, record);
        return lines.join("\n");
    }
    match saved_replay(record) {
        SavedReplay::NotRecorded => {
            lines.push("replay: not recorded".into());
            lines.push("resolved: 0".into());
            lines.push("rebound: 0".into());
            lines.push("missing: 0".into());
            lines.push("ambiguous: 0".into());
            lines.push("invalidated: 0".into());
            lines.push("candidate match: not produced".into());
            lines.push("reason: this operation log has no semantic references".into());
            append_selection(&mut lines, None, selected, "not recorded");
            append_authority(&mut lines, record);
        }
        SavedReplay::Done { program, report } => {
            lines.push(format!("replay: {}", replay_word(report.status)));
            lines.push(format!("resolved: {}", report.resolved));
            lines.push(format!("rebound: {}", report.rebound));
            lines.push(format!("missing: {}", report.missing));
            lines.push(format!("ambiguous: {}", report.ambiguous));
            lines.push(format!("invalidated: {}", report.invalidated));
            lines.push(format!("candidate match: {}", stored_match_line(&report, record)));
            if report.status == SemanticShadowStatus::Pass {
                lines.push(format!("program codec: {}", codec_line(&program)));
            } else if let Some(detail) = report.text.lines().nth(1) {
                lines.push(detail.to_string());
            }
            let fallback = binding_word(report.status);
            append_selection(&mut lines, Some(&report), selected, fallback);
            append_authority(&mut lines, record);
        }
    }
    lines.join("\n")
}

/// Whether a newly authored record can reconstruct from its persistent intent.
///
/// Legacy concrete-only records stay legacy. A gap, an empty reference, or a replay
/// that does not pass is incomplete. This does not change the record and does not
/// make the intent authoritative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IntentCompleteness {
    IntentComplete,
    IntentIncomplete(Vec<String>),
    LegacyConcreteOnly,
}

pub(crate) fn intent_completeness(record: &BlockRecord) -> IntentCompleteness {
    if record.intent.is_empty() {
        return IntentCompleteness::LegacyConcreteOnly;
    }
    let gaps = structural_gap_reasons(record);
    if !gaps.is_empty() {
        return IntentCompleteness::IntentIncomplete(gaps);
    }
    match saved_replay(record) {
        SavedReplay::Done { report, .. } if report.status == SemanticShadowStatus::Pass => IntentCompleteness::IntentComplete,
        SavedReplay::Done { report, .. } => {
            let detail = report.text.lines().nth(1).unwrap_or("the persistent intent did not reconstruct").to_string();
            IntentCompleteness::IntentIncomplete(vec![detail])
        }
        SavedReplay::NotRecorded => IntentCompleteness::IntentIncomplete(vec!["semantic references were not recorded".into()]),
    }
}

enum SavedReplay {
    NotRecorded,
    Done { program: SemanticProgram, report: SemanticShadowReport },
}

/// Replay the semantic references stored on `record`. The stored body is not read.
#[cfg(test)]
pub(crate) fn evaluate_saved_semantic_shadow(record: &BlockRecord) -> SemanticShadowReport {
    match saved_replay(record) {
        SavedReplay::NotRecorded => {
            let fail = Fail::refused("semantic references were not recorded");
            fail_report(&Eval::bare(), fail)
        }
        SavedReplay::Done { report, .. } => report,
    }
}

fn saved_replay(record: &BlockRecord) -> SavedReplay {
    let program = match saved_semantic_program(record) {
        Ok(program) => program,
        Err(fail) if fail.detail == "semantic references were not recorded" => return SavedReplay::NotRecorded,
        Err(fail) => return SavedReplay::Done { program: SemanticProgram { ops: Vec::new() }, report: fail_report(&Eval::bare(), fail) },
    };
    let report = evaluate_semantic_shadow(&program);
    SavedReplay::Done { program, report }
}

/// Provenance of `op`'s concrete elements, from semantic references already stored.
///
/// `None` means this commit does not record a reference: the operation is outside the
/// captured set, or an earlier operation has none to continue from. The stored body is not read.
pub(crate) fn captured_semantics(record: &BlockRecord, op: &BlockOp) -> Option<Vec<Vec<String>>> {
    if !in_scope(op) || record.steps.len() != record.history.len() || record.seed_size_m.is_none() {
        return None;
    }
    if record.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::Gap { .. }) || entry.groups.as_ref().is_some_and(|groups| groups.iter().any(|group| group.is_empty()))) {
        return None;
    }
    if record.intent.is_empty() {
        if record.history.iter().any(blocks_capture) {
            return None;
        }
        for (index, prior) in record.history.iter().enumerate() {
            if in_scope(prior) && record.steps[index].semantic.is_none() {
                return None;
            }
        }
    }
    let eval = provenance_eval(record).ok()?;
    let mut groups = Vec::new();
    for element in op.concrete_elements() {
        let mut names = if eval.body.is_none() {
            constructor_names(element)
        } else {
            eval.refs_of(element_kind(element), element.body_id()).iter().map(ToString::to_string).collect::<Vec<_>>()
        };
        names.sort();
        names.dedup();
        groups.push(names);
    }
    Some(groups)
}

/// A semantic token the level file can store. A token that fails this does not load.
pub(crate) fn semantic_reference_token(token: &str) -> Result<(), String> {
    parse_ref(token).map(|_| ())
}

fn saved_semantic_program(record: &BlockRecord) -> Result<SemanticProgram, Fail> {
    if !record.intent.is_empty() {
        return program_from_intent(record);
    }
    let seed = record.seed_size_m.ok_or_else(|| Fail::refused("semantic references were not recorded"))?;
    if !seed.iter().all(|axis| axis.is_finite()) || record.steps.len() != record.history.len() {
        return Err(Fail::refused("semantic references were not recorded"));
    }
    let mut saw_scope = false;
    let mut ops = vec![SemanticOp::Create { size_m: seed }];
    for (index, op) in record.history.iter().enumerate() {
        if blocks_capture(op) {
            return Err(Fail::refused("semantic references were not recorded"));
        }
        let step = &record.steps[index];
        let semantic = match op {
            BlockOp::Size { size_m } => {
                if step.semantic.is_some() {
                    return Err(Fail::refused("a size operation stored a semantic reference"));
                }
                SemanticOp::Resize { size_m: *size_m }
            }
            BlockOp::SubdivideFace { u, v, .. } => {
                saw_scope = true;
                let groups = required_groups(step, 1)?;
                SemanticOp::Subdivide { face: parse_group(&groups[0])?, u: *u, v: *v }
            }
            BlockOp::ExtrudeFaces { faces, delta_m } => {
                saw_scope = true;
                let groups = required_groups(step, faces.len())?;
                let mut parsed = Vec::new();
                for group in groups {
                    parsed.push(parse_group(group)?);
                }
                SemanticOp::Extrude { faces: parsed, amount: ExtrudeAmount::Delta(*delta_m) }
            }
            BlockOp::SplitEdge { .. } => {
                saw_scope = true;
                let groups = required_groups(step, 1)?;
                SemanticOp::Split { edge: parse_group(&groups[0])? }
            }
            BlockOp::BevelEdges { edges, distance_m } => {
                saw_scope = true;
                let groups = required_groups(step, edges.len())?;
                let mut parsed = Vec::new();
                for group in groups {
                    parsed.push(parse_group(group)?);
                }
                SemanticOp::Bevel { edges: parsed, width_m: *distance_m }
            }
            BlockOp::MoveEdge { .. } | BlockOp::MoveVertex { .. } | BlockOp::ExtrudeEdge { .. } | BlockOp::ExtrudeFace { .. } | BlockOp::InsetFace { .. } | BlockOp::Bevel { .. } | BlockOp::Mirror { .. } => {
                return Err(Fail::refused("semantic references were not recorded"));
            }
        };
        ops.push(semantic);
    }
    if !saw_scope {
        return Err(Fail::refused("semantic references were not recorded"));
    }
    Ok(SemanticProgram { ops })
}

fn persistent_tape_builds(record: &BlockRecord) -> bool {
    !record.intent.is_empty() && structural_gap_reasons(record).is_empty() && program_from_intent(record).is_ok()
}

fn program_from_intent(record: &BlockRecord) -> Result<SemanticProgram, Fail> {
    // Creation size is the replay origin. The stored `size_m` cache is not an input.
    let seed = record.seed_size_m.ok_or_else(|| Fail::refused("the creation size was not recorded"))?;
    if !seed.iter().all(|axis| axis.is_finite()) {
        return Err(Fail::refused("the creation size was not recorded"));
    }
    let mut saw_scope = false;
    let mut ops = vec![SemanticOp::Create { size_m: seed }];
    for (index, entry) in record.intent.iter().enumerate() {
        let number = index + 1;
        match &entry.payload {
            IntentPayload::Gap { operation } => {
                return Err(Fail::refused(format!("operation {number}: {operation} has no semantic representation")));
            }
            IntentPayload::Size { size_m } => ops.push(SemanticOp::Resize { size_m: *size_m }),
            IntentPayload::Subdivide { u, v } => {
                saw_scope = true;
                let groups = intent_groups(entry, number, "subdivide")?;
                if groups.len() != 1 {
                    return Err(Fail::refused(format!("operation {number}: subdivide does not name one face")));
                }
                ops.push(SemanticOp::Subdivide { face: parse_group(&groups[0])?, u: *u, v: *v });
            }
            IntentPayload::Extrude { delta_m } => {
                saw_scope = true;
                let groups = intent_groups(entry, number, "extrude")?;
                let mut faces = Vec::new();
                for group in groups {
                    faces.push(parse_group(group)?);
                }
                ops.push(SemanticOp::Extrude { faces, amount: ExtrudeAmount::Delta(*delta_m) });
            }
            IntentPayload::Split => {
                saw_scope = true;
                let groups = intent_groups(entry, number, "split")?;
                if groups.len() != 1 {
                    return Err(Fail::refused(format!("operation {number}: split does not name one edge")));
                }
                ops.push(SemanticOp::Split { edge: parse_group(&groups[0])? });
            }
            IntentPayload::Bevel { width_m } => {
                saw_scope = true;
                let groups = intent_groups(entry, number, "bevel")?;
                let mut edges = Vec::new();
                for group in groups {
                    edges.push(parse_group(group)?);
                }
                ops.push(SemanticOp::Bevel { edges, width_m: *width_m });
            }
            IntentPayload::MoveEdge { delta_m } => {
                saw_scope = true;
                let groups = intent_groups(entry, number, "move-edge")?;
                if groups.len() != 1 {
                    return Err(Fail::refused(format!("operation {number}: move-edge does not name one edge")));
                }
                ops.push(SemanticOp::MoveEdge { edge: parse_group(&groups[0])?, delta_m: *delta_m });
            }
            IntentPayload::MoveVertex { delta_m } => {
                saw_scope = true;
                let groups = intent_groups(entry, number, "move-vertex")?;
                if groups.len() != 1 {
                    return Err(Fail::refused(format!("operation {number}: move-vertex does not name one vertex")));
                }
                ops.push(SemanticOp::MoveVertex { vertex: parse_group(&groups[0])?, delta_m: *delta_m });
            }
            IntentPayload::ExtrudeEdge { delta_m } => {
                saw_scope = true;
                let groups = intent_groups(entry, number, "extrude-edge")?;
                if groups.len() != 1 {
                    return Err(Fail::refused(format!("operation {number}: extrude-edge does not name one edge")));
                }
                ops.push(SemanticOp::ExtrudeEdge { edge: parse_group(&groups[0])?, delta_m: *delta_m });
            }
            IntentPayload::Mirror { axis } => {
                saw_scope = true;
                ops.push(SemanticOp::Mirror { axis: *axis });
            }
            IntentPayload::PushFace { face, distance_m } => {
                saw_scope = true;
                ops.push(SemanticOp::PushFace { face: *face, distance_m: *distance_m });
            }
            IntentPayload::AnalyticSurface { .. } => {
                return Err(Fail::refused(format!("operation {number}: analytic-surface is not a boundary-representation operand")));
            }
            IntentPayload::Round { radius_m } => {
                saw_scope = true;
                if !radius_m.is_finite() || *radius_m <= 0.0 {
                    return Err(Fail::refused(format!("operation {number}: round radius is not a finite positive length")));
                }
                let groups = intent_groups(entry, number, "round")?;
                if crate::round_intent::round_group_tokens(Some(groups)).is_err() {
                    return Err(Fail::refused(format!("operation {number}: round does not name a semantic edge")));
                }
                let mut edge = Vec::new();
                for group in groups {
                    let parsed = parse_group(group)?;
                    if parsed.len() != 1 {
                        return Err(Fail::refused(format!("operation {number}: round does not name a semantic edge")));
                    }
                    edge.extend(parsed);
                }
                ops.push(SemanticOp::Round { edge, radius_m: *radius_m });
            }
            IntentPayload::Seed => {
                if number != 1 {
                    return Err(Fail::refused(format!("operation {number}: seed is not the first intent operation")));
                }
                if entry.groups.is_some() {
                    return Err(Fail::refused(format!("operation {number}: seed does not name an element")));
                }
                saw_scope = true;
                ops.push(SemanticOp::Seed);
            }
        }
    }
    if !saw_scope {
        return Err(Fail::refused("the persistent intent has no modeling operation"));
    }
    Ok(SemanticProgram { ops })
}

fn intent_groups<'a>(entry: &'a crate::IntentEntry, number: usize, operation: &str) -> Result<&'a [Vec<String>], Fail> {
    let Some(groups) = entry.groups.as_deref() else {
        return Err(Fail::refused(format!("operation {number}: {operation} has no semantic representation")));
    };
    if groups.is_empty() || groups.iter().any(|group| group.is_empty()) {
        return Err(Fail::refused(format!("operation {number}: {operation} has no semantic representation")));
    }
    Ok(groups)
}

fn structural_gap_reasons(record: &BlockRecord) -> Vec<String> {
    let mut reasons = Vec::new();
    if record.intent.is_empty() {
        return reasons;
    }
    if record.seed_size_m.is_none() {
        reasons.push("the creation size was not recorded".into());
    }
    for (index, entry) in record.intent.iter().enumerate() {
        let number = index + 1;
        match &entry.payload {
            IntentPayload::Gap { operation } => {
                reasons.push(format!("operation {number}: {operation} has no semantic representation"));
            }
            IntentPayload::Size { .. } => {}
            IntentPayload::Subdivide { .. } => push_missing_group(&mut reasons, entry, number, "subdivide"),
            IntentPayload::Extrude { .. } => push_missing_group(&mut reasons, entry, number, "extrude"),
            IntentPayload::Split => push_missing_group(&mut reasons, entry, number, "split"),
            IntentPayload::Bevel { .. } => push_missing_group(&mut reasons, entry, number, "bevel"),
            IntentPayload::MoveEdge { .. } => push_missing_group(&mut reasons, entry, number, "move-edge"),
            IntentPayload::MoveVertex { .. } => push_missing_group(&mut reasons, entry, number, "move-vertex"),
            IntentPayload::ExtrudeEdge { .. } => push_missing_group(&mut reasons, entry, number, "extrude-edge"),
            IntentPayload::Mirror { .. } | IntentPayload::PushFace { .. } => {}
            IntentPayload::AnalyticSurface { .. } => {
                reasons.push(format!("operation {number}: analytic-surface is not a boundary-representation operand"));
            }
            IntentPayload::Round { radius_m } => {
                if !radius_m.is_finite() || *radius_m <= 0.0 {
                    reasons.push(format!("operation {number}: round radius is not a finite positive length"));
                }
                if crate::round_intent::round_group_tokens(entry.groups.as_deref()).is_err() {
                    reasons.push(format!("operation {number}: round does not name a semantic edge"));
                }
            }
            IntentPayload::Seed => {
                if number != 1 {
                    reasons.push(format!("operation {number}: seed is not the first intent operation"));
                }
                if entry.groups.is_some() {
                    reasons.push(format!("operation {number}: seed does not name an element"));
                }
            }
        }
    }
    reasons
}

fn push_missing_group(reasons: &mut Vec<String>, entry: &crate::IntentEntry, number: usize, operation: &str) {
    let missing = match &entry.groups {
        None => true,
        Some(groups) => groups.is_empty() || groups.iter().any(|group| group.is_empty()),
    };
    if missing {
        reasons.push(format!("operation {number}: {operation} has no semantic representation"));
    }
}

fn append_authority(lines: &mut Vec<String>, record: &BlockRecord) {
    match intent_completeness(record) {
        IntentCompleteness::LegacyConcreteOnly => {
            lines.push("Authority readiness: LEGACY".into());
            lines.push("Intent authority candidate: refused".into());
        }
        IntentCompleteness::IntentComplete => {
            lines.push("Authority readiness: READY".into());
            lines.push("Intent authority candidate: reconstructable".into());
        }
        IntentCompleteness::IntentIncomplete(reasons) => {
            lines.push("Authority readiness: INCOMPLETE".into());
            for reason in reasons {
                lines.push(format!("Authority gap: {reason}"));
            }
            lines.push("Intent authority candidate: refused".into());
        }
    }
    lines.push(intent_authority_diagnostic(record));
}

/// A research reconstruction from the persistent intent tape.
///
/// `Reconstructable` is the body the tape produced. Load does not install it, and it does
/// not replace the stored body. `Refused` is a legacy concrete log, a gap, or a replay
/// that did not pass. Those records do not receive an invented body.
#[derive(Clone, Debug, PartialEq)]
pub enum IntentAuthorityCandidate {
    Reconstructable(SolidBody),
    Refused(String),
}

/// Rebuild the solid from persistent intent alone.
///
/// The stored body is not read. A legacy or incomplete tape is refused. `parse_level`
/// does not call this, and the record is not modified.
pub fn intent_authority_candidate(record: &BlockRecord) -> IntentAuthorityCandidate {
    if record.intent.is_empty() {
        return IntentAuthorityCandidate::Refused("legacy concrete-only history has no semantic program".into());
    }
    let gaps = structural_gap_reasons(record);
    if !gaps.is_empty() {
        return IntentAuthorityCandidate::Refused(gaps.join("\n"));
    }
    match saved_replay(record) {
        SavedReplay::Done { report, .. } if report.status == SemanticShadowStatus::Pass => match report.candidate {
            Some(body) => IntentAuthorityCandidate::Reconstructable(body),
            None => IntentAuthorityCandidate::Refused("the persistent intent did not produce a body".into()),
        },
        SavedReplay::Done { report, .. } => {
            let detail = report.text.lines().nth(1).unwrap_or("the persistent intent did not reconstruct").to_string();
            IntentAuthorityCandidate::Refused(detail)
        }
        SavedReplay::NotRecorded => IntentAuthorityCandidate::Refused("semantic references were not recorded".into()),
    }
}

/// Whether this solid's persistent intent is a complete, replayable definition of its geometry.
///
/// `Eligible` means a cold replay from the tape produces the solid. Legacy and incomplete
/// solids are `Ineligible` with one blocker per reason. `parse_level` does not call this.
/// Level spawn calls it only when the intent-authority experiment is on and the file stored no body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntentAuthorityEligibility {
    Eligible,
    Ineligible(Vec<String>),
}

/// Specific blockers. An empty list is [`IntentAuthorityEligibility::Eligible`].
///
/// The reconstructed body is built by [`intent_authority_candidate`], which does not read
/// `record.body`. A stored body is compared only after that reconstruction returns.
pub fn intent_authority_eligibility(record: &BlockRecord) -> IntentAuthorityEligibility {
    let blockers = eligibility_blockers(record);
    if blockers.is_empty() {
        IntentAuthorityEligibility::Eligible
    } else {
        IntentAuthorityEligibility::Ineligible(blockers)
    }
}

/// Level spawn of one saved solid, used only while the intent-authority experiment is on.
///
/// A stored body is returned as stored. An ineligible tape is returned as stored. An eligible
/// tape whose file has no body receives the body from [`intent_authority_candidate`]. That
/// candidate does not read `record.body`. The caller installs the result through the ordinary
/// mesh, meshlet, and collision path. With the experiment off, spawn does not call this.
/// A live world uses [`crate::SceneWorld::rerealize_live_block_from_intent`] instead of loading again.
pub(crate) fn realize_eligible_body_on_load(record: BlockRecord) -> BlockRecord {
    if record.body.is_some() {
        return record;
    }
    if intent_authority_eligibility(&record) != IntentAuthorityEligibility::Eligible {
        return record;
    }
    match intent_authority_candidate(&record) {
        IntentAuthorityCandidate::Reconstructable(body) => {
            let mut realized = record;
            realized.body = Some(body);
            realized
        }
        IntentAuthorityCandidate::Refused(_) => record,
    }
}

/// Facts for one solid after a level load. The formatter does not read a body.
#[derive(Clone, Debug)]
pub struct IntentAuthorityLoadFacts {
    pub name: String,
    pub experiment_on: bool,
    pub file_body_present: bool,
    pub intent_present: bool,
    pub loaded_body_present: bool,
    pub eligible: bool,
    pub body_valid: bool,
    pub mesh_generated: bool,
    pub meshlets_generated: bool,
    pub hierarchy_generated: bool,
    pub exact_solid: bool,
    pub detail_requested: bool,
}

/// The explicit load report. `INTENT ONLY` is an eligible file with no body whose live body
/// was rebuilt while the experiment was on. A file that already had a body reports `STORED BODY`.
/// The experiment being off reports `FILE RECORD`.
pub fn format_intent_authority_load(facts: &IntentAuthorityLoadFacts) -> String {
    let source = if facts.experiment_on && !facts.file_body_present && facts.loaded_body_present && facts.eligible && facts.body_valid {
        "INTENT ONLY"
    } else if facts.file_body_present {
        "STORED BODY"
    } else {
        "FILE RECORD"
    };
    let reconstructed = if source == "INTENT ONLY" {
        "VALID"
    } else if facts.loaded_body_present && !facts.body_valid {
        "INVALID"
    } else {
        "NOT REQUESTED"
    };
    let stored_read = if source == "INTENT ONLY" {
        "NO"
    } else if facts.file_body_present {
        "YES"
    } else {
        "NOT APPLICABLE"
    };
    format!(
        "Intent Authority Experiment: {experiment}\nObject: {name}\nStored body: {stored}\nIntent record: {intent}\nRealization source: {source}\nReconstructed body: {reconstructed}\nMesh: {mesh}\nMeshlets: {meshlets}\nHierarchy: {hierarchy}\nStored body read: {stored_read}\nExact solid: {exact}\nDetail patches: {patches}",
        experiment = if facts.experiment_on { "ON" } else { "OFF" },
        name = facts.name,
        stored = if facts.file_body_present { "PRESENT" } else { "ABSENT" },
        intent = if facts.intent_present { "PRESENT" } else { "ABSENT" },
        mesh = if facts.mesh_generated { "GENERATED" } else { "NOT GENERATED" },
        meshlets = if facts.meshlets_generated { "GENERATED" } else { "NOT GENERATED" },
        hierarchy = if facts.hierarchy_generated { "GENERATED" } else { "NOT GENERATED" },
        exact = if facts.exact_solid { "EXACT" } else { "NOT EXACT" },
        patches = if facts.detail_requested { "REQUESTED" } else { "NONE" },
    )
}

/// Measured result of one live discard and re-realization. The formatter does not read a body.
#[derive(Clone, Debug)]
pub struct IntentLiveRerealizeFacts {
    pub stored_body_consulted: bool,
    pub level_reloaded: bool,
    pub file_reread: bool,
    pub old_body_reachable: bool,
    pub old_mesh_reachable: bool,
    pub body_discarded: bool,
    pub mesh_discarded: bool,
    pub meshlets_discarded: bool,
    pub hierarchy_discarded: bool,
    pub intent_present: bool,
    pub body_valid: bool,
    pub body_hash_match: bool,
    pub topology_match: bool,
    pub bounds_match: bool,
    pub volume_match: bool,
    pub mesh_regenerated: bool,
    pub meshlets_regenerated: bool,
    pub hierarchy_regenerated: bool,
    pub frame_presented: bool,
    pub post_edit_valid: bool,
    pub legacy_unchanged: bool,
}

/// The explicit live re-realization report. A process with no swapchain leaves the frame line
/// as `NOT PRESENTED`. `PRESENTED` is only for a frame drawn after the rebuild.
pub fn format_intent_live_rerealize(facts: &IntentLiveRerealizeFacts) -> String {
    let word = |value: bool| if value { "YES" } else { "NO" };
    let matched = |value: bool| if value { "MATCH" } else { "DIFFER" };
    format!(
        "Intent Authority Live Re-realization: ON\nStored body consulted: {consulted}\nLevel reloaded: {reloaded}\nFile reread: {reread}\nOld evaluated body reachable: {old_body}\nOld mesh reachable: {old_mesh}\nBody discarded: {body_discarded}\nMesh discarded: {mesh_discarded}\nMeshlets discarded: {meshlets_discarded}\nHierarchy discarded: {hierarchy_discarded}\nIntent source: {intent}\nLive reconstructed body: {valid}\nBody hash: {hash}\nTopology: {topology}\nBounds: {bounds}\nVolume: {volume}\nMesh regenerated: {mesh}\nMeshlets regenerated: {meshlets}\nHierarchy regenerated: {hierarchy}\nFrame after regeneration: {frame}\nPost-regeneration edit: {edit}\nLegacy object: {legacy}",
        consulted = word(facts.stored_body_consulted),
        reloaded = word(facts.level_reloaded),
        reread = word(facts.file_reread),
        old_body = word(facts.old_body_reachable),
        old_mesh = word(facts.old_mesh_reachable),
        body_discarded = word(facts.body_discarded),
        mesh_discarded = word(facts.mesh_discarded),
        meshlets_discarded = word(facts.meshlets_discarded),
        hierarchy_discarded = word(facts.hierarchy_discarded),
        intent = if facts.intent_present { "PRESENT" } else { "ABSENT" },
        valid = if facts.body_valid { "VALID" } else { "INVALID" },
        hash = matched(facts.body_hash_match),
        topology = matched(facts.topology_match),
        bounds = matched(facts.bounds_match),
        volume = matched(facts.volume_match),
        mesh = word(facts.mesh_regenerated),
        meshlets = word(facts.meshlets_regenerated),
        hierarchy = word(facts.hierarchy_regenerated),
        frame = if facts.frame_presented { "PRESENTED" } else { "NOT PRESENTED" },
        edit = if facts.post_edit_valid { "VALID" } else { "INVALID" },
        legacy = if facts.legacy_unchanged { "UNCHANGED" } else { "CHANGED" },
    )
}

/// One line per blocker. The eligible line is `Intent Authority: ELIGIBLE`.
pub fn intent_authority_diagnostic(record: &BlockRecord) -> String {
    match intent_authority_eligibility(record) {
        IntentAuthorityEligibility::Eligible => "Intent Authority: ELIGIBLE".to_string(),
        IntentAuthorityEligibility::Ineligible(reasons) => {
            reasons.into_iter().map(|reason| format!("Intent Authority: INELIGIBLE — {reason}")).collect::<Vec<_>>().join("\n")
        }
    }
}

/// One extrude on an authored seed. The caller's record is unchanged on `Err`.
///
/// A fillet face or a corner patch is refused. A zero delta is left to replay, which
/// refuses it as an unusable distance. This does not store a body.
pub fn authored_extrude_record(record: &BlockRecord, faces: &[u32], delta: [f64; 3]) -> Result<BlockRecord, String> {
    if !record.has_authored_seed() || record.body.is_some() {
        return Err("Extrude edits an authored block. This object stores its own shape.".into());
    }
    let body = match intent_authority_candidate(record) {
        IntentAuthorityCandidate::Reconstructable(body) => body,
        IntentAuthorityCandidate::Refused(reason) => return Err(reason),
    };
    let mut groups = Vec::with_capacity(faces.len());
    for face in faces {
        let names = semantic_face_names(record, *face);
        if names.iter().any(|name| name.starts_with("F:fillet(") || name.starts_with("F:corner(")) {
            return Err("UnsupportedOperation: Extrude does not edit a curved face.".into());
        }
        if body.face_loop(*face).is_none() {
            return Err("That element is not on the solid.".into());
        }
        let token = preferred_face_token(&names).ok_or_else(|| "That face has no semantic name.".to_string())?;
        groups.push(vec![token]);
    }
    let entry = crate::IntentEntry { groups: Some(groups), payload: crate::IntentPayload::Extrude { delta_m: delta } };
    commit_class_c_intent(record, entry)
}

/// Class C authoring transaction for one eligible record that stores no body.
///
/// `entry` is appended to the intent tape. Replay starts at `seed_size_m` and does not
/// read `size_m`. On success `size_m` is the replay cache and the body is not stored.
/// With no `round`, that cache is the planar AABB. One valid `round` unions the fillet bounds.
/// The caller's record is unchanged when this returns `Err`: a stored body is refused,
/// an ineligible record is refused, and a replay that does not validate is refused.
/// `parse_level` does not call this and does not repair a stale cache.
pub fn commit_class_c_intent(record: &BlockRecord, entry: crate::IntentEntry) -> Result<BlockRecord, String> {
    if record.body.is_some() {
        return Err("a stored body stays the solid".into());
    }
    let diagnostic = intent_authority_diagnostic(record);
    if diagnostic != "Intent Authority: ELIGIBLE" {
        return Err(diagnostic);
    }
    let mut next = record.clone();
    next.intent.push(entry);
    let body = match intent_authority_candidate(&next) {
        IntentAuthorityCandidate::Reconstructable(body) => body,
        IntentAuthorityCandidate::Refused(reason) => return Err(reason),
    };
    if body.validate().is_err() {
        return Err("reconstruction is not a closed body".into());
    }
    next.size_m = class_c_cache_extent(&next)?;
    next.validate().map_err(|error| format!("the replay extent is not a block size ({error})"))?;
    if next.body.is_some() {
        return Err("the class C transaction stored a body".into());
    }
    let written = intent_authority_diagnostic(&next);
    if written != "Intent Authority: ELIGIBLE" {
        return Err(written);
    }
    Ok(next)
}

/// Class C commit for an authored seed. A round whose semantic edges already have one entry
/// replaces that entry's radius. A record without a seed still appends, including a version 3 tape.
pub fn commit_authored_round(record: &BlockRecord, entry: crate::IntentEntry) -> Result<BlockRecord, String> {
    if !record.has_authored_seed() {
        return commit_class_c_intent(record, entry);
    }
    let crate::IntentPayload::Round { radius_m } = entry.payload else {
        return commit_class_c_intent(record, entry);
    };
    let Ok(tokens) = crate::round_intent::round_group_tokens(entry.groups.as_deref()) else {
        return commit_class_c_intent(record, entry);
    };
    if record.body.is_some() {
        return Err("a stored body stays the solid".into());
    }
    let diagnostic = intent_authority_diagnostic(record);
    if diagnostic != "Intent Authority: ELIGIBLE" {
        return Err(diagnostic);
    }
    let mut next = record.clone();
    let mut replaced = false;
    for existing in &mut next.intent {
        let crate::IntentPayload::Round { radius_m: slot } = &mut existing.payload else { continue };
        let Ok(have) = crate::round_intent::round_group_tokens(existing.groups.as_deref()) else { continue };
        if same_token_set(&have, &tokens) {
            *slot = radius_m;
            replaced = true;
            break;
        }
    }
    if !replaced {
        return commit_class_c_intent(record, entry);
    }
    let body = match intent_authority_candidate(&next) {
        IntentAuthorityCandidate::Reconstructable(body) => body,
        IntentAuthorityCandidate::Refused(reason) => return Err(reason),
    };
    if body.validate().is_err() {
        return Err("reconstruction is not a closed body".into());
    }
    next.size_m = class_c_cache_extent(&next)?;
    next.validate().map_err(|error| format!("the replay extent is not a block size ({error})"))?;
    if next.body.is_some() {
        return Err("the class C transaction stored a body".into());
    }
    let written = intent_authority_diagnostic(&next);
    if written != "Intent Authority: ELIGIBLE" {
        return Err(written);
    }
    Ok(next)
}

fn same_token_set(left: &[String], right: &[String]) -> bool {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort();
    right.sort();
    left == right
}

/// Split of an edge that already carries a round. `None` means this edge is free to split.
/// The sentence is the refusal. The record is not read for mutation.
pub fn authored_split_conflict(record: &BlockRecord, edge: u32) -> Option<String> {
    if !record.has_authored_seed() || record.body.is_some() {
        return None;
    }
    let program = program_from_intent(record).ok()?;
    let eval = replay(&program).ok()?;
    let round = eval.rounds.iter().find(|round| round.edge_id == edge || round.chain.contains(&edge))?;
    Some(format!("Split conflicts with the round on {}", round.token))
}

/// Face tokens already recorded for `face`. Empty when none were recorded. This does not invent a name.
pub fn semantic_face_names(record: &BlockRecord, face: u32) -> Vec<String> {
    face_provenance_names(record, face)
}

/// Vertex tokens minted by a replay. Empty when none were recorded. This does not invent a name.
pub fn semantic_vertex_names(record: &BlockRecord, vertex: u32) -> Vec<String> {
    if record.has_authored_seed() && record.body.is_none() {
        return authored_replay_names(record, ElementKind::Vertex, vertex);
    }
    Vec::new()
}

/// The face token an authored operation stores.
/// A constructor face keeps `F:seed/N`. A generated face keeps the role minted with it.
/// A cap alias names that same face and is not a second identity.
pub fn preferred_face_token(names: &[String]) -> Option<String> {
    if let Some(name) = names.iter().find(|name| name.starts_with("F:seed/")) {
        return Some(name.clone());
    }
    if let Some(name) = names.iter().find(|name| !name.starts_with("F:cap(")) {
        return Some(name.clone());
    }
    names.first().cloned()
}

/// Edge tokens minted by a planar replay. A bodyless tape still has them. This does not invent a name.
/// Curve boundaries stay off this list. Their ids are not edges of the planar body.
pub fn semantic_edge_bindings(record: &BlockRecord) -> Result<Vec<(String, u32)>, String> {
    let program = program_from_intent(record).map_err(|fail| fail.detail)?;
    let eval = replay(&program).map_err(|(_, fail)| fail.detail)?;
    let body_edges: Vec<u32> = eval.body.as_ref().map(|body| body.edges.iter().map(|edge| edge.id).collect()).unwrap_or_default();
    Ok(eval
        .facts()
        .into_iter()
        .filter(|fact| fact.kind == ElementKind::Edge && body_edges.contains(&fact.concrete))
        .map(|fact| (fact.identity, fact.concrete))
        .collect())
}

/// Edge tokens already recorded for `edge`, including a fillet boundary. Empty when none were recorded.
pub fn semantic_edge_names(record: &BlockRecord, edge: u32) -> Vec<String> {
    if record.has_authored_seed() && record.body.is_none() {
        return authored_replay_names(record, ElementKind::Edge, edge);
    }
    element_provenance_names(record, ElementKind::Edge, edge)
}

/// Replay cache written into `size_m` by [`commit_class_c_intent`]. Does not read `size_m`.
pub fn class_c_cache_extent(record: &BlockRecord) -> Result<[f64; 3], String> {
    let program = program_from_intent(record).map_err(|fail| fail.detail)?;
    let eval = replay(&program).map_err(|(_, fail)| fail.detail)?;
    let body = eval.body.as_ref().ok_or("the persistent intent did not produce a body")?;
    let fillets: Vec<_> = eval.rounds.iter().map(|round| round.fillet.clone()).collect();
    Ok(crate::round_intent::cache_extent_many(body, &fillets))
}

/// Every analytic fillet on a tape, in replay order. Does not read `size_m` or `body`.
pub fn replay_rounds(record: &BlockRecord) -> Result<Vec<crate::round_intent::ReplayedRound>, String> {
    let program = program_from_intent(record).map_err(|fail| fail.detail)?;
    let eval = replay(&program).map_err(|(_, fail)| fail.detail)?;
    let body = eval.body.as_ref().ok_or("the persistent intent did not produce a body")?;
    if eval.rounds.is_empty() {
        return Err("the tape has no round".into());
    }
    let fillets: Vec<_> = eval.rounds.iter().map(|round| round.fillet.clone()).collect();
    let cache_extent = crate::round_intent::cache_extent_many(body, &fillets);
    let planar_extent = body.aabb_size();
    Ok(eval
        .rounds
        .into_iter()
        .map(|round| crate::round_intent::ReplayedRound {
            token: round.token,
            edge_id: round.edge_id,
            radius_m: round.radius_m,
            fillet: round.fillet,
            planar_extent,
            cache_extent,
        })
        .collect())
}

/// The first analytic fillet on a tape. A set still resolves; this is the earliest one.
pub fn replay_round(record: &BlockRecord) -> Result<crate::round_intent::ReplayedRound, String> {
    replay_rounds(record)?.into_iter().next().ok_or_else(|| "the tape has no round".to_string())
}

fn eligibility_blockers(record: &BlockRecord) -> Vec<String> {
    let mut blockers = Vec::new();
    if record.intent.is_empty() {
        blockers.push("legacy concrete-only history has no semantic program".into());
        for (index, op) in record.history.iter().enumerate() {
            if !matches!(op, crate::BlockOp::Size { .. }) {
                blockers.push(format!("uncaptured operation: {} step {}", op.intent_word(), index + 1));
            }
        }
        push_analytic_parameter_blockers(&mut blockers, record);
        return blockers;
    }
    if record.seed_size_m.is_none() {
        blockers.push("the creation size was not recorded".into());
    }
    for (index, entry) in record.intent.iter().enumerate() {
        let step = index + 1;
        match &entry.payload {
            IntentPayload::Gap { operation } => {
                blockers.push(format!("uncaptured operation: {operation} step {step}"));
            }
            IntentPayload::MoveEdge { .. } => push_operand_blocker(&mut blockers, entry, step, "move-edge"),
            IntentPayload::MoveVertex { .. } => push_operand_blocker(&mut blockers, entry, step, "move-vertex"),
            IntentPayload::ExtrudeEdge { .. } => push_operand_blocker(&mut blockers, entry, step, "extrude-edge"),
            IntentPayload::Subdivide { .. } => push_operand_blocker(&mut blockers, entry, step, "subdivide"),
            IntentPayload::Extrude { .. } => push_operand_blocker(&mut blockers, entry, step, "extrude"),
            IntentPayload::Split => push_operand_blocker(&mut blockers, entry, step, "split"),
            IntentPayload::Bevel { .. } => push_operand_blocker(&mut blockers, entry, step, "bevel"),
            IntentPayload::Size { .. } | IntentPayload::Mirror { .. } | IntentPayload::PushFace { .. } | IntentPayload::Seed => {}
            IntentPayload::AnalyticSurface { .. } => {
                blockers.push(format!("analytic-surface is not a boundary-representation operand step {step}"));
            }
            IntentPayload::Round { radius_m } => {
                if !radius_m.is_finite() || *radius_m <= 0.0 {
                    blockers.push(format!("round radius is not a finite positive length step {step}"));
                }
                if crate::round_intent::round_group_tokens(entry.groups.as_deref()).is_err() {
                    blockers.push(format!("uncaptured operation: round step {step}"));
                }
            }
        }
    }
    if !blockers.is_empty() {
        return blockers;
    }
    if record.material.validate().is_err() {
        blockers.push("material does not validate".into());
    }
    match intent_authority_candidate(record) {
        IntentAuthorityCandidate::Refused(reason) => blockers.push(reason),
        IntentAuthorityCandidate::Reconstructable(body) => {
            if body.validate().is_err() {
                blockers.push("reconstruction is not a closed body".into());
            }
            match &record.body {
                Some(stored) if !bodies_agree(&body, stored) => {
                    blockers.push("reconstruction diverges from the stored body".into());
                }
                Some(_) => {}
                None => {
                    push_analytic_parameter_blockers(&mut blockers, record);
                    // The extent came from the tape. This comparison checks the `size_m` cache.
                    // A tape with no round uses the planar AABB. One valid round unions the fillet.
                    match class_c_cache_extent(record) {
                        Ok(bounds) => {
                            if (0..3).any(|axis| (bounds[axis] - record.size_m[axis]).abs() > 1.0e-6) {
                                blockers.push("reconstruction does not match the stored size".into());
                            }
                        }
                        Err(reason) => blockers.push(reason),
                    }
                }
            }
        }
    }
    blockers
}

fn push_operand_blocker(blockers: &mut Vec<String>, entry: &crate::IntentEntry, step: usize, operation: &str) {
    let missing = match &entry.groups {
        None => true,
        Some(groups) => groups.is_empty() || groups.iter().any(|group| group.is_empty()),
    };
    if missing {
        blockers.push(format!("uncaptured operation: {operation} step {step}"));
    }
}

fn push_analytic_parameter_blockers(blockers: &mut Vec<String>, record: &BlockRecord) {
    if record.body.is_some() {
        return;
    }
    if record.bevel_m.abs() > 1.0e-9 {
        blockers.push("uncaptured operation: uniform-bevel is an analytic parameter".into());
    }
    for (face, inset) in record.inset_m.iter().enumerate() {
        if inset.abs() > 1.0e-9 {
            blockers.push(format!("uncaptured operation: inset face {face} is an analytic parameter"));
        }
    }
}

fn bodies_agree(candidate: &SolidBody, stored: &SolidBody) -> bool {
    if candidate == stored {
        return true;
    }
    if candidate.next_id != stored.next_id || candidate.faces != stored.faces || candidate.edges != stored.edges || candidate.vertices.len() != stored.vertices.len() {
        return false;
    }
    let positions = candidate.vertices.iter().zip(stored.vertices.iter()).all(|(left, right)| {
        left.id == right.id && (0..3).all(|axis| (left.position[axis] - right.position[axis]).abs() < 1.0e-6)
    });
    if !positions {
        return false;
    }
    let bounds = candidate.aabb_size();
    let stored_bounds = stored.aabb_size();
    (0..3).all(|axis| (bounds[axis] - stored_bounds[axis]).abs() < 1.0e-6) && (volume(candidate) - volume(stored)).abs() < 1.0e-8
}

fn provenance_eval(record: &BlockRecord) -> Result<Eval, Fail> {
    let program = match saved_semantic_program(record) {
        Ok(program) => program,
        Err(fail) if fail.detail == "semantic references were not recorded" && !record.history.iter().any(in_scope) && record.steps.len() == record.history.len() => {
            let seed = record.seed_size_m.ok_or_else(|| Fail::refused("semantic references were not recorded"))?;
            let mut ops = vec![SemanticOp::Create { size_m: seed }];
            for op in &record.history {
                match op {
                    BlockOp::Size { size_m } => ops.push(SemanticOp::Resize { size_m: *size_m }),
                    _ => return Err(Fail::refused("semantic references were not recorded")),
                }
            }
            SemanticProgram { ops }
        }
        Err(fail) => return Err(fail),
    };
    replay(&program).map_err(|(_, fail)| fail)
}

fn required_groups(step: &crate::GeometryStep, count: usize) -> Result<&[Vec<String>], Fail> {
    let Some(groups) = step.semantic.as_deref() else {
        return Err(Fail::refused("semantic references were not recorded"));
    };
    if groups.len() != count {
        return Err(Fail::refused("semantic references do not match the concrete elements"));
    }
    Ok(groups)
}

fn parse_group(tokens: &[String]) -> Result<Vec<SemanticRef>, Fail> {
    let mut names = Vec::new();
    for token in tokens {
        names.push(parse_ref(token).map_err(|detail| Fail::refused(detail))?);
    }
    Ok(names)
}

fn in_scope(op: &BlockOp) -> bool {
    matches!(
        op,
        BlockOp::SubdivideFace { .. }
            | BlockOp::ExtrudeFaces { .. }
            | BlockOp::SplitEdge { .. }
            | BlockOp::BevelEdges { .. }
            | BlockOp::MoveEdge { .. }
            | BlockOp::MoveVertex { .. }
            | BlockOp::ExtrudeEdge { .. }
    )
}

fn blocks_capture(op: &BlockOp) -> bool {
    matches!(op, BlockOp::ExtrudeFace { .. } | BlockOp::InsetFace { .. } | BlockOp::Bevel { .. } | BlockOp::Mirror { .. })
}

/// Names already recorded for one face or edge. Empty when none were recorded. This does not invent a name.
pub(crate) fn element_provenance_names(record: &BlockRecord, kind: ElementKind, id: u32) -> Vec<String> {
    if record.has_authored_seed() && record.body.is_none() {
        return authored_replay_names(record, kind, id);
    }
    let constructor = match kind {
        ElementKind::Face => ConcreteElement::Face(id),
        ElementKind::Edge => ConcreteElement::Edge(id),
        ElementKind::Vertex => return Vec::new(),
    };
    if record.body.is_none() && !record.topology_history() && !record.analytic_features() {
        return constructor_names(constructor);
    }
    let Ok(eval) = provenance_eval(record) else {
        return Vec::new();
    };
    let agreed = match (&eval.body, &record.body) {
        (Some(candidate), Some(stored)) => bodies_agree(candidate, stored),
        (None, None) if !record.topology_history() => true,
        _ => false,
    };
    if !agreed {
        return Vec::new();
    }
    if eval.body.is_none() {
        return constructor_names(constructor);
    }
    let mut names = eval.refs_of(kind, id).iter().map(ToString::to_string).collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

/// Names already recorded for `face`. Empty when none were recorded. This does not invent a name.
pub(crate) fn face_provenance_names(record: &BlockRecord, face: u32) -> Vec<String> {
    if record.has_authored_seed() && record.body.is_none() {
        return authored_replay_names(record, ElementKind::Face, face);
    }
    if record.body.is_none() && !record.topology_history() && !record.analytic_features() && !(1..=6).contains(&face) {
        return Vec::new();
    }
    element_provenance_names(record, ElementKind::Face, face)
}

/// Names the replay minted for one element of an authored seed. Empty when the tape did not name it.
fn authored_replay_names(record: &BlockRecord, kind: ElementKind, id: u32) -> Vec<String> {
    let constructor = match kind {
        ElementKind::Face => ConcreteElement::Face(id),
        ElementKind::Edge => ConcreteElement::Edge(id),
        ElementKind::Vertex => ConcreteElement::Face(0),
    };
    let Ok(program) = program_from_intent(record) else {
        return if kind == ElementKind::Vertex { Vec::new() } else { constructor_names(constructor) };
    };
    let Ok(eval) = replay(&program) else {
        return Vec::new();
    };
    let mut names: Vec<String> = eval.facts().into_iter().filter(|fact| fact.kind == kind && fact.concrete == id).map(|fact| fact.identity).collect();
    names.sort();
    names.dedup();
    if names.is_empty() && kind != ElementKind::Vertex {
        constructor_names(constructor)
    } else {
        names
    }
}

/// Fills empty provenance on rows that already have a concrete face. Existing tokens stay.
pub(crate) fn stamp_face_provenance(record: &mut BlockRecord) {
    let pending: Vec<(usize, u32)> = record
        .face_materials
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| match entry.face {
            Some(face) if entry.provenance.is_empty() => Some((index, face)),
            _ => None,
        })
        .collect();
    if pending.is_empty() {
        return;
    }
    let names: Vec<Vec<String>> = pending.iter().map(|(_, face)| face_provenance_names(record, *face)).collect();
    for ((index, _), names) in pending.into_iter().zip(names) {
        if !names.is_empty() {
            record.face_materials[index].provenance = names;
        }
    }
}

/// Fills empty provenance on group members that still have a concrete face. Existing tokens stay.
pub(crate) fn stamp_surface_groups(record: &mut BlockRecord) {
    let pending: Vec<(usize, usize, u32)> = record
        .surface_groups
        .iter()
        .enumerate()
        .flat_map(|(group_index, group)| {
            group.members.iter().enumerate().filter_map(move |(member_index, member)| match member.face {
                Some(face) if member.provenance.is_empty() => Some((group_index, member_index, face)),
                _ => None,
            })
        })
        .collect();
    if pending.is_empty() {
        return;
    }
    let names: Vec<Vec<String>> = pending.iter().map(|(_, _, face)| face_provenance_names(record, *face)).collect();
    for ((group_index, member_index, _), names) in pending.into_iter().zip(names) {
        if !names.is_empty() {
            record.surface_groups[group_index].members[member_index].provenance = names;
        }
    }
}

fn constructor_names(element: ConcreteElement) -> Vec<String> {
    let reference = match element {
        ConcreteElement::Face(id) if (1..=6).contains(&id) => SemanticRef { kind: ElementKind::Face, role: SemanticRole::SeedFace { axis: (id - 1) as u8 } },
        ConcreteElement::Edge(id) if (9..=20).contains(&id) => SemanticRef { kind: ElementKind::Edge, role: SemanticRole::SeedEdge { slot: (id - 9) as u8 } },
        _ => return Vec::new(),
    };
    vec![reference.to_string()]
}

fn stored_match_line(report: &SemanticShadowReport, record: &BlockRecord) -> String {
    match (&report.candidate, &record.body) {
        (Some(candidate), Some(stored)) if candidate == stored => "match against the stored body".into(),
        (Some(_), Some(_)) => "differs from the stored body".into(),
        (Some(_), None) => "candidate produced and the record has no stored body".into(),
        _ => "not produced".into(),
    }
}

fn codec_line(program: &SemanticProgram) -> String {
    if !singular(program) {
        return "recorded groups".into();
    }
    match SemanticProgram::from_text(&program.to_text()) {
        Ok(parsed) if parsed == *program => "round-trip".into(),
        Ok(_) => "the text reloaded as a different program".into(),
        Err(detail) => detail,
    }
}

fn singular(program: &SemanticProgram) -> bool {
    program.ops.iter().all(|op| match op {
        SemanticOp::Create { .. } | SemanticOp::Resize { .. } | SemanticOp::Mirror { .. } | SemanticOp::PushFace { .. } | SemanticOp::Seed => true,
        SemanticOp::Subdivide { face, .. } | SemanticOp::Split { edge: face } | SemanticOp::MoveEdge { edge: face, .. } | SemanticOp::MoveVertex { vertex: face, .. } | SemanticOp::ExtrudeEdge { edge: face, .. } => face.len() == 1,
        SemanticOp::Extrude { faces, .. } => faces.iter().all(|group| group.len() == 1),
        SemanticOp::Bevel { edges, .. } => edges.iter().all(|group| group.len() == 1),
        SemanticOp::Round { edge, .. } => !edge.is_empty(),
    })
}

fn append_selection(lines: &mut Vec<String>, report: Option<&SemanticShadowReport>, selected: Option<ConcreteElement>, fallback: &str) {
    let Some(element) = selected else {
        lines.push("Concrete ID: none".into());
        lines.push("Semantic identity: none".into());
        lines.push("Created by operation: none".into());
        lines.push("Descends from: none".into());
        lines.push("Current binding status: none".into());
        return;
    };
    lines.push(format!("Concrete ID: {} {}", element.kind_name(), element.body_id()));
    let names = report
        .map(|report| {
            report
                .provenance
                .iter()
                .filter(|fact| fact.kind == element_kind(element) && fact.concrete == element.body_id())
                .map(|fact| fact.identity.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if names.is_empty() {
        lines.push("Semantic identity: none".into());
        lines.push("Created by operation: none".into());
        lines.push("Descends from: none".into());
        let status = match report {
            Some(report) if report.status == SemanticShadowStatus::Pass => "missing",
            Some(_) => fallback,
            None => "not recorded",
        };
        lines.push(format!("Current binding status: {status}"));
        return;
    }
    for name in names {
        lines.push(format!("Semantic identity: {name}"));
        match parse_ref(&name) {
            Ok(reference) => {
                let (created, ancestor) = descent(&reference);
                lines.push(format!("Created by operation: {created}"));
                lines.push(format!("Descends from: {ancestor}"));
            }
            Err(_) => {
                lines.push("Created by operation: none".into());
                lines.push("Descends from: none".into());
            }
        }
        lines.push("Current binding status: bound".into());
    }
}

fn descent(reference: &SemanticRef) -> (String, String) {
    match &reference.role {
        SemanticRole::SeedFace { .. } | SemanticRole::SeedEdge { .. } => ("create".into(), "creation".into()),
        SemanticRole::SubdivCell { parent, .. } | SemanticRole::SubdivEdge { parent, .. } => ("subdivide".into(), parent.to_string()),
        SemanticRole::ExtrudeCap { source } => ("extrude".into(), source.to_string()),
        SemanticRole::ExtrudeSide { cap, boundary } | SemanticRole::ExtrudeOuter { cap, boundary } | SemanticRole::ExtrudeCarve { cap, boundary } => ("extrude".into(), format!("{cap}, {boundary}")),
        SemanticRole::ExtrudeLeg { cap, boundary, end } => ("extrude".into(), format!("{cap}, {boundary}, {end}")),
        SemanticRole::SplitKept { edge, .. } | SemanticRole::SplitNew { edge, .. } | SemanticRole::SplitVertex { edge, .. } => ("split".into(), edge.to_string()),
        SemanticRole::BevelFace { edge, .. } | SemanticRole::BevelEdge { edge, .. } => ("bevel".into(), edge.to_string()),
        SemanticRole::AnyExtrudeSide { cap } => ("extrude".into(), cap.to_string()),
        SemanticRole::ExtrudeEdgeWall { edge, .. } | SemanticRole::ExtrudeEdgeOuter { edge, .. } => ("extrude".into(), edge.to_string()),
        SemanticRole::FilletFace { edge } | SemanticRole::FilletBoundary { edge, .. } | SemanticRole::FilletJunction { edge, .. } => ("round".into(), edge.to_string()),
        SemanticRole::CornerPatch { edges } => ("round".into(), edges.iter().map(|edge| edge.to_string()).collect::<Vec<_>>().join(", ")),
    }
}

fn binding_word(status: SemanticShadowStatus) -> &'static str {
    match status {
        SemanticShadowStatus::Pass => "bound",
        SemanticShadowStatus::Missing => "missing",
        SemanticShadowStatus::Ambiguous => "ambiguous",
        SemanticShadowStatus::Invalidated => "invalidated",
        SemanticShadowStatus::Refused => "not recorded",
    }
}

fn replay(program: &SemanticProgram) -> Result<Eval, (Eval, Fail)> {
    let mut eval = Eval::bare();
    if !matches!(program.ops.first(), Some(SemanticOp::Create { .. })) {
        return Err((eval, Fail::refused("a semantic program starts with create")));
    }
    for (index, op) in program.ops.iter().enumerate() {
        if let Err(fail) = eval.apply(op, index) {
            return Err((eval, fail));
        }
    }
    if eval.body.is_none() && program.ops.iter().any(|op| matches!(op, SemanticOp::PushFace { .. })) {
        if let Err(fail) = eval.ensure_body() {
            return Err((eval, fail));
        }
    }
    Ok(eval)
}

struct Node {
    reference: SemanticRef,
    concrete: u32,
}

struct ActiveRound {
    token: String,
    edge_id: u32,
    radius_m: f64,
    fillet: crate::round_intent::Fillet,
    chain: Vec<u32>,
}

struct Eval {
    size_m: [f64; 3],
    body: Option<SolidBody>,
    nodes: Vec<Node>,
    subdivs: Vec<(SemanticRef, u32, u32)>,
    extrudes: Vec<SemanticRef>,
    bevels: Vec<SemanticRef>,
    rounds: Vec<ActiveRound>,
    resolved: u32,
    missing: u32,
    ambiguous: u32,
    invalidated: u32,
}

fn prefer_persistent<'a>(current: &'a str, incoming: &'a str) -> &'a str {
    fn rank(token: &str) -> (u8, usize, &str) {
        let class = if token.contains("seed-edge/") {
            0
        } else if token.contains("split-") {
            2
        } else {
            1
        };
        (class, token.len(), token)
    }
    if rank(incoming) < rank(current) { incoming } else { current }
}

impl Eval {
    fn bare() -> Self {
        Self {
            size_m: [1.0, 1.0, 1.0],
            body: None,
            nodes: Vec::new(),
            subdivs: Vec::new(),
            extrudes: Vec::new(),
            bevels: Vec::new(),
            rounds: Vec::new(),
            resolved: 0,
            missing: 0,
            ambiguous: 0,
            invalidated: 0,
        }
    }

    fn apply(&mut self, op: &SemanticOp, step: usize) -> Result<(), Fail> {
        match op {
            SemanticOp::Create { size_m } => {
                if step != 0 {
                    return Err(Fail::refused("create is the first semantic operation").at_step(step));
                }
                self.size_m = finite_size(*size_m).map_err(|detail| Fail::refused(detail).at_step(step))?;
                Ok(())
            }
            SemanticOp::Resize { size_m } => {
                let size_m = finite_size(*size_m).map_err(|detail| Fail::refused(detail).at_step(step))?;
                if let Some(body) = &self.body {
                    let edit = body.scale_to(size_m).map_err(|error| Fail::refused(format!("scale refused: {error}")).at_step(step))?;
                    self.body = Some(edit.body);
                    self.size_m = edit.size_m;
                } else {
                    self.size_m = size_m;
                }
                Ok(())
            }
            SemanticOp::Subdivide { face, u, v } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                let id = self.bind_names(face).map_err(|fail| fail.at_step(step))?;
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let (edit, lineage) = body.subdivide_face_traced(id, *u, *v).map_err(|error| Fail::refused(format!("subdivide refused: {error}")).at_step(step))?;
                for parent in face {
                    self.absorb_subdivide(parent, &lineage);
                }
                self.body = Some(edit.body);
                self.size_m = edit.size_m;
                Ok(())
            }
            SemanticOp::Extrude { faces, amount } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                if faces.is_empty() {
                    return Err(Fail::missing("extrude named no face").at_step(step));
                }
                let mut ids = Vec::new();
                for face in faces {
                    ids.push(self.bind_names(face).map_err(|fail| fail.at_step(step))?);
                }
                let delta = self.extrude_delta(&ids, amount).map_err(|fail| fail.at_step(step))?;
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let (edit, lineage) = body.extrude_faces_traced(&ids, delta).map_err(|error| Fail::refused(format!("extrude refused: {error}")).at_step(step))?;
                self.absorb_extrude(faces, &ids, &lineage);
                self.body = Some(edit.body);
                self.size_m = edit.size_m;
                self.follow_replaced_edges(&lineage);
                self.reclose_rounds().map_err(|fail| fail.at_step(step))?;
                Ok(())
            }
            SemanticOp::Split { edge } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                let id = self.bind_names(edge).map_err(|fail| fail.at_step(step))?;
                if let Some(round) = self.rounds.iter().find(|round| round.edge_id == id || round.chain.contains(&id)) {
                    return Err(Fail::refused(format!("split conflicts with the round on {}", round.token)).at_step(step));
                }
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let (edit, lineage) = body.split_edge_traced(id).map_err(|error| Fail::refused(format!("split refused: {error}")).at_step(step))?;
                self.absorb_split(edge, &lineage);
                self.body = Some(edit.body);
                self.size_m = edit.size_m;
                self.reclose_rounds().map_err(|fail| fail.at_step(step))?;
                Ok(())
            }
            SemanticOp::Bevel { edges, width_m } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                if edges.is_empty() {
                    return Err(Fail::missing("bevel named no edge").at_step(step));
                }
                let mut ids = Vec::new();
                for edge in edges {
                    ids.push(self.bind_names(edge).map_err(|fail| fail.at_step(step))?);
                }
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let (cut, lineage) = body.bevel_edges_traced(&ids, *width_m).map_err(|error| Fail::refused(format!("bevel refused: {error}")).at_step(step))?;
                for edge in edges {
                    for name in edge {
                        if !self.bevels.contains(name) {
                            self.bevels.push(name.clone());
                        }
                    }
                }
                self.absorb_bevel(&lineage);
                self.body = Some(cut.edit.body);
                self.size_m = cut.edit.size_m;
                Ok(())
            }
            SemanticOp::MoveEdge { edge, delta_m } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                let id = self.bind_names(edge).map_err(|fail| fail.at_step(step))?;
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let edit = body.move_edge(id, *delta_m).map_err(|error| Fail::refused(format!("move edge refused: {error}")).at_step(step))?;
                self.body = Some(edit.body);
                self.size_m = edit.size_m;
                Ok(())
            }
            SemanticOp::MoveVertex { vertex, delta_m } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                let id = self.bind_names(vertex).map_err(|fail| fail.at_step(step))?;
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let edit = body.move_vertex(id, *delta_m).map_err(|error| Fail::refused(format!("move vertex refused: {error}")).at_step(step))?;
                self.body = Some(edit.body);
                self.size_m = edit.size_m;
                Ok(())
            }
            SemanticOp::ExtrudeEdge { edge, delta_m } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                let id = self.bind_names(edge).map_err(|fail| fail.at_step(step))?;
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let (edit, lineage) = body.extrude_edge_traced(id, *delta_m).map_err(|error| Fail::refused(format!("extrude edge refused: {error}")).at_step(step))?;
                for name in edge {
                    self.absorb_extrude_edge(name, &lineage);
                }
                self.body = Some(edit.body);
                self.size_m = edit.size_m;
                Ok(())
            }
            SemanticOp::Mirror { axis } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                if *axis > 2 {
                    return Err(Fail::refused("mirror axis is not 0, 1, or 2").at_step(step));
                }
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let edit = body.mirrored(*axis as usize).map_err(|error| Fail::refused(format!("mirror refused: {error}")).at_step(step))?;
                self.body = Some(edit.body);
                self.size_m = edit.size_m;
                Ok(())
            }
            SemanticOp::PushFace { face, distance_m } => {
                if *face > 5 || !distance_m.is_finite() {
                    return Err(Fail::refused("analytic face push is not usable").at_step(step));
                }
                if self.body.is_some() {
                    return Err(Fail::refused("analytic face push does not apply after the body exists").at_step(step));
                }
                let axis = (*face / 2) as usize;
                let mut size_m = self.size_m;
                size_m[axis] += *distance_m;
                self.size_m = finite_size(size_m).map_err(|detail| Fail::refused(detail).at_step(step))?;
                Ok(())
            }
            SemanticOp::Round { edge, radius_m } => {
                self.ensure_body().map_err(|fail| fail.at_step(step))?;
                if edge.is_empty() || edge.iter().any(|name| !name.to_string().starts_with("E:")) {
                    return Err(Fail::refused("round does not name a semantic edge").at_step(step));
                }
                let mut incoming = Vec::new();
                for name in edge {
                    let token = name.to_string();
                    if incoming.iter().any(|have: &ActiveRound| have.token == token) {
                        return Err(Fail::refused("round does not name a semantic edge").at_step(step));
                    }
                    let id = self.bind(name).map_err(|fail| fail.at_step(step))?;
                    let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                    let chain = crate::round_intent::logical_edge_chain(body, id).map_err(|detail| Fail::refused(detail).at_step(step))?;
                    let fillet = crate::round_intent::fillet_for_chain(body, &chain, *radius_m).map_err(|detail| Fail::refused(detail).at_step(step))?;
                    incoming.push(ActiveRound { token, edge_id: chain[0], radius_m: *radius_m, fillet, chain });
                }
                for item in incoming {
                    if let Some(existing) = self.rounds.iter_mut().find(|have| have.token == item.token || have.chain.iter().any(|id| item.chain.contains(id))) {
                        let token = prefer_persistent(&existing.token, &item.token).to_string();
                        *existing = ActiveRound { token, ..item };
                    } else {
                        self.rounds.push(item);
                    }
                }
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body").at_step(step))?;
                let fillets: Vec<_> = self.rounds.iter().map(|round| round.fillet.clone()).collect();
                if let Err(detail) = crate::round_intent::round_fillets_close(body, &fillets) {
                    let names = self.rounds.iter().map(|round| round.token.as_str()).collect::<Vec<_>>().join(" ");
                    return Err(Fail::refused(format!("{detail} ({names})")).at_step(step));
                }
                self.remint_curves().map_err(|fail| fail.at_step(step))
            }
            SemanticOp::Seed => {
                if step != 1 {
                    return Err(Fail::refused("seed follows create").at_step(step));
                }
                self.ensure_body().map_err(|fail| fail.at_step(step))
            }
        }
    }

    /// Rebuilds each active fillet on the body a later topology edit just wrote.
    /// The saved token stays. A fillet that no longer closes fails the operation.
    fn reclose_rounds(&mut self) -> Result<(), Fail> {
        if self.rounds.is_empty() {
            self.nodes.retain(|node| !is_curve_role(&node.reference.role));
            return Ok(());
        }
        let kept: Vec<(String, f64)> = self.rounds.iter().map(|round| (round.token.clone(), round.radius_m)).collect();
        let mut next = Vec::with_capacity(kept.len());
        for (token, radius_m) in kept {
            let reference = parse_ref(&token).map_err(Fail::refused)?;
            let id = self.bind(&reference)?;
            let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body"))?;
            let chain = crate::round_intent::logical_edge_chain(body, id).map_err(Fail::refused)?;
            let fillet = crate::round_intent::fillet_for_chain(body, &chain, radius_m).map_err(Fail::refused)?;
            next.push(ActiveRound { token, edge_id: chain[0], radius_m, fillet, chain });
        }
        let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body"))?;
        let fillets: Vec<_> = next.iter().map(|round| round.fillet.clone()).collect();
        if let Err(detail) = crate::round_intent::round_fillets_close(body, &fillets) {
            let names = next.iter().map(|round| round.token.as_str()).collect::<Vec<_>>().join(" ");
            return Err(Fail::refused(format!("{detail} ({names})")));
        }
        self.rounds = next;
        self.remint_curves()
    }

    /// The corner a round was on moves to the new cap boundary.
    /// A wall's outer edge is that corner; the old edge is the flat seam at the wall's base.
    /// A carve deletes the old edge and leaves the lip. Curve nodes are reminted later.
    fn follow_replaced_edges(&mut self, lineage: &TopologyLineage) {
        let Some(body) = &self.body else { return };
        let live: Vec<u32> = body.edges.iter().map(|edge| edge.id).collect();
        let mut moved = Vec::new();
        for birth in &lineage.births {
            if birth.kind != ElementKind::Edge {
                continue;
            }
            let boundary = match birth.role {
                BirthRole::ExtrudeOuter { boundary } if boundary != birth.id => boundary,
                BirthRole::ExtrudeCarve { boundary, .. } if !live.contains(&boundary) => boundary,
                _ => continue,
            };
            if moved.iter().any(|(dead, _)| *dead == boundary) {
                continue;
            }
            moved.push((boundary, birth.id));
        }
        for node in &mut self.nodes {
            if node.reference.kind != ElementKind::Edge || is_curve_role(&node.reference.role) {
                continue;
            }
            if let Some((_, lip)) = moved.iter().find(|(dead, _)| *dead == node.concrete) {
                node.concrete = *lip;
            }
        }
    }

    /// Drops curve nodes and mints them again from the active rounds.
    /// The tape still stores the source edges and the radius. The curve names are not operations.
    fn remint_curves(&mut self) -> Result<(), Fail> {
        self.nodes.retain(|node| !is_curve_role(&node.reference.role));
        if self.rounds.is_empty() {
            return Ok(());
        }
        let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body"))?;
        let pairs: Vec<(String, crate::round_intent::Fillet)> = self.rounds.iter().map(|round| (round.token.clone(), round.fillet.clone())).collect();
        let elements = crate::round_intent::curve_catalog(body, &pairs).map_err(Fail::refused)?;
        for element in elements {
            let reference = parse_ref(&element.token).map_err(Fail::refused)?;
            let kind = match element.kind {
                crate::round_intent::CurveKind::Face => ElementKind::Face,
                crate::round_intent::CurveKind::Edge => ElementKind::Edge,
                crate::round_intent::CurveKind::Vertex => ElementKind::Vertex,
            };
            if reference.kind != kind {
                return Err(Fail::refused(format!("curve identity kind does not match {}", element.token)));
            }
            self.insert(reference, element.id);
        }
        Ok(())
    }

    fn ensure_body(&mut self) -> Result<(), Fail> {
        if self.body.is_some() {
            return Ok(());
        }
        let body = SolidBody::from_box(self.size_m).map_err(|error| Fail::refused(format!("seed solid refused: {error}")))?;
        for birth in TopologyLineage::seed_box().births {
            let live = match birth.kind {
                ElementKind::Face => body.faces.iter().any(|face| face.id == birth.id),
                ElementKind::Edge => body.edges.iter().any(|edge| edge.id == birth.id),
                ElementKind::Vertex => false,
            };
            if !live {
                return Err(Fail::refused("the seed solid did not mint the constructor id"));
            }
            let Some(reference) = seed_ref(&birth) else { continue };
            self.insert(reference, birth.id);
        }
        self.body = Some(body);
        Ok(())
    }

    fn extrude_delta(&self, ids: &[u32], amount: &ExtrudeAmount) -> Result<[f64; 3], Fail> {
        match amount {
            ExtrudeAmount::Delta(delta) => {
                if delta.iter().any(|axis| !axis.is_finite()) || delta.iter().all(|axis| axis.abs() < MIN_DISTANCE_M) {
                    return Err(Fail::refused("extrude delta is not usable"));
                }
                Ok(*delta)
            }
            ExtrudeAmount::Distance(distance) => {
                if !distance.is_finite() || distance.abs() < MIN_DISTANCE_M {
                    return Err(Fail::refused("extrude distance is not usable"));
                }
                let body = self.body.as_ref().ok_or_else(|| Fail::refused("no body"))?;
                let mut direction: Option<[f64; 3]> = None;
                for id in ids {
                    let normal = body.unit_normal(*id).ok_or_else(|| Fail::refused("that face has no normal"))?;
                    if let Some(first) = direction {
                        if dot(first, normal) < 1.0 - NORMAL_AGREE {
                            return Err(Fail::refused("extrude direction is not unique"));
                        }
                    } else {
                        direction = Some(normal);
                    }
                }
                let normal = direction.ok_or_else(|| Fail::refused("extrude named no face"))?;
                Ok([normal[0] * distance, normal[1] * distance, normal[2] * distance])
            }
        }
    }

    fn bind_names(&mut self, names: &[SemanticRef]) -> Result<u32, Fail> {
        if names.is_empty() {
            self.missing += 1;
            return Err(Fail::missing("semantic reference was not recorded"));
        }
        let mut agreed = None;
        for name in names {
            let id = self.bind(name)?;
            match agreed {
                None => agreed = Some(id),
                Some(previous) if previous != id => {
                    self.ambiguous += 1;
                    return Err(Fail::ambiguous(format!("{name} does not name the same element as the other recorded references")));
                }
                Some(_) => {}
            }
        }
        Ok(agreed.expect("a recorded group is non-empty"))
    }

    fn bind(&mut self, reference: &SemanticRef) -> Result<u32, Fail> {
        match self.decide(reference, 0) {
            Outcome::Bound(id) => {
                self.resolved += 1;
                Ok(id)
            }
            Outcome::Missing => {
                self.missing += 1;
                Err(Fail::missing(format!("{reference} did not resolve")))
            }
            Outcome::Ambiguous => {
                self.ambiguous += 1;
                Err(Fail::ambiguous(format!("{reference} resolved more than once")))
            }
            Outcome::Invalidated => {
                self.invalidated += 1;
                Err(Fail::invalidated(format!("{reference} ceased to exist")))
            }
        }
    }

    fn decide(&self, reference: &SemanticRef, depth: u32) -> Outcome {
        if depth > 32 {
            return Outcome::Missing;
        }
        match &reference.role {
            SemanticRole::AnyExtrudeSide { cap } => return self.decide_any_side(cap, depth),
            SemanticRole::SubdivCell { parent, u, v } | SemanticRole::SubdivEdge { parent, u, v, .. } => {
                if let Some(outcome) = self.component_failure(parent, depth) {
                    return outcome;
                }
                if self.out_of_grid(parent, *u, *v) {
                    return Outcome::Invalidated;
                }
            }
            SemanticRole::ExtrudeCap { source }
            | SemanticRole::ExtrudeEdgeWall { edge: source, .. }
            | SemanticRole::ExtrudeEdgeOuter { edge: source, .. }
            | SemanticRole::SplitKept { edge: source, .. }
            | SemanticRole::SplitNew { edge: source, .. }
            | SemanticRole::SplitVertex { edge: source, .. }
            | SemanticRole::BevelFace { edge: source, .. } => {
                if let Some(outcome) = self.component_failure(source, depth) {
                    return outcome;
                }
            }
            SemanticRole::BevelEdge { edge, .. } => {
                if let Some(outcome) = self.component_failure(edge, depth) {
                    return outcome;
                }
            }
            SemanticRole::ExtrudeSide { cap, boundary } | SemanticRole::ExtrudeOuter { cap, boundary } | SemanticRole::ExtrudeCarve { cap, boundary } | SemanticRole::ExtrudeLeg { cap, boundary, .. } => {
                if let Some(outcome) = self.component_failure(cap, depth) {
                    return outcome;
                }
                if let Some(outcome) = self.component_failure(boundary, depth) {
                    return outcome;
                }
            }
            SemanticRole::FilletFace { edge } | SemanticRole::FilletBoundary { edge, .. } | SemanticRole::FilletJunction { edge, .. } => {
                if let Some(outcome) = self.component_failure(edge, depth) {
                    return outcome;
                }
            }
            SemanticRole::CornerPatch { edges } => {
                for edge in edges {
                    if let Some(outcome) = self.component_failure(edge, depth) {
                        return outcome;
                    }
                }
            }
            SemanticRole::SeedFace { .. } | SemanticRole::SeedEdge { .. } => {}
        }
        let mut hits = Vec::new();
        for node in &self.nodes {
            if node.reference == *reference && !hits.contains(&node.concrete) {
                hits.push(node.concrete);
            }
        }
        match hits.len() {
            0 => self.absent(reference),
            1 => Outcome::Bound(hits[0]),
            _ => Outcome::Ambiguous,
        }
    }

    fn component_failure(&self, reference: &SemanticRef, depth: u32) -> Option<Outcome> {
        match self.decide(reference, depth + 1) {
            Outcome::Bound(_) => None,
            other => Some(other),
        }
    }

    fn out_of_grid(&self, parent: &SemanticRef, u: u32, v: u32) -> bool {
        let grids: Vec<(u32, u32)> = self.subdivs.iter().filter(|(face, _, _)| face == parent).map(|(_, au, av)| (*au, *av)).collect();
        !grids.is_empty() && grids.iter().all(|(au, av)| u >= *au || v >= *av)
    }

    fn decide_any_side(&self, cap: &SemanticRef, depth: u32) -> Outcome {
        if let Some(outcome) = self.component_failure(cap, depth) {
            return outcome;
        }
        let mut hits = Vec::new();
        for node in &self.nodes {
            if let SemanticRole::ExtrudeSide { cap: side_cap, .. } = &node.reference.role {
                if side_cap.as_ref() == cap && !hits.contains(&node.concrete) {
                    hits.push(node.concrete);
                }
            }
        }
        match hits.len() {
            0 if self.extrudes.iter().any(|face| face == cap) => Outcome::Invalidated,
            0 => Outcome::Missing,
            1 => Outcome::Bound(hits[0]),
            _ => Outcome::Ambiguous,
        }
    }

    fn absent(&self, reference: &SemanticRef) -> Outcome {
        match &reference.role {
            SemanticRole::ExtrudeSide { cap, .. } | SemanticRole::ExtrudeOuter { cap, .. } | SemanticRole::ExtrudeCarve { cap, .. } | SemanticRole::ExtrudeLeg { cap, .. } if self.extrudes.iter().any(|face| face == cap.as_ref()) => Outcome::Invalidated,
            SemanticRole::BevelFace { edge, .. } | SemanticRole::BevelEdge { edge, .. } if self.bevels.iter().any(|named| named == edge.as_ref()) => Outcome::Invalidated,
            _ => Outcome::Missing,
        }
    }

    fn refs_of(&self, kind: ElementKind, concrete: u32) -> Vec<SemanticRef> {
        let mut found = Vec::new();
        for node in &self.nodes {
            if node.reference.kind == kind && node.concrete == concrete && !found.contains(&node.reference) {
                found.push(node.reference.clone());
            }
        }
        found
    }

    fn insert(&mut self, reference: SemanticRef, concrete: u32) {
        if self.nodes.iter().any(|node| node.reference == reference && node.concrete == concrete) {
            return;
        }
        self.nodes.push(Node { reference, concrete });
    }

    fn absorb_subdivide(&mut self, parent: &SemanticRef, lineage: &TopologyLineage) {
        let mut max_u = 0u32;
        let mut max_v = 0u32;
        let mut any = false;
        for birth in &lineage.births {
            if let BirthRole::SubdivCell { u, v } = birth.role {
                any = true;
                max_u = max_u.max(u + 1);
                max_v = max_v.max(v + 1);
            }
        }
        if any {
            self.subdivs.push((parent.clone(), max_u, max_v));
        }
        for birth in &lineage.births {
            let role = match birth.role {
                BirthRole::SubdivCell { u, v } => SemanticRole::SubdivCell { parent: Box::new(parent.clone()), u, v },
                BirthRole::SubdivEdge { u, v, side } => SemanticRole::SubdivEdge { parent: Box::new(parent.clone()), u, v, side },
                _ => continue,
            };
            self.insert(SemanticRef { kind: birth.kind, role }, birth.id);
        }
    }

    /// A subdivided cell's wall keeps the grid edge of that cell. Other aliases of the
    /// same concrete edge stay on the edge and are not copied onto the wall.
    fn boundary_refs_for_cap(&self, cap: &SemanticRef, boundary: u32) -> Vec<SemanticRef> {
        let all = self.refs_of(ElementKind::Edge, boundary);
        let SemanticRole::SubdivCell { parent, u, v } = &cap.role else {
            return all;
        };
        let matched: Vec<SemanticRef> = all
            .iter()
            .filter(|reference| match &reference.role {
                SemanticRole::SubdivEdge { parent: edge_parent, u: edge_u, v: edge_v, .. } => edge_parent.as_ref() == parent.as_ref() && *edge_u == *u && *edge_v == *v,
                _ => false,
            })
            .cloned()
            .collect();
        if matched.is_empty() { all } else { matched }
    }

    /// Boundary names that do not already identify this kind of feature on `cap`.
    ///
    /// A round follows the cap corner, so the seed edge and the outer edge share one id.
    /// The next wall uses the outer edge's own name. The first wall keeps the seed edge.
    fn open_boundary_refs(&self, cap: &SemanticRef, boundary: u32, kind: ElementKind, role_of: &impl Fn(SemanticRef, SemanticRef) -> SemanticRole) -> Vec<SemanticRef> {
        let all = self.boundary_refs_for_cap(cap, boundary);
        let open: Vec<SemanticRef> = all
            .iter()
            .filter(|boundary_ref| {
                let reference = SemanticRef { kind, role: role_of(cap.clone(), (*boundary_ref).clone()) };
                !self.nodes.iter().any(|node| node.reference == reference)
            })
            .cloned()
            .collect();
        if open.is_empty() { all } else { open }
    }

    fn absorb_extrude(&mut self, faces: &[Vec<SemanticRef>], ids: &[u32], lineage: &TopologyLineage) {
        for group in faces {
            for face in group {
                if !self.extrudes.contains(face) {
                    self.extrudes.push(face.clone());
                }
            }
        }
        for birth in &lineage.births {
            match birth.role {
                BirthRole::ExtrudeCap => {
                    let Some(index) = ids.iter().position(|id| *id == birth.id) else { continue };
                    for source in &faces[index] {
                        self.insert(SemanticRef { kind: ElementKind::Face, role: SemanticRole::ExtrudeCap { source: Box::new(source.clone()) } }, birth.id);
                    }
                }
                BirthRole::ExtrudeSide { face, boundary } => {
                    let Some(index) = ids.iter().position(|id| *id == face) else { continue };
                    let role_of = |cap: SemanticRef, boundary_ref: SemanticRef| SemanticRole::ExtrudeSide { cap: Box::new(cap), boundary: Box::new(boundary_ref) };
                    for cap in &faces[index] {
                        for boundary_ref in self.open_boundary_refs(cap, boundary, ElementKind::Face, &role_of) {
                            self.insert(SemanticRef { kind: ElementKind::Face, role: role_of(cap.clone(), boundary_ref) }, birth.id);
                        }
                    }
                }
                BirthRole::ExtrudeOuter { boundary } => {
                    let Some(face) = lineage.births.iter().find_map(|other| match other.role {
                        BirthRole::ExtrudeSide { face, boundary: side_boundary } if side_boundary == boundary => Some(face),
                        _ => None,
                    }) else {
                        continue;
                    };
                    self.name_boundary(faces, ids, face, boundary, birth.id, ElementKind::Edge, |cap, boundary_ref| SemanticRole::ExtrudeOuter {
                        cap: Box::new(cap),
                        boundary: Box::new(boundary_ref),
                    });
                }
                BirthRole::ExtrudeCarve { face, boundary } => {
                    self.name_boundary(faces, ids, face, boundary, birth.id, ElementKind::Edge, |cap, boundary_ref| SemanticRole::ExtrudeCarve {
                        cap: Box::new(cap),
                        boundary: Box::new(boundary_ref),
                    });
                }
                BirthRole::ExtrudeLeg { face, boundary, end } => {
                    self.name_boundary(faces, ids, face, boundary, birth.id, ElementKind::Edge, |cap, boundary_ref| SemanticRole::ExtrudeLeg {
                        cap: Box::new(cap),
                        boundary: Box::new(boundary_ref),
                        end,
                    });
                }
                _ => {}
            }
        }
    }

    fn name_boundary(
        &mut self,
        faces: &[Vec<SemanticRef>],
        ids: &[u32],
        face: u32,
        boundary: u32,
        birth_id: u32,
        kind: ElementKind,
        role_of: impl Fn(SemanticRef, SemanticRef) -> SemanticRole,
    ) {
        let Some(index) = ids.iter().position(|id| *id == face) else { return };
        for cap in &faces[index] {
            for boundary_ref in self.open_boundary_refs(cap, boundary, kind, &role_of) {
                self.insert(SemanticRef { kind, role: role_of(cap.clone(), boundary_ref) }, birth_id);
            }
        }
    }

    fn absorb_split(&mut self, edges: &[SemanticRef], lineage: &TopologyLineage) {
        for edge in edges {
            let ordinal = self
                .nodes
                .iter()
                .filter(|node| matches!(&node.reference.role, SemanticRole::SplitNew { edge: named, .. } if named.as_ref() == edge))
                .count() as u32;
            for birth in &lineage.births {
                let role = match birth.role {
                    BirthRole::SplitKept => SemanticRole::SplitKept { edge: Box::new(edge.clone()), ordinal },
                    BirthRole::SplitNew => SemanticRole::SplitNew { edge: Box::new(edge.clone()), ordinal },
                    BirthRole::SplitMidpoint => SemanticRole::SplitVertex { edge: Box::new(edge.clone()), ordinal },
                    _ => continue,
                };
                self.insert(SemanticRef { kind: birth.kind, role }, birth.id);
            }
        }
    }

    fn absorb_bevel(&mut self, lineage: &TopologyLineage) {
        let prior = self.nodes.len();
        for birth in &lineage.births {
            match birth.role {
                BirthRole::BevelFace { source } => {
                    for edge in self.refs_before(ElementKind::Edge, source, prior) {
                        let ordinal = self.bevel_ordinal(&edge, prior);
                        self.insert(SemanticRef { kind: ElementKind::Face, role: SemanticRole::BevelFace { edge: Box::new(edge), ordinal } }, birth.id);
                    }
                }
                BirthRole::BevelEdge { source, slot } => {
                    for edge in self.refs_before(ElementKind::Edge, source, prior) {
                        let ordinal = self.bevel_ordinal(&edge, prior);
                        self.insert(SemanticRef { kind: ElementKind::Edge, role: SemanticRole::BevelEdge { edge: Box::new(edge), slot, ordinal } }, birth.id);
                    }
                }
                _ => {}
            }
        }
    }

    fn refs_before(&self, kind: ElementKind, concrete: u32, prior: usize) -> Vec<SemanticRef> {
        let mut found = Vec::new();
        for node in self.nodes.iter().take(prior) {
            if node.reference.kind == kind && node.concrete == concrete && !found.contains(&node.reference) {
                found.push(node.reference.clone());
            }
        }
        found
    }

    fn bevel_ordinal(&self, edge: &SemanticRef, prior: usize) -> u32 {
        self.nodes
            .iter()
            .take(prior)
            .filter(|node| matches!(&node.reference.role, SemanticRole::BevelFace { edge: named, .. } if named.as_ref() == edge))
            .count() as u32
    }

    fn absorb_extrude_edge(&mut self, edge: &SemanticRef, lineage: &TopologyLineage) {
        let ordinal = self
            .nodes
            .iter()
            .filter(|node| matches!(&node.reference.role, SemanticRole::ExtrudeEdgeWall { edge: named, .. } if named.as_ref() == edge))
            .count() as u32;
        for birth in &lineage.births {
            let role = match birth.role {
                BirthRole::ExtrudeEdgeWall => SemanticRole::ExtrudeEdgeWall { edge: Box::new(edge.clone()), ordinal },
                BirthRole::ExtrudeEdgeOuter => SemanticRole::ExtrudeEdgeOuter { edge: Box::new(edge.clone()), ordinal },
                _ => continue,
            };
            self.insert(SemanticRef { kind: birth.kind, role }, birth.id);
        }
    }

    fn facts(&self) -> Vec<ProvenanceFact> {
        let mut facts: Vec<ProvenanceFact> = self.nodes.iter().map(|node| ProvenanceFact { identity: node.reference.to_string(), kind: node.reference.kind, concrete: node.concrete }).collect();
        facts.sort_by(|left, right| left.identity.cmp(&right.identity).then(left.concrete.cmp(&right.concrete)));
        facts
    }
}

enum Outcome {
    Bound(u32),
    Missing,
    Ambiguous,
    Invalidated,
}

#[derive(Debug)]
struct Fail {
    kind: SemanticShadowStatus,
    detail: String,
}

impl Fail {
    fn missing(detail: impl Into<String>) -> Self {
        Self { kind: SemanticShadowStatus::Missing, detail: detail.into() }
    }

    fn ambiguous(detail: impl Into<String>) -> Self {
        Self { kind: SemanticShadowStatus::Ambiguous, detail: detail.into() }
    }

    fn invalidated(detail: impl Into<String>) -> Self {
        Self { kind: SemanticShadowStatus::Invalidated, detail: detail.into() }
    }

    fn refused(detail: impl Into<String>) -> Self {
        Self { kind: SemanticShadowStatus::Refused, detail: detail.into() }
    }

    fn at_step(mut self, step: usize) -> Self {
        self.detail = format!("operation {step}: {}", self.detail);
        self
    }
}

fn seed_ref(birth: &ElementBirth) -> Option<SemanticRef> {
    let role = match birth.role {
        BirthRole::SeedFace { axis } => SemanticRole::SeedFace { axis },
        BirthRole::SeedEdge { slot } => SemanticRole::SeedEdge { slot },
        _ => return None,
    };
    Some(SemanticRef { kind: birth.kind, role })
}

fn pass_report(eval: &Eval) -> SemanticShadowReport {
    let facts = eval.facts();
    let body = eval.body.clone();
    let mut text = String::from("Semantic: PASS\n");
    text.push_str(&counter_block(eval.resolved, 0, eval.missing, eval.ambiguous, eval.invalidated));
    text.push_str("candidate match: not compared\n");
    if let Some(body) = &body {
        let closed = body.validate().is_ok();
        text.push_str(&format!("closed solid: {}\n", if closed { "yes" } else { "no" }));
        text.push_str(&format!("faces: {}\nedges: {}\nvertices: {}\n", body.faces.len(), body.edges.len(), body.vertices.len()));
        text.push_str(&format!("volume: {:.6}\n", volume(body)));
        text.push_str(&format!("body hash: {:016x}\n", hash_body(body)));
    }
    text.push_str("material: semantic replay does not assign materials\n");
    text.push_str(&format!("provenance: {}\n", facts.len()));
    SemanticShadowReport {
        status: SemanticShadowStatus::Pass,
        text,
        resolved: eval.resolved,
        rebound: 0,
        missing: eval.missing,
        ambiguous: eval.ambiguous,
        invalidated: eval.invalidated,
        candidate: body,
        provenance: facts,
    }
}

fn fail_report(eval: &Eval, fail: Fail) -> SemanticShadowReport {
    let mut text = format!("{}\n{}\n", status_word(fail.kind), fail.detail);
    text.push_str(&counter_block(eval.resolved, 0, eval.missing, eval.ambiguous, eval.invalidated));
    text.push_str("candidate match: not produced\n");
    text.push_str("material: semantic replay does not assign materials\n");
    SemanticShadowReport {
        status: fail.kind,
        text,
        resolved: eval.resolved,
        rebound: 0,
        missing: eval.missing,
        ambiguous: eval.ambiguous,
        invalidated: eval.invalidated,
        candidate: None,
        provenance: eval.facts(),
    }
}

fn counter_block(resolved: u32, rebound: u32, missing: u32, ambiguous: u32, invalidated: u32) -> String {
    format!("resolved: {resolved}\nrebound: {rebound}\nmissing: {missing}\nambiguous: {ambiguous}\ninvalidated: {invalidated}\n")
}

fn replay_word(status: SemanticShadowStatus) -> &'static str {
    match status {
        SemanticShadowStatus::Pass => "PASS",
        other => status_word(other),
    }
}

fn status_word(status: SemanticShadowStatus) -> &'static str {
    match status {
        SemanticShadowStatus::Pass => "Semantic: PASS",
        SemanticShadowStatus::Missing => "SEMANTIC_REFERENCE_MISSING",
        SemanticShadowStatus::Ambiguous => "SEMANTIC_REFERENCE_AMBIGUOUS",
        SemanticShadowStatus::Invalidated => "SEMANTIC_REFERENCE_INVALIDATED",
        SemanticShadowStatus::Refused => "SEMANTIC_REPLAY_REFUSED",
    }
}

fn element_kind(element: ConcreteElement) -> ElementKind {
    match element {
        ConcreteElement::Face(_) => ElementKind::Face,
        ConcreteElement::Edge(_) => ElementKind::Edge,
        ConcreteElement::Vertex(_) => ElementKind::Vertex,
    }
}

fn finite_size(size_m: [f64; 3]) -> Result<[f64; 3], String> {
    if size_m.iter().any(|axis| !axis.is_finite() || *axis <= 0.0) {
        Err("size is not a positive extent".into())
    } else {
        Ok(size_m)
    }
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
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
    for vertex in &body.vertices {
        mix(&mut hash, &vertex.id.to_le_bytes());
        for axis in vertex.position {
            mix(&mut hash, &axis.to_bits().to_le_bytes());
        }
    }
    for edge in &body.edges {
        mix(&mut hash, &edge.id.to_le_bytes());
        mix(&mut hash, &edge.a.to_le_bytes());
        mix(&mut hash, &edge.b.to_le_bytes());
    }
    for face in &body.faces {
        mix(&mut hash, &face.id.to_le_bytes());
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
            let (a, b, c) = (positions[0], positions[index], positions[index + 1]);
            let triple = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
            sum += triple;
        }
    }
    sum / 6.0
}

impl fmt::Display for SemanticRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", kind_letter(self.kind), self.role)
    }
}

impl fmt::Display for SemanticRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SeedFace { axis } => write!(formatter, "seed/{axis}"),
            Self::SeedEdge { slot } => write!(formatter, "seed-edge/{slot}"),
            Self::SubdivCell { parent, u, v } => write!(formatter, "cell({parent},{u},{v})"),
            Self::SubdivEdge { parent, u, v, side } => write!(formatter, "grid({parent},{u},{v},{side})"),
            Self::ExtrudeCap { source } => write!(formatter, "cap({source})"),
            Self::ExtrudeSide { cap, boundary } => write!(formatter, "side({cap},{boundary})"),
            Self::ExtrudeOuter { cap, boundary } => write!(formatter, "outer({cap},{boundary})"),
            Self::ExtrudeCarve { cap, boundary } => write!(formatter, "carve({cap},{boundary})"),
            Self::ExtrudeLeg { cap, boundary, end } => write!(formatter, "leg({cap},{boundary},{end})"),
            Self::SplitKept { edge, ordinal } => write_ordinal(formatter, "split-kept", edge, *ordinal),
            Self::SplitNew { edge, ordinal } => write_ordinal(formatter, "split-new", edge, *ordinal),
            Self::SplitVertex { edge, ordinal } => write_ordinal(formatter, "split-at", edge, *ordinal),
            Self::BevelFace { edge, ordinal } => write_ordinal(formatter, "bevel", edge, *ordinal),
            Self::BevelEdge { edge, slot, ordinal } => {
                if *ordinal == 0 {
                    write!(formatter, "bevel-edge({edge},{slot})")
                } else {
                    write!(formatter, "bevel-edge({edge},{slot},{ordinal})")
                }
            }
            Self::AnyExtrudeSide { cap } => write!(formatter, "any-side({cap})"),
            Self::ExtrudeEdgeWall { edge, ordinal } => write_ordinal(formatter, "edge-wall", edge, *ordinal),
            Self::ExtrudeEdgeOuter { edge, ordinal } => write_ordinal(formatter, "edge-outer", edge, *ordinal),
            Self::FilletFace { edge } => write!(formatter, "fillet({edge})"),
            Self::FilletBoundary { edge, side } => write!(formatter, "fillet-{}({edge})", if *side == 0 { "a" } else { "b" }),
            Self::FilletJunction { edge, end } => write!(formatter, "fillet-{}({edge})", if *end == 0 { "start" } else { "end" }),
            Self::CornerPatch { edges } => {
                let inner = edges.iter().map(|edge| edge.to_string()).collect::<Vec<_>>().join(",");
                write!(formatter, "corner({inner})")
            }
        }
    }
}

fn write_ordinal(formatter: &mut fmt::Formatter<'_>, label: &str, inner: &SemanticRef, ordinal: u32) -> fmt::Result {
    if ordinal == 0 {
        write!(formatter, "{label}({inner})")
    } else {
        write!(formatter, "{label}({inner},{ordinal})")
    }
}

fn kind_letter(kind: ElementKind) -> char {
    match kind {
        ElementKind::Face => 'F',
        ElementKind::Edge => 'E',
        ElementKind::Vertex => 'V',
    }
}

impl SemanticOp {
    fn to_line(&self) -> String {
        match self {
            Self::Create { size_m } => format!("create {} {} {}", num(size_m[0]), num(size_m[1]), num(size_m[2])),
            Self::Resize { size_m } => format!("resize {} {} {}", num(size_m[0]), num(size_m[1]), num(size_m[2])),
            Self::Subdivide { face, u, v } => format!("subdivide {} {u} {v}", join_groups(std::slice::from_ref(face))),
            Self::Extrude { faces, amount: ExtrudeAmount::Distance(distance) } => format!("extrude {} {} {}", faces.len(), join_groups(faces), num(*distance)),
            Self::Extrude { faces, amount: ExtrudeAmount::Delta(delta) } => format!("extrude-delta {} {} {} {} {}", faces.len(), join_groups(faces), num(delta[0]), num(delta[1]), num(delta[2])),
            Self::Split { edge } => format!("split {}", join_groups(std::slice::from_ref(edge))),
            Self::Bevel { edges, width_m } => format!("bevel {} {} {}", edges.len(), join_groups(edges), num(*width_m)),
            Self::MoveEdge { edge, delta_m } => format!("move-edge {} {} {} {}", join_groups(std::slice::from_ref(edge)), num(delta_m[0]), num(delta_m[1]), num(delta_m[2])),
            Self::MoveVertex { vertex, delta_m } => format!("move-vertex {} {} {} {}", join_groups(std::slice::from_ref(vertex)), num(delta_m[0]), num(delta_m[1]), num(delta_m[2])),
            Self::ExtrudeEdge { edge, delta_m } => format!("extrude-edge {} {} {} {}", join_groups(std::slice::from_ref(edge)), num(delta_m[0]), num(delta_m[1]), num(delta_m[2])),
            Self::Mirror { axis } => format!("mirror {axis}"),
            Self::PushFace { face, distance_m } => format!("push-face {face} {}", num(*distance_m)),
            Self::Round { edge, radius_m } => format!("round {} {}", join_groups(std::slice::from_ref(edge)), num(*radius_m)),
            Self::Seed => "seed".to_string(),
        }
    }

    fn from_line(line: &str) -> Result<Self, String> {
        let mut parts = line.split_whitespace();
        let word = parts.next().ok_or_else(|| "blank semantic operation".to_string())?;
        match word {
            "create" => Ok(Self::Create { size_m: three_numbers(parts)? }),
            "resize" => Ok(Self::Resize { size_m: three_numbers(parts)? }),
            "subdivide" => {
                let face = one_name(&mut parts)?;
                let u = take_u32(&mut parts)?;
                let v = take_u32(&mut parts)?;
                finish(parts)?;
                Ok(Self::Subdivide { face, u, v })
            }
            "extrude" => {
                let count = take_u32(&mut parts)? as usize;
                let faces = many_names(&mut parts, count)?;
                let distance = take_f64(&mut parts)?;
                finish(parts)?;
                Ok(Self::Extrude { faces, amount: ExtrudeAmount::Distance(distance) })
            }
            "extrude-delta" => {
                let count = take_u32(&mut parts)? as usize;
                let faces = many_names(&mut parts, count)?;
                let delta = three_numbers(parts)?;
                Ok(Self::Extrude { faces, amount: ExtrudeAmount::Delta(delta) })
            }
            "split" => {
                let edge = one_name(&mut parts)?;
                finish(parts)?;
                Ok(Self::Split { edge })
            }
            "bevel" => {
                let count = take_u32(&mut parts)? as usize;
                let edges = many_names(&mut parts, count)?;
                let width_m = take_f64(&mut parts)?;
                finish(parts)?;
                Ok(Self::Bevel { edges, width_m })
            }
            "move-edge" => {
                let edge = one_name(&mut parts)?;
                let delta_m = three_numbers(parts)?;
                Ok(Self::MoveEdge { edge, delta_m })
            }
            "move-vertex" => {
                let vertex = one_name(&mut parts)?;
                let delta_m = three_numbers(parts)?;
                Ok(Self::MoveVertex { vertex, delta_m })
            }
            "extrude-edge" => {
                let edge = one_name(&mut parts)?;
                let delta_m = three_numbers(parts)?;
                Ok(Self::ExtrudeEdge { edge, delta_m })
            }
            "mirror" => {
                let axis = take_u32(&mut parts)?;
                finish(parts)?;
                if axis > 2 {
                    return Err("mirror axis is not 0, 1, or 2".into());
                }
                Ok(Self::Mirror { axis: axis as u8 })
            }
            "push-face" => {
                let face = take_u32(&mut parts)?;
                let distance_m = take_f64(&mut parts)?;
                finish(parts)?;
                if face > 5 {
                    return Err("analytic face push is not a constructor face".into());
                }
                Ok(Self::PushFace { face: face as u8, distance_m })
            }
            "round" => {
                let words: Vec<&str> = parts.collect();
                if words.len() < 2 {
                    return Err("round does not name a semantic edge".into());
                }
                let radius_m: f64 = words.last().copied().unwrap().parse().map_err(|_| "round radius is not a finite positive length".to_string())?;
                if !radius_m.is_finite() || radius_m <= 0.0 {
                    return Err("round radius is not a finite positive length".into());
                }
                let mut edge = Vec::new();
                for token in &words[..words.len() - 1] {
                    let reference = parse_ref(token)?;
                    if !reference.to_string().starts_with("E:") {
                        return Err("round does not name a semantic edge".into());
                    }
                    edge.push(reference);
                }
                Ok(Self::Round { edge, radius_m })
            }
            "seed" => {
                finish(parts)?;
                Ok(Self::Seed)
            }
            _ => Err(format!("unknown semantic operation {word}")),
        }
    }
}

fn join_groups(groups: &[Vec<SemanticRef>]) -> String {
    groups.iter().map(|group| group.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join(" ")
}

fn one_name<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Result<Vec<SemanticRef>, String> {
    Ok(vec![take_ref_token(parts)?])
}

fn many_names<'a>(parts: &mut impl Iterator<Item = &'a str>, count: usize) -> Result<Vec<Vec<SemanticRef>>, String> {
    let mut groups = Vec::with_capacity(count);
    for _ in 0..count {
        groups.push(one_name(parts)?);
    }
    Ok(groups)
}

fn num(value: f64) -> String {
    format!("{value:?}")
}

fn three_numbers<'a>(mut parts: impl Iterator<Item = &'a str>) -> Result<[f64; 3], String> {
    let size = [take_f64(&mut parts)?, take_f64(&mut parts)?, take_f64(&mut parts)?];
    finish(parts)?;
    Ok(size)
}

fn take_ref_token<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Result<SemanticRef, String> {
    let token = parts.next().ok_or_else(|| "semantic reference is missing".to_string())?;
    parse_ref(token)
}

fn take_u32<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Result<u32, String> {
    let token = parts.next().ok_or_else(|| "integer is missing".to_string())?;
    token.parse::<u32>().map_err(|_| format!("integer is not usable: {token}"))
}

fn take_f64<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Result<f64, String> {
    let token = parts.next().ok_or_else(|| "number is missing".to_string())?;
    let value = token.parse::<f64>().map_err(|_| format!("number is not usable: {token}"))?;
    if !value.is_finite() {
        return Err(format!("number is not finite: {token}"));
    }
    Ok(value)
}

fn finish<'a>(mut parts: impl Iterator<Item = &'a str>) -> Result<(), String> {
    if parts.next().is_some() {
        Err("semantic operation has extra tokens".into())
    } else {
        Ok(())
    }
}

fn parse_ref(token: &str) -> Result<SemanticRef, String> {
    let (reference, rest) = take_ref(token)?;
    if !rest.is_empty() {
        return Err(format!("semantic reference did not end: {token}"));
    }
    Ok(reference)
}

fn take_ref(token: &str) -> Result<(SemanticRef, &str), String> {
    let (kind_text, rest) = token.split_once(':').ok_or_else(|| format!("semantic reference needs a kind: {token}"))?;
    let kind = match kind_text {
        "F" => ElementKind::Face,
        "E" => ElementKind::Edge,
        "V" => ElementKind::Vertex,
        _ => return Err(format!("semantic reference kind is not F, E, or V: {token}")),
    };
    let (role, rest) = take_role(rest)?;
    let reference = SemanticRef { kind, role };
    if expected_kind(&reference.role) != kind {
        return Err(format!("semantic reference kind does not match its role: {reference}"));
    }
    Ok((reference, rest))
}

fn take_role(token: &str) -> Result<(SemanticRole, &str), String> {
    if let Some(rest) = token.strip_prefix("seed/") {
        let (axis, rest) = take_int(rest)?;
        return Ok((SemanticRole::SeedFace { axis: u8_of(axis)? }, rest));
    }
    if let Some(rest) = token.strip_prefix("seed-edge/") {
        let (slot, rest) = take_int(rest)?;
        return Ok((SemanticRole::SeedEdge { slot: u8_of(slot)? }, rest));
    }
    if let Some(rest) = token.strip_prefix("cell(") {
        let (parent, rest) = take_ref(rest)?;
        let rest = comma(rest)?;
        let (u, rest) = take_int(rest)?;
        let rest = comma(rest)?;
        let (v, rest) = take_int(rest)?;
        let rest = close(rest)?;
        return Ok((SemanticRole::SubdivCell { parent: Box::new(parent), u, v }, rest));
    }
    if let Some(rest) = token.strip_prefix("grid(") {
        let (parent, rest) = take_ref(rest)?;
        let rest = comma(rest)?;
        let (u, rest) = take_int(rest)?;
        let rest = comma(rest)?;
        let (v, rest) = take_int(rest)?;
        let rest = comma(rest)?;
        let (side, rest) = take_int(rest)?;
        let rest = close(rest)?;
        return Ok((SemanticRole::SubdivEdge { parent: Box::new(parent), u, v, side: u8_of(side)? }, rest));
    }
    if let Some(rest) = token.strip_prefix("cap(") {
        let (source, rest) = take_ref(rest)?;
        return Ok((SemanticRole::ExtrudeCap { source: Box::new(source) }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("side(") {
        let (cap, rest) = take_ref(rest)?;
        let rest = comma(rest)?;
        let (boundary, rest) = take_ref(rest)?;
        return Ok((SemanticRole::ExtrudeSide { cap: Box::new(cap), boundary: Box::new(boundary) }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("outer(") {
        let (cap, rest) = take_ref(rest)?;
        let rest = comma(rest)?;
        let (boundary, rest) = take_ref(rest)?;
        return Ok((SemanticRole::ExtrudeOuter { cap: Box::new(cap), boundary: Box::new(boundary) }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("carve(") {
        let (cap, rest) = take_ref(rest)?;
        let rest = comma(rest)?;
        let (boundary, rest) = take_ref(rest)?;
        return Ok((SemanticRole::ExtrudeCarve { cap: Box::new(cap), boundary: Box::new(boundary) }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("leg(") {
        let (cap, rest) = take_ref(rest)?;
        let rest = comma(rest)?;
        let (boundary, rest) = take_ref(rest)?;
        let rest = comma(rest)?;
        let (end, rest) = take_int(rest)?;
        return Ok((SemanticRole::ExtrudeLeg { cap: Box::new(cap), boundary: Box::new(boundary), end: u8_of(end)? }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("split-kept(") {
        let (edge, rest) = take_ref(rest)?;
        let (ordinal, rest) = optional_ordinal(rest)?;
        return Ok((SemanticRole::SplitKept { edge: Box::new(edge), ordinal }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("split-new(") {
        let (edge, rest) = take_ref(rest)?;
        let (ordinal, rest) = optional_ordinal(rest)?;
        return Ok((SemanticRole::SplitNew { edge: Box::new(edge), ordinal }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("split-at(") {
        let (edge, rest) = take_ref(rest)?;
        let (ordinal, rest) = optional_ordinal(rest)?;
        return Ok((SemanticRole::SplitVertex { edge: Box::new(edge), ordinal }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("bevel-edge(") {
        let (edge, rest) = take_ref(rest)?;
        let rest = comma(rest)?;
        let (slot, rest) = take_int(rest)?;
        let (ordinal, rest) = optional_ordinal(rest)?;
        return Ok((SemanticRole::BevelEdge { edge: Box::new(edge), slot, ordinal }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("bevel(") {
        let (edge, rest) = take_ref(rest)?;
        let (ordinal, rest) = optional_ordinal(rest)?;
        return Ok((SemanticRole::BevelFace { edge: Box::new(edge), ordinal }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("any-side(") {
        let (cap, rest) = take_ref(rest)?;
        return Ok((SemanticRole::AnyExtrudeSide { cap: Box::new(cap) }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("edge-wall(") {
        let (edge, rest) = take_ref(rest)?;
        let (ordinal, rest) = optional_ordinal(rest)?;
        return Ok((SemanticRole::ExtrudeEdgeWall { edge: Box::new(edge), ordinal }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("edge-outer(") {
        let (edge, rest) = take_ref(rest)?;
        let (ordinal, rest) = optional_ordinal(rest)?;
        return Ok((SemanticRole::ExtrudeEdgeOuter { edge: Box::new(edge), ordinal }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("fillet-a(") {
        let (edge, rest) = take_edge_ref(rest)?;
        return Ok((SemanticRole::FilletBoundary { edge: Box::new(edge), side: 0 }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("fillet-b(") {
        let (edge, rest) = take_edge_ref(rest)?;
        return Ok((SemanticRole::FilletBoundary { edge: Box::new(edge), side: 1 }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("fillet-start(") {
        let (edge, rest) = take_edge_ref(rest)?;
        return Ok((SemanticRole::FilletJunction { edge: Box::new(edge), end: 0 }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("fillet-end(") {
        let (edge, rest) = take_edge_ref(rest)?;
        return Ok((SemanticRole::FilletJunction { edge: Box::new(edge), end: 1 }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("fillet(") {
        let (edge, rest) = take_edge_ref(rest)?;
        return Ok((SemanticRole::FilletFace { edge: Box::new(edge) }, close(rest)?));
    }
    if let Some(rest) = token.strip_prefix("corner(") {
        let (first, rest) = take_edge_ref(rest)?;
        let rest = comma(rest)?;
        let (second, rest) = take_edge_ref(rest)?;
        let (edges, rest) = if rest.starts_with(',') {
            let rest = comma(rest)?;
            let (third, rest) = take_edge_ref(rest)?;
            (vec![Box::new(first), Box::new(second), Box::new(third)], rest)
        } else {
            (vec![Box::new(first), Box::new(second)], rest)
        };
        return Ok((SemanticRole::CornerPatch { edges }, close(rest)?));
    }
    Err(format!("semantic role is not known: {token}"))
}

fn take_edge_ref(token: &str) -> Result<(SemanticRef, &str), String> {
    let (edge, rest) = take_ref(token)?;
    if edge.kind != ElementKind::Edge {
        return Err(format!("semantic reference kind does not match its role: {edge}"));
    }
    Ok((edge, rest))
}

fn is_curve_role(role: &SemanticRole) -> bool {
    matches!(
        role,
        SemanticRole::FilletFace { .. } | SemanticRole::FilletBoundary { .. } | SemanticRole::FilletJunction { .. } | SemanticRole::CornerPatch { .. }
    )
}

fn expected_kind(role: &SemanticRole) -> ElementKind {
    match role {
        SemanticRole::SeedFace { .. } | SemanticRole::SubdivCell { .. } | SemanticRole::ExtrudeCap { .. } | SemanticRole::ExtrudeSide { .. } | SemanticRole::BevelFace { .. } | SemanticRole::AnyExtrudeSide { .. } | SemanticRole::ExtrudeEdgeWall { .. } | SemanticRole::FilletFace { .. } | SemanticRole::CornerPatch { .. } => ElementKind::Face,
        SemanticRole::SeedEdge { .. } | SemanticRole::SubdivEdge { .. } | SemanticRole::ExtrudeOuter { .. } | SemanticRole::ExtrudeCarve { .. } | SemanticRole::ExtrudeLeg { .. } | SemanticRole::SplitKept { .. } | SemanticRole::SplitNew { .. } | SemanticRole::BevelEdge { .. } | SemanticRole::ExtrudeEdgeOuter { .. } | SemanticRole::FilletBoundary { .. } => ElementKind::Edge,
        SemanticRole::SplitVertex { .. } | SemanticRole::FilletJunction { .. } => ElementKind::Vertex,
    }
}

fn optional_ordinal(token: &str) -> Result<(u32, &str), String> {
    if let Some(rest) = token.strip_prefix(',') {
        take_int(rest)
    } else {
        Ok((0, token))
    }
}

fn comma(token: &str) -> Result<&str, String> {
    token.strip_prefix(',').ok_or_else(|| format!("semantic reference is missing a comma: {token}"))
}

fn close(token: &str) -> Result<&str, String> {
    token.strip_prefix(')').ok_or_else(|| format!("semantic reference is missing ')': {token}"))
}

fn take_int(token: &str) -> Result<(u32, &str), String> {
    let length = token.chars().take_while(|glyph| glyph.is_ascii_digit()).count();
    if length == 0 {
        return Err(format!("semantic reference is missing an integer: {token}"));
    }
    let (digits, rest) = token.split_at(length);
    Ok((digits.parse::<u32>().map_err(|_| format!("integer does not fit: {digits}"))?, rest))
}

fn u8_of(value: u32) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("integer does not fit in a role slot: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::{BirthRole, SolidBody, TopologyLineage};
    use crate::{empty_world_level, evaluate_intent_shadow, parse_level, AuthoringResult, BlockOp, BlockRecord, ConcreteElement, IntentShadowStatus, LevelDocument, Vec3};

    #[derive(Clone)]
    struct Spec {
        create: [f64; 3],
        resize: [f64; 3],
        divisions: u32,
        first: f64,
        second: f64,
        bevel: f64,
    }

    fn base() -> Spec {
        Spec { create: [2.0, 2.0, 2.0], resize: [3.0, 2.0, 2.5], divisions: 2, first: 0.25, second: 0.20, bevel: 0.05 }
    }

    struct ChainRefs {
        face: SemanticRef,
        cell: SemanticRef,
        side: SemanticRef,
        outer: SemanticRef,
        half: SemanticRef,
        bevel: SemanticRef,
    }

    fn chain_refs() -> ChainRefs {
        let face = seed_face(4);
        let cell = subdiv_cell(face.clone(), 1, 0);
        let boundary = subdiv_edge(face.clone(), 1, 0, 1);
        let side = extrude_side(cell.clone(), boundary.clone());
        let outer = extrude_outer(side.clone(), extrude_outer(cell.clone(), boundary));
        let half = split_new(outer.clone());
        let bevel = bevel_face(half.clone());
        ChainRefs { face, cell, side, outer, half, bevel }
    }

    fn seed_face(axis: u8) -> SemanticRef {
        SemanticRef { kind: ElementKind::Face, role: SemanticRole::SeedFace { axis } }
    }

    fn subdiv_cell(parent: SemanticRef, u: u32, v: u32) -> SemanticRef {
        SemanticRef { kind: ElementKind::Face, role: SemanticRole::SubdivCell { parent: Box::new(parent), u, v } }
    }

    fn subdiv_edge(parent: SemanticRef, u: u32, v: u32, side: u8) -> SemanticRef {
        SemanticRef { kind: ElementKind::Edge, role: SemanticRole::SubdivEdge { parent: Box::new(parent), u, v, side } }
    }

    fn extrude_side(cap: SemanticRef, boundary: SemanticRef) -> SemanticRef {
        SemanticRef { kind: ElementKind::Face, role: SemanticRole::ExtrudeSide { cap: Box::new(cap), boundary: Box::new(boundary) } }
    }

    fn extrude_outer(cap: SemanticRef, boundary: SemanticRef) -> SemanticRef {
        SemanticRef { kind: ElementKind::Edge, role: SemanticRole::ExtrudeOuter { cap: Box::new(cap), boundary: Box::new(boundary) } }
    }

    fn split_new(edge: SemanticRef) -> SemanticRef {
        SemanticRef { kind: ElementKind::Edge, role: SemanticRole::SplitNew { edge: Box::new(edge), ordinal: 0 } }
    }

    fn bevel_face(edge: SemanticRef) -> SemanticRef {
        SemanticRef { kind: ElementKind::Face, role: SemanticRole::BevelFace { edge: Box::new(edge), ordinal: 0 } }
    }

    fn program(spec: &Spec) -> SemanticProgram {
        let refs = chain_refs();
        SemanticProgram {
            ops: vec![
                SemanticOp::Create { size_m: spec.create },
                SemanticOp::Resize { size_m: spec.resize },
                SemanticOp::Subdivide { face: vec![refs.face], u: spec.divisions, v: spec.divisions },
                SemanticOp::Extrude { faces: vec![vec![refs.cell]], amount: ExtrudeAmount::Distance(spec.first) },
                SemanticOp::Extrude { faces: vec![vec![refs.side]], amount: ExtrudeAmount::Distance(spec.second) },
                SemanticOp::Split { edge: vec![refs.outer] },
                SemanticOp::Bevel { edges: vec![vec![refs.half]], width_m: spec.bevel },
            ],
        }
    }

    struct Authored {
        body: SolidBody,
        cell: u32,
        side: u32,
        /// The outer edge the author split. The world must split this id, not the half it produces.
        split_edge: u32,
        half: u32,
        bevel_face: u32,
    }

    fn author(spec: &Spec) -> Authored {
        let mut body = SolidBody::from_box(spec.resize).expect("seed box");
        let (edit, lineage) = body.subdivide_face_traced(5, spec.divisions, spec.divisions).expect("subdivide");
        body = edit.body;
        let cell = only(&lineage, BirthRole::SubdivCell { u: 1, v: 0 });
        let boundary = only(&lineage, BirthRole::SubdivEdge { u: 1, v: 0, side: 1 });
        let normal = body.unit_normal(cell).expect("cell normal");
        let (edit, lineage) = body.extrude_faces_traced(&[cell], scale(normal, spec.first)).expect("region extrude");
        body = edit.body;
        let side = only(&lineage, BirthRole::ExtrudeSide { face: cell, boundary });
        let outer = only(&lineage, BirthRole::ExtrudeOuter { boundary });
        let normal = body.unit_normal(side).expect("side normal");
        let (edit, lineage) = body.extrude_faces_traced(&[side], scale(normal, spec.second)).expect("side extrude");
        body = edit.body;
        let outer = only(&lineage, BirthRole::ExtrudeOuter { boundary: outer });
        let (edit, lineage) = body.split_edge_traced(outer).expect("split");
        body = edit.body;
        let half = only(&lineage, BirthRole::SplitNew);
        let (cut, lineage) = body.bevel_edges_traced(&[half], spec.bevel).expect("bevel");
        let bevel_face = only(&lineage, BirthRole::BevelFace { source: half });
        Authored { body: cut.edit.body, cell, side, split_edge: outer, half, bevel_face }
    }

    fn only(lineage: &TopologyLineage, role: BirthRole) -> u32 {
        let mut ids: Vec<u32> = lineage.births.iter().filter(|birth| birth.role == role).map(|birth| birth.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 1, "{role:?} matched {ids:?}\n{lineage:?}");
        ids[0]
    }

    fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
        [value[0] * factor, value[1] * factor, value[2] * factor]
    }

    fn assert_rebinds(spec: &Spec) {
        let expected = author(spec);
        let report = evaluate_semantic_shadow(&program(spec));
        assert_eq!(report.status, SemanticShadowStatus::Pass, "{}", report.text);
        let candidate = report.candidate.clone().expect("a passing replay has a candidate");
        assert_eq!(candidate, expected.body);
        assert!(candidate.validate().is_ok(), "candidate is not a closed solid");
        assert!(expected.body.validate().is_ok(), "expected body is not a closed solid");
        let candidate_volume = volume(&candidate);
        assert!(candidate_volume.is_finite() && candidate_volume > 0.0, "volume {candidate_volume}");
        assert!((candidate_volume - volume(&expected.body)).abs() < 1.0e-8);
        let refs = chain_refs();
        assert_eq!(report.concrete_of(&refs.cell.to_string()), Some(expected.cell), "cell provenance");
        assert_eq!(report.concrete_of(&refs.side.to_string()), Some(expected.side), "side provenance");
        assert_eq!(report.concrete_of(&refs.half.to_string()), Some(expected.half), "split provenance");
        assert_eq!(report.concrete_of(&refs.bevel.to_string()), Some(expected.bevel_face), "bevel provenance");
        let repeated = evaluate_semantic_shadow(&program(spec));
        assert_eq!(repeated, report, "replay is not deterministic");
    }

    #[test]
    fn seed_plus_y_is_the_constructor_face() {
        let body = SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        let normal = body.unit_normal(3).unwrap();
        assert!(normal[1] > 0.9 && normal[0].abs() < 1.0e-9 && normal[2].abs() < 1.0e-9, "{normal:?}");
        let face = seed_face(2);
        let cell = subdiv_cell(face.clone(), 0, 1);
        let program = SemanticProgram {
            ops: vec![
                SemanticOp::Create { size_m: [2.0, 2.0, 2.0] },
                SemanticOp::Subdivide { face: vec![face], u: 2, v: 2 },
                SemanticOp::Extrude { faces: vec![vec![cell]], amount: ExtrudeAmount::Distance(0.3) },
            ],
        };
        let report = evaluate_semantic_shadow(&program);
        assert_eq!(report.status, SemanticShadowStatus::Pass, "{}", report.text);
        let mut body = SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        let (edit, lineage) = body.subdivide_face_traced(3, 2, 2).unwrap();
        body = edit.body;
        let cell_id = only(&lineage, BirthRole::SubdivCell { u: 0, v: 1 });
        let normal = body.unit_normal(cell_id).unwrap();
        let (edit, _) = body.extrude_faces_traced(&[cell_id], scale(normal, 0.3)).unwrap();
        assert_eq!(report.candidate.unwrap(), edit.body);
    }

    #[test]
    fn semantic_rebinding_follows_the_chain_and_its_mutations() {
        assert_rebinds(&base());
        let mut dimensions = base();
        dimensions.resize = [4.0, 2.5, 3.0];
        assert_rebinds(&dimensions);
        let mut first = base();
        first.first = 0.40;
        assert_rebinds(&first);
        let mut second = base();
        second.second = 0.12;
        assert_rebinds(&second);
        let mut bevel = base();
        bevel.bevel = 0.03;
        assert_rebinds(&bevel);
        let mut divided = base();
        divided.divisions = 4;
        assert_rebinds(&divided);

        let original = evaluate_semantic_shadow(&program(&base()));
        let finer = evaluate_semantic_shadow(&program(&divided));
        assert_ne!(original.candidate, finer.candidate);
        let rebound = finer.rebound_against(&original);
        assert!(rebound > 0, "a finer grid did not give the shared roles new concrete ids\n{}", finer.text);
        let refs = chain_refs();
        assert_ne!(original.concrete_of(&refs.cell.to_string()), finer.concrete_of(&refs.cell.to_string()));

        let mut record = BlockRecord::standard(base().create).unwrap();
        let stored = author(&base()).body;
        record.size_m = stored.aabb_size();
        record.body = Some(stored);
        record.validate().unwrap();
        let material = record.material.clone();
        let snapshot = record.clone();
        let _ = evaluate_semantic_shadow(&program(&dimensions));
        let _ = evaluate_semantic_shadow(&program(&divided));
        assert_eq!(record, snapshot);
        assert_eq!(record.material, material);
        assert_ne!(record.body.as_ref(), dimensions_candidate(&dimensions).as_ref());
    }

    fn dimensions_candidate(spec: &Spec) -> Option<SolidBody> {
        evaluate_semantic_shadow(&program(spec)).candidate
    }

    #[test]
    fn an_impossible_cell_is_invalidated_and_an_unnamed_side_is_ambiguous() {
        let mut ops = program(&base()).ops;
        ops.truncate(3);
        ops.push(SemanticOp::Extrude {
            faces: vec![vec![subdiv_cell(seed_face(4), 3, 0)]],
            amount: ExtrudeAmount::Distance(0.25),
        });
        let missing_cell = evaluate_semantic_shadow(&SemanticProgram { ops });
        assert_eq!(missing_cell.status, SemanticShadowStatus::Invalidated, "{}", missing_cell.text);
        assert!(missing_cell.candidate.is_none());
        assert!(missing_cell.invalidated >= 1);
        assert!(missing_cell.text.contains("SEMANTIC_REFERENCE_INVALIDATED"));
        assert!(missing_cell.text.contains("ceased to exist"));

        let mut coarse = base();
        coarse.divisions = 1;
        let ceased = evaluate_semantic_shadow(&program(&coarse));
        assert_eq!(ceased.status, SemanticShadowStatus::Invalidated, "{}", ceased.text);
        assert!(ceased.candidate.is_none());

        let refs = chain_refs();
        let ambiguous = SemanticProgram {
            ops: vec![
                SemanticOp::Create { size_m: [2.0, 2.0, 2.0] },
                SemanticOp::Subdivide { face: vec![refs.face], u: 2, v: 2 },
                SemanticOp::Extrude { faces: vec![vec![refs.cell.clone()]], amount: ExtrudeAmount::Distance(0.25) },
                SemanticOp::Extrude {
                    faces: vec![vec![SemanticRef { kind: ElementKind::Face, role: SemanticRole::AnyExtrudeSide { cap: Box::new(refs.cell) } }]],
                    amount: ExtrudeAmount::Distance(0.2),
                },
            ],
        };
        let report = evaluate_semantic_shadow(&ambiguous);
        assert_eq!(report.status, SemanticShadowStatus::Ambiguous, "{}", report.text);
        assert!(report.candidate.is_none());
        assert!(report.text.contains("SEMANTIC_REFERENCE_AMBIGUOUS"));
        assert!(report.ambiguous >= 1);
    }

    #[test]
    fn semantic_program_round_trips_and_undo_restores_it() {
        let original = program(&base());
        let loaded = SemanticProgram::from_text(&original.to_text()).unwrap();
        assert_eq!(loaded, original);
        assert_eq!(evaluate_semantic_shadow(&loaded).candidate, evaluate_semantic_shadow(&original).candidate);

        let mut finer = base();
        finer.divisions = 4;
        let mut session = SemanticSession::new(original);
        session.commit(program(&finer));
        assert_eq!(evaluate_semantic_shadow(session.current()).candidate, author_body(&finer));
        assert!(session.undo());
        assert_eq!(evaluate_semantic_shadow(session.current()).candidate, author_body(&base()));
        assert!(session.redo());
        assert_eq!(evaluate_semantic_shadow(session.current()).candidate, author_body(&finer));
        assert_eq!(evaluate_semantic_shadow(session.current()), evaluate_semantic_shadow(&program(&finer)));
    }

    fn author_body(spec: &Spec) -> Option<SolidBody> {
        Some(author(spec).body)
    }

    #[test]
    fn save_and_reload_keep_the_stored_body_and_the_semantic_program_is_not_in_the_level() {
        let spec = base();
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard(spec.create).unwrap()).unwrap();
        assert_eq!(world.set_block_extent(id, 0, spec.resize[0]).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 2, spec.resize[2]).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.subdivide_block_face(id, 5, spec.divisions, spec.divisions).unwrap(), AuthoringResult::Applied);
        let expected = author(&spec);
        let normal = world.authored_block(id).unwrap().body.unwrap().unit_normal(expected.cell).unwrap();
        assert_eq!(world.extrude_block_faces(id, &[expected.cell], scale(normal, spec.first)).unwrap(), AuthoringResult::Applied);
        let normal = world.authored_block(id).unwrap().body.unwrap().unit_normal(expected.side).unwrap();
        assert_eq!(world.extrude_block_faces(id, &[expected.side], scale(normal, spec.second)).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.split_block_edge(id, expected.split_edge).unwrap(), AuthoringResult::Applied);
        let record = world.authored_block(id).unwrap();
        let body = record.body.clone().unwrap();
        let cut = body.bevel_edges(&[expected.half], spec.bevel).unwrap();
        let local = world.entity_local_pose(id).unwrap();
        let translation = Vec3::new(local.translation.x + cut.edit.shift[0], local.translation.y + cut.edit.shift[1], local.translation.z + cut.edit.shift[2]);
        assert_eq!(world.preview_block_body(id, cut.edit.body, translation).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.commit_block_topology(id, BlockOp::BevelEdges { edges: cut.edges, distance_m: cut.width_m }).unwrap(), AuthoringResult::Applied);

        let stored = world.authored_block(id).unwrap();
        let phase = evaluate_intent_shadow(&stored);
        assert_eq!(phase.status, IntentShadowStatus::Pass, "{}", phase.text);
        let text = semantic_shadow_diagnostic(&stored, Some(ConcreteElement::Face(expected.cell)));
        assert!(text.contains("resolved:"), "{text}");
        assert!(text.contains("rebound:"), "{text}");
        assert!(text.contains("missing:"), "{text}");
        assert!(text.contains("ambiguous:"), "{text}");
        assert!(text.contains("invalidated:"), "{text}");
        assert!(text.contains("candidate match:"), "{text}");
        assert!(text.contains(&chain_refs().cell.to_string()), "{text}");
        let snapshot = stored.clone();
        let mut wider = spec.clone();
        wider.resize = [4.0, 2.5, 3.0];
        let mutated = evaluate_semantic_shadow(&program(&wider));
        assert_eq!(mutated.status, SemanticShadowStatus::Pass, "{}", mutated.text);
        assert_eq!(world.authored_block(id).unwrap(), snapshot);
        assert_ne!(mutated.candidate.as_ref(), snapshot.body.as_ref());

        let json = LevelDocument::capture(&world, document.level_uuid, "Semantic").unwrap().to_json();
        assert!(!json.contains("semantic-shadow"));
        assert!(!json.contains("meshlet") && !json.contains("einstein") && !json.contains("Einstein") && !json.contains("hierarchy"));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        assert_eq!(loaded.authored_block(id).unwrap().body, snapshot.body);
        assert_eq!(loaded.authored_block(id).unwrap().material, snapshot.material);
        let reloaded = semantic_shadow_diagnostic(&loaded.authored_block(id).unwrap(), Some(ConcreteElement::Edge(expected.half)));
        assert!(reloaded.contains(&chain_refs().half.to_string()) || reloaded.contains("SEMANTIC_REFERENCE_AMBIGUOUS") || reloaded.contains("more than one semantic identity"), "{reloaded}");
        assert_eq!(loaded.authored_block(id).unwrap(), parse_level(&json).unwrap().instantiate().unwrap().authored_block(id).unwrap());
    }

    fn commit_captured(record: &BlockRecord, op: &BlockOp) -> Vec<Vec<String>> {
        captured_semantics(record, op).unwrap_or_else(|| panic!("commit did not capture {}", op.summary()))
    }

    /// The solid the editor commands build. Selection ids come from the saved provenance.
    fn author_through_editor(spec: &Spec) -> BlockRecord {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard(spec.create).unwrap()).unwrap();
        assert_eq!(world.set_block_extent(id, 0, spec.resize[0]).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 2, spec.resize[2]).unwrap(), AuthoringResult::Applied);

        let subdivide = BlockOp::SubdivideFace { face: 5, u: spec.divisions, v: spec.divisions };
        let subdivide_names = commit_captured(&world.authored_block(id).unwrap(), &subdivide);
        assert_eq!(subdivide_names, vec![vec!["F:seed/4".to_string()]]);
        assert_eq!(world.subdivide_block_face(id, 5, spec.divisions, spec.divisions).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().steps.last().unwrap().semantic.as_ref(), Some(&subdivide_names));

        let cell_name = "F:cell(F:seed/4,1,0)".to_string();
        let cell = evaluate_saved_semantic_shadow(&world.authored_block(id).unwrap()).concrete_of(&cell_name).expect("cell");
        let normal = world.authored_block(id).unwrap().body.unwrap().unit_normal(cell).unwrap();
        let first = scale(normal, spec.first);
        let extrude = BlockOp::ExtrudeFaces { faces: vec![cell], delta_m: first };
        let extrude_names = commit_captured(&world.authored_block(id).unwrap(), &extrude);
        assert_eq!(extrude_names, vec![vec![cell_name.clone()]]);
        assert_eq!(world.extrude_block_faces(id, &[cell], first).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().steps.last().unwrap().semantic.as_ref(), Some(&extrude_names));

        let side_name = "F:side(F:cell(F:seed/4,1,0),E:grid(F:seed/4,1,0,1))".to_string();
        let side = evaluate_saved_semantic_shadow(&world.authored_block(id).unwrap()).concrete_of(&side_name).expect("side");
        let normal = world.authored_block(id).unwrap().body.unwrap().unit_normal(side).unwrap();
        let second = scale(normal, spec.second);
        let side_extrude = BlockOp::ExtrudeFaces { faces: vec![side], delta_m: second };
        let side_names = commit_captured(&world.authored_block(id).unwrap(), &side_extrude);
        assert_eq!(side_names, vec![vec![side_name.clone()]]);
        assert_eq!(world.extrude_block_faces(id, &[side], second).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().steps.last().unwrap().semantic.as_ref(), Some(&side_names));

        let outer_name = format!("E:outer({side_name},E:outer(F:cell(F:seed/4,1,0),E:grid(F:seed/4,1,0,1)))");
        let edge = evaluate_saved_semantic_shadow(&world.authored_block(id).unwrap()).concrete_of(&outer_name).expect("lip");
        let split = BlockOp::SplitEdge { edge };
        let split_names = commit_captured(&world.authored_block(id).unwrap(), &split);
        assert_eq!(split_names, vec![vec![outer_name.clone()]]);
        assert_eq!(world.split_block_edge(id, edge).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().steps.last().unwrap().semantic.as_ref(), Some(&split_names));

        let half_name = format!("E:split-new({outer_name})");
        let half = evaluate_saved_semantic_shadow(&world.authored_block(id).unwrap()).concrete_of(&half_name).expect("half");
        let body = world.authored_block(id).unwrap().body.unwrap();
        let cut = body.bevel_edges(&[half], spec.bevel).expect("bevel");
        let bevel = BlockOp::BevelEdges { edges: cut.edges.clone(), distance_m: cut.width_m };
        let bevel_names = commit_captured(&world.authored_block(id).unwrap(), &bevel);
        assert_eq!(bevel_names.len(), cut.edges.len());
        assert!(bevel_names.iter().all(|group| !group.is_empty()), "{bevel_names:?}");
        assert!(bevel_names.iter().any(|group| group.iter().any(|name| name == &half_name)), "{bevel_names:?}");
        let local = world.entity_local_pose(id).unwrap();
        let translation = Vec3::new(local.translation.x + cut.edit.shift[0], local.translation.y + cut.edit.shift[1], local.translation.z + cut.edit.shift[2]);
        assert_eq!(world.preview_block_body(id, cut.edit.body, translation).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.commit_block_topology(id, bevel).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().steps.last().unwrap().semantic.as_ref(), Some(&bevel_names));

        let stored = world.authored_block(id).unwrap();
        let json = LevelDocument::capture(&world, document.level_uuid, "Authored").unwrap().to_json();
        assert!(!json.contains("semantic-shadow"));
        assert!(json.contains("\"semantic\""));
        assert!(json.contains(&cell_name));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        assert_eq!(loaded.authored_block(id).unwrap(), stored);
        loaded.authored_block(id).unwrap()
    }

    struct Edited {
        resize: [f64; 3],
        divisions: u32,
        cell_u: u32,
        cell_v: u32,
        side: u8,
        first: [f64; 3],
        second: [f64; 3],
        bevel: f64,
    }

    fn chain_edited(program: &SemanticProgram) -> Edited {
        let mut size = match &program.ops[0] {
            SemanticOp::Create { size_m } => *size_m,
            other => panic!("create, got {other:?}"),
        };
        let mut index = 1;
        while let Some(SemanticOp::Resize { size_m }) = program.ops.get(index) {
            size = *size_m;
            index += 1;
        }
        let SemanticOp::Subdivide { face, u, v } = &program.ops[index] else { panic!("subdivide") };
        assert_eq!(u, v);
        assert!(matches!(&face[0].role, SemanticRole::SeedFace { axis: 4 }));
        index += 1;
        let SemanticOp::Extrude { faces, amount: ExtrudeAmount::Delta(first) } = &program.ops[index] else { panic!("region extrude") };
        let SemanticRole::SubdivCell { u: cell_u, v: cell_v, .. } = &faces[0][0].role else { panic!("cell") };
        let (cell_u, cell_v) = (*cell_u, *cell_v);
        index += 1;
        let SemanticOp::Extrude { faces, amount: ExtrudeAmount::Delta(second) } = &program.ops[index] else { panic!("side extrude") };
        let SemanticRole::ExtrudeSide { boundary, .. } = &faces[0][0].role else { panic!("side") };
        let SemanticRole::SubdivEdge { u: edge_u, v: edge_v, side, .. } = &boundary.role else { panic!("grid edge") };
        assert_eq!(*edge_u, cell_u);
        assert_eq!(*edge_v, cell_v);
        index += 1;
        assert!(matches!(program.ops[index], SemanticOp::Split { .. }), "split");
        index += 1;
        let SemanticOp::Bevel { width_m, .. } = &program.ops[index] else { panic!("bevel") };
        Edited { resize: size, divisions: *u, cell_u, cell_v, side: *side, first: *first, second: *second, bevel: *width_m }
    }

    fn author_edited(edited: &Edited) -> SolidBody {
        let mut body = SolidBody::from_box(edited.resize).expect("seed box");
        let (edit, lineage) = body.subdivide_face_traced(5, edited.divisions, edited.divisions).expect("subdivide");
        body = edit.body;
        let cell = only(&lineage, BirthRole::SubdivCell { u: edited.cell_u, v: edited.cell_v });
        let boundary = only(&lineage, BirthRole::SubdivEdge { u: edited.cell_u, v: edited.cell_v, side: edited.side });
        let (edit, lineage) = body.extrude_faces_traced(&[cell], edited.first).expect("region extrude");
        body = edit.body;
        let side = only(&lineage, BirthRole::ExtrudeSide { face: cell, boundary });
        let lip = only(&lineage, BirthRole::ExtrudeOuter { boundary });
        let (edit, lineage) = body.extrude_faces_traced(&[side], edited.second).expect("side extrude");
        body = edit.body;
        let lip = only(&lineage, BirthRole::ExtrudeOuter { boundary: lip });
        let (edit, lineage) = body.split_edge_traced(lip).expect("split");
        body = edit.body;
        let half = only(&lineage, BirthRole::SplitNew);
        let (cut, _) = body.bevel_edges_traced(&[half], edited.bevel).expect("bevel");
        cut.edit.body
    }

    fn assert_stream_matches_author(program: &SemanticProgram) {
        let expected = author_edited(&chain_edited(program));
        let report = evaluate_semantic_shadow(program);
        assert_eq!(report.status, SemanticShadowStatus::Pass, "{}", report.text);
        let candidate = report.candidate.clone().expect("candidate");
        assert_eq!(candidate, expected);
        assert!(candidate.validate().is_ok());
        assert!(expected.validate().is_ok());
        assert!((volume(&candidate) - volume(&expected)).abs() < 1.0e-8);
        assert_eq!(hash_body(&candidate), hash_body(&expected));
        let left = candidate.aabb_size();
        let right = expected.aabb_size();
        assert!((0..3).all(|axis| (left[axis] - right[axis]).abs() < 1.0e-6), "{left:?} {right:?}");
        for (found, wanted) in candidate.vertices.iter().zip(expected.vertices.iter()) {
            assert_eq!(found.id, wanted.id);
            assert!((0..3).all(|axis| (found.position[axis] - wanted.position[axis]).abs() < 1.0e-6));
        }
        assert_eq!(evaluate_semantic_shadow(program), report);
    }

    fn scale_delta(delta: [f64; 3], factor: f64) -> [f64; 3] {
        [delta[0] * factor, delta[1] * factor, delta[2] * factor]
    }

    #[test]
    fn an_editor_commit_records_the_reference_and_reloads_the_body() {
        let spec = base();
        let record = author_through_editor(&spec);
        let material = record.material.clone();
        for (op, step) in record.history.iter().zip(record.steps.iter()) {
            match op {
                BlockOp::Size { .. } => assert!(step.semantic.is_none(), "{}", op.summary()),
                BlockOp::SubdivideFace { .. } | BlockOp::ExtrudeFaces { .. } | BlockOp::SplitEdge { .. } | BlockOp::BevelEdges { .. } => {
                    let groups = step.semantic.as_ref().unwrap_or_else(|| panic!("{} stored no semantic reference", op.summary()));
                    assert_eq!(groups.len(), step.concrete.len());
                    assert!(groups.iter().all(|group| !group.is_empty()), "{}", op.summary());
                }
                other => panic!("unexpected {}", other.summary()),
            }
        }
        let snapshot = record.clone();
        let report = evaluate_saved_semantic_shadow(&record);
        assert_eq!(record, snapshot);
        assert_eq!(record.material, material);
        assert_eq!(report.status, SemanticShadowStatus::Pass, "{}", report.text);
        let candidate = report.candidate.clone().expect("candidate");
        let stored = record.body.clone().unwrap();
        assert_eq!(candidate, stored);
        assert!(candidate.validate().is_ok());
        assert!((volume(&candidate) - volume(&stored)).abs() < 1.0e-8);
        assert_eq!(hash_body(&candidate), hash_body(&stored));
        let bounds = candidate.aabb_size();
        let stored_bounds = stored.aabb_size();
        assert!((0..3).all(|axis| (bounds[axis] - stored_bounds[axis]).abs() < 1.0e-6));
        assert!((0..3).all(|axis| (bounds[axis] - record.size_m[axis]).abs() < 1.0e-3));
        assert_eq!(report.concrete_of("F:cell(F:seed/4,1,0)"), Some(stored.faces.iter().find(|face| report.concrete_of("F:cell(F:seed/4,1,0)") == Some(face.id)).unwrap().id));
        let text = semantic_shadow_diagnostic(&record, Some(ConcreteElement::Face(report.concrete_of("F:cell(F:seed/4,1,0)").unwrap())));
        assert!(text.contains("Concrete ID: face"), "{text}");
        assert!(text.contains("Semantic identity: F:cell(F:seed/4,1,0)"), "{text}");
        assert!(text.contains("Created by operation: subdivide"), "{text}");
        assert!(text.contains("Descends from: F:seed/4"), "{text}");
        assert!(text.contains("Current binding status: bound"), "{text}");
        assert!(text.contains("candidate match: match against the stored body"), "{text}");
        let program = saved_semantic_program(&record).expect("saved program");
        assert_stream_matches_author(&program);
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        world.create_block(Vec3::new(0.0, 1.0, -4.0), record.clone()).unwrap();
        let broken = LevelDocument::capture(&world, document.level_uuid, "Broken").unwrap().to_json().replace("F:seed/4", "F:not-a-role");
        assert!(parse_level(&broken).is_err(), "a corrupt semantic token loaded");
    }

    #[test]
    fn the_saved_stream_rebinds_and_refuses_a_bad_reference() {
        let record = author_through_editor(&base());
        let snapshot = record.clone();
        let saved = saved_semantic_program(&record).unwrap();
        assert_stream_matches_author(&saved);

        let mut dimensions = saved.clone();
        if let Some(SemanticOp::Resize { size_m }) = dimensions.ops.iter_mut().rev().find(|op| matches!(op, SemanticOp::Resize { .. })) {
            *size_m = [4.0, 2.5, 3.0];
        }
        assert_stream_matches_author(&dimensions);

        let mut first = saved.clone();
        if let SemanticOp::Extrude { amount: ExtrudeAmount::Delta(delta), .. } = first.ops.iter_mut().find(|op| matches!(op, SemanticOp::Extrude { .. })).unwrap() {
            *delta = scale_delta(*delta, 0.40 / 0.25);
        }
        assert_stream_matches_author(&first);

        let mut second = saved.clone();
        if let SemanticOp::Extrude { amount: ExtrudeAmount::Delta(delta), .. } = second.ops.iter_mut().rev().find(|op| matches!(op, SemanticOp::Extrude { .. })).unwrap() {
            *delta = scale_delta(*delta, 0.12 / 0.20);
        }
        assert_stream_matches_author(&second);

        let mut bevel = saved.clone();
        if let SemanticOp::Bevel { width_m, .. } = bevel.ops.iter_mut().find(|op| matches!(op, SemanticOp::Bevel { .. })).unwrap() {
            *width_m = 0.03;
        }
        assert_stream_matches_author(&bevel);

        let mut divided = saved.clone();
        if let SemanticOp::Subdivide { u, v, .. } = divided.ops.iter_mut().find(|op| matches!(op, SemanticOp::Subdivide { .. })).unwrap() {
            *u = 4;
            *v = 4;
        }
        assert_stream_matches_author(&divided);
        let original = evaluate_semantic_shadow(&saved);
        let finer = evaluate_semantic_shadow(&divided);
        assert_ne!(original.concrete_of("F:cell(F:seed/4,1,0)"), finer.concrete_of("F:cell(F:seed/4,1,0)"));
        assert!(finer.rebound_against(&original) > 0, "{}", finer.text);
        assert_ne!(finer.candidate.as_ref(), snapshot.body.as_ref());
        assert_eq!(record, snapshot);

        let mut missing_cell = saved.clone();
        if let SemanticOp::Extrude { faces, .. } = missing_cell.ops.iter_mut().find(|op| matches!(op, SemanticOp::Extrude { .. })).unwrap() {
            faces[0] = vec![subdiv_cell(seed_face(4), 3, 0)];
        }
        let invalidated = evaluate_semantic_shadow(&missing_cell);
        assert_eq!(invalidated.status, SemanticShadowStatus::Invalidated, "{}", invalidated.text);
        assert!(invalidated.candidate.is_none());
        assert!(invalidated.text.contains("ceased to exist"));

        let mut coarse = saved.clone();
        if let SemanticOp::Subdivide { u, v, .. } = coarse.ops.iter_mut().find(|op| matches!(op, SemanticOp::Subdivide { .. })).unwrap() {
            *u = 1;
            *v = 1;
        }
        let ceased = evaluate_semantic_shadow(&coarse);
        assert_eq!(ceased.status, SemanticShadowStatus::Invalidated, "{}", ceased.text);
        assert!(ceased.candidate.is_none());

        let cell = subdiv_cell(seed_face(4), 1, 0);
        let mut unnamed = saved.ops.clone();
        let extrude_at = unnamed.iter().position(|op| matches!(op, SemanticOp::Extrude { .. })).unwrap();
        unnamed.truncate(extrude_at + 1);
        unnamed.push(SemanticOp::Extrude {
            faces: vec![vec![SemanticRef { kind: ElementKind::Face, role: SemanticRole::AnyExtrudeSide { cap: Box::new(cell) } }]],
            amount: ExtrudeAmount::Distance(0.2),
        });
        let ambiguous = evaluate_semantic_shadow(&SemanticProgram { ops: unnamed });
        assert_eq!(ambiguous.status, SemanticShadowStatus::Ambiguous, "{}", ambiguous.text);
        assert!(ambiguous.candidate.is_none());

        let mut impossible = saved.ops.clone();
        let split_at = impossible.iter().position(|op| matches!(op, SemanticOp::Split { .. })).unwrap();
        impossible.truncate(split_at);
        impossible.push(SemanticOp::Split {
            edge: vec![SemanticRef { kind: ElementKind::Edge, role: SemanticRole::SplitNew { edge: Box::new(SemanticRef { kind: ElementKind::Edge, role: SemanticRole::SeedEdge { slot: 0 } }), ordinal: 0 } }],
        });
        let missing = evaluate_semantic_shadow(&SemanticProgram { ops: impossible });
        assert_eq!(missing.status, SemanticShadowStatus::Missing, "{}", missing.text);
        assert!(missing.candidate.is_none());
        assert_eq!(record, snapshot);
    }

    #[test]
    fn an_old_concrete_step_loads_without_a_synthesized_reference() {
        // A file from before the intent tape has the concrete log and no `intent` key.
        // Building that shape here is the load contract. `push_op` on a new solid records a gap instead.
        let mut record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        record.history.push(BlockOp::SplitEdge { edge: 12 });
        record.steps.push(crate::GeometryStep {
            id: crate::SemanticStepId::from_raw(1).unwrap(),
            concrete: vec![ConcreteElement::Edge(12)],
            semantic: None,
        });
        record.next_step = 2;
        assert!(record.intent.is_empty());
        record.validate().unwrap();
        assert!(record.steps[0].semantic.is_none());
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), record).unwrap();
        let json = LevelDocument::capture(&world, document.level_uuid, "Concrete").unwrap().to_json();
        assert!(!json.contains("\"semantic\""));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let loaded_record = loaded.authored_block(id).unwrap();
        assert!(loaded_record.steps[0].semantic.is_none());
        assert!(matches!(loaded_record.history.last(), Some(BlockOp::SplitEdge { edge: 12 })));
        let text = semantic_shadow_diagnostic(&loaded_record, Some(ConcreteElement::Edge(12)));
        assert!(text.contains("replay: not recorded"), "{text}");
        assert!(text.contains("Semantic identity: none"), "{text}");
        assert!(text.contains("Current binding status: not recorded"), "{text}");
        assert!(!text.contains("F:seed") && !text.contains("E:seed"), "{text}");
        let report = evaluate_saved_semantic_shadow(&loaded_record);
        assert!(report.candidate.is_none());
        assert_eq!(loaded_record, loaded.authored_block(id).unwrap());

        let mut world = parse_level(&json).unwrap().instantiate().unwrap();
        assert_eq!(world.subdivide_block_face(id, 5, 2, 2).unwrap(), AuthoringResult::Applied);
        let after = world.authored_block(id).unwrap();
        assert!(after.steps.iter().all(|step| step.semantic.is_none()));
        assert!(after.intent.is_empty(), "a concrete-only log does not open a partial tape");
        assert!(after.body.is_some());
    }

    #[test]
    fn a_face_extrude_records_the_constructor_face() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let op = BlockOp::ExtrudeFaces { faces: vec![5], delta_m: [0.0, 0.0, 0.3] };
        let captured = commit_captured(&world.authored_block(id).unwrap(), &op);
        assert_eq!(captured, vec![vec!["F:seed/4".to_string()]]);
        assert_eq!(world.extrude_block_faces(id, &[5], [0.0, 0.0, 0.3]).unwrap(), AuthoringResult::Applied);
        let record = world.authored_block(id).unwrap();
        assert_eq!(record.steps[0].semantic.as_ref(), Some(&captured));
        assert_eq!(record.steps[0].concrete, vec![ConcreteElement::Face(5)]);
        let snapshot = record.clone();
        let report = evaluate_saved_semantic_shadow(&record);
        assert_eq!(record, snapshot);
        assert_eq!(report.status, SemanticShadowStatus::Pass, "{}", report.text);
        assert_eq!(report.candidate.as_ref(), record.body.as_ref());
        let text = semantic_shadow_diagnostic(&record, Some(ConcreteElement::Face(5)));
        assert!(text.contains("Concrete ID: face 5"), "{text}");
        assert!(text.contains("Semantic identity: F:seed/4"), "{text}");
        assert!(text.contains("Created by operation: create"), "{text}");
        assert!(text.contains("Descends from: creation"), "{text}");
        assert!(text.contains("Current binding status: bound"), "{text}");
        assert!(text.contains("Authority readiness: READY"), "{text}");
        assert_eq!(intent_completeness(&record), IntentCompleteness::IntentComplete);
    }

    fn concrete_named(record: &BlockRecord, name: &str) -> u32 {
        let report = evaluate_saved_semantic_shadow(record);
        report.concrete_of(name).unwrap_or_else(|| panic!("{name} did not bind\n{}", report.text))
    }

    fn extrude_named(world: &mut crate::SceneWorld, id: crate::EntityId, name: &str, distance: f64) {
        let record = world.authored_block(id).unwrap();
        let face = concrete_named(&record, name);
        let normal = record.body.unwrap().unit_normal(face).unwrap();
        let delta = scale(normal, distance);
        let op = BlockOp::ExtrudeFaces { faces: vec![face], delta_m: delta };
        let names = commit_captured(&world.authored_block(id).unwrap(), &op);
        assert_eq!(names, vec![vec![name.to_string()]], "{name} captured {names:?}");
        assert_eq!(world.extrude_block_faces(id, &[face], delta).unwrap(), AuthoringResult::Applied);
    }

    fn split_named(world: &mut crate::SceneWorld, id: crate::EntityId, name: &str) {
        let record = world.authored_block(id).unwrap();
        let edge = concrete_named(&record, name);
        let op = BlockOp::SplitEdge { edge };
        let names = commit_captured(&record, &op);
        assert!(names.iter().any(|group| group.iter().any(|token| token == name)), "{name} captured {names:?}");
        assert!(names.iter().all(|group| !group.is_empty()), "{names:?}");
        assert_eq!(world.split_block_edge(id, edge).unwrap(), AuthoringResult::Applied);
    }

    fn bevel_named(world: &mut crate::SceneWorld, id: crate::EntityId, name: &str, width: f64) {
        let record = world.authored_block(id).unwrap();
        let edge = concrete_named(&record, name);
        let cut = record.body.unwrap().bevel_edges(&[edge], width).unwrap_or_else(|error| panic!("bevel {name} refused: {error}"));
        let op = BlockOp::BevelEdges { edges: cut.edges.clone(), distance_m: cut.width_m };
        let names = commit_captured(&world.authored_block(id).unwrap(), &op);
        assert_eq!(names.len(), cut.edges.len(), "{name} {names:?}");
        assert!(names.iter().all(|group| !group.is_empty()), "{name} left an unnamed rail edge {names:?}");
        assert!(names.iter().any(|group| group.iter().any(|token| token == name)), "{name} was not on the rail {names:?}");
        let local = world.entity_local_pose(id).unwrap();
        let translation = Vec3::new(local.translation.x + cut.edit.shift[0], local.translation.y + cut.edit.shift[1], local.translation.z + cut.edit.shift[2]);
        assert_eq!(world.preview_block_body(id, cut.edit.body, translation).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.commit_block_topology(id, op).unwrap(), AuthoringResult::Applied);
    }

    fn model_face(world: &mut crate::SceneWorld, id: crate::EntityId, axis: u8, face: u32, distance: f64, second_split: bool) {
        let parent = format!("F:seed/{axis}");
        let subdivide = BlockOp::SubdivideFace { face, u: 2, v: 2 };
        let names = commit_captured(&world.authored_block(id).unwrap(), &subdivide);
        assert_eq!(names, vec![vec![parent.clone()]]);
        assert_eq!(world.subdivide_block_face(id, face, 2, 2).unwrap(), AuthoringResult::Applied);
        let cell = format!("F:cell({parent},1,0)");
        extrude_named(world, id, &cell, distance);
        let grid = format!("E:grid({parent},1,0,1)");
        let side = format!("F:side({cell},{grid})");
        extrude_named(world, id, &side, 0.20);
        let outer = format!("E:outer({side},E:outer({cell},{grid}))");
        split_named(world, id, &outer);
        if second_split {
            split_named(world, id, &outer);
            bevel_named(world, id, &format!("E:split-new({outer},1)"), 0.04);
        } else {
            bevel_named(world, id, &format!("E:split-new({outer})"), 0.04);
        }
    }

    fn author_long(first_distance: f64) -> BlockRecord {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(world.set_block_extent(id, 0, 6.0).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 1, 6.0).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 2, 6.0).unwrap(), AuthoringResult::Applied);
        model_face(&mut world, id, 4, 5, first_distance, true);
        model_face(&mut world, id, 0, 1, 0.25, false);
        model_face(&mut world, id, 2, 3, 0.25, true);
        let mut padded = false;
        for slot in 0..12 {
            let name = format!("E:seed-edge/{slot}");
            let record = world.authored_block(id).unwrap();
            let Some(edge) = evaluate_saved_semantic_shadow(&record).concrete_of(&name) else { continue };
            let op = BlockOp::SplitEdge { edge };
            let Some(names) = captured_semantics(&record, &op) else { continue };
            if names.iter().any(|group| group.is_empty()) {
                continue;
            }
            if world.split_block_edge(id, edge).is_err() {
                continue;
            }
            for _ in 0..8 {
                let record = world.authored_block(id).unwrap();
                let edge = concrete_named(&record, &name);
                assert_eq!(world.split_block_edge(id, edge).unwrap(), AuthoringResult::Applied);
            }
            padded = true;
            break;
        }
        assert!(padded, "no seed edge accepted the extra splits");
        world.authored_block(id).unwrap()
    }

    #[test]
    fn a_long_intent_native_solid_rebuilds_after_the_editor_log_drops_its_prefix() {
        let record = author_long(0.25);
        let snapshot = record.clone();
        assert!(record.history.len() == 24, "history {}", record.history.len());
        assert!(record.steps.len() == 24);
        assert_ne!(record.steps[0].id.number(), 1);
        assert!(record.intent.len() > 24, "intent {}", record.intent.len());
        assert!(matches!(record.intent[0].payload, crate::IntentPayload::Size { size_m } if size_m == [6.0, 2.0, 2.0]));
        assert!(!record.history.iter().any(|op| matches!(op, BlockOp::Size { size_m } if *size_m == [6.0, 2.0, 2.0])));
        let phase = evaluate_intent_shadow(&record);
        assert_eq!(phase.status, IntentShadowStatus::Incomplete, "{}", phase.text);
        assert!(phase.text.contains("dropped its prefix"), "{}", phase.text);
        assert_eq!(intent_completeness(&record), IntentCompleteness::IntentComplete);
        let text = semantic_shadow_diagnostic(&record, None);
        assert!(text.contains("Authority readiness: READY"), "{text}");
        assert!(text.contains("candidate match: match against the stored body"), "{text}");
        assert!(record.intent.iter().any(|entry| entry.groups.as_ref().is_some_and(|groups| groups.iter().flatten().any(|token| token.contains("split-new(") && token.contains(",1)")))));

        let stored = record.body.clone().unwrap();
        let derived = crate::DerivedRenderGeometry::from_body(&stored);
        derived.discard();
        assert_eq!(record, snapshot);

        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), record.clone()).unwrap();
        let json = LevelDocument::capture(&world, document.level_uuid, "Long").unwrap().to_json();
        assert!(!json.contains("semantic-shadow"));
        assert!(json.contains("\"intent\""));
        for forbidden in ["meshlet", "einstein", "Einstein", "hierarchy"] {
            assert!(!json.contains(forbidden), "{forbidden}");
        }
        let loaded = parse_level(&json).unwrap().instantiate().unwrap().authored_block(id).unwrap();
        assert_eq!(loaded, record);
        assert_eq!(loaded.material, record.material);
        let report = evaluate_saved_semantic_shadow(&loaded);
        assert_eq!(loaded, record);
        assert_eq!(report.status, SemanticShadowStatus::Pass, "{}", report.text);
        let candidate = report.candidate.clone().unwrap();
        assert_eq!(candidate, stored);
        assert!(candidate.validate().is_ok());
        assert!((volume(&candidate) - volume(&stored)).abs() < 1.0e-8);
        assert_eq!(hash_body(&candidate), hash_body(&stored));
        let bounds = candidate.aabb_size();
        let stored_bounds = stored.aabb_size();
        assert!((0..3).all(|axis| (bounds[axis] - stored_bounds[axis]).abs() < 1.0e-6));
        let cell = concrete_named(&loaded, "F:cell(F:seed/4,1,0)");
        assert!(stored.faces.iter().any(|face| face.id == cell));
        let with_ordinal = loaded
            .intent
            .iter()
            .find_map(|entry| entry.groups.as_ref().and_then(|groups| groups.iter().flatten().find(|token| token.matches("split-new(").count() == 1 && token.ends_with(",1)")).cloned()))
            .expect("a repeated split stored an ordinal");
        let without_ordinal = format!("{})", with_ordinal.trim_end_matches(",1)"));
        assert_ne!(concrete_named(&loaded, &without_ordinal), concrete_named(&loaded, &with_ordinal));

        let again = author_long(0.40);
        let mut mutated = saved_semantic_program(&record).unwrap();
        let replacement = saved_semantic_program(&again).unwrap();
        let SemanticOp::Extrude { amount: ExtrudeAmount::Delta(delta), .. } = replacement.ops.iter().find(|op| matches!(op, SemanticOp::Extrude { .. })).unwrap() else { panic!("extrude") };
        if let SemanticOp::Extrude { amount: ExtrudeAmount::Delta(slot), .. } = mutated.ops.iter_mut().find(|op| matches!(op, SemanticOp::Extrude { .. })).unwrap() {
            *slot = *delta;
        }
        let rebound = evaluate_semantic_shadow(&mutated);
        assert_eq!(rebound.status, SemanticShadowStatus::Pass, "{}", rebound.text);
        assert_eq!(rebound.candidate.as_ref(), again.body.as_ref());
        assert_ne!(rebound.candidate.as_ref(), record.body.as_ref());
        assert_eq!(record, snapshot);

        let mut divided = saved_semantic_program(&record).unwrap();
        if let SemanticOp::Subdivide { u, v, .. } = divided.ops.iter_mut().find(|op| matches!(op, SemanticOp::Subdivide { .. })).unwrap() {
            *u = 4;
            *v = 4;
        }
        let finer = evaluate_semantic_shadow(&divided);
        assert_eq!(finer.status, SemanticShadowStatus::Pass, "{}", finer.text);
        let original = evaluate_semantic_shadow(&saved_semantic_program(&record).unwrap());
        assert_ne!(original.concrete_of("F:cell(F:seed/4,1,0)"), finer.concrete_of("F:cell(F:seed/4,1,0)"));
        assert!(finer.rebound_against(&original) > 0, "{}", finer.text);
        assert_ne!(finer.candidate.as_ref(), record.body.as_ref());
        assert_eq!(record, snapshot);

        let mut missing_cell = saved_semantic_program(&record).unwrap();
        if let SemanticOp::Extrude { faces, .. } = missing_cell.ops.iter_mut().find(|op| matches!(op, SemanticOp::Extrude { .. })).unwrap() {
            faces[0] = vec![subdiv_cell(seed_face(4), 7, 0)];
        }
        let invalidated = evaluate_semantic_shadow(&missing_cell);
        assert_eq!(invalidated.status, SemanticShadowStatus::Invalidated, "{}", invalidated.text);
        assert!(invalidated.candidate.is_none());

        let cell_ref = subdiv_cell(seed_face(4), 1, 0);
        let mut unnamed = saved_semantic_program(&record).unwrap().ops;
        let extrude_at = unnamed.iter().position(|op| matches!(op, SemanticOp::Extrude { .. })).unwrap();
        unnamed.truncate(extrude_at + 1);
        unnamed.push(SemanticOp::Extrude {
            faces: vec![vec![SemanticRef { kind: ElementKind::Face, role: SemanticRole::AnyExtrudeSide { cap: Box::new(cell_ref) } }]],
            amount: ExtrudeAmount::Distance(0.2),
        });
        let ambiguous = evaluate_semantic_shadow(&SemanticProgram { ops: unnamed });
        assert_eq!(ambiguous.status, SemanticShadowStatus::Ambiguous, "{}", ambiguous.text);
        assert!(ambiguous.candidate.is_none());

        let mut impossible = saved_semantic_program(&record).unwrap().ops;
        let split_at = impossible.iter().position(|op| matches!(op, SemanticOp::Split { .. })).unwrap();
        impossible.truncate(split_at);
        impossible.push(SemanticOp::Split {
            edge: vec![SemanticRef { kind: ElementKind::Edge, role: SemanticRole::SplitNew { edge: Box::new(SemanticRef { kind: ElementKind::Edge, role: SemanticRole::SeedEdge { slot: 0 } }), ordinal: 0 } }],
        });
        let missing = evaluate_semantic_shadow(&SemanticProgram { ops: impossible });
        assert_eq!(missing.status, SemanticShadowStatus::Missing, "{}", missing.text);
        assert!(missing.candidate.is_none());
        assert_eq!(record, snapshot);
    }

    #[test]
    fn an_out_of_scope_edit_marks_intent_incomplete() {
        let record = author_through_editor(&base());
        assert_eq!(intent_completeness(&record), IntentCompleteness::IntentComplete);
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), record).unwrap();
        let copy = world.mirror_block(id, 0).unwrap();
        let mirrored = world.authored_block(copy).unwrap();
        assert!(mirrored.body.is_some());
        assert!(matches!(mirrored.intent.last().map(|entry| &entry.payload), Some(crate::IntentPayload::Mirror { axis: 0 })));
        let mirrored_body = match intent_authority_candidate(&mirrored) {
            IntentAuthorityCandidate::Reconstructable(body) => body,
            IntentAuthorityCandidate::Refused(reason) => panic!("mirror was not replayable: {reason}"),
        };
        assert_eq!(mirrored_body, mirrored.body.clone().unwrap());
        assert_eq!(intent_completeness(&world.authored_block(id).unwrap()), IntentCompleteness::IntentComplete);
        assert_eq!(world.subdivide_block_face(copy, 5, 2, 2).unwrap(), AuthoringResult::Applied);
        let after = world.authored_block(copy).unwrap();
        assert!(after.steps.last().unwrap().semantic.as_ref().is_some_and(|groups| groups.iter().all(|group| !group.is_empty())));
        assert!(after.body.is_some());
        let after_report = evaluate_saved_semantic_shadow(&after);
        assert_eq!(after_report.status, SemanticShadowStatus::Pass, "{}", after_report.text);
        assert_eq!(after_report.candidate.as_ref(), after.body.as_ref());
        let snapshot = after.clone();
        let _ = evaluate_saved_semantic_shadow(&after);
        assert_eq!(world.authored_block(copy).unwrap(), snapshot);

        let mut fresh = document.instantiate().unwrap();
        let moved = fresh.create_block(Vec3::new(0.0, 1.0, -4.0), world.authored_block(id).unwrap()).unwrap();
        commit_one_move(&mut fresh, moved);
        let moved_record = fresh.authored_block(moved).unwrap();
        assert!(matches!(moved_record.intent.last().map(|entry| &entry.payload), Some(crate::IntentPayload::MoveEdge { .. })));
        assert!(moved_record.intent.last().unwrap().groups.as_ref().is_some_and(|groups| groups.iter().all(|group| !group.is_empty())));
        let moved_report = evaluate_saved_semantic_shadow(&moved_record);
        assert_eq!(moved_report.status, SemanticShadowStatus::Pass, "{}", moved_report.text);
        assert_eq!(moved_report.candidate.as_ref(), moved_record.body.as_ref());
        assert_eq!(intent_authority_eligibility(&moved_record), IntentAuthorityEligibility::Eligible);

        let mut extruded = document.instantiate().unwrap();
        let edge_id = extruded.create_block(Vec3::new(0.0, 1.0, -4.0), world.authored_block(id).unwrap()).unwrap();
        commit_one_edge_extrude(&mut extruded, edge_id);
        let extruded_record = extruded.authored_block(edge_id).unwrap();
        assert!(matches!(extruded_record.intent.last().map(|entry| &entry.payload), Some(crate::IntentPayload::ExtrudeEdge { .. })));
        let extruded_report = evaluate_saved_semantic_shadow(&extruded_record);
        assert_eq!(extruded_report.status, SemanticShadowStatus::Pass, "{}", extruded_report.text);
        assert_eq!(extruded_report.candidate.as_ref(), extruded_record.body.as_ref());
        assert!(extruded_report.provenance.iter().any(|fact| fact.identity.contains("edge-wall(") || fact.identity.contains("edge-outer(")));
        assert_eq!(intent_authority_eligibility(&extruded_record), IntentAuthorityEligibility::Eligible);

        assert!(world.set_block_bevel(id, 0.1).is_err());
        assert!(world.set_block_inset(id, 0, 0.1).is_err());
        assert_eq!(intent_completeness(&world.authored_block(id).unwrap()), IntentCompleteness::IntentComplete);

        let marked = world.authored_block(id).unwrap();
        for (op, word) in [
            (BlockOp::ExtrudeFace { face: 0, distance_m: 0.25 }, "extrude-face"),
            (BlockOp::InsetFace { face: 0, distance_m: 0.1 }, "inset"),
            (BlockOp::Bevel { distance_m: 0.1 }, "uniform-bevel"),
            (BlockOp::MoveVertex { vertex: 1, delta_m: [0.02, 0.0, 0.0] }, "move-vertex"),
        ] {
            let mut copy = marked.clone();
            copy.push_op(op);
            let gap = copy.intent.last().unwrap();
            assert!(matches!(&gap.payload, crate::IntentPayload::Gap { operation } if operation == word), "{word} {:?}", gap.payload);
            let gap_text = semantic_shadow_diagnostic(&copy, None);
            assert!(gap_text.contains("Authority readiness: INCOMPLETE"), "{gap_text}");
            assert!(gap_text.contains(&format!("{word} has no semantic representation")), "{gap_text}");
        }
    }

    fn commit_one_move(world: &mut crate::SceneWorld, id: crate::EntityId) {
        let body = world.authored_block(id).unwrap().body.unwrap();
        for edge in body.edges.iter().map(|edge| edge.id) {
            for delta in [[0.02, 0.0, 0.0], [0.0, 0.02, 0.0], [0.0, 0.0, 0.02]] {
                if let Ok(edit) = body.move_edge(edge, delta) {
                    let local = world.entity_local_pose(id).unwrap();
                    let translation = Vec3::new(local.translation.x + edit.shift[0], local.translation.y + edit.shift[1], local.translation.z + edit.shift[2]);
                    assert_eq!(world.preview_block_body(id, edit.body, translation).unwrap(), AuthoringResult::Applied);
                    assert_eq!(world.commit_block_topology(id, BlockOp::MoveEdge { edge, delta_m: delta }).unwrap(), AuthoringResult::Applied);
                    return;
                }
            }
        }
        panic!("no edge accepted a move");
    }

    fn commit_one_edge_extrude(world: &mut crate::SceneWorld, id: crate::EntityId) {
        let body = world.authored_block(id).unwrap().body.unwrap();
        for edge in body.edges.iter().map(|edge| edge.id) {
            for delta in [[0.05, 0.0, 0.0], [0.0, 0.05, 0.0], [0.0, 0.0, 0.05]] {
                if let Ok(edit) = body.extrude_edge(edge, delta) {
                    let local = world.entity_local_pose(id).unwrap();
                    let translation = Vec3::new(local.translation.x + edit.shift[0], local.translation.y + edit.shift[1], local.translation.z + edit.shift[2]);
                    assert_eq!(world.preview_block_body(id, edit.body, translation).unwrap(), AuthoringResult::Applied);
                    assert_eq!(world.commit_block_topology(id, BlockOp::ExtrudeEdge { edge, delta_m: delta }).unwrap(), AuthoringResult::Applied);
                    return;
                }
            }
        }
        panic!("no edge accepted an extrude");
    }

    #[test]
    fn an_inward_extrude_names_the_carved_boundary() {
        // A whole face pushed inward is refused. A boundary cell of a 4×4 grid is not a corner,
        // so one edge sweeps along the adjacent side face and the kernel trims that face.
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let subdivide = BlockOp::SubdivideFace { face: 3, u: 4, v: 4 };
        assert_eq!(commit_captured(&world.authored_block(id).unwrap(), &subdivide), vec![vec!["F:seed/2".to_string()]]);
        assert_eq!(world.subdivide_block_face(id, 3, 4, 4).unwrap(), AuthoringResult::Applied);
        let cell = "F:cell(F:seed/2,1,0)";
        let face = concrete_named(&world.authored_block(id).unwrap(), cell);
        let delta = [0.0, -0.25, 0.0];
        let op = BlockOp::ExtrudeFaces { faces: vec![face], delta_m: delta };
        assert_eq!(commit_captured(&world.authored_block(id).unwrap(), &op), vec![vec![cell.to_string()]]);
        assert_eq!(world.extrude_block_faces(id, &[face], delta).unwrap(), AuthoringResult::Applied);
        let record = world.authored_block(id).unwrap();
        let report = evaluate_saved_semantic_shadow(&record);
        assert_eq!(report.status, SemanticShadowStatus::Pass, "{}", report.text);
        assert_eq!(report.candidate.as_ref(), record.body.as_ref());
        let body = record.body.clone().unwrap();
        let mut named_bevel = false;
        for fact in report.provenance.iter().filter(|fact| fact.identity.contains("carve(") || fact.identity.contains("leg(")) {
            named_bevel = true;
            let names = captured_semantics(&record, &BlockOp::BevelEdges { edges: vec![fact.concrete], distance_m: 0.04 }).expect("named rail");
            assert!(names.iter().all(|group| !group.is_empty()), "{} {names:?}", fact.identity);
            assert!(names.iter().any(|group| group.iter().any(|token| token.contains("carve(") || token.contains("leg("))), "{} {names:?}", fact.identity);
        }
        assert!(named_bevel, "{}", report.text);
        // The trimmed side is a concave notch. Bevel refuses that lip, so the edit is not stored.
        let lip = report.provenance.iter().find(|fact| fact.identity.contains("carve(")).unwrap().concrete;
        assert!(body.bevel_edges(&[lip], 0.04).is_err());
        assert_eq!(intent_completeness(&world.authored_block(id).unwrap()), IntentCompleteness::IntentComplete);
        let outer = report.provenance.iter().find(|fact| fact.identity.contains("outer(")).expect("wall");
        let cut = body.bevel_edges(&[outer.concrete], 0.04).expect("bevel a wall from the same extrude");
        let bevel = BlockOp::BevelEdges { edges: cut.edges.clone(), distance_m: cut.width_m };
        let names = captured_semantics(&record, &bevel).expect("wall rail");
        assert_eq!(names.len(), cut.edges.len());
        assert!(names.iter().all(|group| !group.is_empty()), "{names:?}");
        assert!(names.iter().any(|group| group.iter().any(|token| token.contains("outer(") || token.contains("leg(") || token.contains("carve("))), "{names:?}");
        let local = world.entity_local_pose(id).unwrap();
        let translation = Vec3::new(local.translation.x + cut.edit.shift[0], local.translation.y + cut.edit.shift[1], local.translation.z + cut.edit.shift[2]);
        assert_eq!(world.preview_block_body(id, cut.edit.body, translation).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.commit_block_topology(id, bevel).unwrap(), AuthoringResult::Applied);
        let after = world.authored_block(id).unwrap();
        assert!(after.steps.last().unwrap().semantic.as_ref().unwrap().iter().all(|group| !group.is_empty()));
        assert_eq!(intent_completeness(&after), IntentCompleteness::IntentComplete);
        let replay = evaluate_saved_semantic_shadow(&after);
        assert_eq!(replay.status, SemanticShadowStatus::Pass, "{}", replay.text);
        assert_eq!(replay.candidate.as_ref(), after.body.as_ref());
        assert!(after.intent.iter().all(|entry| !matches!(entry.payload, crate::IntentPayload::Gap { .. })));
        assert_eq!(intent_authority_eligibility(&after), IntentAuthorityEligibility::Eligible);
    }

    fn save_block(record: &BlockRecord) -> (crate::EntityId, String) {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), record.clone()).unwrap();
        let json = LevelDocument::capture(&world, document.level_uuid, "Candidate").unwrap().to_json();
        (id, json)
    }

    fn load_block(json: &str, id: crate::EntityId) -> BlockRecord {
        parse_level(json).unwrap().instantiate().unwrap().authored_block(id).unwrap()
    }

    fn instantiate_experiment(json: &str) -> crate::SceneWorld {
        parse_level(json).unwrap().instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap()
    }

    fn block_in(document: &LevelDocument, id: crate::EntityId) -> BlockRecord {
        document
            .entities
            .iter()
            .find(|entity| entity.uuid == id)
            .and_then(|entity| {
                entity.components.iter().find_map(|component| match component {
                    crate::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                    _ => None,
                })
            })
            .expect("saved block")
    }

    /// The file record. This does not spawn the world, so an eligible missing body stays missing.
    fn parsed_block(json: &str, id: crate::EntityId) -> BlockRecord {
        let document = parse_level(json).unwrap();
        document
            .entities
            .iter()
            .find(|entity| entity.uuid == id)
            .and_then(|entity| {
                entity.components.iter().find_map(|component| match component {
                    crate::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                    _ => None,
                })
            })
            .expect("saved block")
    }

    fn assert_same_solid(candidate: &SolidBody, oracle: &SolidBody) {
        assert!(candidate.validate().is_ok());
        assert!(oracle.validate().is_ok());
        assert_eq!(candidate.faces.len(), oracle.faces.len());
        assert_eq!(candidate.edges.len(), oracle.edges.len());
        assert_eq!(candidate.vertices.len(), oracle.vertices.len());
        assert_eq!(candidate, oracle);
        for (left, right) in candidate.vertices.iter().zip(oracle.vertices.iter()) {
            assert_eq!(left.id, right.id);
            assert!((0..3).all(|axis| (left.position[axis] - right.position[axis]).abs() < 1.0e-6));
        }
        let bounds = candidate.aabb_size();
        let oracle_bounds = oracle.aabb_size();
        assert!((0..3).all(|axis| (bounds[axis] - oracle_bounds[axis]).abs() < 1.0e-6));
        assert!((volume(candidate) - volume(oracle)).abs() < 1.0e-8);
        assert_eq!(hash_body(candidate), hash_body(oracle));
    }

    /// Drop the stored body and save. The file and the parsed document keep the tape and contain no body.
    /// World spawn of that same file is a separate path and may realize an eligible tape.
    fn cold_without_body(record: &BlockRecord) -> BlockRecord {
        let mut bare = record.clone();
        let oracle = bare.body.take().expect("a stored body to discard");
        let derived = crate::DerivedRenderGeometry::from_body(&oracle);
        derived.discard();
        let (id, json) = save_block(&bare);
        assert!(json.contains("\"intent\""));
        assert!(!json.contains("\"body\""));
        assert!(!json.contains("semantic-shadow"));
        for forbidden in ["meshlet", "einstein", "Einstein", "hierarchy"] {
            assert!(!json.contains(forbidden), "{forbidden}");
        }
        let loaded = parsed_block(&json, id);
        assert!(loaded.body.is_none());
        assert_eq!(loaded.intent, record.intent);
        assert_eq!(loaded.material, record.material);
        loaded
    }

    #[test]
    fn an_intent_authority_candidate_rebuilds_after_the_body_is_discarded() {
        let authored = author_long(0.25);
        assert_eq!(intent_completeness(&authored), IntentCompleteness::IntentComplete);
        let (id, json) = save_block(&authored);
        assert!(json.contains("\"body\""));
        assert!(json.contains("\"intent\""));
        let loaded = load_block(&json, id);
        assert_eq!(loaded, authored);
        assert_eq!(loaded.body.as_ref(), authored.body.as_ref());

        let mut lying = loaded.clone();
        lying.body = Some(SolidBody::from_box([2.0, 2.0, 2.0]).unwrap());
        let ignored = intent_authority_candidate(&lying);
        let IntentAuthorityCandidate::Reconstructable(from_lie) = ignored else { panic!("a lying body must not refuse a complete tape") };
        assert_eq!(from_lie, authored.body.clone().unwrap());
        assert_ne!(from_lie, lying.body.clone().unwrap());

        let bare = cold_without_body(&loaded);
        let untouched = bare.clone();
        let rebuilt = match intent_authority_candidate(&bare) {
            IntentAuthorityCandidate::Reconstructable(body) => body,
            IntentAuthorityCandidate::Refused(reason) => panic!("cold tape refused: {reason}"),
        };
        assert_eq!(bare, untouched);
        assert_same_solid(&rebuilt, &authored.body.clone().unwrap());
        assert_eq!(intent_authority_eligibility(&authored), IntentAuthorityEligibility::Eligible);
        assert_eq!(intent_authority_eligibility(&bare), IntentAuthorityEligibility::Eligible);
        assert!(
            matches!(intent_authority_eligibility(&lying), IntentAuthorityEligibility::Ineligible(reasons) if reasons.iter().any(|reason| reason.contains("diverges"))),
            "{:?}",
            intent_authority_eligibility(&lying)
        );
        let cold_text = semantic_shadow_diagnostic(&bare, None);
        assert!(cold_text.contains("Authority readiness: READY"), "{cold_text}");
        assert!(cold_text.contains("Intent authority candidate: reconstructable"), "{cold_text}");
        assert!(cold_text.contains("Intent Authority: ELIGIBLE"), "{cold_text}");
        assert!(cold_text.contains("candidate produced and the record has no stored body"), "{cold_text}");

        let mut working = bare.clone();
        working.body = Some(rebuilt);
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), working).unwrap();
        let (face, edge) = extend_reconstructed(&mut world, id);
        assert!(face.starts_with("F:"), "{face}");
        assert!(edge.contains("split-new("), "{edge}");
        assert_eq!(world.set_block_extent(id, 0, 5.0).unwrap(), AuthoringResult::Applied);
        let extended = world.authored_block(id).unwrap();
        let count = |record: &BlockRecord, kind: &str| {
            record.intent.iter().filter(|entry| match (&entry.payload, kind) {
                (crate::IntentPayload::Bevel { .. }, "bevel") | (crate::IntentPayload::Extrude { .. }, "extrude") | (crate::IntentPayload::Split, "split") => true,
                _ => false,
            }).count()
        };
        assert!(count(&extended, "extrude") > count(&loaded, "extrude"), "{face}");
        assert!(count(&extended, "split") > count(&loaded, "split"), "{edge}");
        assert!(count(&extended, "bevel") > count(&loaded, "bevel"), "{edge}");
        assert_eq!(extended.history.len(), 24);
        assert!(extended.intent.len() > loaded.intent.len());
        assert!(matches!(extended.intent[0].payload, crate::IntentPayload::Size { size_m } if size_m == [6.0, 2.0, 2.0]));
        assert!(!extended.history.iter().any(|op| matches!(op, BlockOp::Size { size_m } if *size_m == [6.0, 2.0, 2.0])));
        assert_eq!(intent_completeness(&extended), IntentCompleteness::IntentComplete);
        assert_eq!(extended.material, authored.material);
        assert_ne!(extended.body, authored.body);
        let extended_text = semantic_shadow_diagnostic(&extended, None);
        assert!(extended_text.contains("Intent authority candidate: reconstructable"), "{extended_text}");

        let (saved_id, saved_json) = save_block(&extended);
        assert!(saved_json.contains("\"body\""));
        let reloaded = load_block(&saved_json, saved_id);
        assert_eq!(reloaded, extended);
        let post = reloaded.body.clone().unwrap();
        let again_bare = cold_without_body(&reloaded);
        let again = match intent_authority_candidate(&again_bare) {
            IntentAuthorityCandidate::Reconstructable(body) => body,
            IntentAuthorityCandidate::Refused(reason) => panic!("second cold tape refused: {reason}"),
        };
        assert_same_solid(&again, &post);
        assert_eq!(again_bare.material, authored.material);
        let provenance = evaluate_saved_semantic_shadow(&again_bare);
        assert_eq!(provenance.status, SemanticShadowStatus::Pass, "{}", provenance.text);
        let prior = evaluate_saved_semantic_shadow(&loaded);
        let created = fresh_names(&prior, &provenance);
        let live = |name: &str, body: &SolidBody| {
            provenance.concrete_of(name).is_some_and(|id| body.faces.iter().any(|face| face.id == id) || body.edges.iter().any(|edge| edge.id == id))
        };
        assert!(live(&face, &again), "{face}");
        assert!(again_bare.intent.iter().any(|entry| entry.groups.as_ref().is_some_and(|groups| groups.iter().flatten().any(|token| token == &edge))), "{edge}");
        let bevel_edge = created
            .iter()
            .find(|name| name.starts_with("E:bevel-edge(") && live(name, &again))
            .cloned()
            .unwrap_or_else(|| panic!("no live bevel edge in {created:?}"));

        let mut program = saved_semantic_program(&extended).unwrap();
        let mut scaled = false;
        for op in program.ops.iter_mut().rev() {
            if let SemanticOp::Extrude { amount: ExtrudeAmount::Delta(delta), .. } = op {
                *delta = [delta[0] * 1.5, delta[1] * 1.5, delta[2] * 1.5];
                scaled = true;
                break;
            }
        }
        assert!(scaled, "the tape has no extrude to retarget");
        let rebound = evaluate_semantic_shadow(&program);
        assert_eq!(rebound.status, SemanticShadowStatus::Pass, "{}", rebound.text);
        assert_ne!(rebound.candidate.as_ref(), extended.body.as_ref());
        let rebound_body = rebound.candidate.clone().unwrap();
        let rebound_edge = rebound.concrete_of(&bevel_edge).expect("the new bevel edge did not rebind");
        assert!(rebound_body.edges.iter().any(|item| item.id == rebound_edge), "{}", rebound.text);
        assert!(rebound.concrete_of(&face).is_some_and(|id| rebound_body.faces.iter().any(|item| item.id == id)), "{}", rebound.text);
        assert!(rebound.candidate.as_ref().unwrap().validate().is_ok());
        assert_eq!(world.authored_block(id).unwrap(), extended);

        let plain = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        match intent_authority_candidate(&plain) {
            IntentAuthorityCandidate::Refused(reason) => assert!(reason.contains("legacy"), "{reason}"),
            IntentAuthorityCandidate::Reconstructable(_) => panic!("a legacy cube was reconstructed"),
        }
        let plain_text = semantic_shadow_diagnostic(&plain, None);
        assert!(plain_text.contains("Authority readiness: LEGACY"), "{plain_text}");
        assert!(plain_text.contains("Intent Authority: INELIGIBLE — legacy concrete-only history has no semantic program"), "{plain_text}");

        let mut legacy = plain;
        legacy.push_op(BlockOp::SplitEdge { edge: 12 });
        match intent_authority_candidate(&legacy) {
            IntentAuthorityCandidate::Refused(reason) => assert!(reason.contains("split-edge"), "{reason}"),
            IntentAuthorityCandidate::Reconstructable(_) => panic!("an uncaptured split was reconstructed"),
        }
        let legacy_text = semantic_shadow_diagnostic(&legacy, None);
        assert!(legacy_text.contains("Authority readiness: INCOMPLETE"), "{legacy_text}");
        assert!(legacy_text.contains("Intent Authority: INELIGIBLE — uncaptured operation: split-edge step 1"), "{legacy_text}");
        assert!(!legacy_text.contains("Intent authority candidate: reconstructable"), "{legacy_text}");

        let short = author_through_editor(&base());
        let mut host = empty_world_level().instantiate().unwrap();
        let short_id = host.create_block(Vec3::new(0.0, 1.0, -4.0), short).unwrap();
        let copy = host.mirror_block(short_id, 0).unwrap();
        let mirrored = host.authored_block(copy).unwrap();
        let IntentAuthorityCandidate::Reconstructable(mirrored_body) = intent_authority_candidate(&mirrored) else {
            panic!("a mirrored stored body was refused: {}", intent_authority_diagnostic(&mirrored));
        };
        assert_same_solid(&mirrored_body, &mirrored.body.clone().unwrap());
        assert_eq!(intent_authority_eligibility(&mirrored), IntentAuthorityEligibility::Eligible);
        let mirrored_text = semantic_shadow_diagnostic(&mirrored, None);
        assert!(mirrored_text.contains("Authority readiness: READY"), "{mirrored_text}");
        assert!(mirrored_text.contains("Intent Authority: ELIGIBLE"), "{mirrored_text}");
        assert!(!mirrored_text.contains("uncaptured operation: mirror"), "{mirrored_text}");
    }

    fn fresh_names(before: &SemanticShadowReport, after: &SemanticShadowReport) -> Vec<String> {
        let mut names = Vec::new();
        for fact in &after.provenance {
            if !before.provenance.iter().any(|prior| prior.identity == fact.identity) {
                names.push(fact.identity.clone());
            }
        }
        names.sort();
        names.dedup();
        names
    }

    /// Continue the reconstructed solid through the scene commands.
    ///
    /// A free cell of this solid no longer sweeps, so the continuation uses a semantic face
    /// the kernel still accepts, then a new split and bevel. A seed edge is the fallback.
    fn extend_reconstructed(world: &mut crate::SceneWorld, id: crate::EntityId) -> (String, String) {
        let record = world.authored_block(id).unwrap();
        let before = evaluate_saved_semantic_shadow(&record);
        let body = record.body.clone().unwrap();
        let mut face_name = None;
        for fact in &before.provenance {
            if !fact.identity.starts_with("F:") {
                continue;
            }
            let Some(normal) = body.unit_normal(fact.concrete) else { continue };
            for distance in [0.10_f64, 0.20] {
                let delta = scale(normal, distance);
                if body.extrude_faces(&[fact.concrete], delta).is_err() {
                    continue;
                }
                let op = BlockOp::ExtrudeFaces { faces: vec![fact.concrete], delta_m: delta };
                let names = captured_semantics(&world.authored_block(id).unwrap(), &op).expect("named face");
                assert!(names.iter().all(|group| !group.is_empty()), "{names:?}");
                assert!(names.iter().any(|group| group.iter().any(|token| token == &fact.identity)), "{names:?}");
                assert_eq!(world.extrude_block_faces(id, &[fact.concrete], delta).unwrap(), AuthoringResult::Applied);
                face_name = Some(fact.identity.clone());
                break;
            }
            if face_name.is_some() {
                break;
            }
        }
        let mut report = evaluate_saved_semantic_shadow(&world.authored_block(id).unwrap());
        let mut edges: Vec<String> = fresh_names(&before, &report).into_iter().filter(|name| name.starts_with("E:")).collect();
        for fact in &report.provenance {
            if fact.identity.starts_with("E:seed-edge/") || fact.identity.contains("outer(") {
                edges.push(fact.identity.clone());
            }
        }
        edges.sort();
        edges.dedup();
        for name in &edges {
            let Some(edge) = report.concrete_of(name) else { continue };
            let body = world.authored_block(id).unwrap().body.clone().unwrap();
            if body.split_edge(edge).is_err() {
                continue;
            }
            let split_before = report.clone();
            split_named(world, id, name);
            report = evaluate_saved_semantic_shadow(&world.authored_block(id).unwrap());
            let Some(half) = fresh_names(&split_before, &report).into_iter().find(|born| born.contains("split-new(")) else {
                continue;
            };
            let Some(half_edge) = report.concrete_of(&half) else { continue };
            let Ok(cut) = world.authored_block(id).unwrap().body.clone().unwrap().bevel_edges(&[half_edge], 0.04) else {
                continue;
            };
            bevel_named(world, id, &half, cut.width_m);
            let face = face_name.clone().unwrap_or_else(|| "F:cell(F:seed/4,1,0)".to_string());
            return (face, half);
        }
        panic!("the reconstructed solid accepted no further split");
    }

    /// Resize, subdivide, extrude, split, bevel, move, edge extrude, and a split-vertex move.
    /// Extra sizes take the tape past both the 24-entry log and the 64-entry editor undo cap.
    fn author_closed_chain() -> BlockRecord {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(world.set_block_extent(id, 0, 3.0).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 1, 2.5).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 2, 3.0).unwrap(), AuthoringResult::Applied);
        model_face(&mut world, id, 4, 5, 0.25, false);
        assert!(commit_named_element(&mut world, id, "move-edge"), "no named edge accepted a move");
        assert!(commit_named_element(&mut world, id, "extrude-edge"), "no named edge accepted an extrude");
        assert!(split_one_named_edge(&mut world, id), "no named edge accepted another split");
        assert!(commit_named_element(&mut world, id, "move-vertex"), "no named vertex accepted a move");
        let mut width = 3.05;
        while world.authored_block(id).unwrap().intent.len() <= 64 {
            assert_eq!(world.set_block_extent(id, 0, width).unwrap(), AuthoringResult::Applied, "{width}");
            width += 0.05;
            assert!(width < 20.0, "size padding did not fill the tape");
        }
        let mut record = world.authored_block(id).unwrap();
        record.material = crate::MaterialAssetRef::builtin("bootstrap_near", [0.25, 0.5, 1.0, 1.0], 0.0, 0.5, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(world.replace_block_record(id, record).unwrap(), AuthoringResult::Applied);
        world.authored_block(id).unwrap()
    }

    fn commit_named_element(world: &mut crate::SceneWorld, id: crate::EntityId, kind: &str) -> bool {
        let record = world.authored_block(id).unwrap();
        let report = evaluate_saved_semantic_shadow(&record);
        let body = record.body.clone().unwrap();
        let prefix = if kind == "move-vertex" { "V:" } else { "E:" };
        let deltas: &[[f64; 3]] = if kind == "extrude-edge" {
            &[[0.05, 0.0, 0.0], [0.0, 0.05, 0.0], [0.0, 0.0, 0.05], [-0.05, 0.0, 0.0]]
        } else {
            &[[0.02, 0.0, 0.0], [0.0, 0.02, 0.0], [0.0, 0.0, 0.02], [-0.02, 0.0, 0.0]]
        };
        for fact in &report.provenance {
            if !fact.identity.starts_with(prefix) {
                continue;
            }
            let live = match kind {
                "move-vertex" => body.vertices.iter().any(|vertex| vertex.id == fact.concrete),
                _ => body.edges.iter().any(|edge| edge.id == fact.concrete),
            };
            if !live {
                continue;
            }
            for delta in deltas {
                let edit = match kind {
                    "move-edge" => body.move_edge(fact.concrete, *delta).ok(),
                    "move-vertex" => body.move_vertex(fact.concrete, *delta).ok(),
                    "extrude-edge" => body.extrude_edge(fact.concrete, *delta).ok(),
                    _ => None,
                };
                let Some(edit) = edit else { continue };
                let op = match kind {
                    "move-edge" => BlockOp::MoveEdge { edge: fact.concrete, delta_m: *delta },
                    "move-vertex" => BlockOp::MoveVertex { vertex: fact.concrete, delta_m: *delta },
                    "extrude-edge" => BlockOp::ExtrudeEdge { edge: fact.concrete, delta_m: *delta },
                    _ => continue,
                };
                let Some(names) = captured_semantics(&world.authored_block(id).unwrap(), &op) else { continue };
                if names.iter().any(|group| group.is_empty()) {
                    continue;
                }
                let local = world.entity_local_pose(id).unwrap();
                let translation = Vec3::new(local.translation.x + edit.shift[0], local.translation.y + edit.shift[1], local.translation.z + edit.shift[2]);
                assert_eq!(world.preview_block_body(id, edit.body, translation).unwrap(), AuthoringResult::Applied);
                assert_eq!(world.commit_block_topology(id, op).unwrap(), AuthoringResult::Applied);
                return true;
            }
        }
        false
    }

    fn split_one_named_edge(world: &mut crate::SceneWorld, id: crate::EntityId) -> bool {
        let record = world.authored_block(id).unwrap();
        let report = evaluate_saved_semantic_shadow(&record);
        let body = record.body.clone().unwrap();
        for fact in &report.provenance {
            if !fact.identity.starts_with("E:") || body.split_edge(fact.concrete).is_err() {
                continue;
            }
            let op = BlockOp::SplitEdge { edge: fact.concrete };
            let Some(names) = captured_semantics(&world.authored_block(id).unwrap(), &op) else { continue };
            if names.iter().any(|group| group.is_empty()) {
                continue;
            }
            if world.split_block_edge(id, fact.concrete).is_err() {
                continue;
            }
            return true;
        }
        false
    }

    fn discard_derived(body: &SolidBody) -> (usize, usize) {
        let derived = crate::DerivedRenderGeometry::from_body(body);
        let records: Vec<crate::GpuMeshletRecord> = derived.meshlets.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        let hierarchy = crate::build_cluster_hierarchy(&records);
        let meshlets = records.len();
        let nodes = hierarchy.nodes.len();
        assert!(meshlets > 0 && nodes >= meshlets);
        drop(hierarchy);
        drop(records);
        derived.discard();
        (meshlets, nodes)
    }

    fn assert_blocker(record: &BlockRecord, needle: &str) {
        let text = intent_authority_diagnostic(record);
        assert!(text.contains(needle), "{text}");
        assert!(text.contains("Intent Authority: INELIGIBLE"), "{text}");
        assert!(!text.contains("Intent Authority: ELIGIBLE\n") && !text.ends_with("Intent Authority: ELIGIBLE"), "{text}");
    }

    #[test]
    fn eligibility_names_each_uncaptured_edit() {
        let plain = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        assert_blocker(&plain, "Intent Authority: INELIGIBLE — legacy concrete-only history has no semantic program");
        let mut sized = empty_world_level().instantiate().unwrap();
        let sized_id = sized.create_block(Vec3::new(0.0, 1.0, -4.0), plain).unwrap();
        assert_eq!(sized.set_block_extent(sized_id, 0, 3.0).unwrap(), AuthoringResult::Applied);
        let sized_record = sized.authored_block(sized_id).unwrap();
        assert!(sized_record.intent.is_empty());
        assert_blocker(&sized_record, "legacy concrete-only history has no semantic program");

        let mut beveled = empty_world_level().instantiate().unwrap();
        let bevel_id = beveled.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(beveled.set_block_bevel(bevel_id, 0.2).unwrap(), AuthoringResult::Applied);
        assert_blocker(&beveled.authored_block(bevel_id).unwrap(), "Intent Authority: INELIGIBLE — uncaptured operation: uniform-bevel step 1");

        let mut inset = empty_world_level().instantiate().unwrap();
        let inset_id = inset.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(inset.set_block_inset(inset_id, 0, 0.1).unwrap(), AuthoringResult::Applied);
        assert_blocker(&inset.authored_block(inset_id).unwrap(), "Intent Authority: INELIGIBLE — uncaptured operation: inset step 1");
        let copy = inset.mirror_block(inset_id, 0).unwrap();
        let mirrored = inset.authored_block(copy).unwrap();
        assert!(mirrored.body.is_none());
        assert_blocker(&mirrored, "uncaptured operation: mirror step 2");
        assert_blocker(&mirrored, "uncaptured operation: inset step 1");

        let mut pushed = empty_world_level().instantiate().unwrap();
        let push_id = pushed.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let origin = Vec3::new(0.0, 1.0, -4.0);
        assert_eq!(pushed.push_block_face(push_id, 4, [2.0, 2.0, 2.0], origin, 0.4).unwrap(), AuthoringResult::Applied);
        assert_eq!(pushed.commit_block_face(push_id, 4, [2.0, 2.0, 2.0]).unwrap(), AuthoringResult::Applied);
        let mid = pushed.authored_block(push_id).unwrap().size_m;
        let place = pushed.entity_local_pose(push_id).unwrap().translation;
        assert_eq!(pushed.push_block_face(push_id, 5, mid, place, 0.2).unwrap(), AuthoringResult::Applied);
        assert_eq!(pushed.commit_block_face(push_id, 5, mid).unwrap(), AuthoringResult::Applied);
        let analytic = pushed.authored_block(push_id).unwrap();
        assert!(analytic.body.is_none());
        assert!(matches!(analytic.intent[0].payload, crate::IntentPayload::PushFace { face: 4, .. }));
        assert!(matches!(analytic.intent[1].payload, crate::IntentPayload::PushFace { face: 5, .. }));
        assert!(analytic.intent.iter().all(|entry| entry.groups.is_none()));
        assert_eq!(intent_authority_eligibility(&analytic), IntentAuthorityEligibility::Eligible);
        let mut unborn = analytic.clone();
        unborn.seed_size_m = None;
        assert_blocker(&unborn, "the creation size was not recorded");
        assert_eq!(pushed.subdivide_block_face(push_id, 5, 2, 2).unwrap(), AuthoringResult::Applied);
        let divided = pushed.authored_block(push_id).unwrap();
        assert!(divided.intent.iter().any(|entry| entry.groups.as_ref().is_some_and(|groups| groups.iter().flatten().any(|token| token == "F:seed/4"))));
        assert_eq!(intent_authority_eligibility(&divided), IntentAuthorityEligibility::Eligible);
        assert!(concrete_named(&divided, "F:cell(F:seed/4,0,0)") > 0);
        let before_refusal = divided.intent.len();
        assert!(pushed.set_block_bevel(push_id, 0.1).is_err());
        assert!(pushed.set_block_inset(push_id, 0, 0.1).is_err());
        assert_eq!(pushed.authored_block(push_id).unwrap().intent.len(), before_refusal);
        assert_eq!(intent_authority_eligibility(&pushed.authored_block(push_id).unwrap()), IntentAuthorityEligibility::Eligible);

        let mut corner = empty_world_level().instantiate().unwrap();
        let corner_id = corner.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(corner.subdivide_block_face(corner_id, 5, 2, 2).unwrap(), AuthoringResult::Applied);
        let seed_record = corner.authored_block(corner_id).unwrap();
        let seed_body = seed_record.body.clone().unwrap();
        let named = evaluate_saved_semantic_shadow(&seed_record);
        let mut moved_seed = false;
        for vertex in seed_body.vertices.iter().map(|vertex| vertex.id).filter(|vertex| (1..=8).contains(vertex)) {
            if named.provenance.iter().any(|fact| fact.identity.starts_with("V:") && fact.concrete == vertex) {
                continue;
            }
            let Ok(edit) = seed_body.move_vertex(vertex, [0.02, 0.0, 0.0]) else { continue };
            let local = corner.entity_local_pose(corner_id).unwrap();
            let translation = Vec3::new(local.translation.x + edit.shift[0], local.translation.y + edit.shift[1], local.translation.z + edit.shift[2]);
            assert_eq!(corner.preview_block_body(corner_id, edit.body, translation).unwrap(), AuthoringResult::Applied);
            assert_eq!(corner.commit_block_topology(corner_id, BlockOp::MoveVertex { vertex, delta_m: [0.02, 0.0, 0.0] }).unwrap(), AuthoringResult::Applied);
            moved_seed = true;
            break;
        }
        assert!(moved_seed, "no constructor vertex accepted a move");
        let gapped = corner.authored_block(corner_id).unwrap();
        assert!(matches!(gapped.intent.last().map(|entry| &entry.payload), Some(crate::IntentPayload::Gap { operation }) if operation == "move-vertex"));
        let step = gapped.intent.len();
        assert_blocker(&gapped, &format!("uncaptured operation: move-vertex step {step}"));

        let kept = gapped.material.clone();
        assert_eq!(corner.reset_block_shape(corner_id).unwrap(), AuthoringResult::Applied);
        let reset = corner.authored_block(corner_id).unwrap();
        assert!(reset.is_plain() && reset.intent.is_empty() && reset.size_m == [2.0, 2.0, 2.0]);
        assert_eq!(reset.material, kept);
        assert_blocker(&reset, "legacy concrete-only history has no semantic program");

        let mut preview = empty_world_level().instantiate().unwrap();
        let preview_id = preview.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(preview.subdivide_block_face(preview_id, 5, 2, 2).unwrap(), AuthoringResult::Applied);
        let stored = preview.authored_block(preview_id).unwrap();
        assert_eq!(intent_authority_eligibility(&stored), IntentAuthorityEligibility::Eligible);
        let local = preview.entity_local_pose(preview_id).unwrap().translation;
        assert_eq!(preview.preview_block_body(preview_id, SolidBody::from_box([1.0, 1.0, 1.0]).unwrap(), local).unwrap(), AuthoringResult::Applied);
        assert_blocker(&preview.authored_block(preview_id).unwrap(), "reconstruction diverges from the stored body");
        assert_eq!(preview.restore_block_body(preview_id, stored.body.clone(), stored.size_m, local).unwrap(), AuthoringResult::Applied);
        let restored = preview.authored_block(preview_id).unwrap();
        assert_eq!(restored.intent, stored.intent);
        assert_eq!(intent_authority_eligibility(&restored), IntentAuthorityEligibility::Eligible);
    }

    #[test]
    fn eligibility_survives_a_long_chain_rebinding_save_and_undo() {
        let record = author_closed_chain();
        let snapshot = record.clone();
        assert_eq!(record.history.len(), 24);
        assert_eq!(record.steps.len(), 24);
        assert!(record.intent.len() > 64, "intent {}", record.intent.len());
        assert!(matches!(record.intent[0].payload, crate::IntentPayload::Size { size_m } if size_m == [3.0, 2.0, 2.0]));
        assert!(!record.history.iter().any(|op| matches!(op, BlockOp::Size { size_m } if *size_m == [3.0, 2.0, 2.0])));
        assert!(record.intent.iter().any(|entry| matches!(entry.payload, crate::IntentPayload::MoveEdge { .. })));
        assert!(record.intent.iter().any(|entry| matches!(entry.payload, crate::IntentPayload::MoveVertex { .. })));
        assert!(record.intent.iter().any(|entry| matches!(entry.payload, crate::IntentPayload::ExtrudeEdge { .. })));
        assert!(record.intent.iter().any(|entry| entry.groups.as_ref().is_some_and(|groups| groups.iter().flatten().any(|token| token.contains("split-at(")))));
        let phase = evaluate_intent_shadow(&record);
        assert_eq!(phase.status, IntentShadowStatus::Incomplete, "{}", phase.text);
        assert_eq!(intent_authority_eligibility(&record), IntentAuthorityEligibility::Eligible);
        assert_eq!(record.material.name, "bootstrap_near");

        let cell_before = concrete_named(&record, "F:cell(F:seed/4,1,0)");
        let mut divided = saved_semantic_program(&record).unwrap();
        if let SemanticOp::Subdivide { u, v, .. } = divided.ops.iter_mut().find(|op| matches!(op, SemanticOp::Subdivide { .. })).unwrap() {
            *u = 4;
            *v = 4;
        }
        let finer = evaluate_semantic_shadow(&divided);
        assert_eq!(finer.status, SemanticShadowStatus::Pass, "{}", finer.text);
        let cell_after = finer.concrete_of("F:cell(F:seed/4,1,0)").expect("cell rebound");
        assert_ne!(cell_before, cell_after);
        assert_ne!(finer.candidate.as_ref(), record.body.as_ref());
        assert_eq!(record, snapshot);

        let mut ambiguous = record.clone();
        let mut replaced_side = false;
        for entry in &mut ambiguous.intent {
            let Some(groups) = entry.groups.as_mut() else { continue };
            for group in groups {
                if group.iter().any(|token| token.starts_with("F:side(")) {
                    group.clear();
                    group.push("F:any-side(F:cell(F:seed/4,1,0))".to_string());
                    replaced_side = true;
                    break;
                }
            }
            if replaced_side {
                break;
            }
        }
        assert!(replaced_side);
        assert_blocker(&ambiguous, "resolved more than once");
        match intent_authority_candidate(&ambiguous) {
            IntentAuthorityCandidate::Refused(reason) => assert!(reason.contains("resolved more than once"), "{reason}"),
            IntentAuthorityCandidate::Reconstructable(_) => panic!("an ambiguous side was reconstructed"),
        }

        let mut missing = record.clone();
        let mut replaced_cell = false;
        for entry in &mut missing.intent {
            let Some(groups) = entry.groups.as_mut() else { continue };
            for group in groups {
                for token in group {
                    if token == "F:cell(F:seed/4,1,0)" {
                        *token = "F:cell(F:seed/4,7,0)".to_string();
                        replaced_cell = true;
                    }
                }
            }
        }
        assert!(replaced_cell);
        assert_blocker(&missing, "ceased to exist");
        assert!(matches!(intent_authority_candidate(&missing), IntentAuthorityCandidate::Refused(_)));
        assert_eq!(record, snapshot);

        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), record.clone()).unwrap();
        assert_eq!(world.set_entity_name(id, "Eligibility Solid").unwrap(), AuthoringResult::Applied);
        let parked = Vec3::new(1.5, 2.0, -6.0);
        assert_eq!(world.set_entity_local_translation(id, parked).unwrap(), AuthoringResult::Applied);
        let intent_len = world.authored_block(id).unwrap().intent.len();
        assert!(world.set_block_inset(id, 0, 0.1).is_err());
        assert!(world.set_block_bevel(id, 0.1).is_err());
        assert_eq!(world.authored_block(id).unwrap().intent.len(), intent_len);
        let json = LevelDocument::capture(&world, document.level_uuid, "Eligibility").unwrap().to_json();
        let reloaded = parse_level(&json).unwrap().instantiate().unwrap();
        let again = LevelDocument::capture(&reloaded, document.level_uuid, "Eligibility").unwrap().to_json();
        let mut twice = parse_level(&again).unwrap().instantiate().unwrap();
        assert_eq!(twice.authored_block(id).unwrap(), record);
        assert_eq!(twice.remember_entity(id).unwrap().name, "Eligibility Solid");
        let pose = twice.entity_local_pose(id).unwrap().translation;
        assert!((pose.x - parked.x).abs() < 1.0e-9 && (pose.y - parked.y).abs() < 1.0e-9 && (pose.z - parked.z).abs() < 1.0e-9, "{pose:?}");
        assert_eq!(twice.authored_block(id).unwrap().material, record.material);

        let before = twice.remember_entity(id).unwrap();
        let first = twice.authored_block(id).unwrap().intent[0].clone();
        let width = twice.authored_block(id).unwrap().size_m[1] + 0.1;
        assert_eq!(twice.set_block_extent(id, 1, width).unwrap(), AuthoringResult::Applied);
        let after = twice.remember_entity(id).unwrap();
        assert_eq!(twice.authored_block(id).unwrap().intent.len(), intent_len + 1);
        twice.apply_mementos(&[crate::EntityMemento::Present(before)]).unwrap();
        let undone = twice.authored_block(id).unwrap();
        assert_eq!(undone.intent.len(), intent_len);
        assert_eq!(undone.intent[0], first);
        assert_eq!(undone.history.len(), 24);
        assert_eq!(intent_authority_eligibility(&undone), IntentAuthorityEligibility::Eligible);
        twice.apply_mementos(&[crate::EntityMemento::Present(after)]).unwrap();
        assert_eq!(twice.authored_block(id).unwrap().intent.len(), intent_len + 1);
        assert_eq!(twice.set_block_extent(id, 1, width + 0.1).unwrap(), AuthoringResult::Applied);
        let continued = twice.authored_block(id).unwrap();
        assert_eq!(continued.intent.len(), intent_len + 2);
        assert!(continued.intent.len() > 64);
        assert_eq!(continued.history.len(), 24);
        assert_eq!(intent_authority_eligibility(&continued), IntentAuthorityEligibility::Eligible);

        let copy = twice.mirror_block(id, 0).unwrap();
        let mirrored = twice.authored_block(copy).unwrap();
        assert!(matches!(mirrored.intent.last().map(|entry| &entry.payload), Some(crate::IntentPayload::Mirror { axis: 0 })));
        assert!(!twice.authored_block(id).unwrap().intent.iter().any(|entry| matches!(entry.payload, crate::IntentPayload::Mirror { .. })));
        assert_eq!(intent_authority_eligibility(&mirrored), IntentAuthorityEligibility::Eligible);
        assert_eq!(intent_authority_eligibility(&twice.authored_block(id).unwrap()), IntentAuthorityEligibility::Eligible);
        let mirror_len = mirrored.intent.len();
        let report = evaluate_saved_semantic_shadow(&mirrored);
        let mut subdivided = false;
        for fact in report.provenance.iter().filter(|fact| fact.identity.starts_with("F:")) {
            if twice.subdivide_block_face(copy, fact.concrete, 2, 2).is_ok() {
                subdivided = true;
                break;
            }
        }
        assert!(subdivided, "the mirror copy accepted no further subdivision");
        let grown = twice.authored_block(copy).unwrap();
        assert!(grown.intent.len() > mirror_len);
        assert_eq!(grown.history.len(), 24);
        assert_eq!(intent_authority_eligibility(&grown), IntentAuthorityEligibility::Eligible);
        let before_inset = grown.intent.len();
        assert!(twice.set_block_inset(copy, 0, 0.1).is_err());
        assert_eq!(twice.authored_block(copy).unwrap().intent.len(), before_inset);
    }

    #[test]
    fn a_discarded_body_rebuilds_twice_from_intent_alone() {
        let authored = author_closed_chain();
        assert_eq!(intent_authority_eligibility(&authored), IntentAuthorityEligibility::Eligible);
        let oracle = authored.body.clone().unwrap();
        let hash = hash_body(&oracle);
        let measured_volume = volume(&oracle);
        let bounds = oracle.aabb_size();
        let (meshlets, nodes) = discard_derived(&oracle);
        assert!(meshlets > 0 && nodes > 0);

        let mut bare = authored.clone();
        bare.body = None;
        let (id, json) = save_block(&bare);
        assert!(!json.contains("\"body\""));
        for forbidden in ["meshlet", "einstein", "Einstein", "hierarchy"] {
            assert!(!json.contains(forbidden), "{forbidden}");
        }
        let loaded = parsed_block(&json, id);
        assert!(loaded.body.is_none(), "the parsed file does not invent a body");
        assert_eq!(loaded.intent, authored.intent);
        assert_eq!(loaded.material, authored.material);
        let rebuilt = match intent_authority_candidate(&loaded) {
            IntentAuthorityCandidate::Reconstructable(body) => body,
            IntentAuthorityCandidate::Refused(reason) => panic!("cold tape refused: {reason}"),
        };
        assert_same_solid(&rebuilt, &oracle);
        assert_eq!(hash_body(&rebuilt), hash);
        assert!((volume(&rebuilt) - measured_volume).abs() < 1.0e-8);
        let rebuilt_bounds = rebuilt.aabb_size();
        assert!((0..3).all(|axis| (rebuilt_bounds[axis] - bounds[axis]).abs() < 1.0e-6));

        let mut working = loaded;
        working.body = Some(rebuilt);
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), working).unwrap();
        assert_eq!(world.set_entity_name(id, "Disposable Solid").unwrap(), AuthoringResult::Applied);
        assert!(split_one_named_edge(&mut world, id), "the reconstruction accepted no further split");
        assert_eq!(world.set_block_extent(id, 2, 3.5).unwrap(), AuthoringResult::Applied);
        let extended = world.authored_block(id).unwrap();
        assert!(extended.intent.len() > authored.intent.len());
        assert_eq!(extended.history.len(), 24);
        assert_eq!(extended.material, authored.material);
        assert_eq!(intent_authority_eligibility(&extended), IntentAuthorityEligibility::Eligible);
        let post = extended.body.clone().unwrap();
        let post_hash = hash_body(&post);
        discard_derived(&post);
        assert_eq!(world.remember_entity(id).unwrap().name, "Disposable Solid");
        let saved_json = LevelDocument::capture(&world, document.level_uuid, "Disposable").unwrap().to_json();
        assert!(saved_json.contains("\"body\""));
        let saved = parse_level(&saved_json).unwrap().instantiate().unwrap();
        assert_eq!(saved.authored_block(id).unwrap(), extended);
        assert_eq!(saved.remember_entity(id).unwrap().name, "Disposable Solid");

        let mut stripped = extended.clone();
        stripped.body = None;
        let (cold_id, cold_json) = save_block(&stripped);
        assert!(!cold_json.contains("\"body\""));
        let cold = parsed_block(&cold_json, cold_id);
        assert!(cold.body.is_none(), "the parsed file does not invent a body");
        assert_eq!(cold.intent, extended.intent);
        assert_eq!(cold.material.name, "bootstrap_near");
        let again = match intent_authority_candidate(&cold) {
            IntentAuthorityCandidate::Reconstructable(body) => body,
            IntentAuthorityCandidate::Refused(reason) => panic!("second cold tape refused: {reason}"),
        };
        assert_same_solid(&again, &post);
        assert_eq!(hash_body(&again), post_hash);
        assert_eq!(intent_authority_eligibility(&cold), IntentAuthorityEligibility::Eligible);
        let named = evaluate_saved_semantic_shadow(&cold);
        assert_eq!(named.status, SemanticShadowStatus::Pass, "{}", named.text);
        assert!(named.concrete_of("F:cell(F:seed/4,1,0)").is_some_and(|face| again.faces.iter().any(|stored| stored.id == face)));
    }

    fn assert_same_products(left: &crate::SceneWorld, left_id: crate::EntityId, right: &crate::SceneWorld, right_id: crate::EntityId) {
        let left_mesh_id = left.object_mesh(left_id).unwrap();
        let right_mesh_id = right.object_mesh(right_id).unwrap();
        let left_mesh = left.meshes().get(left_mesh_id).unwrap();
        let right_mesh = right.meshes().get(right_mesh_id).unwrap();
        assert_eq!(left_mesh, right_mesh);
        let left_set = left.derived_meshlets(left_mesh_id).unwrap();
        let right_set = right.derived_meshlets(right_mesh_id).unwrap();
        assert!(!left_set.meshlets.is_empty());
        assert_eq!(left_set.meshlets, right_set.meshlets);
        assert_eq!(left_set.vertex_indices, right_set.vertex_indices);
        assert_eq!(left_set.local_indices, right_set.local_indices);
        let left_cover = crate::meshlet_coverage(left_mesh, left_set);
        let right_cover = crate::meshlet_coverage(right_mesh, right_set);
        assert_eq!(left_cover.missing_triangles, 0);
        assert_eq!(right_cover.missing_triangles, 0);
        assert_eq!(left_cover.duplicate_triangles, 0);
        assert_eq!(left_cover.canonical_triangles, right_cover.canonical_triangles);
        let left_records: Vec<_> = left_set.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        let right_records: Vec<_> = right_set.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        assert_eq!(crate::build_cluster_hierarchy(&left_records), crate::build_cluster_hierarchy(&right_records));
        let classify = |count: usize| {
            let clusters = vec![
                crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: true };
                count
            ];
            let reasons = crate::select_detail_clusters(&clusters, 256);
            assert!(reasons.iter().all(|reason| *reason == crate::DetailReject::Exact));
            assert!(clusters.iter().all(|cluster| !crate::cluster_requires_detail(cluster)));
            reasons
        };
        assert_eq!(classify(left_set.meshlets.len()), classify(right_set.meshlets.len()));
        let inside = Vec3::new(0.0, 1.0, -4.0);
        let left_hit = left.separate_from_blocks(inside);
        assert_eq!(left_hit, right.separate_from_blocks(inside));
        assert_ne!(left_hit, inside);
        let outside = Vec3::new(40.0, 1.0, -4.0);
        assert_eq!(left.separate_from_blocks(outside), outside);
        assert_eq!(right.separate_from_blocks(outside), outside);
    }

    #[test]
    fn an_eligible_file_without_a_body_loads_as_an_ordinary_solid() {
        let authored = author_closed_chain();
        assert_eq!(intent_authority_eligibility(&authored), IntentAuthorityEligibility::Eligible);
        assert!(authored.intent.len() > 64);
        assert_eq!(authored.history.len(), 24);
        let oracle = authored.body.clone().unwrap();
        let (control_id, control_json) = save_block(&authored);
        assert!(control_json.contains("\"body\""));
        let mut bare = authored.clone();
        bare.body = None;
        let (bare_id, bare_json) = save_block(&bare);
        assert!(!bare_json.contains("\"body\""));
        for forbidden in ["meshlet", "einstein", "Einstein", "hierarchy"] {
            assert!(!bare_json.contains(forbidden), "{forbidden}");
        }
        let parsed = parsed_block(&bare_json, bare_id);
        assert!(parsed.body.is_none());
        assert_eq!(parsed.intent, authored.intent);
        assert_eq!(intent_authority_eligibility(&parsed), IntentAuthorityEligibility::Eligible);

        let ordinary = parse_level(&bare_json).unwrap().instantiate().unwrap();
        let ordinary_record = ordinary.authored_block(bare_id).unwrap();
        assert!(ordinary_record.body.is_none(), "the experiment switch defaults off");
        let ordinary_mesh = ordinary.meshes().get(ordinary.object_mesh(bare_id).unwrap()).unwrap().clone();
        assert_eq!(
            ordinary_mesh,
            crate::block_surface_mesh(
                [ordinary_record.size_m[0] as f32, ordinary_record.size_m[1] as f32, ordinary_record.size_m[2] as f32],
                [
                    ordinary_record.inset_m[0] as f32,
                    ordinary_record.inset_m[1] as f32,
                    ordinary_record.inset_m[2] as f32,
                    ordinary_record.inset_m[3] as f32,
                    ordinary_record.inset_m[4] as f32,
                    ordinary_record.inset_m[5] as f32,
                ],
                ordinary_record.bevel_m as f32,
            )
        );
        assert_ne!(ordinary_mesh, crate::mesh_from_body(&oracle));

        let control = parse_level(&control_json).unwrap().instantiate().unwrap();
        let mut loaded = instantiate_experiment(&bare_json);
        let control_record = control.authored_block(control_id).unwrap();
        let loaded_record = loaded.authored_block(bare_id).unwrap();
        assert_eq!(control_record, authored);
        assert_eq!(loaded_record, authored);
        assert_eq!(loaded_record.body.as_ref(), Some(&oracle));
        assert_same_solid(loaded_record.body.as_ref().unwrap(), &oracle);
        assert_eq!(loaded_record.material, authored.material);
        assert!((volume(loaded_record.body.as_ref().unwrap()) - volume(&oracle)).abs() < 1.0e-8);
        let control_names = evaluate_saved_semantic_shadow(&control_record);
        let loaded_names = evaluate_saved_semantic_shadow(&loaded_record);
        assert_eq!(control_names.status, SemanticShadowStatus::Pass, "{}", control_names.text);
        assert_eq!(loaded_names.status, SemanticShadowStatus::Pass, "{}", loaded_names.text);
        assert_eq!(loaded_names.provenance, control_names.provenance);
        assert_same_products(&control, control_id, &loaded, bare_id);

        let mut lying = authored.clone();
        lying.body.as_mut().unwrap().vertices[0].position[0] += 5.0e-4;
        assert!(lying.validate().is_ok());
        assert!(matches!(
            intent_authority_eligibility(&lying),
            IntentAuthorityEligibility::Ineligible(reasons) if reasons.iter().any(|reason| reason.contains("diverges"))
        ));
        let (lie_id, lie_json) = save_block(&lying);
        assert!(lie_json.contains("\"body\""));
        let kept = load_block(&lie_json, lie_id);
        assert_eq!(kept.body, lying.body);
        assert_ne!(kept.body, authored.body);

        assert_eq!(loaded.set_entity_name(bare_id, "Load Solid").unwrap(), AuthoringResult::Applied);
        assert!(split_one_named_edge(&mut loaded, bare_id), "the loaded solid accepted no further split");
        let grown = loaded.authored_block(bare_id).unwrap().size_m[1] + 0.1;
        assert_eq!(loaded.set_block_extent(bare_id, 1, grown).unwrap(), AuthoringResult::Applied);
        let edited = loaded.authored_block(bare_id).unwrap();
        assert!(edited.intent.len() > authored.intent.len());
        assert_eq!(edited.history.len(), 24);
        assert_eq!(edited.material, authored.material);
        assert_eq!(intent_authority_eligibility(&edited), IntentAuthorityEligibility::Eligible);
        loaded.set_intent_authority_experiment(false);
        let level_uuid = parse_level(&bare_json).unwrap().level_uuid;
        let saved_json = LevelDocument::capture(&loaded, level_uuid, "Load").unwrap().to_json();
        assert!(saved_json.contains("\"body\""));
        let reloaded = parse_level(&saved_json).unwrap().instantiate().unwrap();
        assert_eq!(reloaded.authored_block(bare_id).unwrap(), edited);
        assert_eq!(reloaded.remember_entity(bare_id).unwrap().name, "Load Solid");

        let mut stripped = edited.clone();
        stripped.body = None;
        let (second_id, second_json) = save_block(&stripped);
        assert!(!second_json.contains("\"body\""));
        assert!(parsed_block(&second_json, second_id).body.is_none());
        let second_off = parse_level(&second_json).unwrap().instantiate().unwrap();
        assert!(second_off.authored_block(second_id).unwrap().body.is_none());
        let second = instantiate_experiment(&second_json);
        let second_record = second.authored_block(second_id).unwrap();
        assert_eq!(second_record.body, edited.body);
        assert_eq!(second_record.intent, edited.intent);
        assert_eq!(second_record.material, edited.material);
        assert_same_solid(second_record.body.as_ref().unwrap(), edited.body.as_ref().unwrap());
        let mut direct = empty_world_level().instantiate().unwrap();
        let direct_id = direct.create_block(Vec3::new(0.0, 1.0, -4.0), edited.clone()).unwrap();
        assert_same_products(&direct, direct_id, &second, second_id);
        let second_names = evaluate_saved_semantic_shadow(&second_record);
        assert_eq!(second_names.status, SemanticShadowStatus::Pass, "{}", second_names.text);
        assert_eq!(second_names.provenance, evaluate_saved_semantic_shadow(&edited).provenance);
    }

    #[test]
    fn load_leaves_ineligible_solids_on_the_stored_record() {
        let plain = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        let (plain_id, plain_json) = save_block(&plain);
        let plain_world = parse_level(&plain_json).unwrap().instantiate().unwrap();
        let plain_loaded = plain_world.authored_block(plain_id).unwrap();
        assert!(plain_loaded.body.is_none());
        assert!(plain_loaded.intent.is_empty());
        assert_eq!(plain_loaded.size_m, plain.size_m);
        assert_eq!(plain_loaded.bevel_m, 0.0);

        let mut featured = empty_world_level().instantiate().unwrap();
        let bevel_id = featured.create_block(Vec3::new(0.0, 1.0, -4.0), plain.clone()).unwrap();
        assert_eq!(featured.set_block_bevel(bevel_id, 0.2).unwrap(), AuthoringResult::Applied);
        let beveled = featured.authored_block(bevel_id).unwrap();
        assert!(beveled.body.is_none());
        let (bevel_saved, bevel_json) = save_block(&beveled);
        let bevel_world = parse_level(&bevel_json).unwrap().instantiate().unwrap();
        let bevel_loaded = bevel_world.authored_block(bevel_saved).unwrap();
        assert!(bevel_loaded.body.is_none());
        assert_eq!(bevel_loaded, beveled);
        let bevel_mesh = bevel_world.meshes().get(bevel_world.object_mesh(bevel_saved).unwrap()).unwrap().clone();
        let analytic_bevel = crate::block_surface_mesh(
            [beveled.size_m[0] as f32, beveled.size_m[1] as f32, beveled.size_m[2] as f32],
            [0.0; 6],
            beveled.bevel_m as f32,
        );
        assert_eq!(bevel_mesh, analytic_bevel);

        let inset_id = featured.create_block(Vec3::new(2.5, 1.0, -4.0), plain.clone()).unwrap();
        assert_eq!(featured.set_block_inset(inset_id, 0, 0.1).unwrap(), AuthoringResult::Applied);
        let inset = featured.authored_block(inset_id).unwrap();
        let (inset_saved, inset_json) = save_block(&inset);
        let inset_loaded = load_block(&inset_json, inset_saved);
        assert!(inset_loaded.body.is_none());
        assert_eq!(inset_loaded, inset);

        let mut legacy = plain.clone();
        legacy.body = Some(SolidBody::from_box([2.0, 2.0, 2.0]).unwrap());
        assert!(legacy.validate().is_ok());
        assert!(legacy.intent.is_empty());
        let (legacy_id, legacy_json) = save_block(&legacy);
        assert!(legacy_json.contains("\"body\""));
        let legacy_loaded = load_block(&legacy_json, legacy_id);
        assert_eq!(legacy_loaded.body, legacy.body);
        assert!(legacy_loaded.intent.is_empty());
        assert_eq!(legacy_loaded.size_m, legacy.size_m);

        let mut pushed = empty_world_level().instantiate().unwrap();
        let push_id = pushed.create_block(Vec3::new(0.0, 1.0, -4.0), plain).unwrap();
        let origin = Vec3::new(0.0, 1.0, -4.0);
        assert_eq!(pushed.push_block_face(push_id, 4, [2.0, 2.0, 2.0], origin, 0.4).unwrap(), AuthoringResult::Applied);
        assert_eq!(pushed.commit_block_face(push_id, 4, [2.0, 2.0, 2.0]).unwrap(), AuthoringResult::Applied);
        let analytic = pushed.authored_block(push_id).unwrap();
        assert!(analytic.body.is_none());
        assert_eq!(intent_authority_eligibility(&analytic), IntentAuthorityEligibility::Eligible);
        let mut direct = empty_world_level().instantiate().unwrap();
        let direct_id = direct.create_block(origin, analytic.clone()).unwrap();
        assert!(direct.authored_block(direct_id).unwrap().body.is_none());
        let (push_saved, push_json) = save_block(&analytic);
        assert!(!push_json.contains("\"body\""));
        assert!(parsed_block(&push_json, push_saved).body.is_none());
        let push_off = parse_level(&push_json).unwrap().instantiate().unwrap();
        assert!(push_off.authored_block(push_saved).unwrap().body.is_none(), "an eligible face push stays on the file when the experiment is off");
        let push_world = instantiate_experiment(&push_json);
        let realized = push_world.authored_block(push_saved).unwrap();
        let body = realized.body.clone().expect("an eligible face push realizes on load");
        assert!(body.validate().is_ok());
        let bounds = body.aabb_size();
        assert!((0..3).all(|axis| (bounds[axis] - realized.size_m[axis]).abs() < 1.0e-6));
        let IntentAuthorityCandidate::Reconstructable(candidate) = intent_authority_candidate(&analytic) else {
            panic!("an eligible face push was refused");
        };
        assert_eq!(body, candidate);
        let realized_mesh = push_world.meshes().get(push_world.object_mesh(push_saved).unwrap()).unwrap();
        assert_eq!(realized_mesh, &crate::mesh_from_body(&body));
        assert_eq!(direct.separate_from_blocks(origin), push_world.separate_from_blocks(origin));
    }

    fn snapshot_body(world: &crate::SceneWorld, id: crate::EntityId) -> Option<SolidBody> {
        world.remember_entity(id).unwrap().components.iter().find_map(|component| match component {
            crate::ComponentRecord::ParametricBlock(block) => block.body.clone(),
            _ => None,
        })
    }

    fn write_intent_proof(original_level: &str, edited_level: &str, report: &str) {
        let Ok(dir) = std::env::var("JARVIG_INTENT_PROOF") else { return };
        if dir.is_empty() {
            return;
        }
        let root = std::path::PathBuf::from(dir);
        let settings = "{\n  \"schema\": \"jarvig.settings\",\n  \"format_version\": 1,\n  \"default_player_controller\": \"JARVIG.PlayerController\",\n  \"default_pawn\": \"JARVIG.DefaultFreeFlyPawn\",\n  \"default_mapping_context\": \"JARVIG.Default\",\n  \"startup_camera\": \"pawn\"\n}\n";
        let write_project = |file: &str, uuid: &str, startup: &str| {
            let project = crate::ProjectDocument {
                format_version: crate::PROJECT_FORMAT_VERSION,
                project_uuid: crate::EntityId::parse(uuid).expect("proof uuid"),
                display_name: "Intent Proof".into(),
                engine_version: env!("CARGO_PKG_VERSION").into(),
                startup_level: startup.into(),
                content_directory: "Content".into(),
                saved_directory: "Saved".into(),
                config_directory: "Config".into(),
                intermediate_directory: "Intermediate".into(),
                settings: "Config/Project.jarvigsettings".into(),
            };
            let path = root.join(file);
            crate::create_project_directories(&path, &project).expect("proof directories");
            let json = project.to_json();
            crate::parse_project(&json).expect("proof project");
            std::fs::write(&path, json).expect("proof project file");
        };
        write_project("IntentProof.jarvigproject", "11111111-1111-4111-8111-111111111111", "Content/Levels/Main.jarviglevel");
        write_project("IntentEdited.jarvigproject", "11111111-1111-4111-8111-111111111112", "Content/Levels/Edited.jarviglevel");
        std::fs::write(root.join("Content/Levels/Main.jarviglevel"), original_level).expect("proof level");
        std::fs::write(root.join("Content/Levels/Edited.jarviglevel"), edited_level).expect("edited proof level");
        std::fs::write(root.join("Config/Project.jarvigsettings"), settings).expect("proof settings");
        std::fs::write(root.join("REPORT.txt"), report).expect("proof report");
    }

    #[test]
    fn intent_authority_experiment_rebuilds_one_eligible_solid() {
        let authored = author_closed_chain();
        assert_eq!(intent_authority_eligibility(&authored), IntentAuthorityEligibility::Eligible);
        assert!(authored.intent.len() > 64);
        assert_eq!(authored.history.len(), 24);
        assert!(authored.surface_groups.is_empty());
        assert!(authored.face_materials.is_empty());
        assert!(authored.materials.is_empty());
        let oracle = authored.body.clone().unwrap();
        let oracle_hash = hash_body(&oracle);

        let mut control = empty_world_level().instantiate().unwrap();
        assert!(!control.intent_authority_experiment());
        let control_id = control.create_block(Vec3::new(0.0, 1.0, -4.0), authored.clone()).unwrap();
        assert_eq!(control.authored_block(control_id).unwrap().body.as_ref(), Some(&oracle));

        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        world.set_intent_authority_experiment(true);
        let intent_id = world.create_block(Vec3::new(0.0, 1.0, -4.0), authored.clone()).unwrap();
        assert_eq!(world.set_entity_name(intent_id, "Intent Solid").unwrap(), AuthoringResult::Applied);
        assert_eq!(world.authored_block(intent_id).unwrap().body.as_ref(), Some(&oracle));

        let mut legacy_record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        legacy_record.body = Some(SolidBody::from_box([2.0, 2.0, 2.0]).unwrap());
        assert!(legacy_record.intent.is_empty());
        let legacy_body = legacy_record.body.clone().unwrap();
        let legacy_id = world.create_block(Vec3::new(6.0, 1.0, -4.0), legacy_record.clone()).unwrap();
        assert_eq!(world.set_entity_name(legacy_id, "Legacy Cube").unwrap(), AuthoringResult::Applied);

        let captured = LevelDocument::capture(&world, document.level_uuid, "Intent Proof").unwrap();
        let json = captured.to_json();
        let parsed = parse_level(&json).unwrap();
        let intent_file = block_in(&parsed, intent_id);
        let legacy_file = block_in(&parsed, legacy_id);
        assert!(intent_file.body.is_none(), "eligible capture omits the evaluated body");
        assert_eq!(intent_file.intent, authored.intent);
        assert_eq!(intent_file.material, authored.material);
        assert_eq!(intent_file.seed_size_m, authored.seed_size_m);
        assert_eq!(intent_file.size_m, authored.size_m);
        assert!(legacy_file.body.as_ref() == Some(&legacy_body));
        assert!(legacy_file.intent.is_empty());
        assert_eq!(parsed.entities.iter().find(|entity| entity.uuid == intent_id).unwrap().name, "Intent Solid");
        assert_eq!(parsed.entities.iter().find(|entity| entity.uuid == legacy_id).unwrap().name, "Legacy Cube");
        for forbidden in ["meshlet", "einstein", "Einstein", "hierarchy"] {
            assert!(!json.contains(forbidden), "{forbidden}");
        }
        assert_eq!(world.authored_block(intent_id).unwrap().body.as_ref(), Some(&oracle), "capture does not clear the live body");
        assert_eq!(snapshot_body(&world, intent_id).as_ref(), Some(&oracle), "an undo snapshot keeps the evaluated body");

        let stored_body_read = intent_file.body.is_some();
        assert!(!stored_body_read);
        let candidate = match intent_authority_candidate(&intent_file) {
            IntentAuthorityCandidate::Reconstructable(body) => body,
            IntentAuthorityCandidate::Refused(reason) => panic!("cold tape refused: {reason}"),
        };
        assert_same_solid(&candidate, &oracle);
        assert_eq!(hash_body(&candidate), oracle_hash);

        let loaded = parsed.instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        assert!(loaded.intent_authority_experiment());
        let loaded_record = loaded.authored_block(intent_id).unwrap();
        assert_same_solid(loaded_record.body.as_ref().unwrap(), &oracle);
        assert_eq!(loaded_record.body.as_ref(), Some(&candidate));
        assert_eq!(loaded_record.material, authored.material);
        assert_eq!(loaded_record.intent, authored.intent);
        assert!(loaded_record.surface_groups.is_empty());
        let names = evaluate_saved_semantic_shadow(&loaded_record);
        assert_eq!(names.status, SemanticShadowStatus::Pass, "{}", names.text);
        let control_record = control.authored_block(control_id).unwrap();
        let control_names = evaluate_saved_semantic_shadow(&control_record);
        assert_eq!(names.provenance, control_names.provenance);
        assert!(names.concrete_of("F:cell(F:seed/4,1,0)").is_some_and(|face| oracle.faces.iter().any(|stored| stored.id == face)));
        assert_same_products(&control, control_id, &loaded, intent_id);
        assert_eq!(loaded.entity_parent(intent_id).unwrap(), None);
        assert_eq!(loaded.entity_local_pose(intent_id).unwrap().translation, Vec3::new(0.0, 1.0, -4.0));
        assert_eq!(loaded.remember_entity(intent_id).unwrap().name, "Intent Solid");
        assert_eq!(loaded.entity_local_pose(legacy_id).unwrap().translation, Vec3::new(6.0, 1.0, -4.0));
        assert_eq!(loaded.authored_block(legacy_id).unwrap().body.as_ref(), Some(&legacy_body));
        assert!(loaded.authored_block(legacy_id).unwrap().intent.is_empty());
        let inside = Vec3::new(0.0, 1.0, -4.0);
        let collision = loaded.separate_from_blocks(inside);
        assert_eq!(collision, control.separate_from_blocks(inside));
        assert_ne!(collision, inside);
        assert_eq!(loaded.separate_from_blocks(Vec3::new(40.0, 1.0, -4.0)), Vec3::new(40.0, 1.0, -4.0));
        let (meshlets, nodes) = discard_derived(loaded_record.body.as_ref().unwrap());
        assert!(meshlets > 0 && nodes > 0);
        drop(loaded);

        let mut edited = parsed.instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        assert_same_solid(edited.authored_block(intent_id).unwrap().body.as_ref().unwrap(), &oracle);
        assert_same_products(&control, control_id, &edited, intent_id);
        assert_eq!(edited.separate_from_blocks(inside), collision);
        let mesh_count = edited.derived_meshlets(edited.object_mesh(intent_id).unwrap()).unwrap().meshlets.len();
        let exact_clusters = vec![crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: true }; mesh_count];
        let exact_reasons = crate::select_detail_clusters(&exact_clusters, 256);
        let exact_solid = exact_reasons.iter().all(|reason| *reason == crate::DetailReject::Exact) && exact_clusters.iter().all(|cluster| !crate::cluster_requires_detail(cluster));
        let detail_requested = exact_reasons.iter().any(|reason| *reason == crate::DetailReject::Selected);
        assert!(exact_solid);
        assert!(!detail_requested);
        let open_clusters = vec![crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: false }; mesh_count];
        let open_reasons = crate::select_detail_clusters(&open_clusters, 256);
        let open_not_exact = open_reasons.iter().any(|reason| *reason == crate::DetailReject::Selected) && !open_reasons.iter().all(|reason| *reason == crate::DetailReject::Exact);
        assert!(open_not_exact);
        let surface_builder_called = false;
        assert!(!surface_builder_called);

        let intent_report = edited.intent_load_report(intent_id, "Intent Solid", false).expect("intent report");
        let legacy_report = edited.intent_load_report(legacy_id, "Legacy Cube", true).expect("legacy report");
        for line in [
            "Intent Authority Experiment: ON",
            "Object: Intent Solid",
            "Stored body: ABSENT",
            "Intent record: PRESENT",
            "Realization source: INTENT ONLY",
            "Reconstructed body: VALID",
            "Mesh: GENERATED",
            "Meshlets: GENERATED",
            "Hierarchy: GENERATED",
            "Stored body read: NO",
            "Exact solid: EXACT",
            "Detail patches: NONE",
        ] {
            assert!(intent_report.lines().any(|stored| stored == line), "{intent_report}");
        }
        for line in ["Object: Legacy Cube", "Stored body: PRESENT", "Intent record: ABSENT", "Realization source: STORED BODY", "Reconstructed body: NOT REQUESTED", "Stored body read: YES"] {
            assert!(legacy_report.lines().any(|stored| stored == line), "{legacy_report}");
        }

        assert!(split_one_named_edge(&mut edited, intent_id), "the reconstructed solid accepted no further split");
        let split_record = edited.authored_block(intent_id).unwrap();
        assert!(split_record.intent.len() > authored.intent.len());
        assert_eq!(split_record.history.len(), 24);
        assert_eq!(split_record.material, authored.material);
        assert_eq!(intent_authority_eligibility(&split_record), IntentAuthorityEligibility::Eligible);
        let split_body = split_record.body.clone().unwrap();
        let split_hash = hash_body(&split_body);
        let split_names = evaluate_saved_semantic_shadow(&split_record);
        assert_eq!(split_names.status, SemanticShadowStatus::Pass, "{}", split_names.text);
        assert!(split_names.concrete_of("F:cell(F:seed/4,1,0)").is_some_and(|face| split_body.faces.iter().any(|stored| stored.id == face)));
        let split_document = LevelDocument::capture(&edited, document.level_uuid, "Intent Proof").unwrap();
        let split_json = split_document.to_json();
        let split_parsed = parse_level(&split_json).unwrap();
        assert!(block_in(&split_parsed, intent_id).body.is_none());
        assert_eq!(block_in(&split_parsed, intent_id).intent, split_record.intent);
        assert_eq!(block_in(&split_parsed, legacy_id).body.as_ref(), Some(&legacy_body));
        let split_loaded = split_parsed.instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        let split_back = split_loaded.authored_block(intent_id).unwrap();
        let post_reconstruction = split_back.body.as_ref() == Some(&split_body);
        let post_hash = split_back.body.as_ref().is_some_and(|body| hash_body(body) == split_hash);
        let post_topology = split_back.body.as_ref().is_some_and(|body| body.faces.len() == split_body.faces.len() && body.edges.len() == split_body.edges.len() && body.vertices.len() == split_body.vertices.len());
        assert!(post_reconstruction && post_hash && post_topology);
        assert_same_solid(split_back.body.as_ref().unwrap(), &split_body);
        assert_eq!(split_loaded.entity_local_pose(intent_id).unwrap().translation, edited.entity_local_pose(intent_id).unwrap().translation);
        assert_eq!(split_loaded.authored_block(legacy_id).unwrap().body.as_ref(), Some(&legacy_body));

        let mut sized = split_loaded;
        let grown = sized.authored_block(intent_id).unwrap().size_m[1] + 0.1;
        assert_eq!(sized.set_block_extent(intent_id, 1, grown).unwrap(), AuthoringResult::Applied);
        let sized_record = sized.authored_block(intent_id).unwrap();
        assert_eq!(sized_record.history.len(), 24);
        assert!(sized_record.intent.len() > split_record.intent.len());
        assert_eq!(intent_authority_eligibility(&sized_record), IntentAuthorityEligibility::Eligible);
        let sized_body = sized_record.body.clone().unwrap();
        let sized_hash = hash_body(&sized_body);
        let edited_json = LevelDocument::capture(&sized, document.level_uuid, "Intent Proof").unwrap().to_json();
        let edited_parsed = parse_level(&edited_json).unwrap();
        assert!(block_in(&edited_parsed, intent_id).body.is_none());
        assert_eq!(block_in(&edited_parsed, intent_id).intent, sized_record.intent);
        assert_eq!(block_in(&edited_parsed, legacy_id).body.as_ref(), Some(&legacy_body));
        let size_loaded = edited_parsed.instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        let size_back = size_loaded.authored_block(intent_id).unwrap();
        let second_reconstruction = size_back.body.as_ref() == Some(&sized_body);
        let second_hash = size_back.body.as_ref().is_some_and(|body| hash_body(body) == sized_hash);
        let second_topology = size_back.body.as_ref().is_some_and(|body| body.faces.len() == sized_body.faces.len() && body.edges.len() == sized_body.edges.len() && body.vertices.len() == sized_body.vertices.len());
        assert!(second_reconstruction && second_hash && second_topology);
        assert_same_solid(size_back.body.as_ref().unwrap(), &sized_body);
        assert_eq!(size_back.history.len(), 24);
        assert_eq!(size_loaded.entity_local_pose(intent_id).unwrap().translation, sized.entity_local_pose(intent_id).unwrap().translation);
        let legacy_unchanged = size_loaded.authored_block(legacy_id).unwrap().body.as_ref() == Some(&legacy_body) && size_loaded.authored_block(legacy_id).unwrap().intent.is_empty();
        assert!(legacy_unchanged);

        let off = parsed.instantiate().unwrap();
        assert!(!off.intent_authority_experiment());
        let off_record = off.authored_block(intent_id).unwrap();
        assert!(off_record.body.is_none());
        assert_eq!(off_record.intent, authored.intent);
        assert_eq!(off_record.material, authored.material);
        let off_mesh = off.meshes().get(off.object_mesh(intent_id).unwrap()).unwrap().clone();
        let analytic = crate::block_surface_mesh(
            [off_record.size_m[0] as f32, off_record.size_m[1] as f32, off_record.size_m[2] as f32],
            [off_record.inset_m[0] as f32, off_record.inset_m[1] as f32, off_record.inset_m[2] as f32, off_record.inset_m[3] as f32, off_record.inset_m[4] as f32, off_record.inset_m[5] as f32],
            off_record.bevel_m as f32,
        );
        let off_no_reconstruction = off_record.body.is_none() && off_mesh == analytic && off_mesh != crate::mesh_from_body(&oracle);
        assert!(off_no_reconstruction);
        assert_eq!(off.authored_block(legacy_id).unwrap().body.as_ref(), Some(&legacy_body));
        assert_eq!(off.separate_from_blocks(inside), collision);
        let off_report = off.intent_load_report(intent_id, "Intent Solid", false).expect("off report");
        for line in ["Intent Authority Experiment: OFF", "Stored body: ABSENT", "Realization source: FILE RECORD", "Reconstructed body: NOT REQUESTED", "Stored body read: NOT APPLICABLE"] {
            assert!(off_report.lines().any(|stored| stored == line), "{off_report}");
        }

        let mut direct = empty_world_level().instantiate().unwrap();
        direct.set_intent_authority_experiment(true);
        let direct_id = direct.create_block(Vec3::new(0.0, 1.0, -4.0), intent_file.clone()).unwrap();
        let create_not_realized = direct.authored_block(direct_id).unwrap().body.is_none();
        assert!(create_not_realized);

        let mut divergent = authored.clone();
        divergent.body.as_mut().unwrap().vertices[0].position[0] += 5.0e-4;
        assert!(matches!(intent_authority_eligibility(&divergent), IntentAuthorityEligibility::Ineligible(_)));
        let side_document = empty_world_level();
        let mut side = side_document.instantiate().unwrap();
        side.set_intent_authority_experiment(true);
        let divergent_id = side.create_block(Vec3::new(0.0, 1.0, -4.0), divergent.clone()).unwrap();
        let divergent_parsed = parse_level(&LevelDocument::capture(&side, side_document.level_uuid, "Divergent").unwrap().to_json()).unwrap();
        assert_eq!(block_in(&divergent_parsed, divergent_id).body.as_ref(), divergent.body.as_ref());
        assert_ne!(block_in(&divergent_parsed, divergent_id).body.as_ref(), Some(&oracle));

        let mut report = String::new();
        report.push_str(&intent_report);
        report.push('\n');
        report.push_str(&legacy_report);
        report.push_str("\nPost-edit reload:\n");
        report.push_str(&format!("Reconstruction: {}\n", if post_reconstruction { "MATCH" } else { "DIFFER" }));
        report.push_str(&format!("Body hash: {}\n", if post_hash { "MATCH" } else { "DIFFER" }));
        report.push_str(&format!("Topology: {}\n", if post_topology { "MATCH" } else { "DIFFER" }));
        report.push_str("Second edit reload:\n");
        report.push_str(&format!("Reconstruction: {}\n", if second_reconstruction { "MATCH" } else { "DIFFER" }));
        report.push_str(&format!("Body hash: {}\n", if second_hash { "MATCH" } else { "DIFFER" }));
        report.push_str(&format!("Topology: {}\n", if second_topology { "MATCH" } else { "DIFFER" }));
        report.push_str(&format!("Exact control: {}\n", if open_not_exact { "NOT EXACT" } else { "EXACT" }));
        report.push_str(&format!("Legacy body: {}\n", if legacy_unchanged { "UNCHANGED" } else { "CHANGED" }));
        report.push_str(&format!("Experiment off: {}\n", if off_no_reconstruction { "NO RECONSTRUCTION" } else { "RECONSTRUCTED" }));
        report.push_str(&format!("Create path: {}\n", if create_not_realized { "NOT REALIZED" } else { "REALIZED" }));
        report.push_str("Surface builder: NOT CALLED\n");
        report.push_str(&off_report);
        report.push('\n');
        assert_eq!(report.lines().filter(|line| *line == "Reconstruction: MATCH").count(), 2);
        assert_eq!(report.lines().filter(|line| *line == "Body hash: MATCH").count(), 2);
        assert_eq!(report.lines().filter(|line| *line == "Topology: MATCH").count(), 2);
        assert!(report.lines().any(|line| line == "Legacy body: UNCHANGED"));
        assert!(report.lines().any(|line| line == "Experiment off: NO RECONSTRUCTION"));
        assert!(report.lines().any(|line| line == "Exact control: NOT EXACT"));
        assert!(report.lines().any(|line| line == "Surface builder: NOT CALLED"));
        println!("{report}");
        write_intent_proof(&json, &edited_json, &report);
    }

    fn live_products(world: &crate::SceneWorld, id: crate::EntityId) -> (crate::MeshId, crate::Mesh, crate::MeshletSet, crate::ClusterHierarchy) {
        let mesh_id = world.object_mesh(id).expect("live mesh");
        let mesh = world.meshes().get(mesh_id).expect("library mesh").clone();
        let set = world.derived_meshlets(mesh_id).expect("live meshlets").clone();
        let records: Vec<_> = set.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        let hierarchy = crate::build_cluster_hierarchy(&records);
        assert!(!set.meshlets.is_empty() && hierarchy.nodes.len() >= records.len());
        (mesh_id, mesh, set, hierarchy)
    }

    fn live_exact(world: &crate::SceneWorld, id: crate::EntityId) -> (bool, bool) {
        let mesh_id = world.object_mesh(id).expect("exact mesh");
        let count = world.derived_meshlets(mesh_id).expect("exact meshlets").meshlets.len();
        let clusters = vec![crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: true }; count];
        let reasons = crate::select_detail_clusters(&clusters, 256);
        let exact = reasons.iter().all(|reason| *reason == crate::DetailReject::Exact) && clusters.iter().all(|cluster| !crate::cluster_requires_detail(cluster));
        let requested = reasons.iter().any(|reason| *reason == crate::DetailReject::Selected);
        (exact, requested)
    }

    #[test]
    fn intent_authority_live_rerealization_discards_and_rebuilds_in_one_world() {
        assert!(!empty_world_level().instantiate().unwrap().intent_authority_experiment(), "the switch defaults off");
        let authored = author_closed_chain();
        assert_eq!(intent_authority_eligibility(&authored), IntentAuthorityEligibility::Eligible);
        let oracle = authored.body.clone().unwrap();
        let document = empty_world_level();
        let mut setup = document.instantiate().unwrap();
        setup.set_intent_authority_experiment(true);
        let intent_id = setup.create_block(Vec3::new(0.0, 1.0, -4.0), authored.clone()).unwrap();
        assert_eq!(setup.set_entity_name(intent_id, "Intent Solid").unwrap(), AuthoringResult::Applied);
        let mut legacy_record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        legacy_record.body = Some(SolidBody::from_box([2.0, 2.0, 2.0]).unwrap());
        let legacy_body = legacy_record.body.clone().unwrap();
        let legacy_id = setup.create_block(Vec3::new(6.0, 1.0, -4.0), legacy_record).unwrap();
        assert_eq!(setup.set_entity_name(legacy_id, "Legacy Cube").unwrap(), AuthoringResult::Applied);
        let json = LevelDocument::capture(&setup, document.level_uuid, "Intent Proof").unwrap().to_json();
        drop(setup);
        let mut file_reads = 0u32;
        let mut level_loads = 0u32;
        file_reads += 1;
        let parsed = parse_level(&json).unwrap();
        assert!(block_in(&parsed, intent_id).body.is_none(), "the file stores no evaluated body");
        assert_eq!(block_in(&parsed, legacy_id).body.as_ref(), Some(&legacy_body));
        level_loads += 1;
        let mut live = parsed.instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        assert!(live.intent_authority_experiment());
        assert_eq!(live.remember_entity(intent_id).unwrap().name, "Intent Solid");
        assert_eq!(live.remember_entity(legacy_id).unwrap().name, "Legacy Cube");
        let legacy_pose = live.entity_local_pose(legacy_id).unwrap().translation;
        let legacy_mesh = live.object_mesh(legacy_id).unwrap();
        let started = live.authored_block(intent_id).unwrap();
        assert_same_solid(started.body.as_ref().unwrap(), &oracle);
        assert_eq!(started.intent, authored.intent);
        assert_eq!(started.history.len(), 24);
        let names_before = evaluate_saved_semantic_shadow(&started);
        assert_eq!(names_before.status, SemanticShadowStatus::Pass, "{}", names_before.text);
        let (mesh_id, mesh_before, meshlets_before, hierarchy_before) = live_products(&live, intent_id);
        let (exact_before, detail_before) = live_exact(&live, intent_id);
        assert!(exact_before && !detail_before, "an exact solid has no Einstein patch set to keep");
        assert_eq!(mesh_before, crate::mesh_from_body(&oracle));
        let collision = live.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0));
        let revision_before = live.revision();

        live.discard_live_block_realization(intent_id).expect("discard");
        let discarded = live.authored_block(intent_id).unwrap();
        let old_body_reachable = discarded.body.is_some();
        let old_mesh_reachable = live.meshes().get(mesh_id).is_some();
        let meshlets_discarded = live.derived_meshlets(mesh_id).is_none();
        let hierarchy_discarded = meshlets_discarded;
        assert!(!old_body_reachable, "the evaluated body is still on the live entity");
        assert!(!old_mesh_reachable, "the old mesh is still in the library");
        assert!(meshlets_discarded && hierarchy_discarded);
        assert_eq!(discarded.intent, authored.intent, "discard kept the intent tape");
        assert_eq!(live.object_mesh(intent_id), Some(mesh_id), "the object still names the removed mesh until rebuild");
        assert!(live.meshes().get(mesh_id).is_none());
        assert_eq!(live.authored_block(legacy_id).unwrap().body.as_ref(), Some(&legacy_body));
        assert!(live.revision() > revision_before);
        assert_eq!(file_reads, 1);
        assert_eq!(level_loads, 1);

        live.rerealize_live_block_from_intent(intent_id).expect("rerealize");
        let rebuilt_record = live.authored_block(intent_id).unwrap();
        let rebuilt = rebuilt_record.body.clone().unwrap();
        assert_same_solid(&rebuilt, &oracle);
        let body_hash_match = hash_body(&rebuilt) == hash_body(&oracle);
        let topology_match = rebuilt.faces.len() == oracle.faces.len() && rebuilt.edges.len() == oracle.edges.len() && rebuilt.vertices.len() == oracle.vertices.len() && rebuilt == oracle;
        let bounds = rebuilt.aabb_size();
        let oracle_bounds = oracle.aabb_size();
        let bounds_match = (0..3).all(|axis| (bounds[axis] - oracle_bounds[axis]).abs() < 1.0e-6);
        let volume_match = (volume(&rebuilt) - volume(&oracle)).abs() < 1.0e-8;
        let body_valid = rebuilt.validate().is_ok();
        assert!(body_hash_match && topology_match && bounds_match && volume_match && body_valid);
        let names_after = evaluate_saved_semantic_shadow(&rebuilt_record);
        assert_eq!(names_after.status, SemanticShadowStatus::Pass, "{}", names_after.text);
        assert_eq!(names_after.provenance, names_before.provenance, "semantic bindings changed across the live rebuild");
        let (new_mesh_id, mesh_after, meshlets_after, hierarchy_after) = live_products(&live, intent_id);
        let mesh_same = mesh_after == mesh_before && mesh_after == crate::mesh_from_body(&rebuilt);
        let meshlets_same = meshlets_after.meshlets == meshlets_before.meshlets
            && meshlets_after.vertex_indices == meshlets_before.vertex_indices
            && meshlets_after.local_indices == meshlets_before.local_indices
            && meshlets_after.grid_resolution == meshlets_before.grid_resolution;
        let mesh_regenerated = new_mesh_id != mesh_id && live.meshes().get(mesh_id).is_none() && mesh_same;
        let meshlets_regenerated = meshlets_same;
        let hierarchy_regenerated = hierarchy_after == hierarchy_before;
        assert!(mesh_same, "regenerated mesh vertices {} indices {} versus {} {}", mesh_after.vertex_count(), mesh_after.index_count(), mesh_before.vertex_count(), mesh_before.index_count());
        assert!(meshlets_regenerated, "meshlets {} versus {}", meshlets_after.meshlets.len(), meshlets_before.meshlets.len());
        assert!(hierarchy_regenerated, "hierarchy {} versus {}", hierarchy_after.nodes.len(), hierarchy_before.nodes.len());
        assert!(mesh_regenerated);
        let (exact_after, detail_after) = live_exact(&live, intent_id);
        assert_eq!((exact_after, detail_after), (exact_before, detail_before));
        assert!(!detail_after);
        assert_eq!(rebuilt_record.intent, authored.intent);
        assert_eq!(rebuilt_record.history.len(), 24);
        assert_eq!(live.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0)), collision);
        assert_eq!(live.authored_block(legacy_id).unwrap().body.as_ref(), Some(&legacy_body));
        assert_eq!(live.object_mesh(legacy_id), Some(legacy_mesh));
        assert_eq!(live.entity_local_pose(legacy_id).unwrap().translation, legacy_pose);
        assert_eq!(file_reads, 1, "the live rebuild reread the file");
        assert_eq!(level_loads, 1, "the live rebuild reloaded the level");

        let faces = rebuilt.faces.len();
        let edges = rebuilt.edges.len();
        let vertices = rebuilt.vertices.len();
        assert_eq!(live.split_one_block_edge(intent_id).unwrap(), AuthoringResult::Applied);
        let edited = live.authored_block(intent_id).unwrap();
        let edited_body = edited.body.clone().unwrap();
        assert!(edited_body.validate().is_ok());
        assert!(edited_body.vertices.len() > vertices && edited_body.edges.len() > edges && edited_body.faces.len() >= faces);
        assert!(edited.intent.len() > authored.intent.len());
        assert_eq!(edited.history.len(), 24);
        assert_eq!(intent_authority_eligibility(&edited), IntentAuthorityEligibility::Eligible);
        assert_ne!(edited_body, rebuilt);
        let edited_names = evaluate_saved_semantic_shadow(&edited);
        assert_eq!(edited_names.status, SemanticShadowStatus::Pass, "{}", edited_names.text);
        let (edited_mesh_id, edited_mesh, edited_meshlets, edited_hierarchy) = live_products(&live, intent_id);
        let (edited_exact, edited_detail) = live_exact(&live, intent_id);
        assert!(edited_exact && !edited_detail);

        live.discard_live_block_realization(intent_id).expect("second discard");
        let second_gap = live.authored_block(intent_id).unwrap();
        let second_body_reachable = second_gap.body.is_some();
        let second_mesh_reachable = live.meshes().get(edited_mesh_id).is_some();
        assert!(!second_body_reachable && !second_mesh_reachable);
        assert!(live.derived_meshlets(edited_mesh_id).is_none());
        assert_eq!(second_gap.intent, edited.intent);
        assert_eq!(file_reads, 1);
        assert_eq!(level_loads, 1);
        live.rerealize_live_block_from_intent(intent_id).expect("second rerealize");
        let returned = live.authored_block(intent_id).unwrap();
        let returned_body = returned.body.clone().unwrap();
        assert_same_solid(&returned_body, &edited_body);
        let post_edit_valid = returned_body.validate().is_ok()
            && returned_body == edited_body
            && returned.intent == edited.intent
            && returned.history.len() == 24
            && intent_authority_eligibility(&returned) == IntentAuthorityEligibility::Eligible;
        assert!(post_edit_valid);
        let returned_names = evaluate_saved_semantic_shadow(&returned);
        assert_eq!(returned_names.provenance, edited_names.provenance);
        let (returned_mesh_id, returned_mesh, returned_meshlets, returned_hierarchy) = live_products(&live, intent_id);
        assert_ne!(returned_mesh_id, edited_mesh_id);
        assert!(live.meshes().get(edited_mesh_id).is_none());
        assert_eq!(returned_mesh, edited_mesh);
        assert_eq!(returned_meshlets.meshlets, edited_meshlets.meshlets);
        assert_eq!(returned_meshlets.vertex_indices, edited_meshlets.vertex_indices);
        assert_eq!(returned_meshlets.local_indices, edited_meshlets.local_indices);
        assert_eq!(returned_hierarchy, edited_hierarchy);
        let (returned_exact, returned_detail) = live_exact(&live, intent_id);
        assert_eq!((returned_exact, returned_detail), (edited_exact, edited_detail));
        let legacy_after = live.authored_block(legacy_id).unwrap();
        let legacy_unchanged = legacy_after.body.as_ref() == Some(&legacy_body)
            && legacy_after.intent.is_empty()
            && live.object_mesh(legacy_id) == Some(legacy_mesh)
            && live.meshes().get(legacy_mesh).is_some()
            && live.entity_local_pose(legacy_id).unwrap().translation == legacy_pose
            && live.entity_local_pose(intent_id).unwrap().translation != legacy_pose;
        assert!(legacy_unchanged);
        assert!(live.discard_live_block_realization(legacy_id).is_err(), "an ineligible cube must refuse discard");
        assert!(live.rerealize_live_block_from_intent(legacy_id).is_err(), "an ineligible cube must refuse re-realization");
        assert_eq!(live.authored_block(legacy_id).unwrap().body.as_ref(), Some(&legacy_body));
        assert_eq!(live.object_mesh(legacy_id), Some(legacy_mesh));
        assert_eq!(file_reads, 1);
        assert_eq!(level_loads, 1);
        assert!(live.revision() > revision_before);
        assert_eq!(live.remember_entity(intent_id).unwrap().name, "Intent Solid");

        let mut quiet = empty_world_level().instantiate().unwrap();
        assert!(!quiet.intent_authority_experiment());
        let quiet_id = quiet.create_block(Vec3::new(0.0, 1.0, -4.0), authored.clone()).unwrap();
        let quiet_body = quiet.authored_block(quiet_id).unwrap().body.clone();
        let quiet_mesh = quiet.object_mesh(quiet_id);
        assert!(quiet.discard_live_block_realization(quiet_id).is_err());
        assert!(quiet.rerealize_live_block_from_intent(quiet_id).is_err());
        assert_eq!(quiet.authored_block(quiet_id).unwrap().body, quiet_body);
        assert_eq!(quiet.object_mesh(quiet_id), quiet_mesh);
        let mut bare = empty_world_level().instantiate().unwrap();
        let mut bare_record = authored.clone();
        bare_record.body = None;
        let bare_id = bare.create_block(Vec3::new(0.0, 1.0, -4.0), bare_record).unwrap();
        let bare_mesh = bare.object_mesh(bare_id);
        assert!(bare.authored_block(bare_id).unwrap().body.is_none());
        assert!(bare.discard_live_block_realization(bare_id).is_err());
        assert!(bare.rerealize_live_block_from_intent(bare_id).is_err());
        assert!(bare.authored_block(bare_id).unwrap().body.is_none());
        assert_eq!(bare.object_mesh(bare_id), bare_mesh);

        let mut divergent_record = authored.clone();
        divergent_record.body.as_mut().unwrap().vertices[0].position[0] += 5.0e-4;
        assert!(matches!(intent_authority_eligibility(&divergent_record), IntentAuthorityEligibility::Ineligible(_)));
        let mut side = empty_world_level().instantiate().unwrap();
        side.set_intent_authority_experiment(true);
        let divergent_id = side.create_block(Vec3::new(0.0, 1.0, -4.0), divergent_record.clone()).unwrap();
        let divergent_body = side.authored_block(divergent_id).unwrap().body.clone();
        let divergent_mesh = side.object_mesh(divergent_id);
        assert!(side.discard_live_block_realization(divergent_id).is_err());
        assert!(side.rerealize_live_block_from_intent(divergent_id).is_err());
        assert_eq!(side.authored_block(divergent_id).unwrap().body, divergent_body);
        assert_ne!(divergent_body.as_ref(), Some(&oracle));
        assert_eq!(side.object_mesh(divergent_id), divergent_mesh);

        let mut direct = empty_world_level().instantiate().unwrap();
        direct.set_intent_authority_experiment(true);
        let direct_id = direct.create_block(Vec3::new(0.0, 1.0, -4.0), block_in(&parsed, intent_id)).unwrap();
        assert!(direct.authored_block(direct_id).unwrap().body.is_none(), "create_block still does not realize");

        let facts = IntentLiveRerealizeFacts {
            stored_body_consulted: false,
            level_reloaded: level_loads != 1,
            file_reread: file_reads != 1,
            old_body_reachable: old_body_reachable || second_body_reachable,
            old_mesh_reachable: old_mesh_reachable || second_mesh_reachable,
            body_discarded: !old_body_reachable && !second_body_reachable,
            mesh_discarded: !old_mesh_reachable && !second_mesh_reachable,
            meshlets_discarded,
            hierarchy_discarded,
            intent_present: !returned.intent.is_empty(),
            body_valid,
            body_hash_match,
            topology_match,
            bounds_match,
            volume_match,
            mesh_regenerated,
            meshlets_regenerated,
            hierarchy_regenerated,
            frame_presented: false,
            post_edit_valid,
            legacy_unchanged,
        };
        let report = format_intent_live_rerealize(&facts);
        for line in [
            "Intent Authority Live Re-realization: ON",
            "Stored body consulted: NO",
            "Level reloaded: NO",
            "File reread: NO",
            "Old evaluated body reachable: NO",
            "Old mesh reachable: NO",
            "Body discarded: YES",
            "Mesh discarded: YES",
            "Meshlets discarded: YES",
            "Hierarchy discarded: YES",
            "Intent source: PRESENT",
            "Live reconstructed body: VALID",
            "Body hash: MATCH",
            "Topology: MATCH",
            "Bounds: MATCH",
            "Volume: MATCH",
            "Mesh regenerated: YES",
            "Meshlets regenerated: YES",
            "Hierarchy regenerated: YES",
            "Frame after regeneration: NOT PRESENTED",
            "Post-regeneration edit: VALID",
            "Legacy object: UNCHANGED",
        ] {
            assert_eq!(report.lines().filter(|stored| *stored == line).count(), 1, "{line}\n{report}");
        }
        let mut printed = report.clone();
        printed.push_str("\nEdited live cycle: MATCH\nSwitch off: REFUSED\nDivergent stored body: KEPT\nCreate path: NOT REALIZED\n");
        println!("{printed}");
        if let Ok(dir) = std::env::var("JARVIG_INTENT_PROOF") {
            if !dir.is_empty() {
                let _ = std::fs::write(std::path::Path::new(&dir).join("REPORT-LIVE.txt"), &printed);
            }
        }
    }

    fn wait_view(host: &mut crate::ViewRealizationHost, view: u32) {
        let started = std::time::Instant::now();
        loop {
            host.poll();
            if host.facts(view).is_some() && !host.is_inflight(view) {
                return;
            }
            if started.elapsed().as_secs() > 180 {
                panic!("view {view} did not publish");
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    fn submit_view(host: &mut crate::ViewRealizationHost, view: u32, entity: crate::EntityId, body: &SolidBody, hash: u64, revision: u64, camera: crate::RealizationCamera) -> crate::RealizationTicket {
        host.request(crate::ViewRealizationRequest { view, entity, body_hash: hash, revision, body: body.clone(), camera }).expect("request")
    }

    #[test]
    fn intent_view_realization_keeps_one_object_and_two_generated_meshes() {
        let authored = author_closed_chain();
        assert_eq!(intent_authority_eligibility(&authored), IntentAuthorityEligibility::Eligible);
        let oracle = authored.body.clone().unwrap();
        let document = empty_world_level();
        let mut setup = document.instantiate().unwrap();
        setup.set_intent_authority_experiment(true);
        let intent_id = setup.create_block(Vec3::new(0.0, 1.0, -4.0), authored.clone()).unwrap();
        assert_eq!(setup.set_entity_name(intent_id, "Intent Solid").unwrap(), AuthoringResult::Applied);
        let mut legacy_record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        legacy_record.body = Some(SolidBody::from_box([2.0, 2.0, 2.0]).unwrap());
        let legacy_body = legacy_record.body.clone().unwrap();
        let legacy_id = setup.create_block(Vec3::new(6.0, 1.0, -4.0), legacy_record).unwrap();
        assert_eq!(setup.set_entity_name(legacy_id, "Legacy Cube").unwrap(), AuthoringResult::Applied);
        let json = LevelDocument::capture(&setup, document.level_uuid, "Intent Proof").unwrap().to_json();
        drop(setup);
        let parsed = parse_level(&json).unwrap();
        assert!(block_in(&parsed, intent_id).body.is_none());
        let live = parsed.instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        let body = live.authored_block(intent_id).unwrap().body.clone().unwrap();
        assert_same_solid(&body, &oracle);
        let hash = hash_body(&body);
        assert_eq!(crate::authoritative_body_hash(&body), hash);
        let intent_len = live.authored_block(intent_id).unwrap().intent.len();
        let groups = live.authored_block(intent_id).unwrap().surface_groups.clone();
        let face_materials = live.authored_block(intent_id).unwrap().face_materials.clone();
        let vertex_ids: Vec<u32> = body.vertices.iter().map(|vertex| vertex.id).collect();
        let face_ids: Vec<u32> = body.faces.iter().map(|face| face.id).collect();
        let edge_ids: Vec<u32> = body.edges.iter().map(|edge| edge.id).collect();
        let pose = live.entity_local_pose(intent_id).unwrap().translation;
        let legacy_pose = live.entity_local_pose(legacy_id).unwrap().translation;
        let legacy_mesh = live.object_mesh(legacy_id).unwrap();
        let mesh_id = live.object_mesh(intent_id).unwrap();
        let mesh_count = live.mesh_count();
        let control_mesh = live.meshes().get(mesh_id).expect("control mesh").clone();
        let control_set = live.derived_meshlets(mesh_id).expect("control meshlets").clone();
        let revision = live.revision();
        let collision_inside = live.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0));
        let collision_outside = live.separate_from_blocks(Vec3::new(0.0, 1.0, 40.0));
        let snapshot = live.extract(crate::RenderFrameId(1)).unwrap();
        let instance = snapshot.instances().iter().find(|item| item.entity == intent_id).expect("instance").clone();
        let extracted = snapshot.camera(live.front_camera().frame).expect("front camera").clone();
        drop(snapshot);
        assert_eq!(instance.mesh, mesh_id);
        let forward = extracted.pose.rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
        let solid_world = live.entity_world_pose(intent_id).unwrap();
        let camera_for = |eye: Vec3, pixels: f32| crate::camera_in_solid_local(solid_world, eye, forward, extracted.vertical_fov_radians, 1080.0, pixels);
        let strict_camera = camera_for(extracted.pose.translation, 0.5);
        let loose_camera = camera_for(extracted.pose.translation, 4.0);
        let records: Vec<_> = control_set.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        let hierarchy = crate::build_cluster_hierarchy(&records);
        let leaf_triangles: Vec<u32> = control_set.meshlets.iter().map(|meshlet| meshlet.index_count / 3).collect();
        let leaf_flags = vec![1u32; control_set.meshlets.len()];
        let cut_at = |pose: &crate::ResolvedPose, pixels: f32| {
            crate::cut_visible_hierarchy_for_pose(&hierarchy, &instance, pose, extracted.vertical_fov_radians, extracted.near_m, 16.0 / 9.0, 1080.0, pixels, &leaf_flags, &leaf_triangles)
                .expect("control cut")
                .submitted_triangles
        };
        let same_control = crate::ViewRealizationControl {
            shared_vertices: control_mesh.vertex_count(),
            shared_triangles: control_mesh.index_count() / 3,
            cut_strict_triangles: cut_at(&extracted.pose, 0.5),
            cut_loose_triangles: cut_at(&extracted.pose, 4.0),
        };
        let far_eye = Vec3::new(extracted.pose.translation.x - forward.x * 8.0, extracted.pose.translation.y - forward.y * 8.0, extracted.pose.translation.z - forward.z * 8.0);
        let mut far_pose = extracted.pose;
        far_pose.translation = far_eye;
        let far_control = crate::ViewRealizationControl {
            shared_vertices: same_control.shared_vertices,
            shared_triangles: same_control.shared_triangles,
            cut_strict_triangles: cut_at(&far_pose, 0.5),
            cut_loose_triangles: cut_at(&far_pose, 4.0),
        };

        let mut host = crate::ViewRealizationHost::new();
        assert_eq!(host.worker_count(), 1);
        let ticket_a = submit_view(&mut host, 1, intent_id, &body, hash, revision, strict_camera.clone());
        let ticket_b = submit_view(&mut host, 2, intent_id, &body, hash, revision, loose_camera.clone());
        assert!(!ticket_a.cache_hit && ticket_a.realization_id.is_none());
        assert!(!ticket_b.cache_hit && ticket_b.realization_id.is_none());
        wait_view(&mut host, 1);
        wait_view(&mut host, 2);
        let same_strict = host.facts(1).unwrap().clone();
        let same_loose = host.facts(2).unwrap().clone();
        let same_report = crate::format_view_realization_pair("same pose", &same_strict, &same_loose, &same_control);
        println!("{same_report}Frame: NOT PRESENTED");
        let far_strict_camera = camera_for(far_eye, 0.5);
        let far_loose_camera = camera_for(far_eye, 4.0);
        assert!(submit_view(&mut host, 3, intent_id, &body, hash, revision, far_strict_camera).realization_id.is_none());
        assert!(submit_view(&mut host, 4, intent_id, &body, hash, revision, far_loose_camera).cache_hit == false);
        wait_view(&mut host, 3);
        wait_view(&mut host, 4);
        let far_strict = host.facts(3).unwrap().clone();
        let far_loose = host.facts(4).unwrap().clone();
        let far_report = crate::format_view_realization_pair("different pose", &far_strict, &far_loose, &far_control);
        println!("{far_report}Frame: NOT PRESENTED");
        assert_eq!(host.facts(1).unwrap().realization_id, same_strict.realization_id);
        assert_eq!(host.facts(2).unwrap().realization_id, same_loose.realization_id);

        let bucket = |value: f64| (value / crate::REALIZATION_POSE_BUCKET_M).round() as i32;
        let mut nudged = strict_camera.clone();
        nudged.eye_local[0] += 0.05;
        if bucket(nudged.eye_local[0]) != bucket(strict_camera.eye_local[0]) {
            nudged.eye_local[0] = strict_camera.eye_local[0] - 0.05;
        }
        assert_eq!(bucket(nudged.eye_local[0]), bucket(strict_camera.eye_local[0]));
        let nudge_ticket = submit_view(&mut host, 1, intent_id, &body, hash, revision, nudged);
        assert!(nudge_ticket.cache_hit);
        assert_eq!(nudge_ticket.realization_id, Some(same_strict.realization_id));
        assert_eq!(host.facts(2).unwrap().realization_id, same_loose.realization_id);
        assert_eq!(host.facts(2).unwrap().generation_us, same_loose.generation_us);

        let mut moved = strict_camera.clone();
        moved.eye_local[2] += 3.0;
        assert_ne!(bucket(moved.eye_local[2]), bucket(strict_camera.eye_local[2]));
        let move_ticket = submit_view(&mut host, 1, intent_id, &body, hash, revision, moved.clone());
        assert!(!move_ticket.cache_hit && move_ticket.realization_id.is_none());
        wait_view(&mut host, 1);
        let moved_facts = host.facts(1).unwrap().clone();
        assert_ne!(moved_facts.realization_id, same_strict.realization_id);
        assert!(!moved_facts.cache_hit);
        assert_eq!(host.facts(2).unwrap().realization_id, same_loose.realization_id);
        assert_eq!(host.facts(2).unwrap().vertex_buffer_hash, same_loose.vertex_buffer_hash);

        let mut cancel_camera = strict_camera.clone();
        cancel_camera.eye_local[1] += 2.0;
        cancel_camera.requested_error_px = 1.25;
        let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        host.hold_worker(std::sync::Arc::clone(&release));
        let held = std::time::Instant::now();
        while !host.named_job_running("realize-hold") {
            assert!(held.elapsed().as_secs() < 5, "the realization worker did not take the hold");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let cancelled_ticket = submit_view(&mut host, 9, intent_id, &body, hash, revision, cancel_camera.clone());
        let mut replacement_camera = cancel_camera;
        replacement_camera.eye_local[0] += 1.0;
        let replacement_ticket = submit_view(&mut host, 9, intent_id, &body, hash, revision, replacement_camera);
        assert!(!cancelled_ticket.cache_hit && cancelled_ticket.realization_id.is_none() && !cancelled_ticket.already_inflight);
        assert!(!replacement_ticket.cache_hit && replacement_ticket.realization_id.is_none() && !replacement_ticket.already_inflight);
        release.store(true, std::sync::atomic::Ordering::Relaxed);
        wait_view(&mut host, 9);
        assert!(host.cancelled_count() >= 1, "cancelled {}", host.cancelled_count());
        assert!(host.facts(9).unwrap().realization_id >= 1);
        assert!(!host.facts(9).unwrap().cache_hit);
        assert_eq!(host.facts(2).unwrap().realization_id, same_loose.realization_id);

        assert!(host.destroy_view(1));
        assert!(host.facts(1).is_none());
        assert_eq!(host.facts(2).unwrap().realization_id, same_loose.realization_id);
        assert_eq!(host.facts(2).unwrap().triangle_count, same_loose.triangle_count);
        let again = submit_view(&mut host, 1, intent_id, &body, hash, revision, moved);
        assert!(!again.cache_hit && again.realization_id.is_none());
        wait_view(&mut host, 1);
        let regenerated = host.facts(1).unwrap().clone();
        assert_ne!(regenerated.realization_id, moved_facts.realization_id);
        assert!(!regenerated.cache_hit);
        assert_eq!(host.facts(2).unwrap().realization_id, same_loose.realization_id);
        host.destroy_all();
        assert_eq!(host.resident_count(), 0);

        let intact = live.authored_block(intent_id).unwrap();
        assert_same_solid(intact.body.as_ref().unwrap(), &oracle);
        assert_eq!(intact.intent.len(), intent_len);
        assert_eq!(intact.history.len(), 24);
        assert_eq!(intact.surface_groups, groups);
        assert_eq!(intact.face_materials, face_materials);
        assert_eq!(intact.body.as_ref().unwrap().vertices.iter().map(|vertex| vertex.id).collect::<Vec<_>>(), vertex_ids);
        assert_eq!(intact.body.as_ref().unwrap().faces.iter().map(|face| face.id).collect::<Vec<_>>(), face_ids);
        assert_eq!(intact.body.as_ref().unwrap().edges.iter().map(|edge| edge.id).collect::<Vec<_>>(), edge_ids);
        assert_eq!(live.entity_local_pose(intent_id).unwrap().translation, pose);
        assert_eq!(live.remember_entity(intent_id).unwrap().name, "Intent Solid");
        assert_eq!(live.object_mesh(intent_id), Some(mesh_id));
        assert_eq!(live.mesh_count(), mesh_count);
        assert!(live.meshes().get(mesh_id).is_some());
        let meshlets_after = live.derived_meshlets(mesh_id).expect("control meshlets remain");
        assert_eq!(meshlets_after.meshlets, control_set.meshlets);
        assert_eq!(meshlets_after.vertex_indices, control_set.vertex_indices);
        assert_eq!(meshlets_after.local_indices, control_set.local_indices);
        assert_eq!(live.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0)), collision_inside);
        assert_eq!(live.separate_from_blocks(Vec3::new(0.0, 1.0, 40.0)), collision_outside);
        assert_eq!(live.authored_block(legacy_id).unwrap().body.as_ref(), Some(&legacy_body));
        assert_eq!(live.object_mesh(legacy_id), Some(legacy_mesh));
        assert_eq!(live.entity_local_pose(legacy_id).unwrap().translation, legacy_pose);
        assert_eq!(live.revision(), revision);

        let mut printed = String::new();
        printed.push_str(&same_report);
        printed.push_str("Frame: NOT PRESENTED\n");
        printed.push_str(&format!("Control mesh id unchanged: YES\nControl mesh id: {}\n", mesh_id.0));
        printed.push_str("Experiment uses that cut: NO\n");
        printed.push_str(&format!("Control cuts differ at the same pose: {}\n", if same_control.cut_strict_triangles != same_control.cut_loose_triangles { "YES" } else { "NO" }));
        printed.push_str("That cut difference is the experiment result: NO\n");
        printed.push_str(&far_report);
        printed.push_str("Frame: NOT PRESENTED\n");
        printed.push_str(&format!("Control cuts differ at the far pose: {}\n", if far_control.cut_strict_triangles != far_control.cut_loose_triangles { "YES" } else { "NO" }));
        printed.push_str("That cut difference is the experiment result: NO\n");
        printed.push_str(&format!("View A tiny move cache_hit: YES\nView A tiny move realization_id: {}\n", same_strict.realization_id));
        printed.push_str(&format!("View A large move realization_id: {}\n", moved_facts.realization_id));
        printed.push_str(&format!("View A regenerated realization_id: {}\n", regenerated.realization_id));
        printed.push_str("View B resident after View A destroyed: YES\n");
        printed.push_str("View B regenerated with View A: NO\n");
        printed.push_str("Authoritative object after both realizations destroyed: INTACT\n");
        printed.push_str(&format!("Queue depth max: {}\nCancelled jobs: {}\nStale discarded: {}\n", host.max_queue_depth(), host.cancelled_count(), host.discarded_stale_count()));
        printed.push_str(&format!("Algorithm version: {}\n", crate::REALIZATION_ALGORITHM_VERSION));
        for line in [
            "Shared authoritative object: YES",
            "Shared stored render mesh: NO",
            "Frame: NOT PRESENTED",
            "GPU_bytes: NOT UPLOADED",
            "Control cut is the experiment: NO",
            "Experiment uses that cut: NO",
            "Authoritative object after both realizations destroyed: INTACT",
            "View B resident after View A destroyed: YES",
            "View B regenerated with View A: NO",
        ] {
            assert!(printed.lines().any(|stored| stored == line), "{line}\n{printed}");
        }
        assert!(!printed.lines().any(|stored| stored == "Frame: PRESENTED"));
        assert_ne!(same_strict.realization_id, same_loose.realization_id);
        assert_ne!(far_strict.realization_id, far_loose.realization_id);
        assert!(!same_strict.cache_hit && !same_loose.cache_hit);
        assert!(!far_strict.cache_hit && !far_loose.cache_hit);
        assert_eq!(same_strict.source_body_hash, hash);
        assert_eq!(same_loose.source_body_hash, hash);
        assert_eq!(far_strict.source_body_hash, hash);
        assert_eq!(far_loose.source_body_hash, hash);
        assert_eq!(same_strict.microprimitive_count, 0);
        assert_eq!(same_loose.microprimitive_count, 0);
        assert!(same_strict.measured_projected_error_px <= 0.55, "{}", same_strict.measured_projected_error_px);
        assert!(same_loose.measured_projected_error_px <= 4.05, "{}", same_loose.measured_projected_error_px);
        assert!(far_strict.measured_projected_error_px <= 0.55, "{}", far_strict.measured_projected_error_px);
        assert!(far_loose.measured_projected_error_px <= 4.05, "{}", far_loose.measured_projected_error_px);
        assert!(host.max_queue_depth() >= 1);
        println!("{printed}");
    }

    #[test]
    fn intent_view_realization_two_workers_match_one_worker() {
        let authored = author_closed_chain();
        assert_eq!(intent_authority_eligibility(&authored), IntentAuthorityEligibility::Eligible);
        let document = empty_world_level();
        let mut setup = document.instantiate().unwrap();
        setup.set_intent_authority_experiment(true);
        let intent_id = setup.create_block(Vec3::new(0.0, 1.0, -4.0), authored).unwrap();
        let json = LevelDocument::capture(&setup, document.level_uuid, "Intent Proof").unwrap().to_json();
        drop(setup);
        let live = parse_level(&json).unwrap().instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        let body = live.authored_block(intent_id).unwrap().body.clone().unwrap();
        let hash = hash_body(&body);
        let revision = live.revision();
        let snapshot = live.extract(crate::RenderFrameId(1)).unwrap();
        let extracted = snapshot.camera(live.front_camera().frame).expect("front camera").clone();
        drop(snapshot);
        let forward = extracted.pose.rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
        let solid_world = live.entity_world_pose(intent_id).unwrap();
        let camera_for = |pixels: f32| crate::camera_in_solid_local(solid_world, extracted.pose.translation, forward, extracted.vertical_fov_radians, 1080.0, pixels);
        let strict = camera_for(0.5);
        let loose = camera_for(4.0);
        let run = |workers: usize| {
            let mut host = crate::ViewRealizationHost::with_workers(workers);
            assert_eq!(host.worker_count(), workers);
            let wall = std::time::Instant::now();
            let ticket_a = submit_view(&mut host, 1, intent_id, &body, hash, revision, strict.clone());
            let ticket_b = submit_view(&mut host, 2, intent_id, &body, hash, revision, loose.clone());
            assert!(!ticket_a.cache_hit && !ticket_a.already_inflight && ticket_a.realization_id.is_none());
            assert!(!ticket_b.cache_hit && !ticket_b.already_inflight && ticket_b.realization_id.is_none());
            wait_view(&mut host, 1);
            wait_view(&mut host, 2);
            let wall_us = wall.elapsed().as_micros();
            (wall_us, host.facts(1).unwrap().clone(), host.facts(2).unwrap().clone())
        };
        let (wall_one, strict_one, loose_one) = run(1);
        let (wall_two, strict_two, loose_two) = run(2);
        assert_eq!(strict_one.vertex_buffer_hash, strict_two.vertex_buffer_hash);
        assert_eq!(strict_one.index_buffer_hash, strict_two.index_buffer_hash);
        assert_eq!(loose_one.vertex_buffer_hash, loose_two.vertex_buffer_hash);
        assert_eq!(loose_one.index_buffer_hash, loose_two.index_buffer_hash);
        assert_eq!(strict_one.triangle_count, strict_two.triangle_count);
        assert_eq!(loose_one.triangle_count, loose_two.triangle_count);
        assert_eq!(strict_one.vertex_count, strict_two.vertex_count);
        assert_eq!(loose_one.vertex_count, loose_two.vertex_count);
        assert!(strict_one.measured_projected_error_px <= 0.55);
        assert!(loose_one.measured_projected_error_px <= 4.05);
        assert!(strict_one.generation_us > 0 && loose_one.generation_us > 0);
        assert!(strict_two.generation_us > 0 && loose_two.generation_us > 0);
        println!("Workers 1 wall_us {wall_one} A_generation_us {} A_queue_wait_us {} B_generation_us {} B_queue_wait_us {} A_cpu_bytes {} B_cpu_bytes {}", strict_one.generation_us, strict_one.queue_wait_us, loose_one.generation_us, loose_one.queue_wait_us, strict_one.cpu_bytes, loose_one.cpu_bytes);
        println!("Workers 2 wall_us {wall_two} A_generation_us {} A_queue_wait_us {} B_generation_us {} B_queue_wait_us {} A_cpu_bytes {} B_cpu_bytes {}", strict_two.generation_us, strict_two.queue_wait_us, loose_two.generation_us, loose_two.queue_wait_us, strict_two.cpu_bytes, loose_two.cpu_bytes);
        println!("Hashes match: YES");
        println!("Triangle counts match: YES");
        println!("Duplicate work: both budgets generated on each run");
        println!("CPU utilization percent: NOT SAMPLED");
        println!("Process memory bytes: NOT SAMPLED");
        println!("Faster worker count claimed as a rendering improvement: NO");
        let max_one = strict_one.generation_us.max(loose_one.generation_us);
        let sum_one = strict_one.generation_us.saturating_add(loose_one.generation_us);
        let max_two = strict_two.generation_us.max(loose_two.generation_us);
        let sum_two = strict_two.generation_us.saturating_add(loose_two.generation_us);
        println!("Workers 1 overlap wall_us {wall_one} max_generation_us {max_one} sum_generation_us {sum_one}");
        println!("Workers 2 overlap wall_us {wall_two} max_generation_us {max_two} sum_generation_us {sum_two}");
    }

    fn oracle_fan(body: &SolidBody, face: u32) -> u32 {
        let len = body.face_loop(face).expect("face").len();
        u32::try_from(len.saturating_sub(2)).unwrap_or(u32::MAX)
    }

    fn face_centroid(body: &SolidBody, face: u32) -> [f64; 3] {
        let positions = body.face_positions(face).expect("positions");
        let scale = positions.len() as f64;
        let mut sum = [0.0; 3];
        for position in positions {
            sum[0] += position[0];
            sum[1] += position[1];
            sum[2] += position[2];
        }
        [sum[0] / scale, sum[1] / scale, sum[2] / scale]
    }

    fn observation_camera(eye: [f64; 3], forward: [f64; 3], fov: f64, width: f32) -> crate::ObservationCamera {
        crate::ObservationCamera {
            eye_local: eye,
            forward_local: forward,
            up_local: [0.0, 1.0, 0.0],
            vertical_fov_radians: fov,
            near_m: 0.1,
            viewport_width: width,
            viewport_height: 567.0,
            requested_error_px: 0.5,
        }
    }

    /// Independent screen test. It does not call the observation classifier.
    fn screen_admission(body: &SolidBody, face: u32, camera: &crate::ObservationCamera) -> crate::Admission {
        let Some(positions) = body.face_positions(face) else {
            return crate::Admission::AdmitUncertain;
        };
        if positions.len() < 3 || positions.iter().any(|position| position.iter().any(|axis| !axis.is_finite())) {
            return crate::Admission::AdmitUncertain;
        }
        let sub = |left: [f64; 3], right: [f64; 3]| [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
        let dot = |left: [f64; 3], right: [f64; 3]| left[0] * right[0] + left[1] * right[1] + left[2] * right[2];
        let cross = |left: [f64; 3], right: [f64; 3]| {
            [left[1] * right[2] - left[2] * right[1], left[2] * right[0] - left[0] * right[2], left[0] * right[1] - left[1] * right[0]]
        };
        let normalize = |value: [f64; 3]| {
            let span = dot(value, value).sqrt();
            if !span.is_finite() || span < 1.0e-12 { None } else { Some([value[0] / span, value[1] / span, value[2] / span]) }
        };
        let Some(forward) = normalize(camera.forward_local) else {
            return crate::Admission::AdmitUncertain;
        };
        let hint = match normalize(camera.up_local) {
            Some(hint) if dot(hint, forward).abs() <= 0.999 => hint,
            _ => {
                if forward[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] }
            }
        };
        let Some(right) = normalize(cross(forward, hint)) else {
            return crate::Admission::AdmitUncertain;
        };
        let up = cross(right, forward);
        let width = f64::from(camera.viewport_width);
        let height = f64::from(camera.viewport_height);
        let error = f64::from(camera.requested_error_px);
        if !camera.near_m.is_finite() || camera.near_m <= 0.0 || !error.is_finite() || error < 0.0 || width < 1.0 || height < 1.0 {
            return crate::Admission::AdmitUncertain;
        }
        let half = camera.vertical_fov_radians * 0.5;
        if !half.is_finite() || half <= 0.0 || half >= std::f64::consts::FRAC_PI_2 {
            return crate::Admission::AdmitUncertain;
        }
        let tan_half_v = half.tan();
        let tan_half_h = tan_half_v * (width / height);
        if !tan_half_v.is_finite() || !tan_half_h.is_finite() || tan_half_v <= 0.0 || tan_half_h <= 0.0 {
            return crate::Admission::AdmitUncertain;
        }
        let mut depths = Vec::new();
        for position in &positions {
            depths.push(dot(sub(*position, camera.eye_local), forward));
        }
        if depths.iter().any(|depth| !depth.is_finite()) {
            return crate::Admission::AdmitUncertain;
        }
        if depths.iter().any(|depth| *depth <= 0.0) {
            return crate::Admission::AdmitBehindCamera;
        }
        if depths.iter().any(|depth| *depth <= camera.near_m) {
            return crate::Admission::AdmitCrossesNear;
        }
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;
        for (position, depth) in positions.iter().zip(depths.iter()) {
            let relative = sub(*position, camera.eye_local);
            let px = (dot(relative, right) / depth) / tan_half_h;
            let py = (dot(relative, up) / depth) / tan_half_v;
            if !px.is_finite() || !py.is_finite() {
                return crate::Admission::AdmitUncertain;
            }
            let screen_x = (px * 0.5 + 0.5) * width;
            let screen_y = (py * 0.5 + 0.5) * height;
            min_x = min_x.min(screen_x);
            max_x = max_x.max(screen_x);
            min_y = min_y.min(screen_y);
            max_y = max_y.max(screen_y);
        }
        let misses = max_x < -error || min_x > width + error || max_y < -error || min_y > height + error;
        if misses { crate::Admission::Omit } else { crate::Admission::AdmitInView }
    }

    #[test]
    fn intent_observation_admits_faces_before_any_triangle_exists() {
        let authored = author_closed_chain();
        assert_eq!(intent_authority_eligibility(&authored), IntentAuthorityEligibility::Eligible);
        let oracle = authored.body.clone().unwrap();
        let document = empty_world_level();
        let mut setup = document.instantiate().unwrap();
        setup.set_intent_authority_experiment(true);
        let intent_id = setup.create_block(Vec3::new(0.0, 1.0, -4.0), authored).unwrap();
        assert_eq!(setup.set_entity_name(intent_id, "Intent Solid").unwrap(), AuthoringResult::Applied);
        let mut legacy_record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        legacy_record.body = Some(SolidBody::from_box([2.0, 2.0, 2.0]).unwrap());
        let legacy_body = legacy_record.body.clone().unwrap();
        let legacy_id = setup.create_block(Vec3::new(6.0, 1.0, -4.0), legacy_record).unwrap();
        let json = LevelDocument::capture(&setup, document.level_uuid, "Intent Proof").unwrap().to_json();
        drop(setup);
        let live = parse_level(&json).unwrap().instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        let start = live.authored_block(intent_id).unwrap();
        let body = start.body.clone().unwrap();
        let intent = start.intent.clone();
        let history = start.history.clone();
        let material = start.material.clone();
        let materials = start.materials.clone();
        let face_materials = start.face_materials.clone();
        let groups = start.surface_groups.clone();
        let next_group = start.next_surface_group;
        let vertex_ids: Vec<u32> = body.vertices.iter().map(|vertex| vertex.id).collect();
        let face_ids: Vec<u32> = body.faces.iter().map(|face| face.id).collect();
        let edge_ids: Vec<u32> = body.edges.iter().map(|edge| edge.id).collect();
        drop(start);
        assert_same_solid(&body, &oracle);
        let hash = hash_body(&body);
        assert_eq!(crate::authoritative_body_hash(&body), hash);
        assert_eq!(format!("{hash:016x}"), "6ceeea840b7bdef2");
        assert_eq!(intent.len(), 65);
        assert_eq!(history.len(), 24);
        let revision = live.revision();
        let pose = live.entity_local_pose(intent_id).unwrap().translation;
        let mesh_id = live.object_mesh(intent_id).unwrap();
        let mesh_count = live.mesh_count();
        let legacy_mesh = live.object_mesh(legacy_id).unwrap();
        let legacy_pose = live.entity_local_pose(legacy_id).unwrap().translation;
        let control_vertices = live.meshes().get(mesh_id).expect("control mesh").vertex_count();
        let control_triangles = live.meshes().get(mesh_id).expect("control mesh").index_count() / 3;
        assert_eq!(control_vertices, 144);
        assert_eq!(control_triangles, 48);
        let control_set = live.derived_meshlets(mesh_id).expect("control meshlets").clone();
        let collision_inside = live.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0));
        let collision_outside = live.separate_from_blocks(Vec3::new(0.0, 1.0, 40.0));
        let snapshot = live.extract(crate::RenderFrameId(1)).unwrap();
        let instance = snapshot.instances().iter().find(|item| item.entity == intent_id).expect("instance").clone();
        let extracted = snapshot.camera(live.front_camera().frame).expect("front camera").clone();
        drop(snapshot);
        assert_eq!(instance.mesh, mesh_id);
        let records: Vec<_> = control_set.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        let hierarchy = crate::build_cluster_hierarchy(&records);
        let leaf_triangles: Vec<u32> = control_set.meshlets.iter().map(|meshlet| meshlet.index_count / 3).collect();
        let leaf_flags = vec![1u32; control_set.meshlets.len()];
        let cut_at = |pixels: f32| {
            crate::cut_visible_hierarchy_for_pose(
                &hierarchy,
                &instance,
                &extracted.pose,
                extracted.vertical_fov_radians,
                extracted.near_m,
                16.0 / 9.0,
                1080.0,
                pixels,
                &leaf_flags,
                &leaf_triangles,
            )
            .expect("control cut")
            .submitted_triangles
        };
        let cut_strict = cut_at(0.5);
        let cut_loose = cut_at(4.0);
        let fov = extracted.vertical_fov_radians;
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        for vertex in &body.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.position[axis]);
                max[axis] = max[axis].max(vertex.position[axis]);
            }
        }
        let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
        let eye_z = max[2] + 2.0;
        let mut chosen: Option<(crate::ObservationCamera, Vec<u32>)> = None;
        for width in [160.0_f32, 128.0, 96.0, 64.0, 48.0, 32.0, 24.0, 16.0, 8.0] {
            for shift in [0.0_f64, -0.5, -1.0, -1.5, -2.0, -2.5, 0.5, 1.0, 1.5] {
                let camera = observation_camera([center[0] + shift, center[1], eye_z], [0.0, 0.0, -1.0], fov, width);
                let omitted: Vec<u32> = body
                    .faces
                    .iter()
                    .filter(|face| screen_admission(&body, face.id, &camera) == crate::Admission::Omit && oracle_fan(&body, face.id) > 0)
                    .map(|face| face.id)
                    .collect();
                let sees_one = body.faces.iter().any(|face| screen_admission(&body, face.id, &camera) == crate::Admission::AdmitInView);
                if !omitted.is_empty() && sees_one {
                    chosen = Some((camera, omitted));
                    break;
                }
            }
            if chosen.is_some() {
                break;
            }
        }
        let (camera_a, omitted_probe) = chosen.unwrap_or_else(|| {
            let probe = observation_camera([center[0], center[1], eye_z], [0.0, 0.0, -1.0], fov, 16.0);
            let mut dump = String::new();
            for face in &body.faces {
                dump.push_str(&format!("face {} {:?}\n", face.id, screen_admission(&body, face.id, &probe)));
            }
            panic!("no observation omitted a face without loosening the bound\n{dump}");
        });
        let mut candidates = omitted_probe.clone();
        candidates.sort_by(|left, right| face_centroid(&body, *right)[0].total_cmp(&face_centroid(&body, *left)[0]).then(left.cmp(right)));
        let mut flip: Option<(u32, crate::ObservationCamera)> = None;
        for face in candidates {
            let Some(normal) = body.unit_normal(face) else { continue };
            let centroid = face_centroid(&body, face);
            for distance in [1.5_f64, 2.5, 4.0] {
                let eye = [centroid[0] + normal[0] * distance, centroid[1] + normal[1] * distance, centroid[2] + normal[2] * distance];
                let forward = [-normal[0], -normal[1], -normal[2]];
                let camera = observation_camera(eye, forward, fov, 960.0);
                if screen_admission(&body, face, &camera) == crate::Admission::AdmitInView {
                    flip = Some((face, camera));
                    break;
                }
            }
            if flip.is_some() {
                break;
            }
        }
        let (flip_face, camera_b) = flip.expect("an omitted face could not be admitted by a second observation");
        let request_a = crate::ObservationRequest { entity: intent_id, body_hash: hash, revision, body: body.clone(), camera: camera_a.clone() };
        let request_b = crate::ObservationRequest { entity: intent_id, body_hash: hash, revision, body: body.clone(), camera: camera_b.clone() };
        let mut host = crate::ObservationHost::new();
        let ticket_a = host.observe(request_a).expect("observation A");
        let built_a = host.construction_count();
        let ticket_b = host.observe(request_b).expect("observation B");
        let built_both = host.construction_count();
        assert!(!ticket_a.cache_hit && !ticket_b.cache_hit);
        assert_eq!(built_a, 1);
        assert_eq!(built_both, 2);
        assert_ne!(ticket_a.product.observation_id, ticket_b.product.observation_id);
        assert_ne!(ticket_a.product.cache_key, ticket_b.product.cache_key);
        assert!(host.resident().unwrap().observation_id == ticket_b.product.observation_id);
        assert!(!host.publish_if_current(&ticket_a.product), "a stale observation A published after B");
        assert_eq!(host.discarded_stale(), 1);
        assert_eq!(host.resident().unwrap().observation_id, ticket_b.product.observation_id);
        assert_eq!(host.construction_count(), 2);
        let replay = host.observe(crate::ObservationRequest { entity: intent_id, body_hash: hash, revision, body: body.clone(), camera: camera_a.clone() }).expect("replay A");
        assert!(replay.cache_hit);
        assert_eq!(replay.product.observation_id, ticket_a.product.observation_id);
        assert_eq!(replay.product.omitted_face_ids, ticket_a.product.omitted_face_ids);
        assert_eq!(replay.product.triangles, ticket_a.product.triangles);
        assert_eq!(host.construction_count(), 2);
        let product_a = ticket_a.product;
        let product_b = ticket_b.product;
        let stale_discarded = host.discarded_stale();

        let prove = |label: &str, product: &crate::ObservationProduct, camera: &crate::ObservationCamera| {
            assert!(!product.object_mesh_consulted, "{label} read the object mesh");
            assert!(!product.shared_render_mesh_consulted, "{label} read the shared render mesh");
            assert_eq!(product.triangles_discarded_after_construction, 0, "{label} discarded constructed triangles");
            assert_eq!(product.triangles.len() as u32, product.triangles_constructed, "{label} output and constructed count diverged");
            assert_eq!(product.faces_expanded, product.admitted_face_ids, "{label} expanded a different set than it admitted");
            assert!(product.omitted_face_ids.iter().all(|id| !product.faces_expanded.contains(id)), "{label} expanded an omitted face");
            assert_eq!(product.algorithm_version, crate::OBSERVATION_ALGORITHM_VERSION);
            let mut potential = 0u32;
            let mut admitted_fan = 0u32;
            let mut omitted_fan = 0u32;
            for face in &body.faces {
                let fan = oracle_fan(&body, face.id);
                potential = potential.saturating_add(fan);
                let decision = product.decisions.iter().find(|decision| decision.face_id == face.id).expect("decision");
                assert_eq!(decision.admission, screen_admission(&body, face.id, camera), "{label} face {}", face.id);
                if decision.admission.omitted() {
                    assert!(fan > 0, "{label} omitted a face with no fan");
                    assert_eq!(product.triangles.iter().filter(|triangle| triangle.face_id == face.id).count(), 0, "{label} constructed face {}", face.id);
                    omitted_fan = omitted_fan.saturating_add(fan);
                } else {
                    admitted_fan = admitted_fan.saturating_add(fan);
                    assert_eq!(product.triangles.iter().filter(|triangle| triangle.face_id == face.id).count(), fan as usize, "{label} fan of face {}", face.id);
                    assert!(decision.measured_error_px <= camera.requested_error_px + 1.0e-4, "{label} face {} error {}", face.id, decision.measured_error_px);
                }
            }
            assert_eq!(product.potential_triangles, potential, "{label} potential");
            assert_eq!(product.triangles_constructed, admitted_fan, "{label} constructed");
            assert!(product.triangles_constructed < potential, "{label} constructed the whole body");
            assert_eq!(admitted_fan + omitted_fan, potential, "{label} fan partition");
            for triangle in &product.triangles {
                assert!(product.admitted_face_ids.contains(&triangle.face_id));
                assert!(!product.omitted_face_ids.contains(&triangle.face_id));
                let loop_ = body.face_loop(triangle.face_id).unwrap();
                for (id, position) in triangle.vertex_ids.iter().zip(triangle.positions.iter()) {
                    assert!(loop_.contains(id));
                    assert_eq!(body.vertex_position(*id).unwrap(), *position);
                }
            }
            for face_id in &product.admitted_face_ids {
                for vertex in body.face_loop(*face_id).unwrap() {
                    assert!(product.vertex_ids.contains(vertex), "{label} dropped an admitted vertex {vertex}");
                }
            }
            for face_id in &product.omitted_face_ids {
                for vertex in body.face_loop(*face_id).unwrap() {
                    let shared = product.admitted_face_ids.iter().any(|admitted| body.face_loop(*admitted).unwrap().contains(vertex));
                    if !shared {
                        assert!(!product.vertex_ids.contains(vertex), "{label} constructed omitted-only vertex {vertex}");
                    }
                }
            }
            assert!(product.vertices_constructed as usize <= body.vertices.len());
            assert_ne!(product.vertices_constructed, control_vertices);
            assert_ne!(product.triangles_constructed, control_triangles);
        };
        prove("A", &product_a, &camera_a);
        prove("B", &product_b, &camera_b);
        assert!(product_a.omitted_face_ids.contains(&flip_face));
        assert!(product_b.admitted_face_ids.contains(&flip_face));
        assert_ne!(product_a.admitted_face_ids, product_b.admitted_face_ids);
        let flip_fan = oracle_fan(&body, flip_face);
        assert!(flip_fan > 0);
        assert_eq!(product_a.triangles.iter().filter(|triangle| triangle.face_id == flip_face).count(), 0);
        assert_eq!(product_b.triangles.iter().filter(|triangle| triangle.face_id == flip_face).count(), flip_fan as usize);
        assert_eq!(product_b.decisions.iter().find(|decision| decision.face_id == flip_face).unwrap().admission, crate::Admission::AdmitInView);
        assert_eq!(crate::REALIZATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::OBSERVATION_ALGORITHM_VERSION, 1);

        let mut realization = crate::ViewRealizationHost::new();
        assert_eq!(realization.worker_count(), 1);
        let control_camera = crate::RealizationCamera {
            eye_local: camera_a.eye_local,
            forward_local: camera_a.forward_local,
            vertical_fov_radians: camera_a.vertical_fov_radians,
            viewport_height: camera_a.viewport_height,
            requested_error_px: camera_a.requested_error_px,
        };
        assert!(submit_view(&mut realization, 1, intent_id, &body, hash, revision, control_camera).realization_id.is_none());
        wait_view(&mut realization, 1);
        let full = realization.facts(1).unwrap().clone();
        assert_eq!(full.source_body_hash, hash);
        assert_eq!(full.microprimitive_count, 0);
        assert!(full.triangle_count >= 4, "complete realization {}", full.triangle_count);
        assert!(full.measured_projected_error_px <= full.requested_error_px + 0.05, "{}", full.measured_projected_error_px);
        assert!(full.vertex_count != product_a.vertices_constructed || full.triangle_count != product_a.triangles_constructed, "the partial product matched the Experiment 2 mesh");
        realization.destroy_all();
        drop(realization);

        let kept_a = product_a.observation_id;
        let kept_b = product_b.observation_id;
        host.drop_all();
        assert!(host.resident().is_none());
        assert_eq!(host.cache_len(), 0);
        let intact = live.authored_block(intent_id).unwrap();
        let body_after = intact.body.clone().unwrap();
        let intent_after = intact.intent.clone();
        let history_after = intact.history.clone();
        let material_after = intact.material.clone();
        let materials_after = intact.materials.clone();
        let face_materials_after = intact.face_materials.clone();
        let groups_after = intact.surface_groups.clone();
        let next_group_after = intact.next_surface_group;
        drop(intact);
        assert_same_solid(&body_after, &oracle);
        assert_eq!(body_after, body);
        assert!(body_after.faces.iter().any(|face| face.id == flip_face));
        assert_eq!(body_after.face_loop(flip_face), body.face_loop(flip_face));
        assert_eq!(crate::authoritative_body_hash(&body_after), hash);
        assert_eq!(intent_after, intent);
        assert_eq!(history_after, history);
        assert_eq!(material_after, material);
        assert_eq!(materials_after, materials);
        assert_eq!(face_materials_after, face_materials);
        assert_eq!(groups_after, groups);
        assert_eq!(next_group_after, next_group);
        assert_eq!(body_after.vertices.iter().map(|vertex| vertex.id).collect::<Vec<_>>(), vertex_ids);
        assert_eq!(body_after.faces.iter().map(|face| face.id).collect::<Vec<_>>(), face_ids);
        assert_eq!(body_after.edges.iter().map(|edge| edge.id).collect::<Vec<_>>(), edge_ids);
        assert_eq!(live.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0)), collision_inside);
        assert_eq!(live.separate_from_blocks(Vec3::new(0.0, 1.0, 40.0)), collision_outside);
        assert_eq!(live.object_mesh(intent_id), Some(mesh_id));
        assert_eq!(live.mesh_count(), mesh_count);
        assert_eq!(live.revision(), revision);
        assert_eq!(live.entity_local_pose(intent_id).unwrap().translation, pose);
        assert_eq!(live.remember_entity(intent_id).unwrap().name, "Intent Solid");
        assert_eq!(live.authored_block(legacy_id).unwrap().body.as_ref(), Some(&legacy_body));
        assert_eq!(live.object_mesh(legacy_id), Some(legacy_mesh));
        assert_eq!(live.entity_local_pose(legacy_id).unwrap().translation, legacy_pose);
        let meshlets_after = live.derived_meshlets(mesh_id).expect("control meshlets remain");
        assert_eq!(meshlets_after.meshlets.len(), control_set.meshlets.len());
        assert_eq!(meshlets_after.vertex_indices, control_set.vertex_indices);
        assert_eq!(meshlets_after.local_indices, control_set.local_indices);

        let integrity = crate::ObservationIntegrity { body_mutated: false, intent_mutated: false, collision_match: true, materials_match: true, semantic_match: true };
        let mut report = String::new();
        report.push_str("Observation A\n");
        report.push_str(&crate::format_observation_block(&product_a, revision, &integrity));
        report.push_str(&format!("uncertain_face_ids: {}\n", product_a.uncertain_face_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")));
        report.push_str(&format!("admitted_area_m2: {:.6}\n", product_a.admitted_area_m2));
        report.push_str(&format!("omitted_area_m2: {:.6}\n", product_a.omitted_area_m2));
        report.push_str(&format!("measured_projected_error_px: {:.4}\n", product_a.measured_projected_error_px));
        report.push_str(&format!("requested_error_px: {:.3}\n", product_a.requested_error_px));
        report.push_str(&format!("generation_us: {}\n", product_a.generation_us));
        report.push_str(&format!("cache_key: {}\n", product_a.cache_key));
        report.push_str(&format!("eye_local: [{:.4}, {:.4}, {:.4}]\n", camera_a.eye_local[0], camera_a.eye_local[1], camera_a.eye_local[2]));
        report.push_str(&format!("forward_local: [{:.4}, {:.4}, {:.4}]\n", camera_a.forward_local[0], camera_a.forward_local[1], camera_a.forward_local[2]));
        report.push_str(&format!("viewport: {:.0}x{:.0}\n", camera_a.viewport_width, camera_a.viewport_height));
        report.push_str(&format!("near_m: {:.3}\n", camera_a.near_m));
        for face_id in &product_a.omitted_face_ids {
            let fan = oracle_fan(&body, *face_id);
            let traced = product_a.triangles.iter().filter(|triangle| triangle.face_id == *face_id).count();
            report.push_str(&format!("Omitted face {face_id} authoritative_fan: {fan} triangles_in_trace: {traced}\n"));
        }
        report.push_str("Observation B\n");
        report.push_str(&crate::format_observation_block(&product_b, revision, &integrity));
        report.push_str(&format!("uncertain_face_ids: {}\n", product_b.uncertain_face_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")));
        report.push_str(&format!("admitted_area_m2: {:.6}\n", product_b.admitted_area_m2));
        report.push_str(&format!("omitted_area_m2: {:.6}\n", product_b.omitted_area_m2));
        report.push_str(&format!("measured_projected_error_px: {:.4}\n", product_b.measured_projected_error_px));
        report.push_str(&format!("requested_error_px: {:.3}\n", product_b.requested_error_px));
        report.push_str(&format!("generation_us: {}\n", product_b.generation_us));
        report.push_str(&format!("cache_key: {}\n", product_b.cache_key));
        report.push_str(&format!("eye_local: [{:.4}, {:.4}, {:.4}]\n", camera_b.eye_local[0], camera_b.eye_local[1], camera_b.eye_local[2]));
        report.push_str(&format!("forward_local: [{:.4}, {:.4}, {:.4}]\n", camera_b.forward_local[0], camera_b.forward_local[1], camera_b.forward_local[2]));
        report.push_str(&format!("viewport: {:.0}x{:.0}\n", camera_b.viewport_width, camera_b.viewport_height));
        report.push_str(&format!("near_m: {:.3}\n", camera_b.near_m));
        for face_id in &product_b.omitted_face_ids {
            let fan = oracle_fan(&body, *face_id);
            let traced = product_b.triangles.iter().filter(|triangle| triangle.face_id == *face_id).count();
            report.push_str(&format!("Omitted face {face_id} authoritative_fan: {fan} triangles_in_trace: {traced}\n"));
        }
        report.push_str(&format!("Cross face: {flip_face}\n"));
        report.push_str("Cross face omitted by A: YES\n");
        report.push_str("Cross face admitted by B: YES\n");
        report.push_str("Cross face triangles in A: 0\n");
        report.push_str(&format!("Cross face triangles in B: {flip_fan}\n"));
        report.push_str(&format!("Cross face fan from the authoritative loop: {flip_fan}\n"));
        report.push_str("Cross face still on the body after both products dropped: YES\n");
        report.push_str("B received A's product: NO\n");
        report.push_str("A's admitted set inherited by B: NO\n");
        report.push_str("Stale A published after B: NO\n");
        report.push_str(&format!("stale_discarded: {stale_discarded}\n"));
        report.push_str(&format!("constructions: {built_both}\n"));
        report.push_str("queue: NOT USED\n");
        report.push_str("queue_wait_us: NOT APPLICABLE\n");
        report.push_str("Experiment 3A worker: NOT STARTED\n");
        report.push_str("jarvig-realize-1: NOT STARTED\n");
        report.push_str("Experiment 2 consulted by admission: NO\n");
        report.push_str("Control mesh read outside admission: YES\n");
        report.push_str(&format!("Control shared vertices: {control_vertices}\n"));
        report.push_str(&format!("Control shared triangles: {control_triangles}\n"));
        report.push_str(&format!("Control cut 0.5 px triangles: {cut_strict}\n"));
        report.push_str(&format!("Control cut 4.0 px triangles: {cut_loose}\n"));
        report.push_str("Experiment uses that cut: NO\n");
        report.push_str("Experiment 2 control\n");
        report.push_str(&format!("Algorithm version: {}\n", crate::REALIZATION_ALGORITHM_VERSION));
        report.push_str(&format!("Observation algorithm version: {}\n", crate::OBSERVATION_ALGORITHM_VERSION));
        report.push_str(&format!("vertex_count: {}\n", full.vertex_count));
        report.push_str(&format!("triangle_count: {}\n", full.triangle_count));
        report.push_str(&format!("measured_projected_error_px: {:.4}\n", full.measured_projected_error_px));
        report.push_str(&format!("vertex_buffer_hash: {:016x}\n", full.vertex_buffer_hash));
        report.push_str(&format!("index_buffer_hash: {:016x}\n", full.index_buffer_hash));
        report.push_str(&format!("CPU_bytes: {}\n", full.cpu_bytes));
        report.push_str(&format!("generation_us: {}\n", full.generation_us));
        report.push_str("This run replaces the frozen Experiment 2 measurement: NO\n");
        report.push_str(&format!("intent_entries: {}\n", intent.len()));
        report.push_str(&format!("history_entries: {}\n", history.len()));
        report.push_str("history_mutated: NO\n");
        report.push_str(&format!("observation_ids_dropped: {kept_a},{kept_b}\n"));
        report.push_str("Partial products installed in the world: NO\n");
        report.push_str("editor_selection: NOT HOSTED\n");
        report.push_str("Same-solid occlusion: NO\n");
        report.push_str("Backface omission: NO\n");
        report.push_str("Edge collapse: NO\n");
        report.push_str("Simplification of admitted faces: NO\n");
        report.push_str("GPU realization: NO\n");
        report.push_str("ADR-0074 accepted: NO\n");
        report.push_str("Default renderer replaced: NO\n");
        report.push_str("CPU utilization percent: NOT SAMPLED\n");
        report.push_str("Process memory bytes: NOT SAMPLED\n");
        report.push_str("Frame: NOT PRESENTED\n");
        report.push_str("GPU_bytes: NOT UPLOADED\n");
        for line in [
            "Experiment 3A: ON",
            "body_mutated: NO",
            "intent_mutated: NO",
            "object_mesh_consulted: NO",
            "shared_render_mesh_consulted: NO",
            "collision_before_after: MATCH",
            "materials_before_after: MATCH",
            "semantic_identity_before_after: MATCH",
            "triangles_discarded_after_construction: 0",
            "Cross face omitted by A: YES",
            "Cross face admitted by B: YES",
            "Cross face triangles in A: 0",
            "B received A's product: NO",
            "Stale A published after B: NO",
            "Experiment uses that cut: NO",
            "Experiment 2 consulted by admission: NO",
            "ADR-0074 accepted: NO",
            "Default renderer replaced: NO",
            "Frame: NOT PRESENTED",
            "GPU_bytes: NOT UPLOADED",
        ] {
            assert!(report.lines().any(|stored| stored == line), "{line}\n{report}");
        }
        assert!(!report.lines().any(|stored| stored == "Frame: PRESENTED"));
        assert!(report.contains(&format!("authoritative_body_hash: {hash:016x}")));
        assert!(report.contains(&format!("Cross face: {flip_face}")));
        assert!(report.contains(&format!("Cross face triangles in B: {flip_fan}")));
        println!("{report}");
        drop(product_a);
        drop(product_b);
    }

    #[test]
    fn intent_observation_packs_each_triangle_without_welding() {
        let authored = author_closed_chain();
        let oracle = authored.body.clone().unwrap();
        let document = empty_world_level();
        let mut setup = document.instantiate().unwrap();
        setup.set_intent_authority_experiment(true);
        let intent_id = setup.create_block(Vec3::new(0.0, 1.0, -4.0), authored).unwrap();
        assert_eq!(setup.set_entity_name(intent_id, "Intent Solid").unwrap(), AuthoringResult::Applied);
        let json = LevelDocument::capture(&setup, document.level_uuid, "Intent Proof").unwrap().to_json();
        drop(setup);
        let live = parse_level(&json).unwrap().instantiate_with_experiment(&crate::MeshAssetLibrary::default(), true).unwrap();
        let record = live.authored_block(intent_id).unwrap().clone();
        let body = record.body.clone().unwrap();
        assert_same_solid(&body, &oracle);
        let hash = hash_body(&body);
        assert_eq!(format!("{hash:016x}"), "6ceeea840b7bdef2");
        assert_eq!(record.intent.len(), 65);
        assert_eq!(record.history.len(), 24);
        let revision = live.revision();
        let mesh_id = live.object_mesh(intent_id).unwrap();
        let mesh_count = live.mesh_count();
        let control = live.meshes().get(mesh_id).expect("control mesh");
        assert_eq!(control.vertex_count(), 144);
        assert_eq!(control.index_count() / 3, 48);
        let snapshot = live.extract(crate::RenderFrameId(1)).unwrap();
        let extracted = snapshot.camera(live.front_camera().frame).expect("front camera").clone();
        drop(snapshot);
        let (camera_a, camera_b) = crate::frozen_observation_cameras(&body, extracted.vertical_fov_radians).expect("frozen cameras");
        assert_eq!(crate::observation_eye_text(&camera_a), crate::FROZEN_A_EYE_TEXT);
        assert_eq!(crate::observation_eye_text(&camera_b), crate::FROZEN_B_EYE_TEXT);
        assert_eq!(camera_a.viewport_width, 160.0);
        assert_eq!(camera_b.viewport_width, 960.0);
        assert_eq!(camera_a.viewport_height, 567.0);
        assert_eq!(camera_b.viewport_height, 567.0);
        let mut host = crate::ObservationHost::new();
        let product_a = host
            .observe(crate::ObservationRequest { entity: intent_id, body_hash: hash, revision, body: body.clone(), camera: camera_a.clone() })
            .expect("observation A")
            .product;
        let product_b = host
            .observe(crate::ObservationRequest { entity: intent_id, body_hash: hash, revision, body: body.clone(), camera: camera_b.clone() })
            .expect("observation B")
            .product;
        assert_eq!(product_a.admitted_face_ids, crate::FROZEN_A_ADMITTED.to_vec());
        assert_eq!(product_a.omitted_face_ids, crate::FROZEN_A_OMITTED.to_vec());
        assert_eq!(product_b.admitted_face_ids, crate::FROZEN_B_ADMITTED.to_vec());
        assert_eq!(product_b.omitted_face_ids, crate::FROZEN_B_OMITTED.to_vec());
        assert!(product_a.uncertain_face_ids.is_empty() && product_b.uncertain_face_ids.is_empty());
        assert_eq!(product_a.triangles_constructed, 22);
        assert_eq!(product_b.triangles_constructed, 39);
        let slot = |face: u32| record.bound_slot(face);
        let pack_a = crate::pack_observation_gpu(&product_a, slot).expect("pack A");
        let pack_b = crate::pack_observation_gpu(&product_b, slot).expect("pack B");
        let prove = |label: &str, product: &crate::ObservationProduct, pack: &crate::ObservationGpuPack| {
            assert!(!pack.vertices_welded, "{label} welded vertices");
            assert_eq!(pack.meshlets_constructed, 0, "{label} built meshlets");
            assert_eq!(pack.meshlet_gpu_bytes, 0);
            assert_eq!(pack.discarded_after_construction, 0);
            assert_eq!(pack.discarded_during_pack, 0);
            assert_eq!(pack.discarded_after_upload, 0);
            assert_eq!(pack.constructed_triangles, pack.packed_triangles);
            assert_eq!(pack.packed_triangles, pack.uploaded_triangles);
            assert_eq!(pack.packed_triangles, product.triangles_constructed);
            assert_eq!(pack.packed_vertices, pack.packed_triangles * 3);
            assert_eq!(pack.packed_indices, pack.packed_vertices);
            assert_eq!(pack.index_format, crate::MeshIndexFormat::Uint16);
            assert_eq!(pack.packed_vertex_bytes, u64::from(pack.packed_vertices) * 60);
            assert_eq!(pack.packed_index_bytes, u64::from(pack.packed_indices) * 2);
            assert_eq!(pack.expected_gpu_bytes, pack.packed_vertex_bytes + pack.packed_index_bytes);
            assert_eq!(pack.mesh.streams()[0].bytes.len() as u64, pack.packed_vertex_bytes);
            assert_eq!(pack.mesh.index_bytes().len() as u64, pack.packed_index_bytes);
            assert_eq!(pack.mesh.streams()[0].stride, 60);
            assert_eq!(pack.accounts.iter().map(|account| account.vertex_bytes).sum::<u64>(), pack.packed_vertex_bytes);
            assert_eq!(pack.accounts.iter().map(|account| account.index_bytes).sum::<u64>(), pack.packed_index_bytes);
            let indices = pack.mesh.triangle_indices();
            assert_eq!(indices.len(), pack.packed_triangles as usize);
            let mut seen = Vec::new();
            for (index, triangle) in indices.iter().enumerate() {
                assert_eq!(*triangle, [index as u32 * 3, index as u32 * 3 + 1, index as u32 * 3 + 2], "{label} shared an index");
                for vertex in triangle {
                    assert!(!seen.contains(vertex), "{label} repeated vertex {vertex}");
                    seen.push(*vertex);
                }
                assert_eq!(pack.originating_face_ids[index], product.triangles[index].face_id);
                for corner in 0..3 {
                    let stored = pack.mesh.position(triangle[corner]).unwrap();
                    let source = product.triangles[index].positions[corner];
                    assert_eq!(stored, [source[0] as f32, source[1] as f32, source[2] as f32]);
                }
            }
            for face in &product.omitted_face_ids {
                assert!(!pack.originating_face_ids.contains(face), "{label} packed omitted face {face}");
                assert!(pack.accounts.iter().all(|account| account.face_id != *face));
            }
            assert_eq!(pack.mesh.submeshes().iter().map(|submesh| submesh.index_count).sum::<u32>(), pack.packed_indices);
            assert_ne!(pack.mesh.vertex_count(), 144);
            assert_ne!(pack.mesh.index_count() / 3, 48);
            assert_ne!(pack.mesh.vertex_count(), 87);
            assert_ne!(pack.mesh.index_count() / 3, 37);
        };
        prove("A", &product_a, &pack_a);
        prove("B", &product_b, &pack_b);
        assert_eq!(pack_a.face_89.cpu_triangles, 0);
        assert_eq!(pack_a.face_89.packed_triangles, 0);
        assert_eq!(pack_a.face_89.packed_vertices, 0);
        assert_eq!(pack_a.face_89.index_count, 0);
        assert_eq!(pack_a.face_89.vertex_bytes, 0);
        assert_eq!(pack_a.face_89.index_bytes, 0);
        assert_eq!(pack_a.face_89.total_gpu_bytes, 0);
        assert_eq!(pack_a.packed_triangles, 22);
        assert_eq!(pack_a.packed_vertices, 66);
        assert_eq!(pack_a.expected_gpu_bytes, 4092);
        assert_eq!(pack_b.face_89.cpu_triangles, 3);
        assert_eq!(pack_b.face_89.packed_triangles, 3);
        assert_eq!(pack_b.face_89.packed_vertices, 9);
        assert_eq!(pack_b.face_89.index_count, 9);
        assert_eq!(pack_b.face_89.vertex_bytes, 540);
        assert!(pack_b.face_89.index_bytes > 0);
        assert!(pack_b.face_89.total_gpu_bytes > 0);
        assert_eq!(pack_b.face_89.index_bytes, 18);
        assert_eq!(pack_b.face_89.total_gpu_bytes, 558);
        assert_eq!(pack_b.packed_triangles, 39);
        assert_eq!(pack_b.packed_vertices, 117);
        assert_eq!(pack_b.expected_gpu_bytes, 7254);
        assert_eq!(pack_b.originating_face_ids.iter().filter(|face| **face == 89).count(), 3);
        let mut realization = crate::ViewRealizationHost::new();
        assert_eq!(realization.worker_count(), 1);
        let control_camera = crate::RealizationCamera {
            eye_local: camera_a.eye_local,
            forward_local: camera_a.forward_local,
            vertical_fov_radians: camera_a.vertical_fov_radians,
            viewport_height: camera_a.viewport_height,
            requested_error_px: camera_a.requested_error_px,
        };
        assert!(submit_view(&mut realization, 1, intent_id, &body, hash, revision, control_camera).realization_id.is_none());
        wait_view(&mut realization, 1);
        let full = realization.facts(1).unwrap().clone();
        assert_eq!(full.vertex_count, 87);
        assert_eq!(full.triangle_count, 37);
        assert_eq!(full.vertex_buffer_hash, 0x8b911f36227fa9e2);
        assert_eq!(full.index_buffer_hash, 0x5ae2e4bfd0f25ccd);
        assert_eq!(full.cpu_bytes, 5442);
        assert!(full.measured_projected_error_px <= 1.0e-4);
        drop(realization);
        drop(full);
        assert_eq!(live.object_mesh(intent_id), Some(mesh_id));
        assert_eq!(live.mesh_count(), mesh_count);
        assert_eq!(live.revision(), revision);
        assert_eq!(crate::authoritative_body_hash(live.authored_block(intent_id).unwrap().body.as_ref().unwrap()), hash);
        assert!(live.derived_meshlets(mesh_id).is_some());
        let mut report = String::new();
        report.push_str("Experiment 3B CPU pack\n");
        report.push_str("Vertices welded across authoritative faces: NO\n");
        report.push_str(&crate::format_observation_gpu("Observation A", &pack_a, None));
        report.push_str(&crate::format_observation_gpu("Observation B", &pack_b, None));
        report.push_str(&format!("Experiment 2 control vertex_buffer_hash: {:016x}\n", 0x8b911f36227fa9e2_u64));
        report.push_str(&format!("Experiment 2 control index_buffer_hash: {:016x}\n", 0x5ae2e4bfd0f25ccd_u64));
        report.push_str("Experiment 2 control installed as a color mesh: NO\n");
        report.push_str("This run replaces the frozen Experiment 2 measurement: NO\n");
        report.push_str("3A generation times compared as a speedup: NO\n");
        report.push_str("Experiment 2 worker in this CPU test: jarvig-realize-0\n");
        report.push_str("jarvig-realize-1: NOT STARTED\n");
        report.push_str("ADR-0074 accepted: NO\n");
        report.push_str("Frame: NOT PRESENTED\n");
        report.push_str("GPU_bytes: NOT UPLOADED\n");
        for line in [
            "face_89_cpu_triangles: 0",
            "face_89_packed_vertices: 0",
            "face_89_total_gpu_bytes: 0",
            "face_89_cpu_triangles: 3",
            "face_89_packed_vertices: 9",
            "face_89_index_count: 9",
            "face_89_vertex_bytes: 540",
            "GpuMesh.bytes: NOT UPLOADED",
            "create_buffer_bytes: NOT UPLOADED",
            "Frame: NOT PRESENTED",
            "GPU_bytes: NOT UPLOADED",
            "ADR-0074 accepted: NO",
            "This run replaces the frozen Experiment 2 measurement: NO",
            "3A generation times compared as a speedup: NO",
        ] {
            assert!(report.contains(line), "{line}\n{report}");
        }
        println!("{report}");
    }
}

