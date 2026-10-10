//! Experiment 4. The eligible intent tape drives two observation realizations.
//!
//! The curved patches are an experiment-local trailer. They are not an
//! `IntentPayload`, not a `BlockOp`, and not a level field. This module does
//! not call `realize_direct`, `classify_face`, `fan_admitted`,
//! `authoritative_triangles`, or `build_realization`.

use crate::mesh::{
    create_mesh, Mesh, MeshDesc, MeshError, MeshIndexFormat, MeshTopology, MeshVertexAttribute, MeshVertexFormat, SubmeshDesc, VertexStreamDesc,
};
use crate::microgeometry::projected_detail_px;
use crate::parametric::{BlockRecord, ConcreteElement, IntentEntry, IntentPayload};
use crate::semantic_shadow::{intent_authority_candidate, semantic_shadow_diagnostic, IntentAuthorityCandidate};
use crate::topology::SolidBody;

/// Bump only if this bridge constructor changes. The 3A, 3B, and 3C versions stay where they are.
pub const INTENT_OBSERVATION_ALGORITHM_VERSION: u32 = 1;
pub const PATCH_A: u32 = 1001;
pub const PATCH_B: u32 = 1002;
pub const PATCH_A_TRANSLATION: [f64; 3] = [7.0, 0.0, 0.0];
pub const PATCH_B_TRANSLATION: [f64; 3] = [4.0, 0.0, 0.0];
const INTENT_N_CAP: u32 = 64;
const INTENT_ERROR_SLACK_PX: f32 = 0.05;
const INTENT_VERTEX_STRIDE: u32 = 60;
const FROZEN_3C_AUTHORITY: u64 = 0xac80_f43b_acf3_c609;

/// Object-local camera. Far and Close are the frozen 3C pair shifted by `(4, 0, 0)`.
#[derive(Clone, Copy, Debug)]
pub struct IntentObservationCamera {
    pub eye: [f64; 3],
    pub forward: [f64; 3],
    pub up: [f64; 3],
    pub vertical_fov_radians: f64,
    pub near_m: f64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub requested_error_px: f32,
}

/// Why a loop or a patch was kept. Only `Omit` skips construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentAdmission {
    Omit,
    AdmitInView,
    AdmitBehindCamera,
    AdmitCrossesNear,
    AdmitUncertain,
}

impl IntentAdmission {
    pub fn name(self) -> &'static str {
        match self {
            Self::Omit => "Omit",
            Self::AdmitInView => "AdmitInView",
            Self::AdmitBehindCamera => "AdmitBehindCamera",
            Self::AdmitCrossesNear => "AdmitCrossesNear",
            Self::AdmitUncertain => "AdmitUncertain",
        }
    }

    pub fn omitted(self) -> bool {
        self == Self::Omit
    }
}

/// One curved patch. `n` is the scalar search. A planar face does not use this.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntentPatchAccount {
    pub surface_id: u32,
    pub admission: IntentAdmission,
    pub n: u32,
    pub triangles_constructed: u32,
    pub candidate_grids_emitted: u32,
    pub candidate_grids_discarded: u32,
    pub resolution_predicate_evaluations: u32,
    pub coarser_failed: bool,
    pub coarser_px: f32,
    pub measured_error_px: f32,
    pub packed_triangles: u32,
    pub packed_vertices: u32,
    pub index_count: u32,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub gpu_bytes: u64,
    pub position_hash: u64,
}

/// One planar face from the scratch replay. The fan is exact. There is no `n` ladder.
#[derive(Clone, Debug, PartialEq)]
pub struct IntentPlanarAccount {
    pub face_id: u32,
    pub identities: Vec<String>,
    pub admission: IntentAdmission,
    pub loop_len: u32,
    pub triangles_constructed: u32,
    pub packed_triangles: u32,
    pub packed_vertices: u32,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub gpu_bytes: u64,
    /// FNV-1a 64 of the loop's f64 positions, admitted or not.
    pub position_hash: u64,
}

/// One emitted triangle. Omitted surfaces, including patch 1001, are absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentTraceRecord {
    pub surface_id: u32,
    pub cell_i: u32,
    pub cell_j: u32,
    pub triangle_in_cell: u8,
}

/// One observation. The mesh is the admitted fans plus the admitted patch, unwelded.
#[derive(Clone, Debug)]
pub struct IntentObservationProduct {
    pub authority_hash: u64,
    pub revision: u64,
    pub cache_key: String,
    pub patch_a: IntentPatchAccount,
    pub patch_b: IntentPatchAccount,
    pub planar: Vec<IntentPlanarAccount>,
    pub admitted_planar_ids: Vec<u32>,
    pub omitted_planar_ids: Vec<u32>,
    pub trace: Vec<IntentTraceRecord>,
    pub mesh: Mesh,
    pub constructed_triangles: u32,
    pub packed_triangles: u32,
    pub uploaded_triangles: u32,
    pub discarded_after_construction: u32,
    pub discarded_during_pack: u32,
    pub discarded_after_upload: u32,
    pub packed_vertices: u32,
    pub packed_indices: u32,
    pub packed_vertex_bytes: u64,
    pub packed_index_bytes: u64,
    pub expected_gpu_bytes: u64,
    pub cpu_bytes: u64,
    pub index_format: MeshIndexFormat,
    pub position_hash: u64,
    pub index_hash: u64,
    pub meshlets_constructed: u32,
    pub degenerate_normals: u32,
    pub vertices_welded: bool,
    pub rejected_n_allocated_mesh: bool,
    pub object_mesh_consulted: bool,
    pub record_body_consulted: bool,
    pub authoritative_triangles_consulted: bool,
    pub build_realization_consulted: bool,
    pub realize_direct_consulted: bool,
    pub analytic_specimen_consulted: bool,
    pub einstein_consulted: bool,
    pub candidate_grids_emitted: u32,
    pub candidate_grids_discarded: u32,
    pub resolution_predicate_evaluations: u32,
}

struct Chosen {
    n: u32,
    evaluations: u32,
    coarser_failed: bool,
    coarser_px: f32,
}

struct Emitted {
    surface_id: u32,
    corners: [[f64; 3]; 3],
    trace: IntentTraceRecord,
}

struct PackedSurface {
    surface_id: u32,
    packed_triangles: u32,
    packed_vertices: u32,
    index_count: u32,
    vertex_bytes: u64,
    index_bytes: u64,
    gpu_bytes: u64,
    position_hash: u64,
}

pub fn far_intent_camera() -> IntentObservationCamera {
    intent_camera([4.0, 0.0, 8.0], 160.0)
}

pub fn close_intent_camera() -> IntentObservationCamera {
    intent_camera([4.0, 0.0, 2.2], 960.0)
}

/// Y grows by 0.1 m on a copy. The live record is not the copy.
pub fn size_edit_copy(record: &BlockRecord) -> Result<BlockRecord, String> {
    if record.body.is_some() {
        return Err("the size edit refuses a record that already has a body".into());
    }
    let size_m = [record.size_m[0], record.size_m[1] + 0.1, record.size_m[2]];
    if size_m.iter().any(|axis| !axis.is_finite() || *axis <= 0.0) {
        return Err("the size edit was not a positive finite extent".into());
    }
    let mut copy = record.clone();
    copy.intent.push(IntentEntry { groups: None, payload: IntentPayload::Size { size_m } });
    Ok(copy)
}

