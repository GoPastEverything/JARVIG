//! Experiment 6. A saved modeling tape is the observation authority.
//!
//! The tape is the existing eligible Intent Solid history. This module does not
//! call `realize_intent_observation`, `authority_hash`, `realize_direct`,
//! `authoritative_triangles`, `build_realization`, `frozen_observation_cameras`,
//! or `AnalyticObject::specimen`. The field of view is an argument.

use std::path::{Path, PathBuf};

use crate::mesh::{
    create_mesh, Mesh, MeshDesc, MeshError, MeshIndexFormat, MeshTopology, MeshVertexAttribute, MeshVertexFormat, SubmeshDesc, VertexStreamDesc,
};
use crate::parametric::{BlockRecord, IntentEntry, IntentPayload};
use crate::semantic_shadow::{intent_authority_candidate, IntentAuthorityCandidate};
use crate::topology::SolidBody;
use crate::view_realization::authoritative_body_hash;

/// Bump only if this tape constructor changes. The 3A, 3B, 3C, and 4 versions stay where they are.
pub const AUTHORED_INTENT_ALGORITHM_VERSION: u32 = 1;
pub const AUTHORED_VERTEX_STRIDE: u32 = 60;
pub const AUTHORED_VIEW_HEIGHT: f32 = 567.0;
pub const AUTHORED_A_WIDTH: f32 = 160.0;
pub const AUTHORED_B_WIDTH: f32 = 960.0;
pub const AUTHORED_A_EYE: [f64; 3] = [0.0, 0.0, 3.625];
pub const AUTHORED_B_EYE: [f64; 3] = [2.7801, -0.6250, 3.0850];
pub const EDITED_SIZE_BITS: &str = "401699999999998f 4004cccccccccccd 400a000000000000";
pub const AUTHORED_MAIN_LEVEL_BYTES: u64 = 26849;
const SOURCE_SOLID_UUID: &str = "ef5a0f8a-0eec-49db-b71e-40807a1e4f00";
const PROJECT_UUID: &str = "d6d6d6d6-d6d6-46d6-86d6-d6d6d6d6d6d6";
const LEVEL_UUID: &str = "c6c6c6c6-c6c6-46c6-86c6-c6c6c6c6c6c6";
const SOLID_UUID: &str = "b6b6b6b6-b6b6-46b6-86b6-b6b6b6b6b6b6";
const FROZEN_AUTHORITIES: [u64; 6] = [
    0xac80_f43b_acf3_c609,
    0xfbdd_144e_5cd4_b21f,
    0xd5f4_177d_cd12_550c,
    0x2aa2_4333_7d78_a768,
    0x292a_859c_e57a_494c,
    0x6cee_ea84_0b7b_def2,
];

/// Object-local camera. The vertical field of view is supplied by the caller.
#[derive(Clone, Copy, Debug)]
pub struct AuthoredCamera {
    pub eye: [f64; 3],
    pub forward: [f64; 3],
    pub up: [f64; 3],
    pub vertical_fov_radians: f64,
    pub near_m: f64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub requested_error_px: f32,
}

/// Why a loop was kept. Only `Omit` skips construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoredAdmission {
    Omit,
    AdmitInView,
    AdmitBehindCamera,
    AdmitCrossesNear,
    AdmitUncertain,
}

impl AuthoredAdmission {
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

/// One planar face. The fan is the loop. There is no resolution ladder.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredFaceAccount {
    pub face_id: u32,
    pub admission: AuthoredAdmission,
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

/// One emitted fan triangle. An omitted face is absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthoredTrace {
    pub face_id: u32,
    pub fan_index: u32,
}

/// One observation mesh synthesized from the tape.
#[derive(Clone, Debug)]
pub struct AuthoredProduct {
    pub authority_hash: u64,
    pub revision: u64,
    pub cache_key: String,
    pub faces: Vec<AuthoredFaceAccount>,
    pub admitted_ids: Vec<u32>,
    pub omitted_ids: Vec<u32>,
    pub trace: Vec<AuthoredTrace>,
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
    pub index_format: MeshIndexFormat,
    pub position_hash: u64,
    pub index_hash: u64,
    pub local_body_hash: u64,
    pub meshlets_constructed: u32,
    pub vertices_welded: bool,
    pub degenerate_normals: u32,
    pub object_mesh_consulted: bool,
    pub record_body_consulted: bool,
    pub authoritative_triangles_consulted: bool,
    pub build_realization_consulted: bool,
    pub realize_intent_observation_consulted: bool,
    pub realize_direct_consulted: bool,
    pub analytic_specimen_consulted: bool,
    pub einstein_consulted: bool,
}

struct Emitted {
    face_id: u32,
    corners: [[f64; 3]; 3],
    fan_index: u32,
}

struct PackedFace {
    face_id: u32,
    packed_triangles: u32,
    packed_vertices: u32,
    vertex_bytes: u64,
    index_bytes: u64,
    gpu_bytes: u64,
}

pub fn authored_proof_dir() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\Jeramiah\AppData\Local"));
    root.join("Temp").join("jarvig-authored-intent")
}

/// Post-cache-contract rerun. The measured FAIL project stays in [`authored_proof_dir`].
pub fn authored_rerun_dir() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\Jeramiah\AppData\Local"));
    root.join("Temp").join("jarvig-authored-intent-rerun")
}

