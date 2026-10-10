//! Observation admission. One view decides which authoritative faces it can prove
//! it needs, then fan-triangulates only those faces.
//!
//! An omitted face never enters the fan. This is not a cluster cut, not a meshlet
//! reject, and not the Experiment 2 simplifier. `REALIZATION_ALGORITHM_VERSION`
//! stays where it is.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::entity::EntityId;
use crate::mesh::{
    create_mesh, Mesh, MeshDesc, MeshError, MeshIndexFormat, MeshTopology, MeshVertexAttribute, MeshVertexFormat, SubmeshDesc, VertexStreamDesc,
};
use crate::microgeometry::projected_detail_px;
use crate::topology::SolidBody;

/// Bump when the admission rule changes. This is not [`crate::REALIZATION_ALGORITHM_VERSION`].
pub const OBSERVATION_ALGORITHM_VERSION: u32 = 1;
const POSE_BUCKET_M: f64 = 0.5;
const FORWARD_DEADZONE: f64 = 0.25;
const NEAR_BUCKET_M: f64 = 0.01;

/// Why a face was kept or left unbuilt. `Omit` is the only decision that skips the fan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Omit,
    AdmitInView,
    AdmitBehindCamera,
    AdmitCrossesNear,
    AdmitUncertain,
}

impl Admission {
    pub fn omitted(self) -> bool {
        matches!(self, Self::Omit)
    }

    pub fn uncertain(self) -> bool {
        matches!(self, Self::AdmitBehindCamera | Self::AdmitCrossesNear | Self::AdmitUncertain)
    }
}

/// Eye and forward are in the solid's local frame. Width is required because the frustum has an aspect.
#[derive(Clone, Debug)]
pub struct ObservationCamera {
    pub eye_local: [f64; 3],
    pub forward_local: [f64; 3],
    pub up_local: [f64; 3],
    pub vertical_fov_radians: f64,
    /// Meters. The editor camera uses 0.1 unless a request names another.
    pub near_m: f64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub requested_error_px: f32,
}

/// One observation. The body is a snapshot. The caller does not pass an omit list.
#[derive(Clone, Debug)]
pub struct ObservationRequest {
    pub entity: EntityId,
    pub body_hash: u64,
    pub revision: u64,
    pub body: SolidBody,
    pub camera: ObservationCamera,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceDecision {
    pub face_id: u32,
    pub admission: Admission,
    pub area_m2: f64,
    pub measured_error_px: f32,
}

/// One triangle that was fanned. `face_id` names the source face. It is not the authority.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstructedTriangle {
    pub face_id: u32,
    pub vertex_ids: [u32; 3],
    pub positions: [[f64; 3]; 3],
}

/// The partial product. Dropping it does not write the world.
#[derive(Clone, Debug)]
pub struct ObservationProduct {
    pub observation_id: u64,
    pub cache_key: String,
    pub entity: EntityId,
    pub body_hash: u64,
    pub revision: u64,
    pub authoritative_face_count: u32,
    pub decisions: Vec<FaceDecision>,
    pub admitted_face_ids: Vec<u32>,
    pub omitted_face_ids: Vec<u32>,
    pub uncertain_face_ids: Vec<u32>,
    /// Face ids that entered the fan, in body order. Omitted ids are absent.
    pub faces_expanded: Vec<u32>,
    pub triangles: Vec<ConstructedTriangle>,
    pub vertex_ids: Vec<u32>,
    pub positions: Vec<[f32; 3]>,
    pub potential_triangles: u32,
    pub triangles_constructed: u32,
    pub triangles_discarded_after_construction: u32,
    pub vertices_constructed: u32,
    pub bytes_constructed: u64,
    pub admitted_area_m2: f64,
    pub omitted_area_m2: f64,
    pub measured_projected_error_px: f32,
    pub requested_error_px: f32,
    pub generation_us: u64,
    pub algorithm_version: u32,
    pub object_mesh_consulted: bool,
    pub shared_render_mesh_consulted: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ObservationIntegrity {
    pub body_mutated: bool,
    pub intent_mutated: bool,
    pub collision_match: bool,
    pub materials_match: bool,
    pub semantic_match: bool,
}

#[derive(Clone, Debug)]
pub struct ObservationTicket {
    pub cache_hit: bool,
    pub product: ObservationProduct,
}

/// Cache and publish record for direct calls. This host starts no worker.
pub struct ObservationHost {
    next_id: u64,
    constructions: u32,
    discarded_stale: u32,
    current_key: Option<String>,
    cache: BTreeMap<String, ObservationProduct>,
    resident: Option<ObservationProduct>,
}

impl ObservationHost {
    pub fn new() -> Self {
        Self { next_id: 1, constructions: 0, discarded_stale: 0, current_key: None, cache: BTreeMap::new(), resident: None }
    }