/// The size written by [`size_edit_copy`], as bits. The decimal spelling is not frozen.
pub fn size_edit_bits(record: &BlockRecord) -> String {
    let size_m = [record.size_m[0], record.size_m[1] + 0.1, record.size_m[2]];
    format!("{:016x} {:016x} {:016x}", size_m[0].to_bits(), size_m[1].to_bits(), size_m[2].to_bits())
}

pub fn authority_hash(record: &BlockRecord, patch_b_translation: [f64; 3]) -> Result<u64, String> {
    let text = authority_text(record, patch_b_translation)?;
    let hash = fnv64(text.as_bytes());
    if hash == FROZEN_3C_AUTHORITY {
        return Err("the bridge authority hashed to the frozen 3C specimen".into());
    }
    Ok(hash)
}

/// Scratch loops from the tape, then one observation. Equal planar sets are returned, not repaired.
pub fn realize_intent_observation(
    record: &BlockRecord,
    camera: &IntentObservationCamera,
    revision: u64,
    patch_b_translation: [f64; 3],
) -> Result<IntentObservationProduct, String> {
    if revision == 0 {
        return Err("the authority revision starts at 1".into());
    }
    if record.body.is_some() {
        return Err("record.body is present. Experiment 4 does not read it".into());
    }
    if patch_b_translation.iter().any(|axis| !axis.is_finite()) {
        return Err("patch B translation was not finite".into());
    }
    let hash = authority_hash(record, patch_b_translation)?;
    let body = match intent_authority_candidate(record) {
        IntentAuthorityCandidate::Reconstructable(body) => body,
        IntentAuthorityCandidate::Refused(reason) => return Err(format!("the intent tape did not reconstruct: {reason}")),
    };
    if body.faces.iter().any(|face| face.id == PATCH_A || face.id == PATCH_B) {
        return Err("a scratch face id collides with patch 1001 or 1002".into());
    }
    let mut planar = Vec::new();
    let mut emitted = Vec::new();
    let mut admitted = Vec::new();
    let mut omitted = Vec::new();
    let mut faces: Vec<u32> = body.faces.iter().map(|face| face.id).collect();
    faces.sort_unstable();
    for face_id in faces {
        let account = planar_face(record, &body, face_id, camera, &mut emitted)?;
        if account.admission.omitted() {
            omitted.push(face_id);
        } else {
            admitted.push(face_id);
        }
        planar.push(account);
    }
    let patch_a_admission = classify_patch(PATCH_A, PATCH_A_TRANSLATION, camera)?;
    if patch_a_admission != IntentAdmission::Omit {
        return Err(format!("patch 1001 is {}. The eye is not moved.", patch_a_admission.name()));
    }
    let patch_b_admission = classify_patch(PATCH_B, patch_b_translation, camera)?;
    if patch_b_admission != IntentAdmission::AdmitInView {
        return Err(format!("patch 1002 is {}. This pair requires AdmitInView.", patch_b_admission.name()));
    }
    let chosen = choose_n(patch_b_translation, camera)?;
    let measured = predicate_max_px(patch_b_translation, chosen.n, camera)?;
    let limit = camera.requested_error_px + INTENT_ERROR_SLACK_PX;
    if !measured.is_finite() || measured > limit {
        return Err(format!("patch 1002 measured {measured:.6} px after emit, above {limit:.6}"));
    }
    if chosen.n > 1 && (!chosen.coarser_failed || chosen.coarser_px <= camera.requested_error_px) {
        return Err(format!("patch 1002 emitted n {} without a failing coarser grid", chosen.n));
    }
    let patch_triangles = emit_grid(patch_b_translation, chosen.n, &mut emitted)?;
    if patch_triangles != chosen.n.saturating_mul(chosen.n).saturating_mul(2) {
        return Err(format!("patch 1002 emitted {patch_triangles} triangles for n {}", chosen.n));
    }
    if emitted.iter().any(|item| item.surface_id == PATCH_A) {
        return Err("omitted patch 1001 entered the construction trace".into());
    }
    let packed = pack_intent(&emitted)?;
    let mut patch_a = zero_patch(PATCH_A);
    patch_a.admission = IntentAdmission::Omit;
    let mut patch_b = zero_patch(PATCH_B);
    patch_b.admission = IntentAdmission::AdmitInView;
    patch_b.n = chosen.n;
    patch_b.triangles_constructed = patch_triangles;
    patch_b.candidate_grids_emitted = 1;
    patch_b.resolution_predicate_evaluations = chosen.evaluations;
    patch_b.coarser_failed = chosen.coarser_failed;
    patch_b.coarser_px = chosen.coarser_px;
    patch_b.measured_error_px = measured;
    apply_packed(&mut patch_a, &packed.surfaces)?;
    apply_packed(&mut patch_b, &packed.surfaces)?;
    for account in &mut planar {
        if let Some(bytes) = packed.surfaces.iter().find(|item| item.surface_id == account.face_id) {
            account.packed_triangles = bytes.packed_triangles;
            account.packed_vertices = bytes.packed_vertices;
            account.vertex_bytes = bytes.vertex_bytes;
            account.index_bytes = bytes.index_bytes;
            account.gpu_bytes = bytes.gpu_bytes;
        }
        if account.admission.omitted() && (account.packed_triangles != 0 || account.gpu_bytes != 0 || account.triangles_constructed != 0) {
            return Err(format!("omitted planar face {} contributed primitives", account.face_id));
        }
        if !account.admission.omitted() && account.packed_triangles != account.triangles_constructed {
            return Err(format!("planar face {} dropped triangles during pack", account.face_id));
        }
    }
    if patch_a.gpu_bytes != 0 || patch_a.triangles_constructed != 0 || patch_a.packed_triangles != 0 {
        return Err("omitted patch 1001 contributed primitives".into());
    }
    if patch_b.packed_triangles != patch_b.triangles_constructed {
        return Err("patch 1002 dropped triangles during pack".into());
    }
    let summed_vertex = patch_a.vertex_bytes + patch_b.vertex_bytes + planar.iter().map(|account| account.vertex_bytes).sum::<u64>();
    let summed_index = patch_a.index_bytes + patch_b.index_bytes + planar.iter().map(|account| account.index_bytes).sum::<u64>();
    if summed_vertex != packed.packed_vertex_bytes || summed_index != packed.packed_index_bytes {
        return Err("per-surface byte accounts do not add up to the observation mesh".into());
    }
    if packed.discarded_during_pack != 0 || packed.constructed_triangles != packed.packed_triangles {
        return Err("the intent pack discarded or split triangles".into());
    }
    let trace: Vec<IntentTraceRecord> = emitted.iter().map(|item| item.trace).collect();
    if trace.len() as u32 != packed.constructed_triangles {
        return Err("the construction trace does not match the packed triangles".into());
    }
    Ok(IntentObservationProduct {
        authority_hash: hash,
        revision,
        cache_key: cache_key(hash, revision, camera),
        patch_a,
        patch_b,
        planar,
        admitted_planar_ids: admitted,
        omitted_planar_ids: omitted,
        trace,
        mesh: packed.mesh,
        constructed_triangles: packed.constructed_triangles,
        packed_triangles: packed.packed_triangles,
        uploaded_triangles: packed.packed_triangles,
        discarded_after_construction: 0,
        discarded_during_pack: 0,
        discarded_after_upload: 0,
        packed_vertices: packed.packed_vertices,
        packed_indices: packed.packed_indices,
        packed_vertex_bytes: packed.packed_vertex_bytes,
        packed_index_bytes: packed.packed_index_bytes,
        expected_gpu_bytes: packed.expected_gpu_bytes,
        cpu_bytes: packed.packed_vertex_bytes + packed.packed_index_bytes,
        index_format: packed.index_format,
        position_hash: packed.position_hash,
        index_hash: packed.index_hash,
        meshlets_constructed: 0,
        degenerate_normals: packed.degenerate_normals,
        vertices_welded: false,
        rejected_n_allocated_mesh: false,
        object_mesh_consulted: false,
        record_body_consulted: false,
        authoritative_triangles_consulted: false,
        build_realization_consulted: false,
        realize_direct_consulted: false,
        analytic_specimen_consulted: false,
        einstein_consulted: false,
        candidate_grids_emitted: 1,
        candidate_grids_discarded: 0,
        resolution_predicate_evaluations: chosen.evaluations,
    })
}