pub fn authored_main_level() -> PathBuf {
    PathBuf::from(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof\Content\Levels\Main.jarviglevel")
}

/// Observation A. `vertical_fov_radians` is the caller's extracted front-camera field of view.
pub fn authored_camera_a(vertical_fov_radians: f64) -> AuthoredCamera {
    authored_camera(AUTHORED_A_EYE, AUTHORED_A_WIDTH, vertical_fov_radians)
}

/// Observation B. The eye is the written Experiment 3A eye. It is not searched from a body.
pub fn authored_camera_b(vertical_fov_radians: f64) -> AuthoredCamera {
    authored_camera(AUTHORED_B_EYE, AUTHORED_B_WIDTH, vertical_fov_radians)
}

fn authored_camera(eye: [f64; 3], width: f32, vertical_fov_radians: f64) -> AuthoredCamera {
    AuthoredCamera {
        eye,
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_radians,
        near_m: 0.1,
        viewport_width: width,
        viewport_height: AUTHORED_VIEW_HEIGHT,
        requested_error_px: 0.5,
    }
}

/// FNV-1a 64 of the tape text. The text has no trailer, chart, vertex, index, or entity uuid.
pub fn authored_authority_hash(record: &BlockRecord) -> Result<u64, String> {
    let hash = fnv64(authority_text(record)?.as_bytes());
    if FROZEN_AUTHORITIES.contains(&hash) {
        return Err(format!("the tape authority hashed to a frozen experiment hash {hash:016x}"));
    }
    Ok(hash)
}

/// Bits of the last Size payload. This is the saved edit, not a second increment of `size_m`.
pub fn authored_size_bits(record: &BlockRecord) -> Result<String, String> {
    let Some(entry) = record.intent.last() else {
        return Err("the tape has no last operation".into());
    };
    let IntentPayload::Size { size_m } = entry.payload else {
        return Err("the saved edit did not end in a Size operation".into());
    };
    Ok(format!("{:016x} {:016x} {:016x}", size_m[0].to_bits(), size_m[1].to_bits(), size_m[2].to_bits()))
}

/// True when at least one tape-derived planar position hash or face id differs.
pub fn authored_planar_changed(before: &AuthoredProduct, after: &AuthoredProduct) -> bool {
    let left: Vec<(u32, u64)> = before.faces.iter().map(|account| (account.face_id, account.position_hash)).collect();
    let right: Vec<(u32, u64)> = after.faces.iter().map(|account| (account.face_id, account.position_hash)).collect();
    left != right
}

/// Face loops from the tape, then one fan for each admitted face. An empty product is an error.
pub fn realize_authored_observation(record: &BlockRecord, camera: &AuthoredCamera, revision: u64) -> Result<AuthoredProduct, String> {
    if revision == 0 {
        return Err("the authority revision starts at 1".into());
    }
    if record.body.is_some() {
        return Err("record.body is present. Experiment 6 does not read it".into());
    }
    let hash = authored_authority_hash(record)?;
    let body = match intent_authority_candidate(record) {
        IntentAuthorityCandidate::Reconstructable(body) => body,
        IntentAuthorityCandidate::Refused(reason) => return Err(format!("the intent tape did not reconstruct: {reason}")),
    };
    let local_body_hash = authoritative_body_hash(&body);
    let mut faces = Vec::new();
    let mut emitted = Vec::new();
    let mut admitted = Vec::new();
    let mut omitted = Vec::new();
    let mut face_ids: Vec<u32> = body.faces.iter().map(|face| face.id).collect();
    face_ids.sort_unstable();
    for face_id in face_ids {
        let account = planar_face(&body, face_id, camera, &mut emitted)?;
        if account.admission.omitted() {
            omitted.push(face_id);
        } else {
            admitted.push(face_id);
        }
        faces.push(account);
    }
    if emitted.is_empty() {
        return Err(format!(
            "the observation constructed no triangles. admitted {} omitted {}",
            id_list(&admitted),
            id_list(&omitted)
        ));
    }
    let packed = pack_authored(&emitted)?;
    let mut face_gpu = 0u64;
    for account in &mut faces {
        if let Some(bytes) = packed.faces.iter().find(|item| item.face_id == account.face_id) {
            account.packed_triangles = bytes.packed_triangles;
            account.packed_vertices = bytes.packed_vertices;
            account.vertex_bytes = bytes.vertex_bytes;
            account.index_bytes = bytes.index_bytes;
            account.gpu_bytes = bytes.gpu_bytes;
        }
        if account.admission.omitted() {
            if account.triangles_constructed != 0 || account.gpu_bytes != 0 || account.packed_vertices != 0 {
                return Err(format!("omitted face {} contributed triangles", account.face_id));
            }
        } else {
            let fan = account.loop_len.checked_sub(2).ok_or("a face fan underflowed")?;
            if account.triangles_constructed != fan || account.packed_triangles != fan {
                return Err(format!("face {} was not fanned once", account.face_id));
            }
        }
        face_gpu = face_gpu.saturating_add(account.gpu_bytes);
    }
    if face_gpu != packed.expected_gpu_bytes {
        return Err("the sum of the per-face GPU bytes is not the packed total".into());
    }
    if packed.packed_vertices != packed.packed_triangles.saturating_mul(3) {
        return Err("the pack welded vertices".into());
    }
    let trace: Vec<AuthoredTrace> = emitted.iter().map(|item| AuthoredTrace { face_id: item.face_id, fan_index: item.fan_index }).collect();
    if omitted.iter().any(|id| trace.iter().any(|record| record.face_id == *id)) {
        return Err("an omitted face is in the construction trace".into());
    }
    drop(body);
    Ok(AuthoredProduct {
        authority_hash: hash,
        revision,
        cache_key: cache_key(hash, revision, camera),
        faces,
        admitted_ids: admitted,
        omitted_ids: omitted,
        trace,
        mesh: packed.mesh,
        constructed_triangles: packed.constructed_triangles,
        packed_triangles: packed.packed_triangles,
        uploaded_triangles: packed.packed_triangles,
        discarded_after_construction: 0,
        discarded_during_pack: packed.discarded_during_pack,
        discarded_after_upload: 0,
        packed_vertices: packed.packed_vertices,
        packed_indices: packed.packed_indices,
        packed_vertex_bytes: packed.packed_vertex_bytes,
        packed_index_bytes: packed.packed_index_bytes,
        expected_gpu_bytes: packed.expected_gpu_bytes,
        index_format: packed.index_format,
        position_hash: packed.position_hash,
        index_hash: packed.index_hash,
        local_body_hash,
        meshlets_constructed: 0,
        vertices_welded: false,
        degenerate_normals: packed.degenerate_normals,
        object_mesh_consulted: false,
        record_body_consulted: false,
        authoritative_triangles_consulted: false,
        build_realization_consulted: false,
        realize_intent_observation_consulted: false,
        realize_direct_consulted: false,
        analytic_specimen_consulted: false,
        einstein_consulted: false,
    })
}

pub fn format_authored_product(label: &str, product: &AuthoredProduct) -> String {
    let mut out = String::new();
    out.push_str(&format!("{label} authority_hash: {:016x}\n", product.authority_hash));
    out.push_str(&format!("{label} authority_revision: {}\n", product.revision));
    out.push_str(&format!("{label} cache_key: {}\n", product.cache_key));
    out.push_str(&format!("{label} admitted_face_ids: {}\n", id_list(&product.admitted_ids)));
    out.push_str(&format!("{label} omitted_face_ids: {}\n", id_list(&product.omitted_ids)));
    out.push_str(&format!("{label} local evaluation body hash (not the authority): {:016x}\n", product.local_body_hash));
    for account in &product.faces {
        out.push_str(&format!(
            "{label} face {} admission {} loop {} triangles {} gpu_bytes {} position_hash {:016x}\n",
            account.face_id,
            account.admission.name(),
            account.loop_len,
            account.triangles_constructed,
            account.gpu_bytes,
            account.position_hash
        ));
    }
    out.push_str(&format!("{label} triangles_directly_constructed: {}\n", product.constructed_triangles));
    out.push_str(&format!("{label} packed_triangles: {}\n", product.packed_triangles));
    out.push_str(&format!("{label} uploaded_triangles: {}\n", product.uploaded_triangles));
    out.push_str(&format!("{label} packed_vertices: {}\n", product.packed_vertices));
    out.push_str(&format!("{label} expected_gpu_bytes: {}\n", product.expected_gpu_bytes));
    out.push_str(&format!("{label} position_hash: {:016x}\n", product.position_hash));
    out.push_str(&format!("{label} index_hash: {:016x}\n", product.index_hash));
    out.push_str(&format!("{label} index_format: {}\n", index_format_name(product.index_format)));
    out.push_str(&format!("{label} triangles_discarded_after_construction: {}\n", product.discarded_after_construction));
    out.push_str(&format!("{label} triangles_dropped_during_pack: {}\n", product.discarded_during_pack));
    out.push_str(&format!("{label} triangles_discarded_after_upload: {}\n", product.discarded_after_upload));
    out.push_str(&format!("{label} vertices_welded: {}\n", yes(product.vertices_welded)));
    out.push_str(&format!("{label} meshlets_constructed: {}\n", product.meshlets_constructed));
    out.push_str(&format!("{label} object_mesh_consulted: {}\n", yes(product.object_mesh_consulted)));
    out.push_str(&format!("{label} record_body_consulted: {}\n", yes(product.record_body_consulted)));
    out.push_str(&format!("{label} authoritative_triangles_consulted: {}\n", yes(product.authoritative_triangles_consulted)));
    out.push_str(&format!("{label} build_realization_consulted: {}\n", yes(product.build_realization_consulted)));
    out.push_str(&format!("{label} realize_intent_observation_consulted: {}\n", yes(product.realize_intent_observation_consulted)));
    out.push_str(&format!("{label} realize_direct_consulted: {}\n", yes(product.realize_direct_consulted)));
    out.push_str(&format!("{label} analytic_specimen_consulted: {}\n", yes(product.analytic_specimen_consulted)));
    out.push_str(&format!("{label} einstein_consulted: {}\n", yes(product.einstein_consulted)));
    out.push_str(&format!("{label} construction_trace_records: {}\n", product.trace.len()));
    for record in &product.trace {
        out.push_str(&format!("{label} trace face {} fan {}\n", record.face_id, record.fan_index));
    }
    out
}

/// Copy the eligible tape and the legacy cube into a new project. `Main.jarviglevel` is only read.
pub fn write_authored_intent_project(directory: &Path) -> Result<PathBuf, String> {
    let main = authored_main_level();
    let main_bytes = std::fs::read(&main).map_err(|error| error.to_string())?;
    if main_bytes.len() as u64 != AUTHORED_MAIN_LEVEL_BYTES {
        return Err(format!("Main.jarviglevel is {} bytes", main_bytes.len()));
    }
    let source = crate::parse_level(std::str::from_utf8(&main_bytes).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
    let project_file = directory.join("AuthoredIntent.jarvigproject");
    let project = crate::ProjectDocument {
        format_version: crate::PROJECT_FORMAT_VERSION,
        project_uuid: crate::EntityId::parse(PROJECT_UUID).ok_or("authored project uuid")?,
        display_name: "Authored Intent".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        startup_level: "Content/Levels/Authored.jarviglevel".into(),
        content_directory: "Content".into(),
        saved_directory: "Saved".into(),
        config_directory: "Config".into(),
        intermediate_directory: "Intermediate".into(),
        settings: "Config/Project.jarvigsettings".into(),
    };
    crate::create_project_directories(&project_file, &project).map_err(|error| error.to_string())?;
    let backup = directory.join("Saved").join("Backup");
    crate::save_project_atomic(&project_file, &backup, &project).map_err(|error| error.to_string())?;
    let level = authored_level(&source)?;
    crate::save_level_atomic(&project.startup_level_path(&project_file).map_err(|error| error.to_string())?, &backup, &level).map_err(|error| error.to_string())?;
    let settings = directory.join(&project.settings);
    if let Some(parent) = settings.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(settings, "JARVIG project settings placeholder. This is not a level and not a GPU resource.\n").map_err(|error| error.to_string())?;
    let after = std::fs::read(&main).map_err(|error| error.to_string())?;
    if after != main_bytes {
        return Err("writing the proof project changed Main.jarviglevel".into());
    }
    Ok(project_file)
}

fn authored_level(source: &crate::LevelDocument) -> Result<crate::LevelDocument, String> {
    let settings = entity_named(source, "World Settings")?;
    let mut solid = entity_named(source, "Intent Solid")?;
    let legacy = entity_named(source, "Legacy Cube")?;
    if solid.uuid.to_string() != SOURCE_SOLID_UUID {
        return Err(format!("the source Intent Solid is {}, not {SOURCE_SOLID_UUID}", solid.uuid));
    }
    let block = block_in(&solid)?;
    if block.body.is_some() || block.intent.len() != 65 || block.history.len() != 24 {
        return Err(format!(
            "the source tape is intent {} history {} body {}",
            block.intent.len(),
            block.history.len(),
            block.body.is_some()
        ));
    }
    if crate::intent_authority_diagnostic(block) != "Intent Authority: ELIGIBLE" {
        return Err(crate::intent_authority_diagnostic(block));
    }
    if block_in(&legacy)?.body.is_none() {
        return Err("the source legacy cube has no body".into());
    }
    solid.uuid = crate::EntityId::parse(SOLID_UUID).ok_or("solid uuid")?;
    if solid.uuid.to_string() == SOURCE_SOLID_UUID {
        return Err("the proof uuid reused the Intent Proof specimen".into());
    }
    Ok(crate::LevelDocument {
        format_version: crate::LEVEL_BLOCK_VERSION,
        level_uuid: crate::EntityId::parse(LEVEL_UUID).ok_or("level uuid")?,
        name: "Authored Intent".into(),
        world_settings: source.world_settings.clone(),
        organization: crate::SceneOrganization::default(),
        entities: vec![settings, solid, legacy],
    })
}

fn entity_named(document: &crate::LevelDocument, name: &str) -> Result<crate::EntityRecord, String> {
    document.entities.iter().find(|entity| entity.name == name).cloned().ok_or_else(|| format!("{name} is missing from the source level"))
}

fn block_in(entity: &crate::EntityRecord) -> Result<&BlockRecord, String> {
    entity
        .components
        .iter()
        .find_map(|component| match component {
            crate::ComponentRecord::ParametricBlock(block) => Some(block),
            _ => None,
        })
        .ok_or_else(|| format!("{} has no parametric block", entity.name))
}

fn authority_text(record: &BlockRecord) -> Result<String, String> {
    let seed = record.seed_size_m.ok_or("the eligible tape has no creation size")?;
    if seed.iter().any(|axis| !axis.is_finite()) || record.intent.is_empty() {
        return Err("the eligible tape is empty or its creation size is not finite".into());
    }
    if record.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::AnalyticSurface { .. })) {
        return Err("the authored tape contains analytic-surface".into());
    }
    let mut text = String::from("authored-intent-v1\n");
    text.push_str("authority: eligible intent tape\n");
    text.push_str(&format!("seed {} {} {}\n", bits(seed[0]), bits(seed[1]), bits(seed[2])));
    text.push_str(&format!("intent_count {}\n", record.intent.len()));
    for (index, entry) in record.intent.iter().enumerate() {
        text.push_str(&format!("entry {} {}|{}\n", index + 1, payload_text(&entry.payload)?, groups_text(entry)));
    }
    if text.contains("trailer") || text.contains("analytic-surface") || text.contains("p(u,v)") {
        return Err("the tape authority text included a trailer or a chart".into());
    }
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
        IntentPayload::AnalyticSurface { .. } => return Err("analytic-surface is not an authored-tape operand".into()),
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

fn planar_face(body: &SolidBody, face_id: u32, camera: &AuthoredCamera, emitted: &mut Vec<Emitted>) -> Result<AuthoredFaceAccount, String> {
    let loop_ = body.face_loop(face_id).ok_or_else(|| format!("scratch face {face_id} has no loop"))?;
    if loop_.len() < 3 || loop_.iter().enumerate().any(|(index, id)| loop_[..index].contains(id)) {
        return Err(format!("scratch face {face_id} is not one simple loop"));
    }
    let positions = body.face_positions(face_id).ok_or_else(|| format!("scratch face {face_id} is not one simple loop"))?;
    if positions.len() != loop_.len() {
        return Err(format!("scratch face {face_id} is not one simple loop"));
    }
    let admission = classify_loop(&positions, camera);
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
                face_id,
                corners: [positions[0], positions[index], positions[index + 1]],
                fan_index: index as u32,
            });
        }
        fan
    };
    Ok(AuthoredFaceAccount {
        face_id,
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

fn classify_loop(positions: &[[f64; 3]], camera: &AuthoredCamera) -> AuthoredAdmission {
    if positions.len() < 3 || positions.iter().any(|position| position.iter().any(|axis| !axis.is_finite())) {
        return AuthoredAdmission::AdmitUncertain;
    }
    let Some((right, up, forward)) = camera_basis(camera.forward, camera.up) else {
        return AuthoredAdmission::AdmitUncertain;
    };
    let width = f64::from(camera.viewport_width);
    let height = f64::from(camera.viewport_height);
    let error = f64::from(camera.requested_error_px);
    if !camera.near_m.is_finite() || camera.near_m <= 0.0 || !error.is_finite() || error < 0.0 || width < 1.0 || height < 1.0 {
        return AuthoredAdmission::AdmitUncertain;
    }
    let half = camera.vertical_fov_radians * 0.5;
    if !half.is_finite() || half <= 0.0 || half >= std::f64::consts::FRAC_PI_2 {
        return AuthoredAdmission::AdmitUncertain;
    }
    let tan_half_v = half.tan();
    let tan_half_h = tan_half_v * (width / height);
    if !tan_half_v.is_finite() || !tan_half_h.is_finite() || tan_half_v <= 0.0 || tan_half_h <= 0.0 {
        return AuthoredAdmission::AdmitUncertain;
    }
    let mut depths = Vec::with_capacity(positions.len());
    for position in positions {
        let relative = sub(*position, camera.eye);
        depths.push(dot(relative, forward));
    }
    if depths.iter().any(|depth| !depth.is_finite()) {
        return AuthoredAdmission::AdmitUncertain;
    }
    if depths.iter().any(|depth| *depth <= 0.0) {
        return AuthoredAdmission::AdmitBehindCamera;
    }
    if depths.iter().any(|depth| *depth <= camera.near_m) {
        return AuthoredAdmission::AdmitCrossesNear;
    }
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for (position, depth) in positions.iter().zip(depths.iter()) {
        let relative = sub(*position, camera.eye);
        let x_cam = dot(relative, right);
        let y_cam = dot(relative, up);
        let px = (x_cam / depth) / tan_half_h;
        let py = (y_cam / depth) / tan_half_v;
        if !px.is_finite() || !py.is_finite() {
            return AuthoredAdmission::AdmitUncertain;
        }
        let screen_x = (px * 0.5 + 0.5) * width;
        let screen_y = (py * 0.5 + 0.5) * height;
        min_x = min_x.min(screen_x);
        max_x = max_x.max(screen_x);
        min_y = min_y.min(screen_y);
        max_y = max_y.max(screen_y);
    }
    let misses = max_x < -error || min_x > width + error || max_y < -error || min_y > height + error;
    if misses { AuthoredAdmission::Omit } else { AuthoredAdmission::AdmitInView }
}

struct PackedAuthored {
    mesh: Mesh,
    faces: Vec<PackedFace>,
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

fn pack_authored(emitted: &[Emitted]) -> Result<PackedAuthored, String> {
    if emitted.is_empty() {
        return Err("the authored product constructed no triangles to pack".into());
    }
    let packed_triangles = u32::try_from(emitted.len()).map_err(|_| "too many triangles")?;
    let packed_vertices = packed_triangles.checked_mul(3).ok_or("vertex count overflowed")?;
    let packed_indices = packed_vertices;
    let index_format = if packed_vertices > u32::from(u16::MAX) { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 };
    let index_stride = u64::from(index_format.byte_size());
    let packed_vertex_bytes = u64::from(packed_vertices) * u64::from(AUTHORED_VERTEX_STRIDE);
    let packed_index_bytes = u64::from(packed_indices) * index_stride;
    let mut bytes = Vec::with_capacity(packed_vertex_bytes as usize);
    let mut index_bytes = Vec::with_capacity(packed_index_bytes as usize);
    let mut position_bytes = Vec::with_capacity((packed_vertices as usize) * 12);
    let mut per_face: Vec<(u32, u32)> = Vec::new();
    let mut degenerate_normals = 0u32;
    for (triangle_index, item) in emitted.iter().enumerate() {
        let packed = [f32_corner(item.corners[0])?, f32_corner(item.corners[1])?, f32_corner(item.corners[2])?];
        let (normal, degenerate) = packed_normal(packed[0], packed[1], packed[2]);
        if degenerate {
            degenerate_normals = degenerate_normals.saturating_add(1);
        }
        for corner in packed {
            for value in corner {
                position_bytes.extend_from_slice(&value.to_le_bytes());
            }
            push_vertex(&mut bytes, corner, normal);
        }
        match per_face.iter_mut().find(|(id, _)| *id == item.face_id) {
            Some((_, count)) => *count = count.saturating_add(1),
            None => per_face.push((item.face_id, 1)),
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
            stride: AUTHORED_VERTEX_STRIDE,
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
    let mut faces = Vec::new();
    for (face_id, triangles) in per_face {
        let vertices = triangles.saturating_mul(3);
        let vertex_bytes = u64::from(vertices) * u64::from(AUTHORED_VERTEX_STRIDE);
        let index_bytes = u64::from(vertices) * index_stride;
        faces.push(PackedFace {
            face_id,
            packed_triangles: triangles,
            packed_vertices: vertices,
            vertex_bytes,
            index_bytes,
            gpu_bytes: vertex_bytes + index_bytes,
        });
    }
    Ok(PackedAuthored {
        mesh,
        faces,
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

fn cache_key(hash: u64, revision: u64, camera: &AuthoredCamera) -> String {
    let milli = (f64::from(camera.requested_error_px) * 1000.0).round() as i32;
    format!(
        "authored6-h{hash:016x}-r{revision}-e{:.4}-{:.4}-{:.4}-fov{:016x}-px{milli}-w{:.0}-h{:.0}-a{AUTHORED_INTENT_ALGORITHM_VERSION}",
        camera.eye[0],
        camera.eye[1],
        camera.eye[2],
        camera.vertical_fov_radians.to_bits(),
        camera.viewport_width,
        camera.viewport_height
    )
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

fn id_list(ids: &[u32]) -> String {
    if ids.is_empty() {
        "NONE".to_string()
    } else {
        ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
    }
}

fn yes(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
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

fn length(value: [f64; 3]) -> f64 {
    dot(value, value).sqrt()
}

fn normalize(value: [f64; 3]) -> Option<[f64; 3]> {
    let span = length(value);
    if !span.is_finite() || span < 1.0e-12 { None } else { Some(scale(value, 1.0 / span)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_c_cache_contract() {
        let main = authored_main_level();
        let main_before = std::fs::read(&main).expect("Main.jarviglevel");
        assert_eq!(main_before.len() as u64, AUTHORED_MAIN_LEVEL_BYTES);
        let directory = contract_dir();
        let _keep_unedited = RestoreUnedited { directory: directory.clone() };
        let level_path = directory.join("Content").join("Levels").join("Authored.jarviglevel");
        write_authored_intent_project(&directory).expect("unedited project");
        let first = std::fs::read(&level_path).expect("first save");
        let document = crate::parse_level(std::str::from_utf8(&first).expect("utf8")).expect("parse");
        assert_eq!(document.format_version, 6);
        let backup = directory.join("Saved").join("Backup");
        crate::save_level_atomic(&level_path, &backup, &document).expect("save again");
        let second = std::fs::read(&level_path).expect("second save");
        assert_eq!(first, second, "a matching level saved with no edit changed bytes");
        let text = String::from_utf8(second.clone()).expect("level text");
        assert!(text.contains("\"version\": 1"));
        assert!(!text.contains("\"version\": 2"));
        assert!(!text.contains("size_cache"));
        assert_eq!(crate::LEVEL_BLOCK_VERSION, 6);
        assert_eq!(crate::TYPE_REGISTRY_VERSION, 9);

        let record = specimen(&document);
        assert!(record.body.is_none());
        assert_eq!(record.intent.len(), 65);
        assert_eq!(crate::intent_authority_diagnostic(&record), "Intent Authority: ELIGIBLE");
        let replay_before = replay_aabb(&record);
        assert_eq!(size_bits(replay_before), size_bits(record.size_m));
        let mut cache_only = record.clone();
        cache_only.size_m[1] += 0.1;
        assert_eq!(size_bits(replay_aabb(&cache_only)), size_bits(replay_before));
        assert_eq!(
            crate::intent_authority_diagnostic(&cache_only),
            "Intent Authority: INELIGIBLE — reconstruction does not match the stored size"
        );

        let requested = [record.size_m[0], record.size_m[1] + 0.1, record.size_m[2]];
        let committed = crate::commit_class_c_intent(&record, crate::IntentEntry { groups: None, payload: crate::IntentPayload::Size { size_m: requested } }).expect("commit");
        assert!(committed.body.is_none());
        assert_eq!(committed.intent.len(), 66);
        assert_eq!(committed.history.len(), 24);
        assert_eq!(committed.seed_size_m, record.seed_size_m);
        let committed_replay = replay_aabb(&committed);
        assert_eq!(size_bits(committed.size_m), size_bits(committed_replay));
        assert_eq!(size_bits(committed.size_m), EDITED_SIZE_BITS);
        assert_eq!(authored_size_bits(&committed).expect("payload"), EDITED_SIZE_BITS);
        let mut intent_only = record.clone();
        intent_only.intent.push(crate::IntentEntry { groups: None, payload: crate::IntentPayload::Size { size_m: requested } });
        assert_eq!(size_bits(intent_only.size_m), size_bits(record.size_m));
        assert_eq!(size_bits(replay_aabb(&intent_only)), EDITED_SIZE_BITS);
        assert_eq!(
            crate::intent_authority_diagnostic(&intent_only),
            "Intent Authority: INELIGIBLE — reconstruction does not match the stored size"
        );
        let stale = crate::size_edit_copy(&record).expect("stale append");
        assert_eq!(stale, intent_only);
        let mut stale_document = document.clone();
        install_specimen(&mut stale_document, stale);
        let stale_path = directory.join("Content").join("Levels").join("Authored-stale.jarviglevel");
        crate::save_level_atomic(&stale_path, &backup, &stale_document).expect("save stale");
        let stale_text = std::fs::read_to_string(&stale_path).expect("stale text");
        let stale_parsed = crate::parse_level(&stale_text).expect("parse stale");
        let stale_record = specimen(&stale_parsed);
        assert!(stale_record.body.is_none());
        assert_eq!(stale_record.intent.len(), 66);
        assert_eq!(size_bits(stale_record.size_m), size_bits(record.size_m));
        assert_eq!(stale_record.size_m[1].to_bits(), 0x4004_0000_0000_0000);
        assert_eq!(authored_size_bits(&stale_record).expect("stale payload"), EDITED_SIZE_BITS);
        assert_eq!(
            crate::intent_authority_diagnostic(&stale_record),
            "Intent Authority: INELIGIBLE — reconstruction does not match the stored size"
        );

        let mut round_document = document.clone();
        install_specimen(&mut round_document, committed.clone());
        crate::save_level_atomic(&level_path, &backup, &round_document).expect("save commit");
        let round_text = std::fs::read_to_string(&level_path).expect("round text");
        assert!(!round_text.contains("analytic-surface"));
        assert!(!round_text.contains("\"version\": 2"));
        let round_parsed = specimen(&crate::parse_level(&round_text).expect("parse commit"));
        assert!(round_parsed.body.is_none());
        assert_eq!(round_parsed.intent.len(), 66);
        assert_eq!(round_parsed.history.len(), 24);
        assert_eq!(crate::intent_authority_diagnostic(&round_parsed), "Intent Authority: ELIGIBLE");
        assert_eq!(size_bits(round_parsed.size_m), size_bits(replay_aabb(&round_parsed)));
        assert_eq!(size_bits(round_parsed.size_m), EDITED_SIZE_BITS);
        let mut shifted = round_parsed.clone();
        shifted.size_m[1] = record.size_m[1];
        assert_eq!(size_bits(replay_aabb(&shifted)), EDITED_SIZE_BITS);
        assert_eq!(
            crate::intent_authority_diagnostic(&shifted),
            "Intent Authority: INELIGIBLE — reconstruction does not match the stored size"
        );

        let mut world = crate::SceneWorld::new_session();
        assert!(!world.intent_authority_experiment());
        let id = world.create_block(crate::Vec3::new(0.0, 1.0, -4.0), round_parsed.clone()).expect("place");
        let solids = world.block_solids();
        assert_eq!(solids.len(), 1);
        assert_eq!(size_bits(solids[0].size_m), size_bits(replay_aabb(&round_parsed)));
        let mesh_id = world.object_mesh(id).expect("mesh id");
        let mesh = world.meshes().get(mesh_id).expect("mesh");
        assert_eq!(mesh.vertex_count(), 24);
        assert_eq!(mesh.index_count() / 3, 12);
        let mut poisoned = round_parsed.clone();
        poisoned.size_m[0] += 0.25;
        let mut poisoned_world = crate::SceneWorld::new_session();
        let poisoned_id = poisoned_world.create_block(crate::Vec3::new(0.0, 1.0, -4.0), poisoned.clone()).expect("poisoned place");
        assert_eq!(poisoned_world.block_solids()[0].size_m[0].to_bits(), poisoned.size_m[0].to_bits());
        assert_ne!(poisoned_world.block_solids()[0].size_m[0].to_bits(), replay_aabb(&poisoned)[0].to_bits());
        assert_eq!(size_bits(replay_aabb(&poisoned)), size_bits(replay_aabb(&round_parsed)));
        let poisoned_mesh = poisoned_world.meshes().get(poisoned_world.object_mesh(poisoned_id).expect("id")).expect("mesh");
        assert_eq!(poisoned_mesh.vertex_count(), 24);
        assert_eq!(poisoned_mesh.index_count() / 3, 12);

        let mut stored = record.clone();
        let body = match crate::intent_authority_candidate(&stored) {
            crate::IntentAuthorityCandidate::Reconstructable(body) => body,
            crate::IntentAuthorityCandidate::Refused(reason) => panic!("{reason}"),
        };
        stored.body = Some(body.clone());
        stored.size_m = body.aabb_size();
        stored.validate().expect("matching body");
        let stored_before = stored.clone();
        let refused = crate::commit_class_c_intent(&stored, crate::IntentEntry { groups: None, payload: crate::IntentPayload::Size { size_m: requested } });
        assert!(refused.is_err());
        assert_eq!(stored, stored_before);
        let mut near = stored.clone();
        near.size_m[1] += 5.0e-4;
        near.validate().expect("5e-4 still loads");
        assert!(near.body.is_some());
        let mut wide = stored.clone();
        wide.size_m[1] += 2.0e-3;
        let wide_error = wide.validate().expect_err("wider than 1e-3");
        assert!(wide_error.to_string().contains("block size does not match its body"));

        let plain = crate::BlockRecord::standard([2.0, 2.0, 2.0]).expect("plain");
        assert!(plain.body.is_none());
        assert!(crate::intent_authority_diagnostic(&plain).starts_with("Intent Authority: INELIGIBLE"));
        assert!(crate::commit_class_c_intent(&plain, crate::IntentEntry { groups: None, payload: crate::IntentPayload::Size { size_m: [2.0, 2.1, 2.0] } }).is_err());
        let plain_mesh = crate::block_surface_mesh([2.0_f32, 2.0, 2.0], [0.0_f32; 6], 0.0);
        assert_eq!(plain_mesh.vertex_count(), 24);
        assert_eq!(plain_mesh.index_count() / 3, 12);
        let mut beveled = plain.clone();
        beveled.bevel_m = 0.2;
        beveled.validate().expect("bevel record");
        assert!(crate::intent_authority_diagnostic(&beveled).contains("uniform-bevel"));
        assert!(crate::commit_class_c_intent(&beveled, crate::IntentEntry { groups: None, payload: crate::IntentPayload::Size { size_m: [2.0, 2.0, 2.0] } }).is_err());
        assert!(crate::block_surface_mesh([2.0_f32, 2.0, 2.0], [0.0_f32; 6], 0.2).index_count() / 3 > 12);
        let mut inset = plain.clone();
        inset.inset_m[0] = 0.1;
        inset.validate().expect("inset record");
        assert!(crate::intent_authority_diagnostic(&inset).contains("inset"));
        assert!(crate::block_surface_mesh([2.0_f32, 2.0, 2.0], [0.1_f32, 0.0, 0.0, 0.0, 0.0, 0.0], 0.0).index_count() / 3 > 12);
        let mut analytic = record.clone();
        analytic.intent.push(crate::IntentEntry {
            groups: None,
            payload: crate::IntentPayload::AnalyticSurface {
                identity: "contract".into(),
                chart: "p(u,v)=(u,v,1)/sqrt(u*u+v*v+1)".into(),
                domain_u: [-0.5, 0.5],
                domain_v: [-0.5, 0.5],
                radius_m: 1.0,
                translation_m: [0.0, 0.0, 0.0],
            },
        });
        assert!(crate::intent_authority_diagnostic(&analytic).contains("analytic-surface is not a boundary-representation operand"));
        assert_eq!(size_bits(analytic.size_m), size_bits(record.size_m));
        let before_gap = record.clone();
        let gap = crate::commit_class_c_intent(&record, crate::IntentEntry { groups: None, payload: crate::IntentPayload::Gap { operation: "inset".into() } });
        assert!(gap.is_err());
        assert_eq!(record, before_gap);

        assert_eq!(std::fs::read(&main).expect("Main after"), main_before);
    }

    #[test]
    fn authored_intent_reloads_the_tape_without_presenting() {
        println!("Frame: NOT PRESENTED");
        let main = authored_main_level();
        let main_before = std::fs::read(&main).expect("Main.jarviglevel");
        assert_eq!(main_before.len() as u64, AUTHORED_MAIN_LEVEL_BYTES);
        let fov = crate::bootstrap_shared_world().front.vertical_fov_radians;
        assert!(fov.is_finite() && fov > 0.0);
        let directory = authored_rerun_dir();
        let project = write_authored_intent_project(&directory).expect("proof project");
        // A failing assertion still leaves the editor the unedited 65-entry tape.
        let _restore_unedited = RestoreUnedited { directory: directory.clone() };
        assert!(project.ends_with("AuthoredIntent.jarvigproject"));
        let level_path = directory.join("Content").join("Levels").join("Authored.jarviglevel");
        let text = std::fs::read_to_string(&level_path).expect("level text");
        assert!(!text.contains("analytic-surface"));
        assert!(!text.contains("\"version\": 2"));
        assert!(text.contains("\"version\": 1"));
        assert_eq!(crate::LEVEL_BLOCK_VERSION, 6);
        assert_eq!(crate::TYPE_REGISTRY_VERSION, 9);
        assert_eq!(AUTHORED_VERTEX_STRIDE, crate::observation::OBSERVATION_VERTEX_STRIDE);
        assert_eq!(crate::INTENT_OBSERVATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::DIRECT_REALIZATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::OBSERVATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::REALIZATION_ALGORITHM_VERSION, 1);
        assert_eq!(AUTHORED_INTENT_ALGORITHM_VERSION, 1);
        let document = crate::parse_level(&text).expect("proof reload");
        assert_eq!(document.format_version, 6);
        assert_ne!(document.level_uuid.to_string(), "27983b8a-eeee-494d-bb19-3687686f1684");
        let record = specimen(&document);
        let legacy = legacy_block(&document);
        assert!(record.body.is_none());
        assert!(legacy.body.is_some());
        assert_eq!(record.intent.len(), 65);
        assert_eq!(record.history.len(), 24);
        assert_eq!(specimen_uuid(&document), SOLID_UUID);
        assert_ne!(specimen_uuid(&document), SOURCE_SOLID_UUID);
        assert_eq!(crate::intent_authority_diagnostic(&record), "Intent Authority: ELIGIBLE");
        let hash = authored_authority_hash(&record).expect("hash");
        assert_eq!(authored_authority_hash(&record).expect("stable hash"), hash);
        for frozen in FROZEN_AUTHORITIES {
            assert_ne!(hash, frozen);
        }
        let camera_a = authored_camera_a(fov);
        let camera_b = authored_camera_b(fov);
        assert_eq!(format!("[{:.4}, {:.4}, {:.4}]", camera_a.eye[0], camera_a.eye[1], camera_a.eye[2]), "[0.0000, 0.0000, 3.6250]");
        assert_eq!(format!("[{:.4}, {:.4}, {:.4}]", camera_b.eye[0], camera_b.eye[1], camera_b.eye[2]), "[2.7801, -0.6250, 3.0850]");
        assert_eq!(camera_a.vertical_fov_radians.to_bits(), fov.to_bits());
        assert_eq!(camera_b.vertical_fov_radians.to_bits(), fov.to_bits());
        let observation_a = realize_authored_observation(&record, &camera_a, 1).expect("observation A");
        let observation_b = realize_authored_observation(&record, &camera_b, 1).expect("observation B");
        println!("{}", format_authored_product("Observation A", &observation_a));
        println!("{}", format_authored_product("Observation B", &observation_b));
        assert_eq!(observation_a.authority_hash, hash);
        assert_eq!(observation_b.authority_hash, hash);
        assert_ne!(observation_a.authority_hash, observation_a.local_body_hash);
        assert_eq!(observation_a.revision, 1);
        assert!(observation_a.cache_key.starts_with("authored6-"));
        assert!(!observation_a.cache_key.contains("view-a") && !observation_b.cache_key.contains("view-b"));
        assert_ne!(observation_a.cache_key, observation_b.cache_key);
        assert!(observation_a.cache_key.contains(&format!("fov{:016x}", fov.to_bits())));
        assert!(!observation_a.admitted_ids.is_empty());
        assert!(!observation_b.admitted_ids.is_empty());
        assert_ne!(observation_a.admitted_ids, observation_b.admitted_ids);
        assert_ne!(observation_a.expected_gpu_bytes, observation_b.expected_gpu_bytes);
        assert_eq!(observation_a.constructed_triangles, observation_a.packed_triangles);
        assert_eq!(observation_a.discarded_after_construction, 0);
        assert_eq!(observation_a.discarded_during_pack, 0);
        assert_eq!(observation_a.meshlets_constructed, 0);
        assert!(!observation_a.vertices_welded);
        assert!(!observation_a.object_mesh_consulted);
        assert!(!observation_a.record_body_consulted);
        assert!(!observation_a.realize_intent_observation_consulted);
        assert!(!observation_a.analytic_specimen_consulted);
        for account in observation_a.faces.iter().chain(observation_b.faces.iter()) {
            if account.admission.omitted() {
                assert_eq!(account.triangles_constructed, 0);
                assert_eq!(account.gpu_bytes, 0);
            } else {
                assert_eq!(account.triangles_constructed, account.loop_len - 2);
            }
        }

        let requested = [record.size_m[0], record.size_m[1] + 0.1, record.size_m[2]];
        let edited = crate::commit_class_c_intent(&record, crate::IntentEntry { groups: None, payload: crate::IntentPayload::Size { size_m: requested } }).expect("class C size");
        assert!(edited.body.is_none());
        assert_eq!(size_bits(edited.size_m), EDITED_SIZE_BITS);
        assert_eq!(edited.intent.len(), 66);
        assert_eq!(edited.history.len(), 24);
        assert_eq!(authored_size_bits(&edited).expect("edited bits"), EDITED_SIZE_BITS);
        let in_memory_size = edited.size_m;
        let in_memory_diagnostic = crate::intent_authority_diagnostic(&edited);
        let mut edited_document = document.clone();
        install_specimen(&mut edited_document, edited);
        let backup = directory.join("Saved").join("Backup");
        crate::save_level_atomic(&level_path, &backup, &edited_document).expect("save edit");
        let edited_text = std::fs::read_to_string(&level_path).expect("edited text");
        assert!(!edited_text.contains("analytic-surface"));
        assert!(!edited_text.contains("\"version\": 2"));
        let reloaded = crate::parse_level(&edited_text).expect("edited reload");
        let edited_record = specimen(&reloaded);
        assert!(edited_record.body.is_none());
        assert!(legacy_block(&reloaded).body.is_some());
        assert_eq!(edited_record.intent.len(), 66);
        assert_eq!(edited_record.history.len(), 24);
        assert_eq!(authored_size_bits(&edited_record).expect("parsed bits"), EDITED_SIZE_BITS);
        println!(
            "in-memory size_m bits: {:016x} {:016x} {:016x}",
            in_memory_size[0].to_bits(),
            in_memory_size[1].to_bits(),
            in_memory_size[2].to_bits()
        );
        println!(
            "parsed size_m bits: {:016x} {:016x} {:016x}",
            edited_record.size_m[0].to_bits(),
            edited_record.size_m[1].to_bits(),
            edited_record.size_m[2].to_bits()
        );
        println!("in-memory diagnostic: {in_memory_diagnostic}");
        match crate::intent_authority_candidate(&edited_record) {
            crate::IntentAuthorityCandidate::Reconstructable(body) => {
                let bounds = body.aabb_size();
                println!(
                    "parsed reconstruction aabb bits: {:016x} {:016x} {:016x}",
                    bounds[0].to_bits(),
                    bounds[1].to_bits(),
                    bounds[2].to_bits()
                );
            }
            crate::IntentAuthorityCandidate::Refused(reason) => println!("parsed reconstruction refused: {reason}"),
        }
        println!("parsed diagnostic: {}", crate::intent_authority_diagnostic(&edited_record));
        assert_eq!(crate::intent_authority_diagnostic(&edited_record), "Intent Authority: ELIGIBLE");
        let edited_hash = authored_authority_hash(&edited_record).expect("edited hash");
        assert_ne!(edited_hash, hash);
        for frozen in FROZEN_AUTHORITIES {
            assert_ne!(edited_hash, frozen);
        }
        let edited_a = realize_authored_observation(&edited_record, &camera_a, 2).expect("edited A");
        let edited_b = realize_authored_observation(&edited_record, &camera_b, 2).expect("edited B");
        println!("{}", format_authored_product("Edited Observation A", &edited_a));
        println!("{}", format_authored_product("Edited Observation B", &edited_b));
        assert_eq!(edited_a.revision, 2);
        assert_eq!(edited_a.authority_hash, edited_hash);
        assert!(authored_planar_changed(&observation_a, &edited_a) || authored_planar_changed(&observation_b, &edited_b));
        assert_ne!(edited_a.admitted_ids, edited_b.admitted_ids);
        assert!(!edited_a.admitted_ids.is_empty() && !edited_b.admitted_ids.is_empty());
        assert_ne!(edited_a.cache_key, observation_a.cache_key);
        assert_ne!(edited_a.expected_gpu_bytes, edited_b.expected_gpu_bytes);

        write_authored_intent_project(&directory).expect("restore the unedited tape");
        let restored = crate::parse_level(&std::fs::read_to_string(&level_path).expect("restored")).expect("restored parse");
        assert_eq!(specimen(&restored).intent.len(), 65);
        assert!(specimen(&restored).body.is_none());
        assert_eq!(std::fs::read(&main).expect("Main after"), main_before);
        println!("Frame: NOT PRESENTED");
    }

    fn contract_dir() -> std::path::PathBuf {
        let root = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"C:\Users\Jeramiah\AppData\Local"));
        root.join("Temp").join("jarvig-class-c-contract")
    }

    fn size_bits(size_m: [f64; 3]) -> String {
        format!("{:016x} {:016x} {:016x}", size_m[0].to_bits(), size_m[1].to_bits(), size_m[2].to_bits())
    }

    fn replay_aabb(record: &BlockRecord) -> [f64; 3] {
        match crate::intent_authority_candidate(record) {
            crate::IntentAuthorityCandidate::Reconstructable(body) => body.aabb_size(),
            crate::IntentAuthorityCandidate::Refused(reason) => panic!("{reason}"),
        }
    }

    fn specimen(document: &crate::LevelDocument) -> BlockRecord {
        document
            .entities
            .iter()
            .find(|entity| entity.name == "Intent Solid")
            .and_then(|entity| {
                entity.components.iter().find_map(|component| match component {
                    crate::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                    _ => None,
                })
            })
            .expect("Intent Solid")
    }

    fn specimen_uuid(document: &crate::LevelDocument) -> String {
        document.entities.iter().find(|entity| entity.name == "Intent Solid").expect("Intent Solid").uuid.to_string()
    }

    fn legacy_block(document: &crate::LevelDocument) -> BlockRecord {
        document
            .entities
            .iter()
            .find(|entity| entity.name == "Legacy Cube")
            .and_then(|entity| {
                entity.components.iter().find_map(|component| match component {
                    crate::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                    _ => None,
                })
            })
            .expect("Legacy Cube")
    }

    struct RestoreUnedited {
        directory: std::path::PathBuf,
    }

    impl Drop for RestoreUnedited {
        fn drop(&mut self) {
            let _ = write_authored_intent_project(&self.directory);
        }
    }

    fn install_specimen(document: &mut crate::LevelDocument, block: BlockRecord) {
        let entity = document.entities.iter_mut().find(|entity| entity.name == "Intent Solid").expect("Intent Solid");
        for component in &mut entity.components {
            if let crate::ComponentRecord::ParametricBlock(slot) = component {
                *slot = block;
                return;
            }
        }
    }
}