    pub fn construction_count(&self) -> u32 {
        self.constructions
    }

    pub fn discarded_stale(&self) -> u32 {
        self.discarded_stale
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn resident(&self) -> Option<&ObservationProduct> {
        self.resident.as_ref()
    }

    /// Classify and fan from this request's snapshot. A different key does not reuse another product.
    pub fn observe(&mut self, request: ObservationRequest) -> Result<ObservationTicket, String> {
        if crate::authoritative_body_hash(&request.body) != request.body_hash {
            return Err("the snapshot hash does not match the request".into());
        }
        let key = cache_key(&request);
        self.current_key = Some(key.clone());
        if let Some(product) = self.cache.get(&key) {
            self.resident = Some(product.clone());
            return Ok(ObservationTicket { cache_hit: true, product: product.clone() });
        }
        self.constructions = self.constructions.saturating_add(1);
        let product = construct_observation(&request, self.next_id, &key)?;
        self.next_id = self.next_id.saturating_add(1);
        self.cache.insert(key, product.clone());
        self.resident = Some(product.clone());
        Ok(ObservationTicket { cache_hit: false, product })
    }

    /// A finished product whose key is no longer current does not become resident.
    pub fn publish_if_current(&mut self, product: &ObservationProduct) -> bool {
        if self.current_key.as_deref() == Some(product.cache_key.as_str()) {
            self.cache.insert(product.cache_key.clone(), product.clone());
            self.resident = Some(product.clone());
            true
        } else {
            self.discarded_stale = self.discarded_stale.saturating_add(1);
            false
        }
    }

    pub fn drop_all(&mut self) {
        self.cache.clear();
        self.resident = None;
        self.current_key = None;
    }
}

pub fn format_observation_block(product: &ObservationProduct, revision: u64, integrity: &ObservationIntegrity) -> String {
    let mut out = String::new();
    out.push_str("Experiment 3A: ON\n");
    out.push_str(&format!("authoritative_body_hash: {:016x}\n", product.body_hash));
    out.push_str(&format!("body_revision: {revision}\n"));
    out.push_str(&format!("observation_id: {}\n", product.observation_id));
    out.push_str(&format!("authoritative_face_count: {}\n", product.authoritative_face_count));
    out.push_str(&format!("admitted_face_count: {}\n", product.admitted_face_ids.len()));
    out.push_str(&format!("omitted_face_count: {}\n", product.omitted_face_ids.len()));
    out.push_str(&format!("admitted_face_ids: {}\n", id_list(&product.admitted_face_ids)));
    out.push_str(&format!("omitted_face_ids: {}\n", id_list(&product.omitted_face_ids)));
    out.push('\n');
    out.push_str(&format!("potential_triangles: {}\n", product.potential_triangles));
    out.push_str(&format!("triangles_constructed: {}\n", product.triangles_constructed));
    out.push_str(&format!("triangles_discarded_after_construction: {}\n", product.triangles_discarded_after_construction));
    out.push('\n');
    out.push_str(&format!("vertices_constructed: {}\n", product.vertices_constructed));
    out.push_str(&format!("bytes_constructed: {}\n", product.bytes_constructed));
    out.push('\n');
    out.push_str(&format!("body_mutated: {}\n", yes(integrity.body_mutated)));
    out.push_str(&format!("intent_mutated: {}\n", yes(integrity.intent_mutated)));
    out.push_str(&format!("object_mesh_consulted: {}\n", yes(product.object_mesh_consulted)));
    out.push_str(&format!("shared_render_mesh_consulted: {}\n", yes(product.shared_render_mesh_consulted)));
    out.push('\n');
    out.push_str(&format!("collision_before_after: {}\n", matched(integrity.collision_match)));
    out.push_str(&format!("materials_before_after: {}\n", matched(integrity.materials_match)));
    out.push_str(&format!("semantic_identity_before_after: {}\n", matched(integrity.semantic_match)));
    out
}

fn construct_observation(request: &ObservationRequest, observation_id: u64, key: &str) -> Result<ObservationProduct, String> {
    let started = Instant::now();
    let body = &request.body;
    let camera = &request.camera;
    let mut decisions = Vec::new();
    let mut admitted_face_ids = Vec::new();
    let mut omitted_face_ids = Vec::new();
    let mut uncertain_face_ids = Vec::new();
    let mut potential_triangles = 0u32;
    let mut admitted_area_m2 = 0.0f64;
    let mut omitted_area_m2 = 0.0f64;
    for face in &body.faces {
        let admission = classify_face(body, face.id, camera);
        let area = body.face_area(face.id).unwrap_or(0.0);
        let fan = loop_fan(body.face_loop(face.id).map(|loop_| loop_.len()).unwrap_or(0));
        potential_triangles = potential_triangles.saturating_add(fan);
        decisions.push(FaceDecision { face_id: face.id, admission, area_m2: area, measured_error_px: 0.0 });
        if admission.omitted() {
            omitted_face_ids.push(face.id);
            omitted_area_m2 += area;
        } else {
            admitted_face_ids.push(face.id);
            admitted_area_m2 += area;
            if admission.uncertain() {
                uncertain_face_ids.push(face.id);
            }
        }
    }
    let faces_expanded = admitted_face_ids.clone();
    let triangles = fan_admitted(body, &faces_expanded)?;
    let triangles_constructed = u32::try_from(triangles.len()).unwrap_or(u32::MAX);
    let triangles_discarded_after_construction = 0u32;
    let mut vertex_ids = Vec::new();
    for face_id in &faces_expanded {
        let Some(loop_) = body.face_loop(*face_id) else { continue };
        for id in loop_ {
            if !vertex_ids.contains(id) {
                vertex_ids.push(*id);
            }
        }
    }
    let mut positions = Vec::with_capacity(vertex_ids.len());
    let mut position_bytes = Vec::new();
    for id in &vertex_ids {
        let position = body.vertex_position(*id).ok_or_else(|| format!("missing vertex {id}"))?;
        let stored = [position[0] as f32, position[1] as f32, position[2] as f32];
        for value in stored {
            position_bytes.extend_from_slice(&value.to_le_bytes());
        }
        positions.push(stored);
    }
    let index_bytes = triangles.len().saturating_mul(12);
    let tag_bytes = triangles.len().saturating_mul(4);
    let bytes_constructed = (position_bytes.len() + index_bytes + tag_bytes) as u64;
    let mut measured_projected_error_px = 0.0f32;
    for decision in &mut decisions {
        if decision.admission.omitted() {
            continue;
        }
        let error = face_error_px(body, decision.face_id, &triangles, camera);
        decision.measured_error_px = error;
        measured_projected_error_px = measured_projected_error_px.max(error);
    }
    Ok(ObservationProduct {
        observation_id,
        cache_key: key.to_string(),
        entity: request.entity,
        body_hash: request.body_hash,
        revision: request.revision,
        authoritative_face_count: u32::try_from(body.faces.len()).unwrap_or(u32::MAX),
        decisions,
        admitted_face_ids,
        omitted_face_ids,
        uncertain_face_ids,
        faces_expanded,
        triangles,
        vertices_constructed: u32::try_from(vertex_ids.len()).unwrap_or(u32::MAX),
        vertex_ids,
        positions,
        potential_triangles,
        triangles_constructed,
        triangles_discarded_after_construction,
        bytes_constructed,
        admitted_area_m2,
        omitted_area_m2,
        measured_projected_error_px,
        requested_error_px: camera.requested_error_px,
        generation_us: started.elapsed().as_micros() as u64,
        algorithm_version: OBSERVATION_ALGORITHM_VERSION,
        object_mesh_consulted: false,
        shared_render_mesh_consulted: false,
    })
}

/// Fan only `admitted`. The slice is the expanded set. There is no later delete.
fn fan_admitted(body: &SolidBody, admitted: &[u32]) -> Result<Vec<ConstructedTriangle>, String> {
    let mut triangles = Vec::new();
    for face_id in admitted {
        let loop_ = body.face_loop(*face_id).ok_or_else(|| format!("missing face {face_id}"))?;
        if loop_.len() < 3 {
            continue;
        }
        for index in 1..loop_.len() - 1 {
            let vertex_ids = [loop_[0], loop_[index], loop_[index + 1]];
            let mut positions = [[0.0; 3]; 3];
            for corner in 0..3 {
                positions[corner] = body.vertex_position(vertex_ids[corner]).ok_or_else(|| format!("missing vertex {}", vertex_ids[corner]))?;
            }
            triangles.push(ConstructedTriangle { face_id: *face_id, vertex_ids, positions });
        }
    }
    Ok(triangles)
}

fn classify_face(body: &SolidBody, face: u32, camera: &ObservationCamera) -> Admission {
    let Some(positions) = body.face_positions(face) else {
        return Admission::AdmitUncertain;
    };
    if positions.len() < 3 || positions.iter().any(|position| position.iter().any(|axis| !axis.is_finite())) {
        return Admission::AdmitUncertain;
    }
    let Some((right, up, forward)) = camera_basis(camera.forward_local, camera.up_local) else {
        return Admission::AdmitUncertain;
    };
    let width = f64::from(camera.viewport_width);
    let height = f64::from(camera.viewport_height);
    let error = f64::from(camera.requested_error_px);
    if !camera.near_m.is_finite() || camera.near_m <= 0.0 || !error.is_finite() || error < 0.0 || width < 1.0 || height < 1.0 {
        return Admission::AdmitUncertain;
    }
    let half = camera.vertical_fov_radians * 0.5;
    if !half.is_finite() || half <= 0.0 || half >= std::f64::consts::FRAC_PI_2 {
        return Admission::AdmitUncertain;
    }
    let tan_half_v = half.tan();
    let tan_half_h = tan_half_v * (width / height);
    if !tan_half_v.is_finite() || !tan_half_h.is_finite() || tan_half_v <= 0.0 || tan_half_h <= 0.0 {
        return Admission::AdmitUncertain;
    }
    let mut depths = Vec::with_capacity(positions.len());
    for position in &positions {
        let relative = sub(*position, camera.eye_local);
        depths.push(dot(relative, forward));
    }
    if depths.iter().any(|depth| !depth.is_finite()) {
        return Admission::AdmitUncertain;
    }
    if depths.iter().any(|depth| *depth <= 0.0) {
        return Admission::AdmitBehindCamera;
    }
    if depths.iter().any(|depth| *depth <= camera.near_m) {
        return Admission::AdmitCrossesNear;
    }
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for (position, depth) in positions.iter().zip(depths.iter()) {
        let relative = sub(*position, camera.eye_local);
        let x_cam = dot(relative, right);
        let y_cam = dot(relative, up);
        let px = (x_cam / depth) / tan_half_h;
        let py = (y_cam / depth) / tan_half_v;
        if !px.is_finite() || !py.is_finite() {
            return Admission::AdmitUncertain;
        }
        let screen_x = (px * 0.5 + 0.5) * width;
        let screen_y = (py * 0.5 + 0.5) * height;
        min_x = min_x.min(screen_x);
        max_x = max_x.max(screen_x);
        min_y = min_y.min(screen_y);
        max_y = max_y.max(screen_y);
    }
    let misses = max_x < -error || min_x > width + error || max_y < -error || min_y > height + error;
    if misses { Admission::Omit } else { Admission::AdmitInView }
}

fn face_error_px(body: &SolidBody, face: u32, triangles: &[ConstructedTriangle], camera: &ObservationCamera) -> f32 {
    let Some(forward) = normalize(camera.forward_local) else {
        return 0.0;
    };
    let tan_half = ((camera.vertical_fov_radians * 0.5) as f32).tan();
    let mut max_px = 0.0f32;
    for triangle in triangles.iter().filter(|triangle| triangle.face_id == face) {
        for (id, position) in triangle.vertex_ids.iter().zip(triangle.positions.iter()) {
            let Some(authoritative) = body.vertex_position(*id) else {
                return f32::MAX;
            };
            let delta = sub(*position, authoritative);
            let deviation = length(delta);
            if deviation < 1.0e-6 {
                continue;
            }
            let depth = dot(sub(*position, camera.eye_local), forward);
            if depth < 0.05 {
                continue;
            }
            max_px = max_px.max(projected_detail_px(deviation as f32, depth as f32, camera.viewport_height, tan_half));
        }
    }
    max_px
}

fn cache_key(request: &ObservationRequest) -> String {
    let quantize = |value: f64| (value / POSE_BUCKET_M).round() as i32;
    let sign = |value: f64| if value > FORWARD_DEADZONE { 1 } else if value < -FORWARD_DEADZONE { -1 } else { 0 };
    let forward = normalize(request.camera.forward_local).unwrap_or([0.0, 0.0, -1.0]);
    let milli = (request.camera.requested_error_px * 1000.0).round() as i32;
    let width = request.camera.viewport_width.round() as i32;
    let height = request.camera.viewport_height.round() as i32;
    let near = (request.camera.near_m / NEAR_BUCKET_M).round() as i32;
    format!(
        "obs-e{}-h{:016x}-r{}-px{milli}-p{}-{}-{}-f{}-{}-{}-w{width}-h{height}-n{near}-a{}",
        request.entity,
        request.body_hash,
        request.revision,
        quantize(request.camera.eye_local[0]),
        quantize(request.camera.eye_local[1]),
        quantize(request.camera.eye_local[2]),
        sign(forward[0]),
        sign(forward[1]),
        sign(forward[2]),
        OBSERVATION_ALGORITHM_VERSION
    )
}

fn loop_fan(len: usize) -> u32 {
    u32::try_from(len.saturating_sub(2)).unwrap_or(u32::MAX)
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
    ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
}

fn yes(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

fn matched(value: bool) -> &'static str {
    if value { "MATCH" } else { "DIFFER" }
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

/// Proof stride of one observation vertex. Position, color, texcoord, normal, tangent.
pub const OBSERVATION_VERTEX_STRIDE: u32 = 60;
/// Frozen Experiment 3A viewport height. Experiment 3B does not re-admit at another size.
pub const FROZEN_OBSERVATION_HEIGHT: f32 = 567.0;
pub const FROZEN_A_WIDTH: f32 = 160.0;
pub const FROZEN_B_WIDTH: f32 = 960.0;
pub const FROZEN_A_EYE_TEXT: &str = "[0.0000, 0.0000, 3.6250]";
pub const FROZEN_B_EYE_TEXT: &str = "[2.7801, -0.6250, 3.0850]";
pub const FROZEN_A_ADMITTED: [u32; 10] = [3, 4, 5, 6, 33, 35, 36, 47, 50, 52];
pub const FROZEN_A_OMITTED: [u32; 9] = [1, 2, 44, 60, 63, 66, 68, 75, 89];
pub const FROZEN_B_ADMITTED: [u32; 15] = [1, 3, 4, 6, 33, 36, 44, 47, 52, 60, 63, 66, 68, 75, 89];
pub const FROZEN_B_OMITTED: [u32; 4] = [2, 5, 35, 50];
/// The specimen cross face. Observation A omits it. Observation B admits its fan.
pub const OBSERVATION_SPECIMEN_FACE: u32 = 89;

/// Byte account of one authoritative face inside one observation pack.
/// A boundary vertex of another face does not add bytes here. Nothing is welded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceGpuAccount {
    pub face_id: u32,
    pub cpu_triangles: u32,
    pub packed_triangles: u32,
    pub packed_vertices: u32,
    pub index_count: u32,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub total_gpu_bytes: u64,
    pub first_vertex: u32,
    pub first_index: u32,
}

/// What the frame thread is allowed to upload for one observation.
/// `expected_gpu_bytes` is the provenance sum. It is not read back from [`Mesh`].
#[derive(Clone, Debug)]
pub struct ObservationGpuPack {
    pub mesh: Mesh,
    pub accounts: Vec<FaceGpuAccount>,
    pub originating_face_ids: Vec<u32>,
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
    pub meshlets_constructed: u32,
    pub meshlet_gpu_bytes: u64,
    pub degenerate_normals: u32,
    pub face_89: FaceGpuAccount,
    pub vertices_welded: bool,
}

/// Sizes the device accepted for one mesh. The editor fills this after upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservationUploadProof {
    pub gpu_mesh_bytes: u64,
    pub vertex_create_bytes: u64,
    pub index_create_bytes: u64,
}

/// The exact Experiment 3A cameras. A different body fails closed. This does not search for a new pose.
pub fn frozen_observation_cameras(body: &SolidBody, vertical_fov_radians: f64) -> Result<(ObservationCamera, ObservationCamera), String> {
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
    let mut chosen: Option<ObservationCamera> = None;
    for width in [160.0_f32, 128.0, 96.0, 64.0, 48.0, 32.0, 24.0, 16.0, 8.0] {
        for shift in [0.0_f64, -0.5, -1.0, -1.5, -2.0, -2.5, 0.5, 1.0, 1.5] {
            let camera = frozen_camera([center[0] + shift, center[1], eye_z], [0.0, 0.0, -1.0], vertical_fov_radians, width);
            let omitted = body.faces.iter().any(|face| classify_face(body, face.id, &camera) == Admission::Omit && loop_fan(body.face_loop(face.id).map(|loop_| loop_.len()).unwrap_or(0)) > 0);
            let sees_one = body.faces.iter().any(|face| classify_face(body, face.id, &camera) == Admission::AdmitInView);
            if omitted && sees_one {
                chosen = Some(camera);
                break;
            }
        }
        if chosen.is_some() {
            break;
        }
    }
    let camera_a = chosen.ok_or("frozen observation A was not recovered")?;
    if eye_text(camera_a.eye_local) != FROZEN_A_EYE_TEXT || camera_a.viewport_width != FROZEN_A_WIDTH {
        return Err(format!("observation A camera drifted to {} at width {}", eye_text(camera_a.eye_local), camera_a.viewport_width));
    }
    let mut omitted_ids = Vec::new();
    for face in &body.faces {
        if classify_face(body, face.id, &camera_a) == Admission::Omit && loop_fan(body.face_loop(face.id).map(|loop_| loop_.len()).unwrap_or(0)) > 0 {
            omitted_ids.push(face.id);
        }
    }
    omitted_ids.sort_by(|left, right| face_centroid(body, *right)[0].total_cmp(&face_centroid(body, *left)[0]).then(left.cmp(right)));
    let mut camera_b = None;
    for face in omitted_ids {
        let Some(normal) = body.unit_normal(face) else { continue };
        let centroid = face_centroid(body, face);
        for distance in [1.5_f64, 2.5, 4.0] {
            let eye = [centroid[0] + normal[0] * distance, centroid[1] + normal[1] * distance, centroid[2] + normal[2] * distance];
            let forward = [-normal[0], -normal[1], -normal[2]];
            let camera = frozen_camera(eye, forward, vertical_fov_radians, FROZEN_B_WIDTH);
            if classify_face(body, face, &camera) == Admission::AdmitInView {
                camera_b = Some(camera);
                break;
            }
        }
        if camera_b.is_some() {
            break;
        }
    }
    let camera_b = camera_b.ok_or("frozen observation B was not recovered")?;
    if eye_text(camera_b.eye_local) != FROZEN_B_EYE_TEXT || camera_b.viewport_width != FROZEN_B_WIDTH {
        return Err(format!("observation B camera drifted to {} at width {}", eye_text(camera_b.eye_local), camera_b.viewport_width));
    }
    if forward_signs(camera_a.forward_local) != [0, 0, -1] || forward_signs(camera_b.forward_local) != [0, 0, -1] {
        return Err(format!(
            "frozen forward signs drifted A {:?} B {:?}",
            forward_signs(camera_a.forward_local),
            forward_signs(camera_b.forward_local)
        ));
    }
    Ok((camera_a, camera_b))
}

pub fn observation_eye_text(camera: &ObservationCamera) -> String {
    eye_text(camera.eye_local)
}

pub fn observation_forward_text(camera: &ObservationCamera) -> String {
    eye_text(camera.forward_local)
}

/// Pack admitted triangles without welding. Each triangle owns three vertices and three indices.
/// `slot_of` is read and not written. An omitted face contributes no vertices.
pub fn pack_observation_gpu(product: &ObservationProduct, slot_of: impl Fn(u32) -> u32) -> Result<ObservationGpuPack, String> {
    if product.triangles.is_empty() {
        return Err("the observation constructed no triangles to pack".into());
    }
    if product.triangles_discarded_after_construction != 0 {
        return Err("the observation discarded triangles after construction".into());
    }
    let packed_triangles = u32::try_from(product.triangles.len()).map_err(|_| "too many triangles")?;
    if packed_triangles != product.triangles_constructed {
        return Err("constructed and stored triangle counts disagree".into());
    }
    let packed_vertices = packed_triangles.saturating_mul(3);
    let packed_indices = packed_vertices;
    let index_format = if packed_vertices > u32::from(u16::MAX) { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 };
    let index_stride = u64::from(index_format.byte_size());
    let packed_vertex_bytes = u64::from(packed_vertices) * u64::from(OBSERVATION_VERTEX_STRIDE);
    let packed_index_bytes = u64::from(packed_indices) * index_stride;
    let expected_gpu_bytes = packed_vertex_bytes + packed_index_bytes;
    let mut bytes = Vec::with_capacity(packed_vertex_bytes as usize);
    let mut index_bytes = Vec::with_capacity(packed_index_bytes as usize);
    let mut originating_face_ids = Vec::with_capacity(product.triangles.len());
    let mut slots = Vec::with_capacity(product.triangles.len());
    let mut degenerate_normals = 0u32;
    for (triangle_index, triangle) in product.triangles.iter().enumerate() {
        if product.omitted_face_ids.contains(&triangle.face_id) || !product.admitted_face_ids.contains(&triangle.face_id) {
            return Err(format!("omitted face {} entered the pack", triangle.face_id));
        }
        let corners = [
            [triangle.positions[0][0] as f32, triangle.positions[0][1] as f32, triangle.positions[0][2] as f32],
            [triangle.positions[1][0] as f32, triangle.positions[1][1] as f32, triangle.positions[1][2] as f32],
            [triangle.positions[2][0] as f32, triangle.positions[2][1] as f32, triangle.positions[2][2] as f32],
        ];
        let (normal, degenerate) = packed_normal(corners[0], corners[1], corners[2]);
        if degenerate {
            degenerate_normals = degenerate_normals.saturating_add(1);
        }
        let base = (triangle_index as u32).saturating_mul(3);
        for corner in corners {
            push_vertex(&mut bytes, corner, normal);
        }
        for offset in 0..3 {
            push_index(&mut index_bytes, index_format, base + offset);
        }
        originating_face_ids.push(triangle.face_id);
        slots.push(slot_of(triangle.face_id));
    }
    if bytes.len() as u64 != packed_vertex_bytes || index_bytes.len() as u64 != packed_index_bytes {
        return Err("provenance bytes did not match the emitted buffers".into());
    }
    let mut submeshes = Vec::new();
    let mut run = 0usize;
    while run < slots.len() {
        let slot = slots[run];
        let start = run;
        while run < slots.len() && slots[run] == slot {
            run += 1;
        }
        submeshes.push(SubmeshDesc {
            first_index: (start as u32).saturating_mul(3),
            index_count: ((run - start) as u32).saturating_mul(3),
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: slot,
        });
    }
    let mesh = create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: OBSERVATION_VERTEX_STRIDE,
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
        index_bytes,
        submeshes,
    })
    .map_err(|error: MeshError| error.to_string())?;
    let stream_bytes = mesh.streams().first().map(|stream| stream.bytes.len()).unwrap_or(0) as u64;
    let mesh_index_bytes = mesh.index_bytes().len() as u64;
    if stream_bytes != packed_vertex_bytes || mesh_index_bytes != packed_index_bytes {
        return Err("the mesh buffers do not match the provenance total".into());
    }
    let mut accounts = Vec::new();
    let mut cursor = 0usize;
    while cursor < originating_face_ids.len() {
        let face_id = originating_face_ids[cursor];
        let start = cursor;
        while cursor < originating_face_ids.len() && originating_face_ids[cursor] == face_id {
            cursor += 1;
        }
        let cpu_triangles = product.triangles.iter().filter(|triangle| triangle.face_id == face_id).count() as u32;
        let packed = (cursor - start) as u32;
        if packed != cpu_triangles {
            return Err(format!("face {face_id} was split or dropped during pack"));
        }
        let vertices = packed.saturating_mul(3);
        let index_count = vertices;
        let vertex_bytes = u64::from(vertices) * u64::from(OBSERVATION_VERTEX_STRIDE);
        let index_bytes = u64::from(index_count) * index_stride;
        accounts.push(FaceGpuAccount {
            face_id,
            cpu_triangles,
            packed_triangles: packed,
            packed_vertices: vertices,
            index_count,
            vertex_bytes,
            index_bytes,
            total_gpu_bytes: vertex_bytes + index_bytes,
            first_vertex: (start as u32).saturating_mul(3),
            first_index: (start as u32).saturating_mul(3),
        });
    }
    let summed_vertices = accounts.iter().map(|account| account.vertex_bytes).sum::<u64>();
    let summed_indices = accounts.iter().map(|account| account.index_bytes).sum::<u64>();
    if summed_vertices != packed_vertex_bytes || summed_indices != packed_index_bytes {
        return Err("per-face accounts do not add up to the observation total".into());
    }
    let mut covered = vec![false; packed_vertices as usize];
    for account in &accounts {
        let start = account.first_vertex as usize;
        let end = start + account.packed_vertices as usize;
        if end > covered.len() {
            return Err("a face range runs past the vertex buffer".into());
        }
        for slot in &mut covered[start..end] {
            if *slot {
                return Err("face vertex ranges overlap".into());
            }
            *slot = true;
        }
    }
    if covered.iter().any(|slot| !*slot) {
        return Err("a packed vertex has no originating face".into());
    }
    let face_89 = accounts.iter().copied().find(|account| account.face_id == OBSERVATION_SPECIMEN_FACE).unwrap_or(FaceGpuAccount {
        face_id: OBSERVATION_SPECIMEN_FACE,
        cpu_triangles: 0,
        packed_triangles: 0,
        packed_vertices: 0,
        index_count: 0,
        vertex_bytes: 0,
        index_bytes: 0,
        total_gpu_bytes: 0,
        first_vertex: 0,
        first_index: 0,
    });
    Ok(ObservationGpuPack {
        mesh,
        accounts,
        originating_face_ids,
        constructed_triangles: product.triangles_constructed,
        packed_triangles,
        uploaded_triangles: packed_triangles,
        discarded_after_construction: product.triangles_discarded_after_construction,
        discarded_during_pack: 0,
        discarded_after_upload: 0,
        packed_vertices,
        packed_indices,
        packed_vertex_bytes,
        packed_index_bytes,
        expected_gpu_bytes,
        index_format,
        meshlets_constructed: 0,
        meshlet_gpu_bytes: 0,
        degenerate_normals,
        face_89,
        vertices_welded: false,
    })
}