pub fn format_intent_product(label: &str, product: &IntentObservationProduct) -> String {
    let mut out = String::new();
    out.push_str(&format!("{label}\n"));
    out.push_str(&format!("authority_hash: {:016x}\n", product.authority_hash));
    out.push_str(&format!("authority_revision: {}\n", product.revision));
    out.push_str(&format!("cache_key: {}\n", product.cache_key));
    out.push_str(&format!("INTENT_OBSERVATION_ALGORITHM_VERSION: {INTENT_OBSERVATION_ALGORITHM_VERSION}\n"));
    out.push_str("curved trailer is an IntentPayload: NO\n");
    out.push_str("curved trailer is experiment-local: YES\n");
    for account in [&product.patch_a, &product.patch_b] {
        out.push_str(&format!("surface {} admission: {}\n", account.surface_id, account.admission.name()));
        out.push_str(&format!("surface {} n: {}\n", account.surface_id, account.n));
        out.push_str(&format!("surface {} triangles_constructed: {}\n", account.surface_id, account.triangles_constructed));
        out.push_str(&format!("surface {} measured_projected_error_px: {:.6}\n", account.surface_id, account.measured_error_px));
        out.push_str(&format!("surface {} coarser_predicate_failed: {}\n", account.surface_id, yes(account.coarser_failed)));
        out.push_str(&format!("surface {} coarser_px: {:.6}\n", account.surface_id, account.coarser_px));
        out.push_str(&format!("surface {} candidate_grids_emitted: {}\n", account.surface_id, account.candidate_grids_emitted));
        out.push_str(&format!("surface {} candidate_grids_discarded: {}\n", account.surface_id, account.candidate_grids_discarded));
        out.push_str(&format!("surface {} resolution_predicate_evaluations: {}\n", account.surface_id, account.resolution_predicate_evaluations));
        out.push_str(&format!("surface {} packed_triangles: {}\n", account.surface_id, account.packed_triangles));
        out.push_str(&format!("surface {} packed_vertices: {}\n", account.surface_id, account.packed_vertices));
        out.push_str(&format!("surface {} index_count: {}\n", account.surface_id, account.index_count));
        out.push_str(&format!("surface {} vertex_bytes: {}\n", account.surface_id, account.vertex_bytes));
        out.push_str(&format!("surface {} index_bytes: {}\n", account.surface_id, account.index_bytes));
        out.push_str(&format!("surface {} gpu_bytes: {}\n", account.surface_id, account.gpu_bytes));
        out.push_str(&format!("surface {} position_hash: {:016x}\n", account.surface_id, account.position_hash));
    }
    out.push_str(&format!("planar admitted: {}\n", id_list(&product.admitted_planar_ids)));
    out.push_str(&format!("planar omitted: {}\n", id_list(&product.omitted_planar_ids)));
    for account in &product.planar {
        let identity = if account.identities.is_empty() {
            "name was not on the replay".to_string()
        } else {
            account.identities.join(" | ")
        };
        out.push_str(&format!("planar face {} admission: {}\n", account.face_id, account.admission.name()));
        out.push_str(&format!("planar face {} semantic identity: {identity}\n", account.face_id));
        out.push_str(&format!("planar face {} concrete id: {}\n", account.face_id, account.face_id));
        out.push_str(&format!("planar face {} triangles_constructed: {}\n", account.face_id, account.triangles_constructed));
        out.push_str(&format!("planar face {} packed_triangles: {}\n", account.face_id, account.packed_triangles));
        out.push_str(&format!("planar face {} gpu_bytes: {}\n", account.face_id, account.gpu_bytes));
        out.push_str(&format!("planar face {} position_hash: {:016x}\n", account.face_id, account.position_hash));
        out.push_str(&format!("planar face {} n ladder: NO\n", account.face_id));
    }
    out.push_str(&format!("triangles_directly_constructed: {}\n", product.constructed_triangles));
    out.push_str(&format!("packed_triangles: {}\n", product.packed_triangles));
    out.push_str(&format!("uploaded_triangles: {}\n", product.uploaded_triangles));
    out.push_str(&format!("packed_vertices: {}\n", product.packed_vertices));
    out.push_str(&format!("expected_gpu_bytes: {}\n", product.expected_gpu_bytes));
    out.push_str(&format!("index_format: {}\n", index_format_name(product.index_format)));
    out.push_str(&format!("position_hash: {:016x}\n", product.position_hash));
    out.push_str(&format!("index_hash: {:016x}\n", product.index_hash));
    out.push_str(&format!("triangles_discarded_after_construction: {}\n", product.discarded_after_construction));
    out.push_str(&format!("triangles_dropped_during_pack: {}\n", product.discarded_during_pack));
    out.push_str(&format!("triangles_discarded_after_upload: {}\n", product.discarded_after_upload));
    out.push_str(&format!("candidate_grids_emitted: {}\n", product.candidate_grids_emitted));
    out.push_str(&format!("candidate_grids_discarded: {}\n", product.candidate_grids_discarded));
    out.push_str(&format!("resolution_predicate_evaluations: {}\n", product.resolution_predicate_evaluations));
    out.push_str("resolution_predicate_evaluations are a discard count: NO\n");
    out.push_str(&format!("rejected n allocated a mesh: {}\n", yes(product.rejected_n_allocated_mesh)));
    out.push_str(&format!("vertices_welded: {}\n", yes(product.vertices_welded)));
    out.push_str(&format!("meshlets_constructed: {}\n", product.meshlets_constructed));
    out.push_str(&format!("object_mesh_consulted: {}\n", yes(product.object_mesh_consulted)));
    out.push_str(&format!("record_body_consulted: {}\n", yes(product.record_body_consulted)));
    out.push_str(&format!("authoritative_triangles_consulted: {}\n", yes(product.authoritative_triangles_consulted)));
    out.push_str(&format!("build_realization_consulted: {}\n", yes(product.build_realization_consulted)));
    out.push_str(&format!("realize_direct_consulted: {}\n", yes(product.realize_direct_consulted)));
    out.push_str(&format!("analytic_specimen_consulted: {}\n", yes(product.analytic_specimen_consulted)));
    out.push_str(&format!("einstein_consulted: {}\n", yes(product.einstein_consulted)));
    out.push_str(&format!("construction_trace_records: {}\n", product.trace.len()));
    out.push_str(&format!("construction_trace_contains_surface_1001: {}\n", yes(product.trace.iter().any(|record| record.surface_id == PATCH_A))));
    for record in &product.trace {
        out.push_str(&format!(
            "trace surface {} cell {} {} triangle {}\n",
            record.surface_id, record.cell_i, record.cell_j, record.triangle_in_cell
        ));
    }
    out
}

/// True when at least one tape-derived planar position hash or face id differs.
pub fn planar_geometry_changed(before: &IntentObservationProduct, after: &IntentObservationProduct) -> bool {
    let left: Vec<(u32, u64)> = before.planar.iter().map(|account| (account.face_id, account.position_hash)).collect();
    let right: Vec<(u32, u64)> = after.planar.iter().map(|account| (account.face_id, account.position_hash)).collect();
    left != right
}