pub fn format_observation_gpu(label: &str, pack: &ObservationGpuPack, upload: Option<ObservationUploadProof>) -> String {
    let mut out = String::new();
    let face = pack.face_89;
    out.push_str(&format!("{label} / face 89\n"));
    out.push_str(&format!("face_89_cpu_triangles: {}\n", face.cpu_triangles));
    out.push_str(&format!("face_89_packed_triangles: {}\n", face.packed_triangles));
    out.push_str(&format!("face_89_packed_vertices: {}\n", face.packed_vertices));
    out.push_str(&format!("face_89_index_count: {}\n", face.index_count));
    out.push_str(&format!("face_89_vertex_bytes: {}\n", face.vertex_bytes));
    out.push_str(&format!("face_89_index_bytes: {}\n", face.index_bytes));
    out.push_str(&format!("face_89_total_gpu_bytes: {}\n", face.total_gpu_bytes));
    out.push_str(&format!("{label} reconciliation\n"));
    out.push_str(&format!("constructed_triangles: {}\n", pack.constructed_triangles));
    out.push_str(&format!("packed_triangles: {}\n", pack.packed_triangles));
    out.push_str(&format!("uploaded_triangles: {}\n", pack.uploaded_triangles));
    out.push_str(&format!("discarded_after_construction: {}\n", pack.discarded_after_construction));
    out.push_str(&format!("discarded_during_pack: {}\n", pack.discarded_during_pack));
    out.push_str(&format!("discarded_after_upload: {}\n", pack.discarded_after_upload));
    out.push_str(&format!("packed_vertices: {}\n", pack.packed_vertices));
    out.push_str(&format!("packed_indices: {}\n", pack.packed_indices));
    out.push_str(&format!("packed_vertex_bytes: {}\n", pack.packed_vertex_bytes));
    out.push_str(&format!("packed_index_bytes: {}\n", pack.packed_index_bytes));
    out.push_str(&format!("expected_gpu_bytes: {}\n", pack.expected_gpu_bytes));
    out.push_str(&format!("index_format: {}\n", index_format_name(pack.index_format)));
    out.push_str(&format!("vertices_welded: {}\n", yes(pack.vertices_welded)));
    out.push_str(&format!("meshlets_constructed: {}\n", pack.meshlets_constructed));
    out.push_str(&format!("meshlet_gpu_bytes: {}\n", pack.meshlet_gpu_bytes));
    out.push_str(&format!("degenerate_normals_kept: {}\n", pack.degenerate_normals));
    match upload {
        Some(upload) => {
            out.push_str(&format!("GpuMesh.bytes: {}\n", upload.gpu_mesh_bytes));
            out.push_str(&format!("create_buffer_bytes: {}\n", upload.vertex_create_bytes + upload.index_create_bytes));
            out.push_str(&format!("vertex_create_bytes: {}\n", upload.vertex_create_bytes));
            out.push_str(&format!("index_create_bytes: {}\n", upload.index_create_bytes));
        }
        None => {
            out.push_str("GpuMesh.bytes: NOT UPLOADED\n");
            out.push_str("create_buffer_bytes: NOT UPLOADED\n");
        }
    }
    out
}

fn frozen_camera(eye: [f64; 3], forward: [f64; 3], fov: f64, width: f32) -> ObservationCamera {
    ObservationCamera {
        eye_local: eye,
        forward_local: forward,
        up_local: [0.0, 1.0, 0.0],
        vertical_fov_radians: fov,
        near_m: 0.1,
        viewport_width: width,
        viewport_height: FROZEN_OBSERVATION_HEIGHT,
        requested_error_px: 0.5,
    }
}

fn eye_text(eye: [f64; 3]) -> String {
    format!("[{:.4}, {:.4}, {:.4}]", eye[0], eye[1], eye[2])
}

fn forward_signs(forward: [f64; 3]) -> [i32; 3] {
    let forward = normalize(forward).unwrap_or(forward);
    let sign = |value: f64| if value > FORWARD_DEADZONE { 1 } else if value < -FORWARD_DEADZONE { -1 } else { 0 };
    [sign(forward[0]), sign(forward[1]), sign(forward[2])]
}

fn face_centroid(body: &SolidBody, face: u32) -> [f64; 3] {
    let Some(positions) = body.face_positions(face) else {
        return [0.0, 0.0, 0.0];
    };
    let scale_by = positions.len() as f64;
    let mut sum = [0.0; 3];
    for position in positions {
        sum[0] += position[0];
        sum[1] += position[1];
        sum[2] += position[2];
    }
    if scale_by == 0.0 { [0.0, 0.0, 0.0] } else { [sum[0] / scale_by, sum[1] / scale_by, sum[2] / scale_by] }
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