fn intent_camera(eye: [f64; 3], width: f32) -> IntentObservationCamera {
    IntentObservationCamera {
        eye,
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_radians: 60.0_f64.to_radians(),
        near_m: 0.1,
        viewport_width: width,
        viewport_height: 567.0,
        requested_error_px: 0.5,
    }
}

fn authority_text(record: &BlockRecord, patch_b_translation: [f64; 3]) -> Result<String, String> {
    let seed = record.seed_size_m.ok_or("the eligible tape has no creation size")?;
    if seed.iter().any(|axis| !axis.is_finite()) || record.intent.is_empty() {
        return Err("the eligible tape is empty or its creation size is not finite".into());
    }
    let mut text = String::from("intent-observation-v1\n");
    text.push_str("authority: eligible intent tape plus experiment-local curved trailer\n");
    text.push_str(&format!("seed {} {} {}\n", bits(seed[0]), bits(seed[1]), bits(seed[2])));
    text.push_str(&format!("intent_count {}\n", record.intent.len()));
    for (index, entry) in record.intent.iter().enumerate() {
        text.push_str(&format!("entry {} {}|{}\n", index + 1, payload_text(&entry.payload)?, groups_text(entry)));
    }
    text.push_str("EXPERIMENT-LOCAL curved trailer. Not an IntentPayload. Not a BlockOp. Not a level field.\n");
    text.push_str("equation p(u,v)=(u,v,1)/sqrt(u*u+v*v+1)\n");
    text.push_str("domain u,v in [-0.5,0.5]\n");
    text.push_str(&format!(
        "patch {PATCH_A} translation {} {} {}\n",
        bits(PATCH_A_TRANSLATION[0]),
        bits(PATCH_A_TRANSLATION[1]),
        bits(PATCH_A_TRANSLATION[2])
    ));
    text.push_str(&format!(
        "patch {PATCH_B} translation {} {} {}\n",
        bits(patch_b_translation[0]),
        bits(patch_b_translation[1]),
        bits(patch_b_translation[2])
    ));
    Ok(text)
}

fn payload_text(payload: &IntentPayload) -> Result<String, String> {
    let line = match payload {
        IntentPayload::Size { size_m } => format!("size {} {} {}", bits(size_m[0]), bits(size_m[1]), bits(size_m[2])),
        IntentPayload::Subdivide { u, v } => format!("subdivide {u} {v}"),
        IntentPayload::Extrude { delta_m } => format!("extrude {} {} {}", bits(delta_m[0]), bits(delta_m[1]), bits(delta_m[2])),
        IntentPayload::Split => "split".to_string(),
        IntentPayload::Bevel { width_m } => format!("bevel {}", bits(*width_m)),
        IntentPayload::MoveEdge { delta_m } => format!("move-edge {} {} {}", bits(delta_m[0]), bits(delta_m[1]), bits(delta_m[2])),
        IntentPayload::MoveVertex { delta_m } => format!("move-vertex {} {} {}", bits(delta_m[0]), bits(delta_m[1]), bits(delta_m[2])),
        IntentPayload::ExtrudeEdge { delta_m } => format!("extrude-edge {} {} {}", bits(delta_m[0]), bits(delta_m[1]), bits(delta_m[2])),
        IntentPayload::Mirror { axis } => format!("mirror {axis}"),
        IntentPayload::PushFace { face, distance_m } => format!("push-face {face} {}", bits(*distance_m)),
        IntentPayload::Gap { operation } => format!("gap {}:{}", operation.len(), operation),
        IntentPayload::AnalyticSurface { identity, .. } => format!("analytic-surface {identity}"),
        IntentPayload::Round { radius_m } => format!("round {}", bits(*radius_m)),
        IntentPayload::Seed => "seed".to_string(),
    };
    if line.chars().any(|ch| ch == '|' || ch == '\n') {
        return Err("an intent payload did not fit the authority text".into());
    }
    Ok(line)
}

fn groups_text(entry: &IntentEntry) -> String {
    let Some(groups) = &entry.groups else {
        return "groups:none".to_string();
    };
    let mut text = format!("groups:{}", groups.len());
    for group in groups {
        text.push_str(&format!(":n{}", group.len()));
        for token in group {
            text.push_str(&format!(":b{}:{token}", token.len()));
        }
    }
    text
}

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn planar_face(
    record: &BlockRecord,
    body: &SolidBody,
    face_id: u32,
    camera: &IntentObservationCamera,
    emitted: &mut Vec<Emitted>,
) -> Result<IntentPlanarAccount, String> {
    let loop_ = body.face_loop(face_id).ok_or_else(|| format!("scratch face {face_id} has no loop"))?;
    if loop_.len() < 3 || loop_.iter().enumerate().any(|(index, id)| loop_[..index].contains(id)) {
        return Err(format!("scratch face {face_id} is not one simple loop"));
    }
    let positions = body.face_positions(face_id).ok_or_else(|| format!("scratch face {face_id} is not one simple loop"))?;
    if positions.len() != loop_.len() {
        return Err(format!("scratch face {face_id} is not one simple loop"));
    }
    let admission = classify_loop(&positions, camera);
    let identities = face_identities(record, face_id);
    let mut position_bytes = Vec::new();
    for position in &positions {
        for axis in position {
            position_bytes.extend_from_slice(&axis.to_bits().to_le_bytes());
        }
    }
    let fan = u32::try_from(positions.len() - 2).map_err(|_| format!("scratch face {face_id} fan overflowed"))?;
    let triangles = if admission.omitted() {
        0
    } else {
        for index in 1..positions.len() - 1 {
            emitted.push(Emitted {
                surface_id: face_id,
                corners: [positions[0], positions[index], positions[index + 1]],
                trace: IntentTraceRecord { surface_id: face_id, cell_i: index as u32, cell_j: 0, triangle_in_cell: 0 },
            });
        }
        fan
    };
    Ok(IntentPlanarAccount {
        face_id,
        identities,
        admission,
        loop_len: positions.len() as u32,
        triangles_constructed: triangles,
        packed_triangles: 0,
        packed_vertices: 0,
        vertex_bytes: 0,
        index_bytes: 0,
        gpu_bytes: 0,
        position_hash: fnv64(&position_bytes),
    })
}

fn face_identities(record: &BlockRecord, face: u32) -> Vec<String> {
    let text = semantic_shadow_diagnostic(record, Some(ConcreteElement::Face(face)));
    let mut names = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Semantic identity: ") {
            if rest != "none" {
                names.push(rest.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn classify_loop(positions: &[[f64; 3]], camera: &IntentObservationCamera) -> IntentAdmission {
    if positions.len() < 3 || positions.iter().any(|position| position.iter().any(|axis| !axis.is_finite())) {
        return IntentAdmission::AdmitUncertain;
    }
    let Some((right, up, forward)) = camera_basis(camera.forward, camera.up) else {
        return IntentAdmission::AdmitUncertain;
    };
    let width = f64::from(camera.viewport_width);
    let height = f64::from(camera.viewport_height);
    let error = f64::from(camera.requested_error_px);
    if !camera.near_m.is_finite() || camera.near_m <= 0.0 || !error.is_finite() || error < 0.0 || width < 1.0 || height < 1.0 {
        return IntentAdmission::AdmitUncertain;
    }
    let half = camera.vertical_fov_radians * 0.5;
    if !half.is_finite() || half <= 0.0 || half >= std::f64::consts::FRAC_PI_2 {
        return IntentAdmission::AdmitUncertain;
    }
    let tan_half_v = half.tan();
    let tan_half_h = tan_half_v * (width / height);
    if !tan_half_v.is_finite() || !tan_half_h.is_finite() || tan_half_v <= 0.0 || tan_half_h <= 0.0 {
        return IntentAdmission::AdmitUncertain;
    }
    let mut depths = Vec::with_capacity(positions.len());
    for position in positions {
        depths.push(dot(sub(*position, camera.eye), forward));
    }
    if depths.iter().any(|depth| !depth.is_finite()) {
        return IntentAdmission::AdmitUncertain;
    }
    if depths.iter().any(|depth| *depth <= 0.0) {
        return IntentAdmission::AdmitBehindCamera;
    }
    if depths.iter().any(|depth| *depth <= camera.near_m) {
        return IntentAdmission::AdmitCrossesNear;
    }
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for (position, depth) in positions.iter().zip(depths.iter()) {
        let relative = sub(*position, camera.eye);
        let px = (dot(relative, right) / depth) / tan_half_h;
        let py = (dot(relative, up) / depth) / tan_half_v;
        if !px.is_finite() || !py.is_finite() {
            return IntentAdmission::AdmitUncertain;
        }
        let screen_x = (px * 0.5 + 0.5) * width;
        let screen_y = (py * 0.5 + 0.5) * height;
        min_x = min_x.min(screen_x);
        max_x = max_x.max(screen_x);
        min_y = min_y.min(screen_y);
        max_y = max_y.max(screen_y);
    }
    let misses = max_x < -error || min_x > width + error || max_y < -error || min_y > height + error;
    if misses { IntentAdmission::Omit } else { IntentAdmission::AdmitInView }
}

fn classify_patch(surface_id: u32, translation: [f64; 3], camera: &IntentObservationCamera) -> Result<IntentAdmission, String> {
    let corners = patch_corners(translation)?;
    let Some((right, up, forward)) = camera_basis(camera.forward, camera.up) else {
        return Ok(IntentAdmission::AdmitUncertain);
    };
    let width = f64::from(camera.viewport_width);
    let height = f64::from(camera.viewport_height);
    let error = f64::from(camera.requested_error_px);
    if !camera.near_m.is_finite() || camera.near_m <= 0.0 || !error.is_finite() || error < 0.0 || width < 1.0 || height < 1.0 {
        return Ok(IntentAdmission::AdmitUncertain);
    }
    let half = camera.vertical_fov_radians * 0.5;
    if !half.is_finite() || half <= 0.0 || half >= std::f64::consts::FRAC_PI_2 {
        return Ok(IntentAdmission::AdmitUncertain);
    }
    let tan_half_v = half.tan();
    let tan_half_h = tan_half_v * (width / height);
    if !tan_half_v.is_finite() || !tan_half_h.is_finite() || tan_half_v <= 0.0 || tan_half_h <= 0.0 {
        return Ok(IntentAdmission::AdmitUncertain);
    }
    let mut depths = [0.0; 8];
    for (index, corner) in corners.iter().enumerate() {
        depths[index] = dot(sub(*corner, camera.eye), forward);
    }
    if depths.iter().any(|depth| !depth.is_finite()) {
        return Ok(IntentAdmission::AdmitUncertain);
    }
    if depths.iter().any(|depth| *depth <= 0.0) {
        return Ok(IntentAdmission::AdmitBehindCamera);
    }
    if depths.iter().any(|depth| *depth <= camera.near_m) {
        return Ok(IntentAdmission::AdmitCrossesNear);
    }
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for (corner, depth) in corners.iter().zip(depths.iter()) {
        let relative = sub(*corner, camera.eye);
        let px = (dot(relative, right) / depth) / tan_half_h;
        let py = (dot(relative, up) / depth) / tan_half_v;
        if !px.is_finite() || !py.is_finite() {
            return Ok(IntentAdmission::AdmitUncertain);
        }
        let screen_x = (px * 0.5 + 0.5) * width;
        let screen_y = (py * 0.5 + 0.5) * height;
        min_x = min_x.min(screen_x);
        max_x = max_x.max(screen_x);
        min_y = min_y.min(screen_y);
        max_y = max_y.max(screen_y);
    }
    let misses = max_x < -error || min_x > width + error || max_y < -error || min_y > height + error;
    let _ = surface_id;
    Ok(if misses { IntentAdmission::Omit } else { IntentAdmission::AdmitInView })
}

fn predicate_max_px(translation: [f64; 3], n: u32, camera: &IntentObservationCamera) -> Result<f32, String> {
    if n == 0 || n > INTENT_N_CAP {
        return Err(format!("n {n} is outside 1..={INTENT_N_CAP}"));
    }
    let Some((_, _, forward)) = camera_basis(camera.forward, camera.up) else {
        return Err("the intent camera basis is degenerate".into());
    };
    let tan_half = ((camera.vertical_fov_radians * 0.5) as f32).tan();
    if !tan_half.is_finite() || tan_half <= 0.0 || !camera.requested_error_px.is_finite() {
        return Err("the intent camera field of view is not usable".into());
    }
    let mut max_px = 0.0f32;
    let mut samples = 0u32;
    for j in 0..n {
        for i in 0..n {
            for which in [0u8, 1] {
                let corners = triangle_corners(translation, n, i, j, which)?;
                let centroid = parameter_centroid(n, i, j, which)?;
                let point = place(translation, centroid.0, centroid.1)?;
                let depth = dot(sub(point, camera.eye), forward);
                if !depth.is_finite() {
                    return Err("patch sample depth was not finite".into());
                }
                if depth < 0.05 {
                    return Err(format!("patch sample depth {depth} m is under 0.05 m"));
                }
                let Some(deviation) = plane_distance(point, corners[0], corners[1], corners[2]) else {
                    max_px = f32::MAX;
                    samples = samples.saturating_add(1);
                    continue;
                };
                if !deviation.is_finite() {
                    max_px = f32::MAX;
                    samples = samples.saturating_add(1);
                    continue;
                }
                let px = projected_detail_px(deviation as f32, depth as f32, camera.viewport_height, tan_half);
                if !px.is_finite() {
                    max_px = f32::MAX;
                } else {
                    max_px = max_px.max(px);
                }
                samples = samples.saturating_add(1);
            }
        }
    }
    let expected = n.checked_mul(n).and_then(|value| value.checked_mul(2)).ok_or("predicate sample count overflowed")?;
    if samples != expected {
        return Err(format!("predicate sampled {samples} centroids, not {expected}"));
    }
    Ok(max_px)
}

fn choose_n(translation: [f64; 3], camera: &IntentObservationCamera) -> Result<Chosen, String> {
    let mut n = 1u32;
    let mut evaluations = 0u32;
    let mut coarser_failed = false;
    let mut coarser_px = 0.0f32;
    loop {
        if n > INTENT_N_CAP {
            return Err(format!("patch 1002 stayed above {:.3} px through n = {INTENT_N_CAP}", camera.requested_error_px));
        }
        let scalar = predicate_max_px(translation, n, camera)?;
        evaluations = evaluations.saturating_add(1);
        if scalar <= camera.requested_error_px {
            return Ok(Chosen { n, evaluations, coarser_failed, coarser_px });
        }
        coarser_failed = true;
        coarser_px = scalar;
        n = n.saturating_add(1);
    }
}

fn emit_grid(translation: [f64; 3], n: u32, emitted: &mut Vec<Emitted>) -> Result<u32, String> {
    let mut count = 0u32;
    for j in 0..n {
        for i in 0..n {
            for which in [0u8, 1] {
                let corners = triangle_corners(translation, n, i, j, which)?;
                emitted.push(Emitted {
                    surface_id: PATCH_B,
                    corners,
                    trace: IntentTraceRecord { surface_id: PATCH_B, cell_i: i, cell_j: j, triangle_in_cell: which },
                });
                count = count.saturating_add(1);
            }
        }
    }
    Ok(count)
}

struct PackedIntent {
    mesh: Mesh,
    surfaces: Vec<PackedSurface>,
    constructed_triangles: u32,
    packed_triangles: u32,
    packed_vertices: u32,
    packed_indices: u32,
    packed_vertex_bytes: u64,
    packed_index_bytes: u64,
    expected_gpu_bytes: u64,
    index_format: MeshIndexFormat,
    position_hash: u64,
    index_hash: u64,
    degenerate_normals: u32,
    discarded_during_pack: u32,
}

fn pack_intent(emitted: &[Emitted]) -> Result<PackedIntent, String> {
    if emitted.is_empty() {
        return Err("the intent product constructed no triangles to pack".into());
    }
    if emitted.iter().any(|item| item.surface_id == PATCH_A) {
        return Err("pack received omitted patch 1001".into());
    }
    let packed_triangles = u32::try_from(emitted.len()).map_err(|_| "too many triangles")?;
    let packed_vertices = packed_triangles.checked_mul(3).ok_or("vertex count overflowed")?;
    let packed_indices = packed_vertices;
    let index_format = if packed_vertices > u32::from(u16::MAX) { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 };
    let index_stride = u64::from(index_format.byte_size());
    let packed_vertex_bytes = u64::from(packed_vertices) * u64::from(INTENT_VERTEX_STRIDE);
    let packed_index_bytes = u64::from(packed_indices) * index_stride;
    let mut bytes = Vec::with_capacity(packed_vertex_bytes as usize);
    let mut index_bytes = Vec::with_capacity(packed_index_bytes as usize);
    let mut position_bytes = Vec::with_capacity((packed_vertices as usize) * 12);
    let mut per_surface: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut degenerate_normals = 0u32;
    for (triangle_index, item) in emitted.iter().enumerate() {
        let packed = [f32_corner(item.corners[0])?, f32_corner(item.corners[1])?, f32_corner(item.corners[2])?];
        let (normal, degenerate) = packed_normal(packed[0], packed[1], packed[2]);
        if degenerate {
            degenerate_normals = degenerate_normals.saturating_add(1);
        }
        let mut local = Vec::new();
        for corner in packed {
            for value in corner {
                let encoded = value.to_le_bytes();
                position_bytes.extend_from_slice(&encoded);
                local.extend_from_slice(&encoded);
            }
            push_vertex(&mut bytes, corner, normal);
        }
        match per_surface.iter_mut().find(|(id, _)| *id == item.surface_id) {
            Some((_, buffer)) => buffer.extend_from_slice(&local),
            None => per_surface.push((item.surface_id, local)),
        }
        let base = (triangle_index as u32).saturating_mul(3);
        for offset in 0..3u32 {
            push_index(&mut index_bytes, index_format, base + offset);
        }
    }
    if bytes.len() as u64 != packed_vertex_bytes || index_bytes.len() as u64 != packed_index_bytes {
        return Err("packed byte lengths drifted from the triangle count".into());
    }
    let mesh = create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: INTENT_VERTEX_STRIDE,
            attributes: vec![
                MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
            ],
            bytes,
        }],
        index_format,
        index_bytes: index_bytes.clone(),
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: packed_indices,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .map_err(|error: MeshError| error.to_string())?;
    let mut surfaces = Vec::new();
    for (surface_id, buffer) in per_surface {
        let triangles = emitted.iter().filter(|item| item.surface_id == surface_id).count() as u32;
        let vertices = triangles.saturating_mul(3);
        let vertex_bytes = u64::from(vertices) * u64::from(INTENT_VERTEX_STRIDE);
        let index_bytes = u64::from(vertices) * index_stride;
        surfaces.push(PackedSurface {
            surface_id,
            packed_triangles: triangles,
            packed_vertices: vertices,
            index_count: vertices,
            vertex_bytes,
            index_bytes,
            gpu_bytes: vertex_bytes + index_bytes,
            position_hash: fnv64(&buffer),
        });
    }
    Ok(PackedIntent {
        mesh,
        surfaces,
        constructed_triangles: packed_triangles,
        packed_triangles,
        packed_vertices,
        packed_indices,
        packed_vertex_bytes,
        packed_index_bytes,
        expected_gpu_bytes: packed_vertex_bytes + packed_index_bytes,
        index_format,
        position_hash: fnv64(&position_bytes),
        index_hash: fnv64(&index_bytes),
        degenerate_normals,
        discarded_during_pack: 0,
    })
}

fn apply_packed(account: &mut IntentPatchAccount, surfaces: &[PackedSurface]) -> Result<(), String> {
    if let Some(bytes) = surfaces.iter().find(|item| item.surface_id == account.surface_id) {
        account.packed_triangles = bytes.packed_triangles;
        account.packed_vertices = bytes.packed_vertices;
        account.index_count = bytes.index_count;
        account.vertex_bytes = bytes.vertex_bytes;
        account.index_bytes = bytes.index_bytes;
        account.gpu_bytes = bytes.gpu_bytes;
        account.position_hash = bytes.position_hash;
    }
    Ok(())
}

fn zero_patch(surface_id: u32) -> IntentPatchAccount {
    IntentPatchAccount {
        surface_id,
        admission: IntentAdmission::AdmitUncertain,
        n: 0,
        triangles_constructed: 0,
        candidate_grids_emitted: 0,
        candidate_grids_discarded: 0,
        resolution_predicate_evaluations: 0,
        coarser_failed: false,
        coarser_px: 0.0,
        measured_error_px: 0.0,
        packed_triangles: 0,
        packed_vertices: 0,
        index_count: 0,
        vertex_bytes: 0,
        index_bytes: 0,
        gpu_bytes: 0,
        position_hash: 0,
    }
}

fn cache_key(hash: u64, revision: u64, camera: &IntentObservationCamera) -> String {
    let milli = (f64::from(camera.requested_error_px) * 1000.0).round() as i32;
    format!(
        "intent4-h{hash:016x}-r{revision}-e{:.4}-{:.4}-{:.4}-px{milli}-w{:.0}-h{:.0}-a{INTENT_OBSERVATION_ALGORITHM_VERSION}",
        camera.eye[0], camera.eye[1], camera.eye[2], camera.viewport_width, camera.viewport_height
    )
}

fn patch_corners(translation: [f64; 3]) -> Result<[[f64; 3]; 8], String> {
    let x_pos = place(translation, 0.5, 0.0)?;
    let x_neg = place(translation, -0.5, 0.0)?;
    let y_pos = place(translation, 0.0, 0.5)?;
    let y_neg = place(translation, 0.0, -0.5)?;
    let z_max_point = place(translation, 0.0, 0.0)?;
    let z00 = place(translation, -0.5, -0.5)?;
    let z01 = place(translation, -0.5, 0.5)?;
    let z10 = place(translation, 0.5, -0.5)?;
    let z11 = place(translation, 0.5, 0.5)?;
    let xmin = x_neg[0].min(x_pos[0]);
    let xmax = x_neg[0].max(x_pos[0]);
    let ymin = y_neg[1].min(y_pos[1]);
    let ymax = y_neg[1].max(y_pos[1]);
    let zmin = z00[2].min(z01[2]).min(z10[2]).min(z11[2]);
    let zmax = z_max_point[2];
    let mut corners = [[0.0; 3]; 8];
    let mut index = 0;
    for z in [zmin, zmax] {
        for y in [ymin, ymax] {
            for x in [xmin, xmax] {
                corners[index] = [x, y, z];
                index += 1;
            }
        }
    }
    Ok(corners)
}

fn parameter(n: u32, index: u32) -> f64 {
    -0.5 + f64::from(index) / f64::from(n)
}

fn parameter_centroid(n: u32, i: u32, j: u32, which: u8) -> Result<(f64, f64), String> {
    let uv = triangle_parameters(n, i, j, which)?;
    Ok(((uv[0].0 + uv[1].0 + uv[2].0) / 3.0, (uv[0].1 + uv[1].1 + uv[2].1) / 3.0))
}

fn triangle_parameters(n: u32, i: u32, j: u32, which: u8) -> Result<[(f64, f64); 3], String> {
    if i >= n || j >= n {
        return Err("cell is outside the grid".into());
    }
    let (a, b, c) = match which {
        0 => ((i, j), (i + 1, j), (i + 1, j + 1)),
        1 => ((i, j), (i + 1, j + 1), (i, j + 1)),
        _ => return Err("a cell has only two triangles".into()),
    };
    Ok([(parameter(n, a.0), parameter(n, a.1)), (parameter(n, b.0), parameter(n, b.1)), (parameter(n, c.0), parameter(n, c.1))])
}

fn triangle_corners(translation: [f64; 3], n: u32, i: u32, j: u32, which: u8) -> Result<[[f64; 3]; 3], String> {
    let uv = triangle_parameters(n, i, j, which)?;
    Ok([place(translation, uv[0].0, uv[0].1)?, place(translation, uv[1].0, uv[1].1)?, place(translation, uv[2].0, uv[2].1)?])
}

fn place(translation: [f64; 3], u: f64, v: f64) -> Result<[f64; 3], String> {
    let radius = (u * u + v * v + 1.0).sqrt();
    if !radius.is_finite() || radius <= 0.0 {
        return Err("spherical square radius was not finite".into());
    }
    let point = [u / radius + translation[0], v / radius + translation[1], 1.0 / radius + translation[2]];
    if point.iter().any(|axis| !axis.is_finite()) {
        return Err("spherical square point was not finite".into());
    }
    Ok(point)
}

fn plane_distance(point: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    let normal = cross(sub(b, a), sub(c, a));
    let length = (dot(normal, normal)).sqrt();
    if !length.is_finite() || length < 1.0e-18 {
        return None;
    }
    Some(dot(sub(point, a), normal).abs() / length)
}

fn f32_corner(corner: [f64; 3]) -> Result<[f32; 3], String> {
    let packed = [corner[0] as f32, corner[1] as f32, corner[2] as f32];
    if packed.iter().any(|axis| !axis.is_finite()) {
        return Err("a packed corner was not finite".into());
    }
    Ok(packed)
}

fn packed_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> ([f32; 3], bool) {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
    let length_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    if !length_sq.is_finite() || length_sq < 1.0e-16 {
        return ([0.0, 1.0, 0.0], true);
    }
    let inverse = 1.0 / length_sq.sqrt();
    ([normal[0] * inverse, normal[1] * inverse, normal[2] * inverse], false)
}

fn push_vertex(bytes: &mut Vec<u8>, position: [f32; 3], normal: [f32; 3]) {
    for value in position.into_iter().chain([1.0, 1.0, 1.0]).chain([0.0, 0.0]).chain(normal).chain([1.0, 0.0, 0.0, 1.0]) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_index(bytes: &mut Vec<u8>, format: MeshIndexFormat, index: u32) {
    match format {
        MeshIndexFormat::Uint16 => bytes.extend_from_slice(&(index as u16).to_le_bytes()),
        MeshIndexFormat::Uint32 => bytes.extend_from_slice(&index.to_le_bytes()),
    }
}

fn index_format_name(format: MeshIndexFormat) -> &'static str {
    match format {
        MeshIndexFormat::Uint16 => "Uint16",
        MeshIndexFormat::Uint32 => "Uint32",
    }
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn yes(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

fn id_list(ids: &[u32]) -> String {
    if ids.is_empty() {
        "NONE".to_string()
    } else {
        ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
    }
}

fn camera_basis(forward: [f64; 3], up_hint: [f64; 3]) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    let forward = normalize(forward)?;
    let hint = match normalize(up_hint) {
        Some(hint) if dot(hint, forward).abs() <= 0.999 => hint,
        _ => {
            if forward[1].abs() < 0.9 {
                [0.0, 1.0, 0.0]
            } else {
                [0.0, 0.0, 1.0]
            }
        }
    };
    let right = normalize(cross(forward, hint))?;
    let up = cross(right, forward);
    Some((right, up, forward))
}

fn normalize(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = (dot(value, value)).sqrt();
    if !length.is_finite() || length < 1.0e-12 {
        None
    } else {
        Some(scale(value, 1.0 / length))
    }
}

fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[1] * right[2] - left[2] * right[1], left[2] * right[0] - left[0] * right[2], left[0] * right[1] - left[1] * right[0]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::ComponentRecord;
    use crate::project::load_level_file;
    use crate::semantic_shadow::{evaluate_saved_semantic_shadow, intent_authority_eligibility, IntentAuthorityEligibility};
    use crate::topology::ElementKind;

    const LEVEL: &str = r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof\Content\Levels\Main.jarviglevel";
    const FROZEN_FAR_POSITION: u64 = 0xd483_cd5f_6133_8a61;
    const FROZEN_CLOSE_POSITION: u64 = 0x6e2e_84eb_1989_2f71;

    #[test]
    fn intent_observation_realizes_the_tape_without_presenting() {
        println!("Frame: NOT PRESENTED");
        assert_eq!(INTENT_OBSERVATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::observation::OBSERVATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::view_realization::REALIZATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::direct_realization::DIRECT_REALIZATION_ALGORITHM_VERSION, 1);
        assert_eq!(INTENT_VERTEX_STRIDE, crate::observation::OBSERVATION_VERTEX_STRIDE);
        let bytes = std::fs::read(LEVEL).expect("Intent Proof level");
        assert_eq!(bytes.len(), 26849, "Main.jarviglevel bytes");
        let document = load_level_file(std::path::Path::new(LEVEL)).expect("level parses");
        let record = document
            .entities
            .iter()
            .find(|entity| entity.name == "Intent Solid")
            .and_then(|entity| {
                entity.components.iter().find_map(|component| match component {
                    ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                    _ => None,
                })
            })
            .expect("Intent Solid");
        assert!(record.body.is_none());
        assert_eq!(record.intent.len(), 65);
        assert_eq!(record.history.len(), 24);
        assert_eq!(intent_authority_eligibility(&record), IntentAuthorityEligibility::Eligible);
        let saved = record.clone();
        let hash = authority_hash(&record, PATCH_B_TRANSLATION).expect("authority");
        assert_ne!(hash, FROZEN_3C_AUTHORITY);
        let far_camera = far_intent_camera();
        let close_camera = close_intent_camera();
        let far = realize_intent_observation(&record, &far_camera, 1, PATCH_B_TRANSLATION).expect("far");
        let close = realize_intent_observation(&record, &close_camera, 1, PATCH_B_TRANSLATION).expect("close");
        println!("{}", format_intent_product("Observation Far", &far));
        println!("{}", format_intent_product("Observation Close", &close));
        assert_eq!(far.authority_hash, hash);
        assert_eq!(close.authority_hash, hash);
        assert_eq!(far.revision, 1);
        assert!(far.cache_key.starts_with("intent4-"));
        assert!(!far.cache_key.starts_with("direct-"));
        assert_patch_omitted(&far.patch_a);
        assert_patch_omitted(&close.patch_a);
        assert_frozen_patch(&far.patch_b, 6, 72, 216, 432, 13392, "0.424494", "0.613221", 6);
        assert_frozen_patch(&close.patch_b, 14, 392, 1176, 2352, 72912, "0.462054", "0.536666", 14);
        assert!(close.patch_b.n > far.patch_b.n);
        assert_ne!(far.patch_b.position_hash, FROZEN_FAR_POSITION);
        assert_ne!(far.patch_b.position_hash, FROZEN_CLOSE_POSITION);
        assert_ne!(close.patch_b.position_hash, FROZEN_FAR_POSITION);
        assert_ne!(close.patch_b.position_hash, FROZEN_CLOSE_POSITION);
        assert!(!far.rejected_n_allocated_mesh);
        assert!(!far.trace.iter().any(|record| record.surface_id == PATCH_A));
        assert!(!close.trace.iter().any(|record| record.surface_id == PATCH_A));
        assert_consulted(&far);
        assert_consulted(&close);
        let names = evaluate_saved_semantic_shadow(&record);
        for account in &far.planar {
            let mut expected: Vec<String> = names
                .provenance
                .iter()
                .filter(|fact| fact.kind == ElementKind::Face && fact.concrete == account.face_id)
                .map(|fact| fact.identity.clone())
                .collect();
            expected.sort();
            expected.dedup();
            assert_eq!(account.identities, expected, "face {}", account.face_id);
            if account.admission.omitted() {
                assert_eq!(account.triangles_constructed, 0);
            } else {
                assert_eq!(account.triangles_constructed, account.loop_len - 2);
                assert_eq!(account.packed_triangles, account.triangles_constructed);
            }
        }
        println!("planar far admitted: {}", id_list(&far.admitted_planar_ids));
        println!("planar far omitted: {}", id_list(&far.omitted_planar_ids));
        println!("planar close admitted: {}", id_list(&close.admitted_planar_ids));
        println!("planar close omitted: {}", id_list(&close.omitted_planar_ids));
        if far.admitted_planar_ids == close.admitted_planar_ids {
            let dir = std::path::PathBuf::from(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof");
            let mut text = String::from("Frame: NOT PRESENTED\n");
            text.push_str(&format!("authority_hash: {hash:016x}\n"));
            text.push_str(&format!("planar far admitted: {}\n", id_list(&far.admitted_planar_ids)));
            text.push_str(&format!("planar close admitted: {}\n", id_list(&close.admitted_planar_ids)));
            text.push_str("Planar admitted sets equal: YES\n");
            text.push_str("Size edit: NOT RUN\n");
            text.push_str("Experiment 4: FAIL\n");
            text.push_str("Failure: the frozen camera pair admitted the same planar faces\n");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("REPORT-INTENT-4.txt"), text);
            panic!("Experiment 4: FAIL. The frozen camera pair admitted the same planar faces. The edit was not run.");
        }
        let copy = size_edit_copy(&record).expect("size copy");
        assert_eq!(copy.intent.len(), 66);
        assert_eq!(record.intent.len(), 65);
        assert_eq!(saved, record);
        assert!(copy.body.is_none());
        assert_eq!(copy.history.len(), 24);
        let edited_hash = authority_hash(&copy, PATCH_B_TRANSLATION).expect("edited authority");
        assert_ne!(edited_hash, hash);
        println!("edited size bits: {}", size_edit_bits(&record));
        let far_edited = realize_intent_observation(&copy, &far_camera, 2, PATCH_B_TRANSLATION).expect("edited far");
        let close_edited = realize_intent_observation(&copy, &close_camera, 2, PATCH_B_TRANSLATION).expect("edited close");
        assert_eq!(far_edited.revision, 2);
        assert_ne!(far_edited.cache_key, far.cache_key);
        assert_frozen_patch(&far_edited.patch_b, 6, 72, 216, 432, 13392, "0.424494", "0.613221", 6);
        assert_frozen_patch(&close_edited.patch_b, 14, 392, 1176, 2352, 72912, "0.462054", "0.536666", 14);
        assert!(planar_geometry_changed(&far, &far_edited) || planar_geometry_changed(&close, &close_edited), "the size edit left every planar position hash unchanged");
        let shifted = realize_intent_observation(&record, &far_camera, 1, [4.25, 0.0, 0.0]).expect("shifted patch");
        assert_ne!(shifted.patch_b.position_hash, far.patch_b.position_hash);
        assert_ne!(shifted.patch_b.position_hash, FROZEN_FAR_POSITION);
        assert_ne!(shifted.patch_b.position_hash, FROZEN_CLOSE_POSITION);
        assert_ne!(shifted.authority_hash, hash);
        println!("translation-only patch grid missed the previous grid: YES");
        println!("Frame: NOT PRESENTED");
    }

    fn assert_patch_omitted(account: &IntentPatchAccount) {
        assert_eq!(account.surface_id, PATCH_A);
        assert_eq!(account.admission, IntentAdmission::Omit);
        assert_eq!(account.n, 0);
        assert_eq!(account.triangles_constructed, 0);
        assert_eq!(account.gpu_bytes, 0);
        assert_eq!(account.candidate_grids_emitted, 0);
        assert_eq!(account.resolution_predicate_evaluations, 0);
    }

    fn assert_frozen_patch(account: &IntentPatchAccount, n: u32, triangles: u32, vertices: u32, index_bytes: u64, gpu_bytes: u64, error: &str, coarser: &str, evaluations: u32) {
        assert_eq!(account.surface_id, PATCH_B);
        assert_eq!(account.admission, IntentAdmission::AdmitInView);
        assert_eq!(account.n, n);
        assert_eq!(account.triangles_constructed, triangles);
        assert_eq!(account.packed_triangles, triangles);
        assert_eq!(account.packed_vertices, vertices);
        assert_eq!(account.vertex_bytes, u64::from(vertices) * 60);
        assert_eq!(account.index_bytes, index_bytes);
        assert_eq!(account.gpu_bytes, gpu_bytes);
        assert_eq!(account.candidate_grids_emitted, 1);
        assert_eq!(account.candidate_grids_discarded, 0);
        assert_eq!(account.resolution_predicate_evaluations, evaluations);
        assert!(account.coarser_failed);
        assert_eq!(format!("{:.6}", account.measured_error_px), error);
        assert_eq!(format!("{:.6}", account.coarser_px), coarser);
    }

    fn assert_consulted(product: &IntentObservationProduct) {
        assert!(!product.object_mesh_consulted);
        assert!(!product.record_body_consulted);
        assert!(!product.authoritative_triangles_consulted);
        assert!(!product.build_realization_consulted);
        assert!(!product.realize_direct_consulted);
        assert!(!product.analytic_specimen_consulted);
        assert!(!product.einstein_consulted);
        assert_eq!(product.meshlets_constructed, 0);
        assert!(!product.vertices_welded);
        assert_eq!(product.discarded_after_construction, 0);
        assert_eq!(product.discarded_during_pack, 0);
        assert_eq!(product.constructed_triangles, product.packed_triangles);
        assert_eq!(product.packed_triangles, product.uploaded_triangles);
    }
}
