//! Experiment 7. One persisted semantic round on one straight edge.
//!
//! The token is a semantic edge reference, not a surface-group name. Replay does not
//! read `size_m` and does not store the fillet as triangles. ADR-0074 stays Proposed.
//! `analytic-surface` is not this operation.
//!
//! [`realize_round_solid`] is the editor observation of that same round: the planar
//! replay, with one edge replaced by the fillet at the camera's arc division. The
//! mesh is not written onto the record.

use std::path::{Path, PathBuf};

use crate::mesh::{
    create_mesh, Mesh, MeshDesc, MeshError, MeshIndexFormat, MeshTopology, MeshVertexAttribute, MeshVertexFormat, SubmeshDesc, VertexStreamDesc,
};
use crate::microgeometry::projected_detail_px;
use crate::parametric::{BlockRecord, IntentEntry, IntentPayload};
use crate::topology::{SolidBody, TopologyPick};

pub const CURVE_RADIUS_M: f64 = 0.05;
pub const CURVE_EDITED_RADIUS_M: f64 = 0.04;
pub const CURVE_ERROR_PX: f32 = 0.5;
pub const CURVE_ERROR_SLACK_PX: f32 = 0.05;
pub const CURVE_N_CAP: u32 = 64;
pub const CURVE_FAR_MULTIPLE: f64 = 80.0;
pub const CURVE_CLOSE_MULTIPLE: f64 = 16.0;
pub const CURVE_VERTEX_STRIDE: u32 = 60;
pub const CURVE_VIEW_HEIGHT: f32 = 567.0;
pub const CURVE_FAR_WIDTH: f32 = 160.0;
pub const CURVE_CLOSE_WIDTH: f32 = 960.0;
pub const CURVE_FOV: f64 = f64::from_bits(0x3ff0_c152_382d_7365);
/// Fixed presentation distances. They do not scale with radius, so a radius edit changes the curve on screen.
pub const CURVE_PRESENT_FAR_M: f64 = 0.55;
pub const CURVE_PRESENT_CLOSE_M: f64 = 0.18;
pub const CURVE_PRESENT_LATERAL: f64 = 0.62;
pub const CURVE_PRESENT_NEAR_M: f64 = 0.02;
pub const CURVE_PRESENT_HEIGHT: f32 = 700.0;
pub const CURVE_PRESENT_FAR_WIDTH: f32 = 520.0;
pub const CURVE_PRESENT_CLOSE_WIDTH: f32 = 980.0;
const CORNER_WINDOW_RADII: f64 = 8.0;
const CORNER_WING_RADII: f64 = 4.0;
const CORNER_FIN_RADII: f64 = 0.11;
const CURVE_MAIN_LEVEL_BYTES: u64 = 26849;
const SOURCE_SOLID_UUID: &str = "ef5a0f8a-0eec-49db-b71e-40807a1e4f00";
const PROJECT_UUID: &str = "e7e7e7e7-e7e7-47e7-87e7-e7e7e7e7e7e7";
const LEVEL_UUID: &str = "e7e7e7e7-e7e7-47e7-87e7-e7e7e7e7e7e8";
const SOLID_UUID: &str = "e7e7e7e7-e7e7-47e7-87e7-e7e7e7e7e7ea";

/// Stable name of one round. It is the edge token, not a display string.
pub fn feature_id(token: &str) -> String {
    format!("round({token})")
}

/// One analytic fillet. The solid stored beside it is still the planar replay.
#[derive(Clone, Debug)]
pub struct Fillet {
    pub edge_id: u32,
    pub radius_m: f64,
    pub origin: [f64; 3],
    pub direction: [f64; 3],
    pub length_m: f64,
    pub normal0: [f64; 3],
    pub normal1: [f64; 3],
    pub inward0: [f64; 3],
    pub inward1: [f64; 3],
    pub center_offset: [f64; 3],
    pub tangent_m: f64,
    pub span: f64,
    pub axis: [f64; 3],
    pub wing: [f64; 3],
}

/// The fillet a tape resolved. Built without reading `size_m` or a stored body.
#[derive(Clone, Debug)]
pub struct ReplayedRound {
    pub token: String,
    pub edge_id: u32,
    pub radius_m: f64,
    pub fillet: Fillet,
    pub planar_extent: [f64; 3],
    pub cache_extent: [f64; 3],
}

/// Object-local camera aimed along the outward bisector at the arc center.
#[derive(Clone, Copy, Debug)]
pub struct RoundCamera {
    pub eye: [f64; 3],
    pub forward: [f64; 3],
    pub up: [f64; 3],
    pub vertical_fov_radians: f64,
    pub near_m: f64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub requested_error_px: f32,
}

/// One observation mesh. The strip product omits the planar faces. The corner product includes them.
/// Neither mesh is stored on the solid.
#[derive(Clone, Debug)]
pub struct RoundProduct {
    pub semantic_feature_id: String,
    pub semantic_edge_id: String,
    pub radius_m: f64,
    pub observation_id: String,
    pub requested_error_px: f32,
    pub measured_error_px: f32,
    pub chosen_arc_divisions: u32,
    pub vertices_constructed: u32,
    pub triangles_constructed: u32,
    pub triangles_discarded_after_construction: u32,
    pub cpu_bytes: u64,
    pub gpu_bytes: u64,
    pub position_hash: u64,
    pub cache_extent: [f64; 3],
    pub replay_consulted_size_m: bool,
    pub record_body_consulted: bool,
    pub object_mesh_consulted: bool,
    pub mesh: Mesh,
    pub packed_vertex_bytes: u64,
    pub packed_index_bytes: u64,
    pub index_format: MeshIndexFormat,
}

impl Fillet {
    pub fn outward_bisector(&self) -> Result<[f64; 3], String> {
        unit(add(self.normal0, self.normal1)).ok_or_else(|| "the outward bisector is degenerate".to_string())
    }

    pub fn arc_center(&self) -> [f64; 3] {
        add(add(self.origin, scale(self.direction, self.length_m * 0.5)), self.center_offset)
    }

    pub fn point(&self, along: f64, angle: f64) -> [f64; 3] {
        let edge = add(self.origin, scale(self.direction, along));
        let center = add(edge, self.center_offset);
        add(center, scale(self.radial(angle), self.radius_m))
    }

    fn radial(&self, angle: f64) -> [f64; 3] {
        let (sin, cos) = angle.sin_cos();
        add(scale(self.normal0, cos), scale(self.wing, sin))
    }

    pub fn analytic_bounds(&self) -> ([f64; 3], [f64; 3]) {
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        for along in [0.0, self.length_m] {
            for angle in self.extrema_angles() {
                let point = self.point(along, angle);
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
        (min, max)
    }

    fn extrema_angles(&self) -> Vec<f64> {
        let mut angles = vec![0.0, self.span];
        for axis in 0..3 {
            let cosine = self.normal0[axis];
            let sine = self.wing[axis];
            if cosine.abs() < 1.0e-15 && sine.abs() < 1.0e-15 {
                continue;
            }
            let angle = sine.atan2(cosine);
            for candidate in [angle, angle + std::f64::consts::PI, angle - std::f64::consts::PI] {
                if candidate > 1.0e-12 && candidate < self.span - 1.0e-12 {
                    angles.push(candidate);
                }
            }
        }
        angles
    }
}

/// Planar AABB, or that box union the fillet, as a size. No fillet returns [`SolidBody::aabb_size`].
pub fn cache_extent(body: &SolidBody, fillet: Option<&Fillet>) -> [f64; 3] {
    let Some(fillet) = fillet else {
        return body.aabb_size();
    };
    let Some((mut min, mut max)) = vertex_bounds(body) else {
        return body.aabb_size();
    };
    let (arc_min, arc_max) = fillet.analytic_bounds();
    for axis in 0..3 {
        min[axis] = min[axis].min(arc_min[axis]);
        max[axis] = max[axis].max(arc_max[axis]);
    }
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}

/// Fail closed unless `edge` is one straight manifold edge of two planar faces and `radius_m` fits.
pub fn validate_fillet(body: &SolidBody, edge: u32, radius_m: f64) -> Result<Fillet, String> {
    if !radius_m.is_finite() || radius_m <= 0.0 {
        return Err("round radius is not a finite positive length".into());
    }
    let (start_id, end_id) = body.edges.iter().find(|item| item.id == edge).map(|item| (item.a, item.b)).ok_or("the semantic edge is missing")?;
    if start_id == end_id {
        return Err("the source edge is degenerate".into());
    }
    let origin = body.vertex_position(start_id).ok_or("the source edge is degenerate")?;
    let end = body.vertex_position(end_id).ok_or("the source edge is degenerate")?;
    let direction = unit(sub(end, origin)).ok_or("the source edge is degenerate")?;
    let length_m = distance(origin, end);
    if length_m < 1.0e-8 {
        return Err("the source edge is degenerate".into());
    }
    let mut faces = body.faces_of_edge(edge);
    if faces.len() != 2 {
        return Err("the edge is not a manifold edge of exactly two faces".into());
    }
    faces.sort_unstable();
    let normal0 = body.unit_normal(faces[0]).ok_or("an adjacent face is not planar")?;
    let normal1 = body.unit_normal(faces[1]).ok_or("an adjacent face is not planar")?;
    if !face_is_planar(body, faces[0], normal0) || !face_is_planar(body, faces[1], normal1) {
        return Err("an adjacent face is not planar".into());
    }
    if dot(normal0, normal1).abs() > 0.999 {
        return Err("adjacent faces are coplanar".into());
    }
    let inward0 = inward_on_face(body, faces[0], start_id, end_id, direction)?;
    let inward1 = inward_on_face(body, faces[1], start_id, end_id, direction)?;
    let into0 = dot(inward1, normal0);
    let into1 = dot(inward0, normal1);
    if into0 > 1.0e-6 || into1 > 1.0e-6 {
        return Err("concave ambiguity".into());
    }
    if into0 > -1.0e-4 || into1 > -1.0e-4 {
        return Err("orientation is ambiguous".into());
    }
    if !step_is_inside_face(body, faces[0], origin, end, inward0) || !step_is_inside_face(body, faces[1], origin, end, inward1) {
        return Err("orientation is ambiguous".into());
    }
    let span = dot(normal0, normal1).clamp(-1.0, 1.0).acos();
    if !(1.0e-4..std::f64::consts::PI - 1.0e-4).contains(&span) {
        return Err("adjacent faces are coplanar".into());
    }
    let axis = unit(cross(normal0, normal1)).ok_or("orientation is ambiguous")?;
    let tangent_m = radius_m * (span * 0.5).tan();
    if !tangent_m.is_finite() || tangent_m <= 0.0 {
        return Err("the round radius does not fit the local wedge".into());
    }
    let center_from_0 = sub(scale(inward0, tangent_m), scale(normal0, radius_m));
    let center_from_1 = sub(scale(inward1, tangent_m), scale(normal1, radius_m));
    if distance(center_from_0, center_from_1) > 1.0e-6 {
        return Err("orientation is ambiguous".into());
    }
    let clearance0 = face_clearance(body, faces[0], origin, direction, length_m, inward0)?;
    let clearance1 = face_clearance(body, faces[1], origin, direction, length_m, inward1)?;
    if tangent_m >= clearance0 || tangent_m >= clearance1 {
        return Err("the round radius does not fit the local wedge".into());
    }
    if foreign_topology_crosses(body, edge, &faces, origin, direction, length_m, normal0, normal1, tangent_m) {
        return Err("the round crosses neighboring topology".into());
    }
    let wing = unit(cross(axis, normal0)).ok_or("orientation is ambiguous")?;
    Ok(Fillet {
        edge_id: edge,
        radius_m,
        origin,
        direction,
        length_m,
        normal0,
        normal1,
        inward0,
        inward1,
        center_offset: center_from_0,
        tangent_m,
        span,
        axis,
        wing,
    })
}

/// First semantic edge token, in lexicographic order, that accepts `radius_m`.
pub fn select_round_token(record: &BlockRecord, radius_m: f64) -> Result<String, String> {
    let bindings = crate::semantic_edge_bindings(record)?;
    let body = match crate::intent_authority_candidate(record) {
        crate::IntentAuthorityCandidate::Reconstructable(body) => body,
        crate::IntentAuthorityCandidate::Refused(reason) => return Err(reason),
    };
    let mut tokens: Vec<String> = bindings.iter().map(|(token, _)| token.clone()).collect();
    tokens.sort();
    tokens.dedup();
    let mut refusals = Vec::new();
    for token in tokens {
        let ids: Vec<u32> = bindings.iter().filter(|(name, _)| name == &token).map(|(_, id)| *id).collect();
        if ids.len() != 1 {
            refusals.push(format!("{token} ambiguous"));
            continue;
        }
        let fillet = match validate_fillet(&body, ids[0], radius_m) {
            Ok(fillet) => fillet,
            Err(reason) => {
                if refusals.len() < 8 {
                    refusals.push(format!("{token} {reason}"));
                }
                continue;
            }
        };
        let bisector = match fillet.outward_bisector() {
            Ok(bisector) => bisector,
            Err(reason) => {
                refusals.push(format!("{token} {reason}"));
                continue;
            }
        };
        if bisector[1].abs() >= 0.95 {
            refusals.push(format!("{token} bisector parallel to up"));
            continue;
        }
        return Ok(token);
    }
    Err(format!("no semantic edge accepted a round of {radius_m} m ({})", refusals.join("; ")))
}

pub fn round_entry(token: &str, radius_m: f64) -> IntentEntry {
    IntentEntry { groups: Some(vec![vec![token.to_string()]]), payload: IntentPayload::Round { radius_m } }
}

pub fn observation_camera(fillet: &Fillet, distance_m: f64, width: f32, height: f32, fov: f64, error_px: f32) -> Result<RoundCamera, String> {
    if !distance_m.is_finite() || distance_m <= fillet.radius_m {
        return Err("the observation distance does not see the round".into());
    }
    let bisector = fillet.outward_bisector()?;
    if bisector[1].abs() >= 0.95 {
        return Err("the outward bisector is parallel to world up".into());
    }
    let eye = add(fillet.arc_center(), scale(bisector, distance_m));
    Ok(RoundCamera {
        eye,
        forward: scale(bisector, -1.0),
        up: [0.0, 1.0, 0.0],
        vertical_fov_radians: fov,
        near_m: 0.1,
        viewport_width: width,
        viewport_height: height,
        requested_error_px: error_px,
    })
}

/// Choose the arc division from projected sagitta, then emit that strip. A rejected division allocates nothing.
pub fn realize_round_observation(record: &BlockRecord, camera: &RoundCamera, observation_id: &str, _revision: u64) -> Result<RoundProduct, String> {
    let replayed = crate::replay_round(record)?;
    let divisions = choose_arc_divisions(&replayed.fillet, camera)?;
    let measured = projected_sagitta_px(&replayed.fillet, camera, divisions)?;
    if measured > f64::from(camera.requested_error_px) + f64::from(CURVE_ERROR_SLACK_PX) {
        return Err(format!("measured error {measured} px exceeds the requested budget"));
    }
    let emitted = emit_strip(&replayed.fillet, divisions)?;
    if emitted.discarded != 0 {
        return Err("the round discarded triangles after construction".into());
    }
    let packed = pack_strip(&emitted)?;
    Ok(RoundProduct {
        semantic_feature_id: feature_id(&replayed.token),
        semantic_edge_id: replayed.token,
        radius_m: replayed.radius_m,
        observation_id: observation_id.to_string(),
        requested_error_px: camera.requested_error_px,
        measured_error_px: measured as f32,
        chosen_arc_divisions: divisions,
        vertices_constructed: emitted.vertices,
        triangles_constructed: emitted.triangles,
        triangles_discarded_after_construction: emitted.discarded,
        cpu_bytes: packed.cpu_bytes,
        gpu_bytes: packed.gpu_bytes,
        position_hash: packed.position_hash,
        cache_extent: replayed.cache_extent,
        replay_consulted_size_m: false,
        record_body_consulted: false,
        object_mesh_consulted: false,
        mesh: packed.mesh,
        packed_vertex_bytes: packed.vertex_bytes,
        packed_index_bytes: packed.index_bytes,
        index_format: packed.index_format,
    })
}

pub fn format_round_product(product: &RoundProduct) -> String {
    format!(
        "semantic_feature_id: {}\nsemantic_edge_id: {}\nradius: {:.5} m\nobservation_id: {}\nrequested_error_px: {:.3}\nmeasured_error_px: {:.6}\nchosen_arc_divisions: {}\nvertices_constructed: {}\ntriangles_constructed: {}\ntriangles_discarded_after_construction: {}\ncpu_bytes: {}\ngpu_bytes: {}\nposition_hash: {:016x}\nreplay_consulted_size_m: {}\nrecord_body_consulted: {}\nobject_mesh_consulted: {}\n",
        product.semantic_feature_id,
        product.semantic_edge_id,
        product.radius_m,
        product.observation_id,
        product.requested_error_px,
        product.measured_error_px,
        product.chosen_arc_divisions,
        product.vertices_constructed,
        product.triangles_constructed,
        product.triangles_discarded_after_construction,
        product.cpu_bytes,
        product.gpu_bytes,
        product.position_hash,
        no(product.replay_consulted_size_m),
        no(product.record_body_consulted),
        no(product.object_mesh_consulted),
    )
}

/// Three-quarter view of the arc. The eye stays a fixed distance from the fillet, independent of radius.
pub fn presentation_camera(fillet: &Fillet, distance_m: f64, width: f32, height: f32, fov: f64, error_px: f32) -> Result<RoundCamera, String> {
    if !distance_m.is_finite() || distance_m <= fillet.radius_m * 2.0 || width < 2.0 || height < 2.0 {
        return Err("the presentation camera does not frame the round".into());
    }
    let bisector = fillet.outward_bisector()?;
    if bisector[1].abs() >= 0.95 {
        return Err("the outward bisector is parallel to world up".into());
    }
    let center = fillet.arc_center();
    let eye = add(add(center, scale(bisector, distance_m)), scale(fillet.direction, distance_m * CURVE_PRESENT_LATERAL));
    let forward = unit(sub(center, eye)).ok_or_else(|| "the presentation camera sits on the arc".to_string())?;
    if dot(forward, [0.0, 1.0, 0.0]).abs() >= 0.98 {
        return Err("the presentation camera looks along world up".into());
    }
    Ok(RoundCamera {
        eye,
        forward,
        up: [0.0, 1.0, 0.0],
        vertical_fov_radians: fov,
        near_m: CURVE_PRESENT_NEAR_M,
        viewport_width: width,
        viewport_height: height,
        requested_error_px: error_px,
    })
}

/// The local corner with the sharp edge still in place. Observation only. Nothing is stored.
pub fn sharp_corner_mesh(fillet: &Fillet) -> Result<Mesh, String> {
    let (along0, along1, wing) = corner_window(fillet);
    let mut tris = Vec::new();
    push_wing(&mut tris, fillet, along0, along1, wing, true);
    pack_corner(&tris).map(|packed| packed.mesh)
}

/// Fins along the arc rulings and the wing outline. Observation only. Nothing is stored.
pub fn round_wire_mesh(fillet: &Fillet, divisions: u32) -> Result<Mesh, String> {
    if divisions == 0 || divisions > CURVE_N_CAP {
        return Err("wire divisions are outside the arc cap".into());
    }
    let (along0, along1, wing) = corner_window(fillet);
    let mut tris = Vec::new();
    push_arc_fins(&mut tris, fillet, along0, along1, divisions);
    push_wing_fins(&mut tris, fillet, along0, along1, wing);
    pack_corner(&tris).map(|packed| packed.mesh)
}

/// Choose arc divisions from the projected sagitta, then emit the trimmed faces, the arc, and the segment fins.
/// A rejected division allocates nothing. The mesh is not written onto the record.
pub fn realize_round_corner(record: &BlockRecord, camera: &RoundCamera, observation_id: &str, _revision: u64) -> Result<RoundProduct, String> {
    let replayed = crate::replay_round(record)?;
    let divisions = choose_arc_divisions(&replayed.fillet, camera)?;
    let measured = projected_sagitta_px(&replayed.fillet, camera, divisions)?;
    if measured > f64::from(camera.requested_error_px) + f64::from(CURVE_ERROR_SLACK_PX) {
        return Err(format!("measured error {measured} px exceeds the requested budget"));
    }
    let (along0, along1, wing) = corner_window(&replayed.fillet);
    let mut tris = Vec::new();
    push_wing(&mut tris, &replayed.fillet, along0, along1, wing, false);
    push_arc(&mut tris, &replayed.fillet, along0, along1, divisions);
    push_caps(&mut tris, &replayed.fillet, along0, along1, wing, divisions);
    push_arc_fins(&mut tris, &replayed.fillet, along0, along1, divisions);
    push_wing_fins(&mut tris, &replayed.fillet, along0, along1, wing);
    if tris.is_empty() {
        return Err("the corner constructed no triangles".into());
    }
    let packed = pack_corner(&tris)?;
    let vertices = u32::try_from(tris.len() * 3).map_err(|_| "too many corner vertices")?;
    let triangles = u32::try_from(tris.len()).map_err(|_| "too many corner triangles")?;
    Ok(RoundProduct {
        semantic_feature_id: feature_id(&replayed.token),
        semantic_edge_id: replayed.token,
        radius_m: replayed.radius_m,
        observation_id: observation_id.to_string(),
        requested_error_px: camera.requested_error_px,
        measured_error_px: measured as f32,
        chosen_arc_divisions: divisions,
        vertices_constructed: vertices,
        triangles_constructed: triangles,
        triangles_discarded_after_construction: 0,
        cpu_bytes: packed.cpu_bytes,
        gpu_bytes: packed.gpu_bytes,
        position_hash: packed.position_hash,
        cache_extent: replayed.cache_extent,
        replay_consulted_size_m: false,
        record_body_consulted: false,
        object_mesh_consulted: false,
        mesh: packed.mesh,
        packed_vertex_bytes: packed.vertex_bytes,
        packed_index_bytes: packed.index_bytes,
        index_format: packed.index_format,
    })
}

/// Arc division the observation error budget asks for. A rejected division allocates nothing.
pub fn arc_divisions(fillet: &Fillet, camera: &RoundCamera) -> Result<u32, String> {
    choose_arc_divisions(fillet, camera)
}

/// Planar cage segment while `fillet` is the observation.
///
/// The rounded edge returns `None` because the arc replaces it. A perpendicular end
/// moves onto the tangent, so the wire does not keep the sharp vertex.
pub fn round_cage_segment(body: &SolidBody, fillet: &Fillet, edge: u32) -> Option<([f64; 3], [f64; 3])> {
    let (start_id, end_id) = body.edges.iter().find(|item| item.id == edge).map(|item| (item.a, item.b))?;
    let mut start = body.vertex_position(start_id)?;
    let mut end = body.vertex_position(end_id)?;
    if edge == fillet.edge_id || (station_on_fillet(fillet, start).is_some() && station_on_fillet(fillet, end).is_some()) {
        return None;
    }
    let Ok((face0, face1, _, _)) = fillet_incident(body, fillet) else {
        return Some((start, end));
    };
    for (vertex, along, _) in endpoint_vertices(body, fillet) {
        if start_id == vertex || end_id == vertex {
            shift_spliced_end(body, fillet, edge, vertex, face0, face1, along, start_id, &mut start, end_id, &mut end);
        }
    }
    Some((start, end))
}

fn shift_spliced_end(
    body: &SolidBody,
    fillet: &Fillet,
    edge: u32,
    vertex: u32,
    face0: u32,
    face1: u32,
    along: f64,
    start_id: u32,
    start: &mut [f64; 3],
    end_id: u32,
    end: &mut [f64; 3],
) {
    if !end_is_spliced(body, vertex, face0, face1, fillet) {
        return;
    }
    let Some(tangent) = tangent_point(body, fillet, edge, along) else { return };
    if start_id == vertex {
        *start = tangent;
    }
    if end_id == vertex {
        *end = tangent;
    }
}

fn tangent_point(body: &SolidBody, fillet: &Fillet, edge: u32, along: f64) -> Option<[f64; 3]> {
    for face in body.faces_of_edge(edge) {
        let normal = body.unit_normal(face)?;
        if dot(normal, fillet.normal0) > 0.999 {
            return Some(fillet.point(along, 0.0));
        }
        if dot(normal, fillet.normal1) > 0.999 {
            return Some(fillet.point(along, fillet.span));
        }
    }
    None
}

fn fillet_incident(body: &SolidBody, fillet: &Fillet) -> Result<(u32, u32, u32, u32), String> {
    let (start_id, end_id) = body
        .edges
        .iter()
        .find(|item| item.id == fillet.edge_id)
        .map(|item| (item.a, item.b))
        .ok_or("the semantic edge is missing")?;
    let incidents = body.faces_of_edge(fillet.edge_id);
    if incidents.len() != 2 {
        return Err("the edge is not a manifold edge of exactly two faces".into());
    }
    let normal_a = body.unit_normal(incidents[0]).ok_or("an adjacent face is not planar")?;
    let (face0, face1) = if dot(normal_a, fillet.normal0) > 0.999 {
        (incidents[0], incidents[1])
    } else if dot(normal_a, fillet.normal1) > 0.999 {
        (incidents[1], incidents[0])
    } else {
        return Err("the round lost its adjacent faces".into());
    };
    let normal_b = body.unit_normal(face1).ok_or("an adjacent face is not planar")?;
    if dot(normal_b, fillet.normal1) <= 0.999 {
        return Err("the round lost its adjacent faces".into());
    }
    Ok((face0, face1, start_id, end_id))
}

fn end_is_spliced(body: &SolidBody, vertex: u32, face0: u32, face1: u32, fillet: &Fillet) -> bool {
    let Some(normal0) = body.unit_normal(face0) else { return false };
    let Some(normal1) = body.unit_normal(face1) else { return false };
    let extras: Vec<u32> = body.faces_of_vertex(vertex).into_iter().filter(|face| {
        let Some(normal) = body.unit_normal(*face) else { return true };
        !normals_agree(normal, normal0) && !normals_agree(normal, normal1)
    }).collect();
    if extras.len() != 1 {
        return false;
    }
    let Some(normal) = body.unit_normal(extras[0]) else { return false };
    dot(normal, fillet.direction).abs() > 0.999
}

/// Whether one edge can be drawn as a closed fillet. A shared corner returns the reason and writes nothing.
pub fn round_edge_closes(body: &SolidBody, edge: u32, radius_m: f64) -> Result<(), String> {
    let fillet = validate_fillet(body, edge, radius_m)?;
    let _ = emit_round_solid(body, &fillet, 4)?;
    Ok(())
}

/// The whole planar solid with one edge replaced by the fillet.
///
/// A perpendicular end face receives the arc. Any other open end is a disk in the
/// fillet plane. A shared corner that has more than one extra face returns `Err`
/// and builds nothing. The record is not modified.
pub fn realize_round_solid(record: &BlockRecord, camera: &RoundCamera, observation_id: &str, _revision: u64) -> Result<RoundProduct, String> {
    let rounds = crate::replay_rounds(record)?;
    let body = match crate::intent_authority_candidate(record) {
        crate::IntentAuthorityCandidate::Reconstructable(body) => body,
        crate::IntentAuthorityCandidate::Refused(reason) => return Err(reason),
    };
    let mut divisions = 1u32;
    let mut measured = 0.0f64;
    for round in &rounds {
        let chosen = choose_arc_divisions(&round.fillet, camera)?;
        let error = projected_sagitta_px(&round.fillet, camera, chosen)?;
        if error > f64::from(camera.requested_error_px) + f64::from(CURVE_ERROR_SLACK_PX) {
            return Err(format!("measured error {error} px exceeds the requested budget"));
        }
        divisions = divisions.max(chosen);
        measured = measured.max(error);
    }
    let fillets: Vec<_> = rounds.iter().map(|round| round.fillet.clone()).collect();
    let tris = emit_fillet_set(&body, &fillets, divisions)?;
    let packed = pack_corner(&tris)?;
    let vertices = u32::try_from(tris.len().saturating_mul(3)).map_err(|_| "too many round vertices")?;
    let triangles = u32::try_from(tris.len()).map_err(|_| "too many round triangles")?;
    let mut tokens: Vec<_> = rounds.iter().map(|round| round.token.clone()).collect();
    tokens.sort();
    let semantic_edge_id = tokens.join(" ");
    let first = &rounds[0];
    Ok(RoundProduct {
        semantic_feature_id: feature_id(&semantic_edge_id),
        semantic_edge_id,
        radius_m: first.radius_m,
        observation_id: observation_id.to_string(),
        requested_error_px: camera.requested_error_px,
        measured_error_px: measured as f32,
        chosen_arc_divisions: divisions,
        vertices_constructed: vertices,
        triangles_constructed: triangles,
        triangles_discarded_after_construction: 0,
        cpu_bytes: packed.cpu_bytes,
        gpu_bytes: packed.gpu_bytes,
        position_hash: packed.position_hash,
        cache_extent: first.cache_extent,
        replay_consulted_size_m: false,
        record_body_consulted: false,
        object_mesh_consulted: false,
        mesh: packed.mesh,
        packed_vertex_bytes: packed.vertex_bytes,
        packed_index_bytes: packed.index_bytes,
        index_format: packed.index_format,
    })
}

/// The same fillet at another radius. The edge, the span, and the face normals stay.
///
/// `center_offset` and `tangent_m` scale with the radius. This does not read a record
/// and does not replay an intent tape.
pub fn fillet_scaled(fillet: &Fillet, radius_m: f64) -> Result<Fillet, String> {
    if !radius_m.is_finite() || radius_m <= 0.0 || !fillet.radius_m.is_finite() || fillet.radius_m <= 0.0 {
        return Err("round radius is not a finite positive length".into());
    }
    let factor = radius_m / fillet.radius_m;
    let mut scaled = fillet.clone();
    scaled.radius_m = radius_m;
    scaled.tangent_m = fillet.tangent_m * factor;
    scaled.center_offset = scale(fillet.center_offset, factor);
    Ok(scaled)
}

/// How far the arc crown sits from the sharp edge, per meter of radius, and the direction of that motion.
///
/// The direction points from the sharp edge toward the crown. Motion along it increases the radius.
pub fn fillet_crown_step(fillet: &Fillet) -> Result<(f64, [f64; 3]), String> {
    let sharp = add(fillet.origin, scale(fillet.direction, fillet.length_m * 0.5));
    let crown = fillet.point(fillet.length_m * 0.5, fillet.span * 0.5);
    let delta = sub(crown, sharp);
    let span = distance(delta, [0.0; 3]);
    if !span.is_finite() || span <= 1.0e-12 || fillet.radius_m <= 0.0 {
        return Err("the fillet crown does not leave the sharp edge".into());
    }
    let away = unit(delta).ok_or("the fillet crown does not leave the sharp edge")?;
    Ok((span / fillet.radius_m, away))
}

/// Largest radius at which `edge` still closes. Smaller radii inside the result close too.
///
/// The search uses the body it is given. It does not replay an intent tape.
pub fn maximum_round_radius(body: &SolidBody, edge: u32) -> Result<f64, String> {
    let seed = 0.001;
    round_edge_closes(body, edge, seed)?;
    let mut low = seed;
    let mut high = seed;
    while high < 1000.0 {
        let next = (high * 2.0).min(1000.0);
        if next <= high {
            break;
        }
        if round_edge_closes(body, edge, next).is_err() {
            high = next;
            break;
        }
        low = next;
        high = next;
        if next >= 1000.0 {
            return Ok(next);
        }
    }
    if round_edge_closes(body, edge, high).is_ok() {
        return Ok(high);
    }
    for _ in 0..24 {
        let mid = 0.5 * (low + high);
        if round_edge_closes(body, edge, mid).is_ok() {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(low)
}

/// Radius after one drag step along the crown direction.
///
/// `along_m` is the signed distance from the drag start, toward the side that leaves the sharp edge.
/// `crown_per_radius` is the crown distance divided by the radius. `fine` keeps one tenth of that motion.
/// `snap_m` rounds the result. The result stays inside `minimum_m` and `maximum_m`.
pub fn dragged_round_radius(origin_m: f64, along_m: f64, crown_per_radius: f64, fine: bool, snap_m: Option<f64>, minimum_m: f64, maximum_m: f64) -> f64 {
    let step = if crown_per_radius.is_finite() && crown_per_radius > 1.0e-8 { crown_per_radius } else { 1.0 };
    let mut motion = if along_m.is_finite() { along_m / step } else { 0.0 };
    if fine {
        motion *= 0.1;
    }
    let mut radius = if origin_m.is_finite() { origin_m + motion } else { minimum_m };
    if let Some(snap) = snap_m.filter(|snap| snap.is_finite() && *snap > 0.0) {
        radius = (radius / snap).round() * snap;
    } else {
        radius = (radius / 0.0001).round() * 0.0001;
    }
    let floor = if minimum_m.is_finite() && minimum_m > 0.0 { minimum_m } else { 0.001 };
    let ceiling = if maximum_m.is_finite() { maximum_m.max(floor) } else { floor };
    radius.clamp(floor, ceiling)
}

/// Observation mesh for a fillet that is already resolved.
///
/// The body and the fillet are the inputs. This does not read a record, a stored body, or an intent tape.
pub fn observe_resolved_fillet(body: &SolidBody, fillet: &Fillet, camera: &RoundCamera) -> Result<(Mesh, u32), String> {
    observe_resolved_fillets(body, std::slice::from_ref(fillet), camera)
}

/// One observation of every fillet. Arc divisions are the finest the camera asks for, and they are not stored.
pub fn observe_resolved_fillets(body: &SolidBody, fillets: &[Fillet], camera: &RoundCamera) -> Result<(Mesh, u32), String> {
    if fillets.is_empty() {
        return Err("the tape has no round".into());
    }
    let mut divisions = 1u32;
    for fillet in fillets {
        let chosen = choose_arc_divisions(fillet, camera)?;
        let measured = projected_sagitta_px(fillet, camera, chosen)?;
        if measured > f64::from(camera.requested_error_px) + f64::from(CURVE_ERROR_SLACK_PX) {
            return Err(format!("measured error {measured} px exceeds the requested budget"));
        }
        divisions = divisions.max(chosen);
    }
    let tris = emit_fillet_set(body, fillets, divisions)?;
    let packed = pack_corner(&tris)?;
    Ok((packed.mesh, divisions))
}

/// Tokens of one round operation. Each group is one semantic edge. An empty group and a repeated token are refused.
pub fn round_group_tokens(groups: Option<&[Vec<String>]>) -> Result<Vec<String>, ()> {
    let Some(groups) = groups else { return Err(()) };
    if groups.is_empty() {
        return Err(());
    }
    let mut tokens = Vec::new();
    for group in groups {
        let [token] = group.as_slice() else { return Err(()) };
        if !token.starts_with("E:") || tokens.iter().any(|have: &String| have == token) {
            return Err(());
        }
        tokens.push(token.clone());
    }
    Ok(tokens)
}

/// One round operation. The groups are the semantic edge set and the radius is shared.
pub fn round_entry_set(tokens: &[String], radius_m: f64) -> IntentEntry {
    let mut ordered = tokens.to_vec();
    ordered.sort();
    ordered.dedup();
    IntentEntry {
        groups: Some(ordered.into_iter().map(|token| vec![token]).collect()),
        payload: IntentPayload::Round { radius_m },
    }
}

/// One analytic fillet plus the concrete chain it covers. The chain is derived. The token is what is saved.
#[derive(Clone, Debug)]
pub struct RoundFeature {
    pub token: String,
    pub fillet: Fillet,
    pub chain: Vec<u32>,
}

/// Whether a requested radius can be authored on `selected` beside `kept`.
#[derive(Clone, Debug)]
pub enum RoundAssessment {
    /// `clamped` is set when the authored radius is below the request.
    Ready { radius_m: f64, maximum_m: f64, clamped: bool },
    Conflict { tokens: Vec<String>, reason: String },
}

/// Maximal colinear chain that contains `edge`. Fragments of one straight side come back together.
pub fn logical_edge_chain(body: &SolidBody, edge: u32) -> Result<Vec<u32>, String> {
    let (origin, direction, length) = edge_frame(body, edge)?;
    let normal_pair = edge_normals(body, edge)?;
    let mut chain = vec![edge];
    for sign in [-1.0, 1.0] {
        let mut cursor = edge;
        let mut from = if sign < 0.0 { origin } else { add(origin, scale(direction, length)) };
        for _ in 0..64 {
            let Some(next) = colinear_continuation(body, cursor, from, direction, normal_pair) else { break };
            if chain.contains(&next) {
                break;
            }
            if sign < 0.0 {
                chain.insert(0, next);
            } else {
                chain.push(next);
            }
            let (next_origin, next_direction, next_length) = edge_frame(body, next)?;
            let far = add(next_origin, scale(next_direction, next_length));
            from = if distance(next_origin, from) > distance(far, from) { next_origin } else { far };
            cursor = next;
        }
    }
    Ok(chain)
}

/// The semantic edge a picked fragment belongs to. A seed name wins over a grid or split fragment.
pub fn persistent_edge_token(body: &SolidBody, bindings: &[(String, u32)], edge: u32) -> Result<String, String> {
    let chain = logical_edge_chain(body, edge)?;
    let mut tokens: Vec<&str> = bindings.iter().filter(|(_, id)| chain.contains(id)).map(|(token, _)| token.as_str()).collect();
    if tokens.is_empty() {
        return Err("That edge has no semantic name.".into());
    }
    tokens.sort_by_key(|token| persistent_rank(token));
    tokens.dedup();
    Ok(tokens[0].to_string())
}

fn persistent_rank(token: &str) -> (u8, usize, &str) {
    let class = if token.contains("seed-edge/") {
        0
    } else if token.contains("split-") {
        2
    } else {
        1
    };
    (class, token.len(), token)
}

/// The fillet of one logical edge. Several colinear fragments become one fillet.
pub fn fillet_for_chain(body: &SolidBody, chain: &[u32], radius_m: f64) -> Result<Fillet, String> {
    if chain.len() == 1 {
        return validate_fillet(body, chain[0], radius_m);
    }
    let (start, end) = chain_endpoints(body, chain)?;
    let origin = body.vertex_position(start).ok_or("the source edge is degenerate")?;
    let end_point = body.vertex_position(end).ok_or("the source edge is degenerate")?;
    let direction = unit(sub(end_point, origin)).ok_or("the source edge is degenerate")?;
    let length_m = distance(origin, end_point);
    if length_m < 1.0e-8 {
        return Err("the source edge is degenerate".into());
    }
    let sample_edge = *chain.iter().max_by(|left, right| edge_length(body, **left).partial_cmp(&edge_length(body, **right)).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(&chain[0]);
    let mut probe = radius_m.min(0.001).max(1.0e-6);
    let mut sample = None;
    for _ in 0..12 {
        if let Ok(fillet) = validate_fillet(body, sample_edge, probe) {
            sample = Some(fillet);
            break;
        }
        probe *= 0.1;
    }
    let sample = sample.ok_or_else(|| "the round radius does not fit the local wedge".to_string())?;
    let mut fillet = fillet_scaled(&sample, radius_m)?;
    fillet.origin = origin;
    fillet.direction = direction;
    fillet.length_m = length_m;
    fillet.edge_id = chain[0];
    if fillet.tangent_m >= length_m {
        return Err("the round radius does not fit the local wedge".into());
    }
    let clearance0 = surface_clearance(body, fillet.normal0, origin, direction, length_m, fillet.inward0)?;
    let clearance1 = surface_clearance(body, fillet.normal1, origin, direction, length_m, fillet.inward1)?;
    if fillet.tangent_m >= clearance0 || fillet.tangent_m >= clearance1 {
        return Err("the round radius does not fit the local wedge".into());
    }
    Ok(fillet)
}

/// Largest shared radius at which `selected` can be authored beside `kept`.
pub fn assess_round_set(body: &SolidBody, kept: &[RoundFeature], selected: &[RoundFeature], requested_m: f64) -> RoundAssessment {
    if selected.is_empty() {
        return RoundAssessment::Conflict { tokens: Vec::new(), reason: "Round needs a semantic edge.".into() };
    }
    let mut locked: Option<f64> = None;
    let mut lock_tokens: Vec<String> = Vec::new();
    for feature in selected {
        for other in kept {
            if !features_share_corner(body, feature, other) {
                continue;
            }
            match locked {
                Some(radius) if (radius - other.fillet.radius_m).abs() > 1.0e-6 => {
                    let mut tokens = vec![feature.token.clone(), other.token.clone()];
                    tokens.extend(lock_tokens.iter().cloned());
                    tokens.sort();
                    tokens.dedup();
                    return RoundAssessment::Conflict {
                        tokens,
                        reason: format!(
                            "{} and {} meet at a corner. Their radii are {:.3} m and {:.3} m.",
                            feature.token, other.token, feature.fillet.radius_m, other.fillet.radius_m
                        ),
                    };
                }
                Some(_) => lock_tokens.push(other.token.clone()),
                None => {
                    locked = Some(other.fillet.radius_m);
                    lock_tokens.push(other.token.clone());
                }
            }
        }
    }
    let mut target = if requested_m.is_finite() { requested_m.max(0.001) } else { 0.001 };
    let mut clamped = false;
    if let Some(lock) = locked {
        if (target - lock).abs() > 1.0e-4 {
            target = lock;
            clamped = true;
        }
    }
    let basis: Vec<Fillet> = selected.iter().map(|feature| feature.fillet.clone()).collect();
    let kept_fillets: Vec<Fillet> = kept.iter().map(|feature| feature.fillet.clone()).collect();
    let closes = |radius: f64| -> bool {
        let mut live = Vec::new();
        for fillet in &basis {
            let Ok(scaled) = fillet_scaled(fillet, radius) else { return false };
            live.push(scaled);
        }
        live.extend(kept_fillets.iter().cloned());
        emit_fillet_set(body, &live, 4).is_ok()
    };
    if let Some(lock) = locked {
        if !closes(lock) {
            let tokens = conflict_tokens(selected, kept);
            return RoundAssessment::Conflict {
                tokens,
                reason: format!("The neighboring round is {lock:.3} m, and that radius does not fit these edges."),
            };
        }
        let maximum = lock;
        let radius = target.min(maximum).max(0.001);
        return RoundAssessment::Ready { radius_m: radius, maximum_m: maximum, clamped: clamped || radius + 1.0e-9 < requested_m };
    }
    // A two-fillet corner can miss its apex at 0.001 m and still close at the requested radius.
    // Three equal-radius fillets at a convex corner close. A set that cannot close stays a conflict.
    let mut low = 0.001;
    if !closes(low) {
        let mut found = if (target - low).abs() > 1.0e-12 && closes(target) { Some(target) } else { None };
        if found.is_none() {
            let mut cursor = low;
            while cursor < 1000.0 {
                let next = (cursor * 2.0).min(1000.0);
                if next <= cursor {
                    break;
                }
                if closes(next) {
                    found = Some(next);
                    break;
                }
                cursor = next;
            }
        }
        let Some(opening) = found else {
            let tokens = conflict_tokens(selected, kept);
            return RoundAssessment::Conflict { tokens, reason: "Those edges cannot share a round.".into() };
        };
        low = opening;
    }
    let floor = low;
    let mut high = low;
    while high < 1000.0 {
        let next = (high * 2.0).min(1000.0);
        if next <= high {
            break;
        }
        if !closes(next) {
            high = next;
            break;
        }
        low = next;
        high = next;
        if next >= 1000.0 {
            break;
        }
    }
    let maximum = if closes(high) {
        high
    } else {
        for _ in 0..24 {
            let mid = 0.5 * (low + high);
            if closes(mid) {
                low = mid;
            } else {
                high = mid;
            }
        }
        low
    };
    let mut radius = target.min(maximum).max(floor);
    if !closes(radius) {
        radius = floor;
    }
    let moved = radius + 1.0e-9 < requested_m || radius > requested_m + 1.0e-9;
    RoundAssessment::Ready { radius_m: radius, maximum_m: maximum, clamped: clamped || moved }
}

fn conflict_tokens(selected: &[RoundFeature], kept: &[RoundFeature]) -> Vec<String> {
    let mut tokens: Vec<String> = selected.iter().map(|feature| feature.token.clone()).collect();
    tokens.extend(kept.iter().map(|feature| feature.token.clone()));
    tokens.sort();
    tokens.dedup();
    tokens
}

fn features_share_corner(body: &SolidBody, left: &RoundFeature, right: &RoundFeature) -> bool {
    let mut shared = false;
    for (vertex, _, _) in endpoint_vertices(body, &left.fillet) {
        for (other, _, _) in endpoint_vertices(body, &right.fillet) {
            if vertex == other {
                shared = true;
            }
        }
    }
    shared
}

/// Union of the planar bounds and every fillet. No fillet returns the planar size.
pub fn cache_extent_many(body: &SolidBody, fillets: &[Fillet]) -> [f64; 3] {
    if fillets.is_empty() {
        return body.aabb_size();
    }
    let Some((mut min, mut max)) = vertex_bounds(body) else {
        return body.aabb_size();
    };
    for fillet in fillets {
        let (arc_min, arc_max) = fillet.analytic_bounds();
        for axis in 0..3 {
            min[axis] = min[axis].min(arc_min[axis]);
            max[axis] = max[axis].max(arc_max[axis]);
        }
    }
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}

fn edge_length(body: &SolidBody, edge: u32) -> f64 {
    edge_frame(body, edge).map(|(_, _, length)| length).unwrap_or(0.0)
}

fn edge_frame(body: &SolidBody, edge: u32) -> Result<([f64; 3], [f64; 3], f64), String> {
    let (start, end) = body.edges.iter().find(|item| item.id == edge).map(|item| (item.a, item.b)).ok_or("the semantic edge is missing")?;
    let origin = body.vertex_position(start).ok_or("the source edge is degenerate")?;
    let stop = body.vertex_position(end).ok_or("the source edge is degenerate")?;
    let direction = unit(sub(stop, origin)).ok_or("the source edge is degenerate")?;
    let length = distance(origin, stop);
    if length < 1.0e-8 {
        return Err("the source edge is degenerate".into());
    }
    Ok((origin, direction, length))
}

fn edge_normals(body: &SolidBody, edge: u32) -> Result<[[f64; 3]; 2], String> {
    let faces = body.faces_of_edge(edge);
    if faces.len() != 2 {
        return Err("the edge is not a manifold edge of exactly two faces".into());
    }
    let normal0 = body.unit_normal(faces[0]).ok_or("an adjacent face is not planar")?;
    let normal1 = body.unit_normal(faces[1]).ok_or("an adjacent face is not planar")?;
    Ok([normal0, normal1])
}

fn normals_agree(left: [f64; 3], right: [f64; 3]) -> bool {
    dot(left, right) > 0.999
}

fn same_normal_pair(left: [[f64; 3]; 2], right: [[f64; 3]; 2]) -> bool {
    (normals_agree(left[0], right[0]) && normals_agree(left[1], right[1])) || (normals_agree(left[0], right[1]) && normals_agree(left[1], right[0]))
}

fn vertex_near(body: &SolidBody, point: [f64; 3]) -> Option<u32> {
    let mut best: Option<(u32, f64)> = None;
    for vertex in &body.vertices {
        let gap = distance(vertex.position, point);
        if gap > 1.0e-4 {
            continue;
        }
        if best.is_none_or(|(_, previous)| gap < previous) {
            best = Some((vertex.id, gap));
        }
    }
    best.map(|(id, _)| id)
}

fn colinear_continuation(body: &SolidBody, cursor: u32, from: [f64; 3], direction: [f64; 3], normals: [[f64; 3]; 2]) -> Option<u32> {
    let vertex = vertex_near(body, from)?;
    let mut found = None;
    for edge in &body.edges {
        if edge.id == cursor || (edge.a != vertex && edge.b != vertex) {
            continue;
        }
        let Ok((origin, next_direction, length)) = edge_frame(body, edge.id) else { continue };
        if dot(next_direction, direction).abs() < 1.0 - 1.0e-6 {
            continue;
        }
        let far = add(origin, scale(next_direction, length));
        let on_line = |point: [f64; 3]| {
            let delta = sub(point, from);
            let axial = dot(delta, direction);
            distance(delta, scale(direction, axial)) <= 1.0e-4
        };
        if !on_line(origin) || !on_line(far) {
            continue;
        }
        let Ok(next_normals) = edge_normals(body, edge.id) else { continue };
        if !same_normal_pair(next_normals, normals) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(edge.id);
    }
    found
}

fn chain_endpoints(body: &SolidBody, chain: &[u32]) -> Result<(u32, u32), String> {
    if chain.is_empty() {
        return Err("the semantic edge is missing".into());
    }
    let vertices = |edge: u32| -> Result<(u32, u32), String> {
        body.edges.iter().find(|item| item.id == edge).map(|item| (item.a, item.b)).ok_or_else(|| "the semantic edge is missing".to_string())
    };
    if chain.len() == 1 {
        return vertices(chain[0]);
    }
    let (first_a, first_b) = vertices(chain[0])?;
    let (second_a, second_b) = vertices(chain[1])?;
    let start = if first_a != second_a && first_a != second_b { first_a } else { first_b };
    let (last_a, last_b) = vertices(*chain.last().unwrap())?;
    let (prev_a, prev_b) = vertices(chain[chain.len() - 2])?;
    let end = if last_a != prev_a && last_a != prev_b { last_a } else { last_b };
    Ok((start, end))
}

fn endpoint_vertices(body: &SolidBody, fillet: &Fillet) -> Vec<(u32, f64, f64)> {
    let end = edge_point(fillet, fillet.length_m);
    let mut found = Vec::new();
    for vertex in &body.vertices {
        let start_gap = distance(vertex.position, fillet.origin);
        let end_gap = distance(vertex.position, end);
        if start_gap <= 1.0e-4 {
            found.push((vertex.id, 0.0, start_gap));
        } else if end_gap <= 1.0e-4 {
            found.push((vertex.id, fillet.length_m, end_gap));
        }
    }
    found.sort_by(|left, right| left.2.partial_cmp(&right.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept = Vec::new();
    for (id, along, gap) in found {
        if kept.iter().any(|(have, have_along, _): &(u32, f64, f64)| *have == id || (*have_along - along).abs() < 1.0e-6) {
            continue;
        }
        kept.push((id, along, gap));
    }
    kept
}

fn station_on_fillet(fillet: &Fillet, point: [f64; 3]) -> Option<f64> {
    let along = dot(sub(point, fillet.origin), fillet.direction);
    if along < -1.0e-4 || along > fillet.length_m + 1.0e-4 {
        return None;
    }
    let clamped = along.clamp(0.0, fillet.length_m);
    let closest = add(fillet.origin, scale(fillet.direction, clamped));
    (distance(closest, point) <= 1.0e-4).then_some(clamped)
}

fn angle_for_normal(fillet: &Fillet, normal: [f64; 3]) -> Option<f64> {
    if normals_agree(fillet.normal0, normal) {
        Some(0.0)
    } else if normals_agree(fillet.normal1, normal) {
        Some(fillet.span)
    } else {
        None
    }
}

fn surface_clearance(body: &SolidBody, normal: [f64; 3], origin: [f64; 3], direction: [f64; 3], length_m: f64, inward: [f64; 3]) -> Result<f64, String> {
    let region: Vec<u32> = body.faces.iter().filter(|face| body.unit_normal(face.id).is_some_and(|face_normal| normals_agree(face_normal, normal))).map(|face| face.id).collect();
    let mut clearance = f64::MAX;
    let mut hits = 0u32;
    for edge in &body.edges {
        let incident = body.faces_of_edge(edge.id);
        let inside = incident.iter().filter(|face| region.contains(face)).count();
        if inside == 0 || (inside == incident.len() && incident.len() == 2) {
            continue;
        }
        let (Some(start), Some(stop)) = (body.vertex_position(edge.a), body.vertex_position(edge.b)) else {
            return Err("an adjacent face is missing".into());
        };
        if station_on_fillet_line(origin, direction, length_m, start) && station_on_fillet_line(origin, direction, length_m, stop) {
            continue;
        }
        let (u0, v0) = project(origin, direction, inward, start);
        let (u1, v1) = project(origin, direction, inward, stop);
        if let Some(height) = min_positive_v(u0, v0, u1, v1, length_m) {
            hits = hits.saturating_add(1);
            clearance = clearance.min(height);
        }
    }
    if hits == 0 || !clearance.is_finite() {
        return Err("the adjacent face has no measurable clearance".into());
    }
    Ok(clearance)
}

fn station_on_fillet_line(origin: [f64; 3], direction: [f64; 3], length_m: f64, point: [f64; 3]) -> bool {
    let along = dot(sub(point, origin), direction);
    if along < -1.0e-4 || along > length_m + 1.0e-4 {
        return false;
    }
    let closest = add(origin, scale(direction, along.clamp(0.0, length_m)));
    distance(closest, point) <= 1.0e-4
}

struct CornerPatch {
    vertex: u32,
    left: usize,
    right: usize,
    left_along: f64,
    right_along: f64,
    center: [f64; 3],
    radius: f64,
    shared_normal: [f64; 3],
    t_shared: [f64; 3],
    t_left: [f64; 3],
    t_right: [f64; 3],
    apex: [f64; 3],
    pie_normal: [f64; 3],
}

struct FreeEnd {
    fillet: usize,
    vertex: u32,
    along: f64,
    spliced: bool,
}

/// Three fillets of one radius meeting at a convex vertex. The patch is the sphere
/// between the three touch points. It has no pie and it does not keep the sharp vertex.
struct TrihedralPatch {
    vertex: u32,
    members: [(usize, f64); 3],
    center: [f64; 3],
    radius: f64,
    /// Outward face normal, then the point where the sphere touches that face.
    touches: [([f64; 3], [f64; 3]); 3],
}

struct FilletClose {
    patches: Vec<CornerPatch>,
    trihedrals: Vec<TrihedralPatch>,
    free_ends: Vec<FreeEnd>,
}

fn fillet_is_simple_edge(body: &SolidBody, fillet: &Fillet) -> bool {
    if !fillet_matches_current_edge(body, fillet) {
        return false;
    }
    let Ok((face0, face1, start, end)) = fillet_incident(body, fillet) else { return false };
    for vertex in [start, end] {
        for face in body.faces_of_vertex(vertex) {
            let Some(normal) = body.unit_normal(face) else { return false };
            let sibling = (normals_agree(normal, fillet.normal0) && face != face0) || (normals_agree(normal, fillet.normal1) && face != face1);
            if sibling {
                return false;
            }
        }
    }
    true
}

fn fillet_matches_current_edge(body: &SolidBody, fillet: &Fillet) -> bool {
    let Some(edge) = body.edges.iter().find(|item| item.id == fillet.edge_id) else { return false };
    let (Some(start), Some(stop)) = (body.vertex_position(edge.a), body.vertex_position(edge.b)) else { return false };
    if (distance(start, stop) - fillet.length_m).abs() > 1.0e-5 {
        return false;
    }
    let origin_gap = distance(start, fillet.origin).min(distance(stop, fillet.origin));
    let end = edge_point(fillet, fillet.length_m);
    let end_gap = distance(start, end).min(distance(stop, end));
    origin_gap < 1.0e-4 && end_gap < 1.0e-4
}

fn parallel_fillets_crowd(left: &Fillet, right: &Fillet) -> bool {
    if dot(left.direction, right.direction).abs() < 0.999 {
        return false;
    }
    let Some(shared) = [left.normal0, left.normal1].into_iter().find(|normal| normals_agree(*normal, right.normal0) || normals_agree(*normal, right.normal1)) else {
        return false;
    };
    let inward = if normals_agree(left.normal0, shared) { left.inward0 } else { left.inward1 };
    let separation = dot(sub(right.origin, left.origin), inward).abs();
    left.tangent_m + right.tangent_m >= separation - 1.0e-6
}

fn support_faces_at(body: &SolidBody, fillet: &Fillet, vertex: u32) -> Result<(u32, u32), String> {
    let mut face0 = None;
    let mut face1 = None;
    for face in body.faces_of_vertex(vertex) {
        let Some(normal) = body.unit_normal(face) else { continue };
        if normals_agree(normal, fillet.normal0) {
            face0 = Some(face);
        } else if normals_agree(normal, fillet.normal1) {
            face1 = Some(face);
        }
    }
    Ok((face0.ok_or("the round lost its adjacent faces")?, face1.ok_or("the round lost its adjacent faces")?))
}

fn build_corner_patch(body: &SolidBody, fillets: &[Fillet], left: usize, right: usize, vertex: u32, left_along: f64, right_along: f64) -> Result<CornerPatch, String> {
    let first = &fillets[left];
    let second = &fillets[right];
    if (first.radius_m - second.radius_m).abs() > 1.0e-5 {
        return Err(format!("the round cannot close a shared corner. Their radii are {:.3} m and {:.3} m.", first.radius_m, second.radius_m));
    }
    let stop = |fillet: &Fillet, along: f64| if along < fillet.length_m * 0.5 { fillet.tangent_m } else { fillet.length_m - fillet.tangent_m };
    let left_stop = stop(first, left_along);
    let right_stop = stop(second, right_along);
    if left_stop <= 1.0e-6 || right_stop <= 1.0e-6 || left_stop >= first.length_m - 1.0e-6 || right_stop >= second.length_m - 1.0e-6 {
        return Err("the round radius does not fit the local wedge".into());
    }
    let left_center = add(edge_point(first, left_stop), first.center_offset);
    let right_center = add(edge_point(second, right_stop), second.center_offset);
    if distance(left_center, right_center) > 1.0e-4 {
        return Err("the round cannot close a shared corner".into());
    }
    let center = scale(add(left_center, right_center), 0.5);
    let shared_normal = [first.normal0, first.normal1]
        .into_iter()
        .find(|normal| normals_agree(*normal, second.normal0) || normals_agree(*normal, second.normal1))
        .ok_or("the round cannot close a shared corner")?;
    let unique = |fillet: &Fillet| if normals_agree(fillet.normal0, shared_normal) { fillet.normal1 } else { fillet.normal0 };
    let left_normal = unique(first);
    let right_normal = unique(second);
    if normals_agree(left_normal, right_normal) || normals_agree(left_normal, shared_normal) || normals_agree(right_normal, shared_normal) {
        return Err("the round cannot close a shared corner".into());
    }
    let radius = first.radius_m;
    let t_shared = add(center, scale(shared_normal, radius));
    let t_left = add(center, scale(left_normal, radius));
    let t_right = add(center, scale(right_normal, radius));
    let plane = unit(cross(sub(t_left, center), sub(t_right, center))).ok_or("the round cannot close a shared corner")?;
    let vertex_pos = body.vertex_position(vertex).ok_or("the source edge is degenerate")?;
    let mut apex = None;
    let mut best_t = f64::MAX;
    for edge in &body.edges {
        if edge.a != vertex && edge.b != vertex {
            continue;
        }
        let other = if edge.a == vertex { edge.b } else { edge.a };
        let other_pos = body.vertex_position(other).ok_or("the source edge is degenerate")?;
        let travel = sub(other_pos, vertex_pos);
        let Some(direction) = unit(travel) else { continue };
        if dot(direction, first.direction).abs() > 0.999 || dot(direction, second.direction).abs() > 0.999 {
            continue;
        }
        let denom = dot(travel, plane);
        if denom.abs() < 1.0e-8 {
            continue;
        }
        let parameter = dot(sub(center, vertex_pos), plane) / denom;
        if parameter <= 1.0e-3 || parameter >= 1.0 - 1.0e-4 || parameter >= best_t {
            continue;
        }
        best_t = parameter;
        apex = Some(add(vertex_pos, scale(travel, parameter)));
    }
    let apex = apex.ok_or("the round cannot close a shared corner")?;
    let mut pie_normal = plane;
    if dot(pie_normal, sub(vertex_pos, apex)) < 0.0 {
        pie_normal = scale(pie_normal, -1.0);
    }
    Ok(CornerPatch {
        vertex,
        left,
        right,
        left_along,
        right_along,
        center,
        radius,
        shared_normal,
        t_shared,
        t_left,
        t_right,
        apex,
        pie_normal,
    })
}

fn arc_points(center: [f64; 3], radius: f64, start: [f64; 3], stop: [f64; 3], divisions: u32) -> Result<Vec<[f64; 3]>, String> {
    let from = unit(sub(start, center)).ok_or("the round cannot close a shared corner")?;
    let to = unit(sub(stop, center)).ok_or("the round cannot close a shared corner")?;
    let angle = dot(from, to).clamp(-1.0, 1.0).acos();
    let mut points = Vec::with_capacity(divisions as usize + 1);
    for step in 0..=divisions {
        let t = f64::from(step) / f64::from(divisions.max(1));
        let direction = if angle < 1.0e-6 {
            from
        } else {
            let (sin_from, sin_to) = (((1.0 - t) * angle).sin(), (t * angle).sin());
            unit(add(scale(from, sin_from / angle.sin()), scale(to, sin_to / angle.sin()))).ok_or("the round cannot close a shared corner")?
        };
        points.push(add(center, scale(direction, radius)));
    }
    Ok(points)
}

fn side_along_edge(body: &SolidBody, fillet: &Fillet, corner: u32, neighbor: u32) -> Option<u8> {
    let edge = body.edge_between(corner, neighbor)?;
    let faces = body.faces_of_edge(edge);
    let on0 = faces.iter().any(|face| body.unit_normal(*face).is_some_and(|normal| normals_agree(normal, fillet.normal0)));
    let on1 = faces.iter().any(|face| body.unit_normal(*face).is_some_and(|normal| normals_agree(normal, fillet.normal1)));
    match (on0, on1) {
        (true, false) => Some(0),
        (false, true) => Some(1),
        _ => None,
    }
}

fn end_arc_points(body: &SolidBody, fillet: &Fillet, vertex: u32, previous: u32, next: u32, along: f64, divisions: u32) -> Result<Vec<[f64; 3]>, String> {
    let (Some(previous_side), Some(next_side)) = (side_along_edge(body, fillet, vertex, previous), side_along_edge(body, fillet, vertex, next)) else {
        return Err("the round cannot close this corner".into());
    };
    if previous_side == next_side {
        return Err("the round cannot close this corner".into());
    }
    let start_angle = if previous_side == 0 { 0.0 } else { fillet.span };
    let end_angle = if next_side == 0 { 0.0 } else { fillet.span };
    let mut points = Vec::with_capacity(divisions as usize + 1);
    for step in 0..=divisions {
        let t = f64::from(step) / f64::from(divisions);
        points.push(fillet.point(along, start_angle + (end_angle - start_angle) * t));
    }
    Ok(points)
}

fn corner_loop_points(body: &SolidBody, patch: &CornerPatch, face_normal: [f64; 3], previous: u32, fillets: &[Fillet]) -> Result<Vec<[f64; 3]>, String> {
    if normals_agree(face_normal, patch.shared_normal) {
        return Ok(vec![patch.t_shared]);
    }
    let (fillet, tangent) = if angle_for_normal(&fillets[patch.left], face_normal).is_some() {
        (&fillets[patch.left], patch.t_left)
    } else if angle_for_normal(&fillets[patch.right], face_normal).is_some() {
        (&fillets[patch.right], patch.t_right)
    } else {
        return Err("the round cannot close a shared corner".into());
    };
    let from_fillet = body.vertex_position(previous).is_some_and(|point| station_on_fillet(fillet, point).is_some());
    if from_fillet {
        Ok(vec![tangent, patch.apex])
    } else {
        Ok(vec![patch.apex, tangent])
    }
}

fn support_point(fillets: &[Fillet], face_normal: [f64; 3], point: [f64; 3]) -> Result<Option<[f64; 3]>, String> {
    let mut hit = None;
    for fillet in fillets {
        if station_on_fillet(fillet, point).is_none() {
            continue;
        }
        let Some(angle) = angle_for_normal(fillet, face_normal) else { continue };
        let station = station_on_fillet(fillet, point).unwrap_or(0.0);
        if hit.is_some() {
            return Err("the round cannot close a shared corner".into());
        }
        hit = Some(fillet.point(station, angle));
    }
    Ok(hit)
}

fn boundary_loops(body: &SolidBody, faces: &[u32]) -> Result<Vec<Vec<u32>>, String> {
    let mut boundary: Vec<(u32, u32)> = Vec::new();
    for face in faces {
        let loop_ = body.face_loop(*face).ok_or("an adjacent face is missing")?;
        for index in 0..loop_.len() {
            let start = loop_[index];
            let stop = loop_[(index + 1) % loop_.len()];
            if let Some(position) = boundary.iter().position(|(from, to)| *from == stop && *to == start) {
                boundary.swap_remove(position);
            } else {
                boundary.push((start, stop));
            }
        }
    }
    let mut loops = Vec::new();
    while let Some((start, stop)) = boundary.pop() {
        let mut loop_ = vec![start, stop];
        while loop_.first() != loop_.last() {
            let tail = *loop_.last().unwrap();
            let Some(position) = boundary.iter().position(|(from, _)| *from == tail) else {
                return Err("the adjacent face lost the edge".into());
            };
            let (_, next) = boundary.swap_remove(position);
            loop_.push(next);
            if loop_.len() > body.vertices.len() + faces.len() + 4 {
                return Err("the adjacent face lost the edge".into());
            }
        }
        loop_.pop();
        if loop_.len() >= 3 {
            loops.push(loop_);
        }
    }
    if loops.is_empty() {
        return Err("an adjacent face is missing".into());
    }
    Ok(loops)
}

fn coplanar_components(body: &SolidBody, normal: [f64; 3]) -> Vec<Vec<u32>> {
    let faces: Vec<u32> = body.faces.iter().filter(|face| body.unit_normal(face.id).is_some_and(|face_normal| normals_agree(face_normal, normal))).map(|face| face.id).collect();
    let mut remaining = faces;
    let mut components = Vec::new();
    while let Some(seed) = remaining.pop() {
        let mut component = vec![seed];
        let mut grew = true;
        while grew {
            grew = false;
            let mut index = 0;
            while index < remaining.len() {
                let face = remaining[index];
                let touches = component.iter().any(|have| faces_share_edge(body, *have, face));
                if touches {
                    component.push(remaining.swap_remove(index));
                    grew = true;
                } else {
                    index += 1;
                }
            }
        }
        components.push(component);
    }
    components
}

fn faces_share_edge(body: &SolidBody, left: u32, right: u32) -> bool {
    let Some(loop_) = body.face_loop(left) else { return false };
    loop_.iter().any(|vertex| body.faces_of_vertex(*vertex).contains(&right) && loop_.iter().filter(|other| body.edge_between(*vertex, **other).is_some()).any(|other| body.face_loop(right).is_some_and(|right_loop| right_loop.contains(vertex) && right_loop.contains(other))))
}

fn component_touches_fillet(body: &SolidBody, faces: &[u32], fillets: &[Fillet], normal: [f64; 3]) -> bool {
    faces.iter().any(|face| {
        body.face_loop(*face).is_some_and(|loop_| {
            loop_.iter().any(|vertex| {
                body.vertex_position(*vertex).is_some_and(|point| {
                    fillets.iter().any(|fillet| station_on_fillet(fillet, point).is_some() && angle_for_normal(fillet, normal).is_some())
                })
            })
        })
    })
}

fn rewrite_loop(
    body: &SolidBody,
    loop_: &[u32],
    normal: [f64; 3],
    fillets: &[Fillet],
    patches: &[CornerPatch],
    trihedrals: &[TrihedralPatch],
    free_ends: &[FreeEnd],
    divisions: u32,
) -> Result<Vec<[f64; 3]>, String> {
    let mut positions = Vec::new();
    for index in 0..loop_.len() {
        let vertex = loop_[index];
        let previous = loop_[(index + loop_.len() - 1) % loop_.len()];
        let next = loop_[(index + 1) % loop_.len()];
        if let Some(tri) = trihedrals.iter().find(|tri| tri.vertex == vertex) {
            let Some((_, point)) = tri.touches.iter().find(|(touch_normal, _)| normals_agree(*touch_normal, normal)) else {
                return Err("the round cannot close a shared corner".into());
            };
            push_unique(&mut positions, *point);
            continue;
        }
        if let Some(patch) = patches.iter().find(|patch| patch.vertex == vertex) {
            for point in corner_loop_points(body, patch, normal, previous, fillets)? {
                push_unique(&mut positions, point);
            }
            continue;
        }
        if let Some(end) = free_ends.iter().find(|end| end.vertex == vertex && end.spliced && normals_agree_abs(normal, fillets[end.fillet].direction)) {
            let fillet = &fillets[end.fillet];
            for point in end_arc_points(body, fillet, vertex, previous, next, end.along, divisions)? {
                push_unique(&mut positions, point);
            }
            continue;
        }
        let point = body.vertex_position(vertex).ok_or("a face vertex is missing")?;
        if let Some(moved) = support_point(fillets, normal, point)? {
            push_unique(&mut positions, moved);
        } else {
            push_unique(&mut positions, point);
        }
    }
    if positions.len() < 3 {
        return Err("the adjacent face collapsed".into());
    }
    Ok(positions)
}

fn normals_agree_abs(left: [f64; 3], right: [f64; 3]) -> bool {
    dot(left, right).abs() > 0.999
}

fn push_sphere_corner(tris: &mut Vec<CornerTri>, patch: &CornerPatch, divisions: u32) -> Result<(), String> {
    let mid = unit(add(add(sub(patch.t_shared, patch.center), sub(patch.t_left, patch.center)), sub(patch.t_right, patch.center))).ok_or("the round cannot close a shared corner")?;
    let crown = add(patch.center, scale(mid, patch.radius));
    for (start, stop) in [(patch.t_shared, patch.t_left), (patch.t_left, patch.t_right), (patch.t_right, patch.t_shared)] {
        let arc = arc_points(patch.center, patch.radius, start, stop, divisions)?;
        for pair in arc.windows(2) {
            let wanted = unit(sub(add(pair[0], pair[1]), scale(patch.center, 2.0))).unwrap_or(mid);
            push_tri(tris, crown, pair[0], pair[1], wanted);
        }
    }
    Ok(())
}

fn push_pie(tris: &mut Vec<CornerTri>, patch: &CornerPatch, divisions: u32) -> Result<(), String> {
    let arc = arc_points(patch.center, patch.radius, patch.t_left, patch.t_right, divisions)?;
    for pair in arc.windows(2) {
        push_tri(tris, patch.apex, pair[0], pair[1], patch.pie_normal);
    }
    Ok(())
}

/// Whether these fillets close into one surface. Divisions stay out of the record.
pub fn round_fillets_close(body: &SolidBody, fillets: &[Fillet]) -> Result<(), String> {
    let _ = emit_fillet_set(body, fillets, 4)?;
    Ok(())
}

const CURVE_PICK_DIVISIONS: u32 = 8;

/// What a curve element is. Arc divisions are not a kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveKind {
    Face,
    Edge,
    Vertex,
}

/// One semantic curve minted from a round. The samples are a pick mesh.
/// They are not authored edges, and they are not saved.
#[derive(Clone, Debug)]
pub struct CurveElement {
    pub token: String,
    pub id: u32,
    pub kind: CurveKind,
    pub source: String,
    pub sources: Vec<String>,
    pub segment: ([f64; 3], [f64; 3]),
    pub outline: Vec<[f64; 3]>,
    pub samples: Vec<[f64; 3]>,
    /// Trimmed along range of a fillet face. Observation wires use it. It is not a division count.
    pub along: Option<(f64, f64)>,
}

/// Stable id for one curve slot of one token. Body ids stay below this range.
pub fn round_curve_id(token: &str, slot: u8) -> u32 {
    let mut hash = 2_166_136_261u32;
    for byte in token.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    0x1000_0000 | (hash & 0x0FFF_FFF0) | u32::from(slot & 0x0F)
}

/// Fillet faces, tangent boundaries, end junctions, and corner patches for these rounds.
///
/// `rounds` is the fillet order. A single fillet still mints full-length boundaries.
/// The same token and slot mint the same id after replay.
pub fn curve_catalog(body: &SolidBody, rounds: &[(String, Fillet)]) -> Result<Vec<CurveElement>, String> {
    if rounds.is_empty() {
        return Err("the tape has no round".into());
    }
    let fillets: Vec<Fillet> = rounds.iter().map(|(_, fillet)| fillet.clone()).collect();
    let closed = close_fillets(body, &fillets)?;
    let mut elements = Vec::new();
    for (index, (source, fillet)) in rounds.iter().enumerate() {
        let (start, end) = trimmed_fillet_range(fillet, index, &closed.patches, &closed.trihedrals)?;
        push_fillet_curves(&mut elements, source, fillet, start, end);
    }
    for patch in &closed.patches {
        let mut sources = vec![rounds[patch.left].0.clone(), rounds[patch.right].0.clone()];
        sources.sort();
        let token = format!("F:corner({})", sources.join(","));
        let mut tris = Vec::new();
        push_sphere_corner(&mut tris, patch, CURVE_PICK_DIVISIONS)?;
        push_pie(&mut tris, patch, CURVE_PICK_DIVISIONS)?;
        elements.push(corner_element(token, sources, vec![patch.t_shared, patch.t_left, patch.apex, patch.t_right], &tris));
    }
    for patch in &closed.trihedrals {
        let mut sources: Vec<String> = patch.members.iter().map(|(index, _)| rounds[*index].0.clone()).collect();
        sources.sort();
        let token = format!("F:corner({})", sources.join(","));
        let mut tris = Vec::new();
        push_trihedral(&mut tris, patch, CURVE_PICK_DIVISIONS)?;
        let outline = vec![patch.touches[0].1, patch.touches[1].1, patch.touches[2].1];
        elements.push(corner_element(token, sources, outline, &tris));
    }
    let mut ids: Vec<u32> = elements.iter().map(|element| element.id).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("the round minted two curve identities with the same id".into());
    }
    Ok(elements)
}

fn push_fillet_curves(elements: &mut Vec<CurveElement>, source: &str, fillet: &Fillet, start: f64, end: f64) {
    let a0 = fillet.point(start, 0.0);
    let a1 = fillet.point(end, 0.0);
    let b0 = fillet.point(start, fillet.span);
    let b1 = fillet.point(end, fillet.span);
    let face_token = format!("F:fillet({source})");
    let mut tris = Vec::new();
    push_arc(&mut tris, fillet, start, end, CURVE_PICK_DIVISIONS);
    elements.push(CurveElement {
        token: face_token.clone(),
        id: round_curve_id(&face_token, 0),
        kind: CurveKind::Face,
        source: source.to_string(),
        sources: vec![source.to_string()],
        segment: (a0, b1),
        outline: vec![a0, a1, b1, b0],
        samples: flatten_tris(&tris),
        along: Some((start, end)),
    });
    push_boundary(elements, &format!("E:fillet-a({source})"), 1, source, a0, a1);
    push_boundary(elements, &format!("E:fillet-b({source})"), 2, source, b0, b1);
    let start_point = fillet.point(start, fillet.span * 0.5);
    let end_point = fillet.point(end, fillet.span * 0.5);
    push_junction(elements, &format!("V:fillet-start({source})"), 3, source, start_point);
    push_junction(elements, &format!("V:fillet-end({source})"), 4, source, end_point);
}

fn push_boundary(elements: &mut Vec<CurveElement>, token: &str, slot: u8, source: &str, start: [f64; 3], end: [f64; 3]) {
    elements.push(CurveElement {
        token: token.to_string(),
        id: round_curve_id(token, slot),
        kind: CurveKind::Edge,
        source: source.to_string(),
        sources: vec![source.to_string()],
        segment: (start, end),
        outline: vec![start, end],
        samples: vec![start, end],
        along: None,
    });
}

fn push_junction(elements: &mut Vec<CurveElement>, token: &str, slot: u8, source: &str, point: [f64; 3]) {
    elements.push(CurveElement {
        token: token.to_string(),
        id: round_curve_id(token, slot),
        kind: CurveKind::Vertex,
        source: source.to_string(),
        sources: vec![source.to_string()],
        segment: (point, point),
        outline: vec![point],
        samples: vec![point],
        along: None,
    });
}

fn corner_element(token: String, sources: Vec<String>, outline: Vec<[f64; 3]>, tris: &[CornerTri]) -> CurveElement {
    let segment = (outline.first().copied().unwrap_or([0.0; 3]), outline.get(1).copied().unwrap_or([0.0; 3]));
    CurveElement {
        id: round_curve_id(&token, 0),
        token,
        kind: CurveKind::Face,
        source: sources.join(","),
        sources,
        segment,
        outline,
        samples: flatten_tris(tris),
        along: None,
    }
}

fn flatten_tris(tris: &[CornerTri]) -> Vec<[f64; 3]> {
    let mut samples = Vec::with_capacity(tris.len() * 3);
    for tri in tris {
        samples.extend_from_slice(&tri.position);
    }
    samples
}

/// The nearest sample of `kind`. Face samples are triangles. An edge is its segment.
/// A vertex is its point. Tessellation steps inside a sample are not identities.
pub fn pick_curve_elements(elements: &[CurveElement], kind: CurveKind, origin: [f64; 3], direction: [f64; 3], slack: f64) -> Option<TopologyPick> {
    let mut best: Option<TopologyPick> = None;
    for element in elements {
        if element.kind != kind {
            continue;
        }
        let hit = match kind {
            CurveKind::Face => ray_mesh(origin, direction, &element.samples).map(|ray_t| TopologyPick { id: element.id, ray_t, distance: 0.0 }),
            CurveKind::Edge | CurveKind::Vertex => {
                let (ray_t, distance) = ray_segment_local(origin, direction, element.segment.0, element.segment.1);
                (distance <= slack && ray_t >= 0.0).then_some(TopologyPick { id: element.id, ray_t, distance })
            }
        };
        if let Some(hit) = hit {
            best = Some(match best {
                Some(current) if closer_pick(current, hit) => current,
                _ => hit,
            });
        }
    }
    best
}

/// One cage segment. The id is the caller's edge, not a new curve.
pub fn pick_segment(id: u32, start: [f64; 3], end: [f64; 3], origin: [f64; 3], direction: [f64; 3], slack: f64) -> Option<TopologyPick> {
    let (ray_t, distance) = ray_segment_local(origin, direction, start, end);
    (distance <= slack && ray_t >= 0.0).then_some(TopologyPick { id, ray_t, distance })
}

/// Whether a hit on the planar face is still the visible surface.
/// A point inside a fillet strip or a corner blend belongs to the curve.
pub fn planar_face_keeps_point(body: &SolidBody, fillets: &[Fillet], face: u32, point: [f64; 3]) -> bool {
    let Some(normal) = body.unit_normal(face) else {
        return true;
    };
    if fillets.iter().any(|fillet| fillet_cut_contains(fillet, normal, point)) {
        return false;
    }
    let Ok(closed) = close_fillets(body, fillets) else {
        return true;
    };
    for patch in &closed.patches {
        if corner_shadow_contains(body, patch.vertex, patch.apex, &[patch.t_left, patch.t_right, patch.apex], face, point) {
            return false;
        }
    }
    for patch in &closed.trihedrals {
        let touches = [patch.touches[0].1, patch.touches[1].1, patch.touches[2].1];
        if corner_shadow_contains(body, patch.vertex, patch.center, &touches, face, point) {
            return false;
        }
    }
    true
}

/// The fillet face or corner patch that owns a point on planar `face`.
///
/// A corner shadow wins over the fillet strips that meet there. A point in one strip is that
/// fillet. The flat interior returns none. Observation triangles are not consulted, so a click
/// still names the surface when the pick mesh is empty.
pub fn round_surface_at(body: &SolidBody, tokens: &[String], fillets: &[Fillet], face: u32, point: [f64; 3]) -> Option<u32> {
    if tokens.len() != fillets.len() || fillets.is_empty() {
        return None;
    }
    let normal = body.unit_normal(face)?;
    let Ok(closed) = close_fillets(body, fillets) else {
        return fillet_strip_at(tokens, fillets, normal, point);
    };
    for patch in &closed.patches {
        if corner_shadow_contains(body, patch.vertex, patch.apex, &[patch.t_left, patch.t_right, patch.apex], face, point) {
            return Some(round_curve_id(&corner_token(&[tokens[patch.left].clone(), tokens[patch.right].clone()]), 0));
        }
    }
    for patch in &closed.trihedrals {
        let touches = [patch.touches[0].1, patch.touches[1].1, patch.touches[2].1];
        if corner_shadow_contains(body, patch.vertex, patch.center, &touches, face, point) {
            let sources: Vec<String> = patch.members.iter().map(|(index, _)| tokens[*index].clone()).collect();
            return Some(round_curve_id(&corner_token(&sources), 0));
        }
    }
    fillet_strip_at(tokens, fillets, normal, point)
}

fn corner_token(sources: &[String]) -> String {
    let mut sources = sources.to_vec();
    sources.sort();
    format!("F:corner({})", sources.join(","))
}

fn fillet_strip_at(tokens: &[String], fillets: &[Fillet], normal: [f64; 3], point: [f64; 3]) -> Option<u32> {
    let mut best: Option<(f64, u32)> = None;
    for (token, fillet) in tokens.iter().zip(fillets.iter()) {
        let Some(into) = fillet_strip_depth(fillet, normal, point) else { continue };
        let id = round_curve_id(&format!("F:fillet({token})"), 0);
        let replace = best.is_none_or(|(depth, _)| into < depth);
        if replace {
            best = Some((into, id));
        }
    }
    best.map(|(_, id)| id)
}

fn fillet_strip_depth(fillet: &Fillet, face_normal: [f64; 3], point: [f64; 3]) -> Option<f64> {
    let inward = if normals_agree(fillet.normal0, face_normal) {
        fillet.inward0
    } else if normals_agree(fillet.normal1, face_normal) {
        fillet.inward1
    } else {
        return None;
    };
    let along = dot(sub(point, fillet.origin), fillet.direction);
    if along < -1.0e-4 || along > fillet.length_m + 1.0e-4 {
        return None;
    }
    let closest = add(fillet.origin, scale(fillet.direction, along.clamp(0.0, fillet.length_m)));
    let into = dot(sub(point, closest), inward);
    (into > -1.0e-4 && into < fillet.tangent_m - 1.0e-5).then_some(into)
}

/// The face a viewport ray selects. A planar hit inside a fillet strip or a corner shadow
/// is that semantic surface. A curve sample closer than the planar hit stays the curve.
/// The flat interior stays the planar face. An empty pick mesh does not give the strip back.
pub fn visible_authored_face(
    body: &SolidBody,
    tokens: &[String],
    fillets: &[Fillet],
    curves: &[CurveElement],
    origin: [f64; 3],
    direction: [f64; 3],
) -> Option<TopologyPick> {
    let direction = unit(direction)?;
    let planar = body.pick_face(origin, direction);
    let curve = pick_curve_elements(curves, CurveKind::Face, origin, direction, 0.0);
    let Some(planar) = planar else {
        return curve;
    };
    if fillets.is_empty() {
        return Some(planar);
    }
    let point = add(origin, scale(direction, planar.ray_t));
    if let Some(id) = round_surface_at(body, tokens, fillets, planar.id, point) {
        let ray_t = curve.as_ref().filter(|hit| hit.id == id).map(|hit| hit.ray_t).unwrap_or(planar.ray_t);
        return Some(TopologyPick { id, ray_t, distance: 0.0 });
    }
    if let Some(hit) = curve {
        if hit.ray_t < planar.ray_t - 1.0e-6 {
            return Some(hit);
        }
    }
    Some(planar)
}

fn fillet_cut_contains(fillet: &Fillet, face_normal: [f64; 3], point: [f64; 3]) -> bool {
    let inward = if normals_agree(fillet.normal0, face_normal) {
        fillet.inward0
    } else if normals_agree(fillet.normal1, face_normal) {
        fillet.inward1
    } else {
        return false;
    };
    let along = dot(sub(point, fillet.origin), fillet.direction);
    if along < -1.0e-4 || along > fillet.length_m + 1.0e-4 {
        return false;
    }
    let closest = add(fillet.origin, scale(fillet.direction, along.clamp(0.0, fillet.length_m)));
    let into = dot(sub(point, closest), inward);
    into > -1.0e-4 && into < fillet.tangent_m - 1.0e-5
}

fn corner_shadow_contains(body: &SolidBody, vertex: u32, _far: [f64; 3], extra: &[[f64; 3]], face: u32, point: [f64; 3]) -> bool {
    let Some(loop_) = body.face_loop(face) else {
        return false;
    };
    if !loop_.contains(&vertex) {
        return false;
    }
    let Some(position) = body.vertex_position(vertex) else {
        return false;
    };
    let Some(normal) = body.unit_normal(face) else {
        return false;
    };
    let mut on_face = Vec::new();
    for item in extra {
        if distance(*item, position) <= 1.0e-6 {
            continue;
        }
        if dot(sub(*item, position), normal).abs() > 1.0e-4 {
            continue;
        }
        on_face.push(*item);
    }
    if on_face.len() < 2 {
        return false;
    }
    for index in 0..on_face.len() - 1 {
        if triangle_holds(position, on_face[index], on_face[index + 1], point, normal) {
            return true;
        }
    }
    false
}

fn triangle_holds(a: [f64; 3], b: [f64; 3], c: [f64; 3], point: [f64; 3], normal: [f64; 3]) -> bool {
    let area = dot(cross(sub(b, a), sub(c, a)), normal);
    if area.abs() <= 1.0e-12 {
        return false;
    }
    let sign = if area > 0.0 { 1.0 } else { -1.0 };
    let same = |start: [f64; 3], end: [f64; 3]| sign * dot(cross(sub(end, start), sub(point, start)), normal) >= -1.0e-8;
    same(a, b) && same(b, c) && same(c, a)
}

fn closer_pick(current: TopologyPick, incoming: TopologyPick) -> bool {
    current.ray_t < incoming.ray_t - 1.0e-9 || ((current.ray_t - incoming.ray_t).abs() <= 1.0e-9 && current.distance <= incoming.distance)
}

fn ray_mesh(origin: [f64; 3], direction: [f64; 3], samples: &[[f64; 3]]) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut index = 0;
    while index + 2 < samples.len() {
        if let Some(ray_t) = ray_triangle_local(origin, direction, [samples[index], samples[index + 1], samples[index + 2]]) {
            best = Some(best.map(|have| have.min(ray_t)).unwrap_or(ray_t));
        }
        index += 3;
    }
    best
}

fn ray_segment_local(origin: [f64; 3], direction: [f64; 3], a: [f64; 3], b: [f64; 3]) -> (f64, f64) {
    let edge = sub(b, a);
    let edge_len2 = dot(edge, edge);
    if edge_len2 < 1.0e-18 {
        let along = dot(sub(a, origin), direction).max(0.0);
        return (along, distance(a, add(origin, scale(direction, along))));
    }
    let toward_a = sub(a, origin);
    let dir_edge = dot(direction, edge);
    let dir_a = dot(direction, toward_a);
    let edge_a = dot(edge, toward_a);
    let denom = edge_len2 - dir_edge * dir_edge;
    let (s, t) = if denom.abs() < 1.0e-12 {
        (dir_a.max(0.0), 0.0)
    } else {
        let mut t = (dir_a * dir_edge - edge_a) / denom;
        let mut s = dir_a + t * dir_edge;
        if t < 0.0 {
            t = 0.0;
            s = dir_a.max(0.0);
        } else if t > 1.0 {
            t = 1.0;
            s = dot(direction, sub(b, origin)).max(0.0);
        } else if s < 0.0 {
            s = 0.0;
            t = (-edge_a / edge_len2).clamp(0.0, 1.0);
        }
        (s, t)
    };
    let on_ray = add(origin, scale(direction, s));
    let on_edge = add(a, scale(edge, t));
    (s, distance(on_ray, on_edge))
}

fn ray_triangle_local(origin: [f64; 3], direction: [f64; 3], triangle: [[f64; 3]; 3]) -> Option<f64> {
    let edge_u = sub(triangle[1], triangle[0]);
    let edge_v = sub(triangle[2], triangle[0]);
    let p = cross(direction, edge_v);
    let det = dot(edge_u, p);
    if det.abs() < 1.0e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = sub(origin, triangle[0]);
    let u = dot(tvec, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(tvec, edge_u);
    let v = dot(direction, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(edge_v, q) * inv;
    (t > 1.0e-6).then_some(t)
}

/// Groups fillet ends into two-fillet corners, three-fillet corners, and free ends.
///
/// Four or more fillets at one vertex are refused. The caller builds nothing from a refusal.
fn close_fillets(body: &SolidBody, fillets: &[Fillet]) -> Result<FilletClose, String> {
    for index in 0..fillets.len() {
        for other in index + 1..fillets.len() {
            if parallel_fillets_crowd(&fillets[index], &fillets[other]) {
                return Err("the round radius does not fit the local wedge".into());
            }
        }
    }
    let mut ends: Vec<(usize, u32, f64)> = Vec::new();
    for (index, fillet) in fillets.iter().enumerate() {
        let mut hits = endpoint_vertices(body, fillet);
        hits.retain(|(_, along, _)| *along < 1.0e-4 || (*along - fillet.length_m).abs() < 1.0e-4);
        if hits.len() != 2 {
            return Err("the semantic edge is missing".into());
        }
        for (vertex, along, _) in hits {
            ends.push((index, vertex, along));
        }
    }
    let mut groups: Vec<(u32, Vec<(usize, f64)>)> = Vec::new();
    for (fillet, vertex, along) in ends {
        if let Some(group) = groups.iter_mut().find(|(id, _)| *id == vertex) {
            group.1.push((fillet, along));
        } else {
            groups.push((vertex, vec![(fillet, along)]));
        }
    }
    let mut patches = Vec::new();
    let mut trihedrals = Vec::new();
    let mut free_ends = Vec::new();
    for (vertex, members) in groups {
        if members.len() > 3 {
            return Err("the round cannot close a shared corner".into());
        }
        if members.len() == 3 {
            trihedrals.push(build_trihedral(body, fillets, &members, vertex)?);
            continue;
        }
        if members.len() == 2 {
            patches.push(build_corner_patch(body, fillets, members[0].0, members[1].0, vertex, members[0].1, members[1].1)?);
            continue;
        }
        let (fillet_index, along) = members[0];
        let fillet = &fillets[fillet_index];
        let (face0, face1) = support_faces_at(body, fillet, vertex)?;
        let caps: Vec<u32> = body.faces_of_vertex(vertex).into_iter().filter(|face| {
            let Some(normal) = body.unit_normal(*face) else { return false };
            !normals_agree(normal, fillet.normal0) && !normals_agree(normal, fillet.normal1)
        }).collect();
        let spliced = end_is_spliced(body, vertex, face0, face1, fillet);
        if !spliced && caps.len() > 1 {
            return Err("the round cannot close a shared corner".into());
        }
        free_ends.push(FreeEnd { fillet: fillet_index, vertex, along, spliced });
    }
    Ok(FilletClose { patches, trihedrals, free_ends })
}

fn build_trihedral(body: &SolidBody, fillets: &[Fillet], members: &[(usize, f64)], vertex: u32) -> Result<TrihedralPatch, String> {
    if members.len() != 3 {
        return Err("the round cannot close a shared corner".into());
    }
    for index in 0..3 {
        for other in index + 1..3 {
            let first = &fillets[members[index].0];
            let second = &fillets[members[other].0];
            if (first.radius_m - second.radius_m).abs() > 1.0e-5 {
                return Err(format!(
                    "the round cannot close a shared corner. Their radii are {:.3} m and {:.3} m.",
                    first.radius_m, second.radius_m
                ));
            }
        }
    }
    let stop = |fillet: &Fillet, along: f64| if along < fillet.length_m * 0.5 { fillet.tangent_m } else { fillet.length_m - fillet.tangent_m };
    let mut centers = Vec::with_capacity(3);
    for (index, along) in members {
        let fillet = &fillets[*index];
        let stopped = stop(fillet, *along);
        if stopped <= 1.0e-6 || stopped >= fillet.length_m - 1.0e-6 {
            return Err("the round radius does not fit the local wedge".into());
        }
        centers.push(add(edge_point(fillet, stopped), fillet.center_offset));
    }
    if distance(centers[0], centers[1]) > 1.0e-4 || distance(centers[0], centers[2]) > 1.0e-4 || distance(centers[1], centers[2]) > 1.0e-4 {
        return Err("the round cannot close a shared corner".into());
    }
    let center = scale(add(add(centers[0], centers[1]), centers[2]), 1.0 / 3.0);
    let mut normals = Vec::new();
    for (index, _) in members {
        for normal in [fillets[*index].normal0, fillets[*index].normal1] {
            if normals.iter().any(|have: &[f64; 3]| normals_agree(*have, normal)) {
                continue;
            }
            normals.push(normal);
        }
    }
    if normals.len() != 3 {
        return Err("the round cannot close a shared corner".into());
    }
    let radius = fillets[members[0].0].radius_m;
    let vertex_pos = body.vertex_position(vertex).ok_or("the source edge is degenerate")?;
    if distance(vertex_pos, center) <= radius + 1.0e-6 {
        return Err("the round cannot close a shared corner".into());
    }
    let sum = add(add(normals[0], normals[1]), normals[2]);
    if dot(sub(vertex_pos, center), sum) <= 1.0e-6 {
        return Err("the round cannot close a shared corner".into());
    }
    let touches = [
        (normals[0], add(center, scale(normals[0], radius))),
        (normals[1], add(center, scale(normals[1], radius))),
        (normals[2], add(center, scale(normals[2], radius))),
    ];
    Ok(TrihedralPatch {
        vertex,
        members: [members[0], members[1], members[2]],
        center,
        radius,
        touches,
    })
}

fn trimmed_fillet_range(fillet: &Fillet, index: usize, patches: &[CornerPatch], trihedrals: &[TrihedralPatch]) -> Result<(f64, f64), String> {
    let mut start: f64 = 0.0;
    let mut end: f64 = fillet.length_m;
    let mut pull = |along: f64| {
        if along < fillet.length_m * 0.5 {
            start = start.max(fillet.tangent_m);
        } else {
            end = end.min(fillet.length_m - fillet.tangent_m);
        }
    };
    for patch in patches {
        if patch.left == index {
            pull(patch.left_along);
        } else if patch.right == index {
            pull(patch.right_along);
        }
    }
    for tri in trihedrals {
        for (member, along) in &tri.members {
            if *member == index {
                pull(*along);
            }
        }
    }
    if end - start < 1.0e-6 {
        return Err("the round radius does not fit the local wedge".into());
    }
    Ok((start, end))
}

fn push_trihedral(tris: &mut Vec<CornerTri>, patch: &TrihedralPatch, divisions: u32) -> Result<(), String> {
    let points = [patch.touches[0].1, patch.touches[1].1, patch.touches[2].1];
    let mid = unit(add(add(sub(points[0], patch.center), sub(points[1], patch.center)), sub(points[2], patch.center))).ok_or("the round cannot close a shared corner")?;
    let crown = add(patch.center, scale(mid, patch.radius));
    for (start, stop) in [(points[0], points[1]), (points[1], points[2]), (points[2], points[0])] {
        let arc = arc_points(patch.center, patch.radius, start, stop, divisions)?;
        for pair in arc.windows(2) {
            let wanted = unit(sub(add(pair[0], pair[1]), scale(patch.center, 2.0))).unwrap_or(mid);
            push_tri(tris, crown, pair[0], pair[1], wanted);
        }
    }
    Ok(())
}

/// One closed observation of every fillet.
///
/// A single fillet whose analytic segment is still the concrete edge uses the original emitter.
/// A chain, or several fillets, trims each supporting surface once. Two fillets at a vertex
/// close with a sphere patch and a planar pie. Three equal-radius fillets at a convex vertex
/// close with one spherical corner and no sharp point. Four or more are refused.
fn emit_fillet_set(body: &SolidBody, fillets: &[Fillet], divisions: u32) -> Result<Vec<CornerTri>, String> {
    if fillets.is_empty() {
        return Err("the tape has no round".into());
    }
    if divisions == 0 || divisions > CURVE_N_CAP {
        return Err("arc divisions are outside the arc cap".into());
    }
    if fillets.len() == 1 && fillet_is_simple_edge(body, &fillets[0]) {
        return emit_round_solid(body, &fillets[0], divisions);
    }
    let closed = close_fillets(body, fillets)?;
    let patches = &closed.patches;
    let trihedrals = &closed.trihedrals;
    let free_ends = &closed.free_ends;
    let mut covered = Vec::new();
    let mut tris = Vec::new();
    let mut normals = Vec::new();
    for fillet in fillets {
        for normal in [fillet.normal0, fillet.normal1] {
            if normals.iter().any(|have: &[f64; 3]| normals_agree(*have, normal)) {
                continue;
            }
            normals.push(normal);
        }
    }
    for normal in normals {
        for component in coplanar_components(body, normal) {
            if !component_touches_fillet(body, &component, fillets, normal) {
                continue;
            }
            for face in &component {
                covered.push(*face);
            }
            for loop_ in boundary_loops(body, &component)? {
                let positions = rewrite_loop(body, &loop_, normal, fillets, patches, trihedrals, free_ends, divisions)?;
                push_fan(&mut tris, &positions, normal)?;
            }
        }
    }
    for face in &body.faces {
        if covered.contains(&face.id) {
            continue;
        }
        let normal = body.unit_normal(face.id).ok_or("a face is not planar")?;
        let loop_ = body.face_loop(face.id).ok_or("a face is empty")?;
        let positions = rewrite_loop(body, loop_, normal, fillets, patches, trihedrals, free_ends, divisions)?;
        push_fan(&mut tris, &positions, normal)?;
    }
    for (index, fillet) in fillets.iter().enumerate() {
        let (start, end) = trimmed_fillet_range(fillet, index, patches, trihedrals)?;
        push_arc(&mut tris, fillet, start, end, divisions);
    }
    for end in free_ends {
        if end.spliced {
            continue;
        }
        let fillet = &fillets[end.fillet];
        let outward = if end.along < fillet.length_m * 0.5 { scale(fillet.direction, -1.0) } else { fillet.direction };
        push_disk(&mut tris, fillet, end.along, outward, divisions);
    }
    for patch in patches {
        push_sphere_corner(&mut tris, patch, divisions)?;
        push_pie(&mut tris, patch, divisions)?;
    }
    for patch in trihedrals {
        push_trihedral(&mut tris, patch, divisions)?;
    }
    if tris.is_empty() {
        return Err("the round constructed no triangles".into());
    }
    Ok(tris)
}

fn corner_window(fillet: &Fillet) -> (f64, f64, f64) {
    let half = (fillet.length_m * 0.5).min(CORNER_WINDOW_RADII * fillet.radius_m * 0.5);
    let mid = fillet.length_m * 0.5;
    (mid - half, mid + half, CORNER_WING_RADII * fillet.radius_m)
}

struct CornerTri {
    position: [[f64; 3]; 3],
    normal: [f64; 3],
}

fn push_tri(tris: &mut Vec<CornerTri>, a: [f64; 3], b: [f64; 3], c: [f64; 3], wanted: [f64; 3]) {
    let geo = cross(sub(b, a), sub(c, a));
    if dot(geo, geo) < 1.0e-20 {
        return;
    }
    if dot(geo, wanted) < 0.0 {
        tris.push(CornerTri { position: [a, c, b], normal: wanted });
    } else {
        tris.push(CornerTri { position: [a, b, c], normal: wanted });
    }
}

fn push_wing(tris: &mut Vec<CornerTri>, fillet: &Fillet, along0: f64, along1: f64, wing: f64, sharp: bool) {
    for (inward, normal, angle) in [(fillet.inward0, fillet.normal0, 0.0), (fillet.inward1, fillet.normal1, fillet.span)] {
        let start = if sharp { edge_point(fillet, along0) } else { fillet.point(along0, angle) };
        let end = if sharp { edge_point(fillet, along1) } else { fillet.point(along1, angle) };
        let reach = if sharp { fillet.tangent_m + wing } else { wing };
        let outer0 = add(start, scale(inward, reach));
        let outer1 = add(end, scale(inward, reach));
        push_tri(tris, start, end, outer1, normal);
        push_tri(tris, start, outer1, outer0, normal);
    }
}

fn push_arc(tris: &mut Vec<CornerTri>, fillet: &Fillet, along0: f64, along1: f64, divisions: u32) {
    for step in 0..divisions {
        let angle0 = fillet.span * f64::from(step) / f64::from(divisions);
        let angle1 = fillet.span * f64::from(step + 1) / f64::from(divisions);
        let normal = fillet_radial(fillet, (angle0 + angle1) * 0.5);
        let a0 = fillet.point(along0, angle0);
        let b0 = fillet.point(along1, angle0);
        let b1 = fillet.point(along1, angle1);
        let a1 = fillet.point(along0, angle1);
        push_tri(tris, a0, b0, b1, normal);
        push_tri(tris, a0, b1, a1, normal);
    }
}

fn emit_round_solid(body: &SolidBody, fillet: &Fillet, divisions: u32) -> Result<Vec<CornerTri>, String> {
    if divisions == 0 || divisions > CURVE_N_CAP {
        return Err("arc divisions are outside the arc cap".into());
    }
    let (face0, face1, start_id, end_id) = fillet_incident(body, fillet)?;
    for face in &body.faces {
        if face.id == face0 || face.id == face1 {
            continue;
        }
        if face.vertices.contains(&start_id) && face.vertices.contains(&end_id) {
            return Err("the round cannot close a face that contains the whole edge".into());
        }
    }
    let mut spliced = Vec::new();
    let mut caps = Vec::new();
    for (vertex, along, outward) in [(start_id, 0.0, scale(fillet.direction, -1.0)), (end_id, fillet.length_m, fillet.direction)] {
        let extras: Vec<u32> = body.faces_of_vertex(vertex).into_iter().filter(|face| *face != face0 && *face != face1).collect();
        if extras.len() > 1 {
            return Err("the round cannot close a shared corner".into());
        }
        if end_is_spliced(body, vertex, face0, face1, fillet) {
            spliced.push((extras[0], vertex, along));
            continue;
        }
        caps.push((along, outward));
    }
    let mut tris = Vec::new();
    for face in &body.faces {
        let normal = body.unit_normal(face.id).ok_or("a face is not planar")?;
        if face.id == face0 {
            push_fan(&mut tris, &trim_incident(body, face.id, start_id, end_id, fillet, 0.0)?, normal)?;
        } else if face.id == face1 {
            push_fan(&mut tris, &trim_incident(body, face.id, start_id, end_id, fillet, fillet.span)?, normal)?;
        } else if let Some((_, vertex, along)) = spliced.iter().find(|(id, _, _)| *id == face.id) {
            push_fan(&mut tris, &splice_end(body, face.id, *vertex, face0, face1, fillet, *along, divisions)?, normal)?;
        } else {
            let positions = body.face_positions(face.id).ok_or("a face is empty")?;
            push_fan(&mut tris, &positions, normal)?;
        }
    }
    push_arc(&mut tris, fillet, 0.0, fillet.length_m, divisions);
    for (along, outward) in caps {
        push_disk(&mut tris, fillet, along, outward, divisions);
    }
    if tris.is_empty() {
        return Err("the round constructed no triangles".into());
    }
    Ok(tris)
}

fn push_fan(tris: &mut Vec<CornerTri>, loop_: &[[f64; 3]], normal: [f64; 3]) -> Result<(), String> {
    if loop_.len() < 3 {
        return Err("the adjacent face collapsed".into());
    }
    let triangles = triangulate_polygon(loop_, normal).ok_or("the round face does not triangulate")?;
    for triangle in triangles {
        push_tri(tris, loop_[triangle[0]], loop_[triangle[1]], loop_[triangle[2]], normal);
    }
    Ok(())
}

/// Ears of a simple polygon. A triangle fan spills across a concave fillet inset.
fn triangulate_polygon(loop_: &[[f64; 3]], normal: [f64; 3]) -> Option<Vec<[usize; 3]>> {
    let count = loop_.len();
    if count < 3 || !normal.iter().all(|axis| axis.is_finite()) {
        return None;
    }
    if count == 3 {
        return Some(vec![[0, 1, 2]]);
    }
    let mut area = [0.0; 3];
    for index in 0..count {
        area = add(area, cross(loop_[index], loop_[(index + 1) % count]));
    }
    if dot(area, area) < 1.0e-20 {
        return None;
    }
    let mut order: Vec<usize> = (0..count).collect();
    if dot(area, normal) < 0.0 {
        order.reverse();
    }
    let mut triangles = Vec::new();
    while order.len() > 3 {
        let mut ear = None;
        for index in 0..order.len() {
            let previous = order[(index + order.len() - 1) % order.len()];
            let current = order[index];
            let next = order[(index + 1) % order.len()];
            if !polygon_turn_is_convex(loop_[previous], loop_[current], loop_[next], normal) {
                continue;
            }
            let blocked = order.iter().any(|other| {
                *other != previous && *other != current && *other != next && point_in_triangle(loop_[*other], loop_[previous], loop_[current], loop_[next], normal)
            });
            if blocked {
                continue;
            }
            ear = Some(index);
            break;
        }
        let Some(index) = ear else { return None };
        let previous = order[(index + order.len() - 1) % order.len()];
        let current = order[index];
        let next = order[(index + 1) % order.len()];
        triangles.push([previous, current, next]);
        order.remove(index);
    }
    triangles.push([order[0], order[1], order[2]]);
    Some(triangles)
}

fn polygon_turn_is_convex(previous: [f64; 3], current: [f64; 3], next: [f64; 3], normal: [f64; 3]) -> bool {
    dot(cross(sub(current, previous), sub(next, current)), normal) >= -1.0e-12
}

fn point_in_triangle(point: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3], normal: [f64; 3]) -> bool {
    let area = |u: [f64; 3], v: [f64; 3], w: [f64; 3]| dot(cross(sub(v, u), sub(w, u)), normal);
    let whole = area(a, b, c);
    if whole.abs() < 1.0e-16 {
        return false;
    }
    let alpha = area(point, b, c) / whole;
    let beta = area(a, point, c) / whole;
    let gamma = area(a, b, point) / whole;
    alpha >= -1.0e-8 && beta >= -1.0e-8 && gamma >= -1.0e-8
}

fn push_disk(tris: &mut Vec<CornerTri>, fillet: &Fillet, along: f64, outward: [f64; 3], divisions: u32) {
    let center = add(edge_point(fillet, along), fillet.center_offset);
    for step in 0..divisions {
        let angle0 = fillet.span * f64::from(step) / f64::from(divisions);
        let angle1 = fillet.span * f64::from(step + 1) / f64::from(divisions);
        push_tri(tris, center, fillet.point(along, angle0), fillet.point(along, angle1), outward);
    }
}

fn trim_incident(body: &SolidBody, face: u32, start_id: u32, end_id: u32, fillet: &Fillet, angle: f64) -> Result<Vec<[f64; 3]>, String> {
    let loop_ = body.face_loop(face).ok_or("the adjacent face lost the edge")?;
    if !loop_.contains(&start_id) || !loop_.contains(&end_id) {
        return Err("the adjacent face lost the edge".into());
    }
    let mut positions = Vec::with_capacity(loop_.len());
    for vertex in loop_ {
        let point = if *vertex == start_id {
            fillet.point(0.0, angle)
        } else if *vertex == end_id {
            fillet.point(fillet.length_m, angle)
        } else {
            body.vertex_position(*vertex).ok_or("a face vertex is missing")?
        };
        push_unique(&mut positions, point);
    }
    if positions.len() < 3 {
        return Err("the adjacent face collapsed".into());
    }
    Ok(positions)
}

fn splice_end(
    body: &SolidBody,
    face: u32,
    vertex: u32,
    face0: u32,
    face1: u32,
    fillet: &Fillet,
    along: f64,
    divisions: u32,
) -> Result<Vec<[f64; 3]>, String> {
    let loop_ = body.face_loop(face).ok_or("the end face lost the corner")?;
    let Some(index) = loop_.iter().position(|id| *id == vertex) else {
        return Err("the end face lost the corner".into());
    };
    let count = loop_.len();
    let prev = loop_[(index + count - 1) % count];
    let next = loop_[(index + 1) % count];
    let prev_side = neighbor_side(body, prev, face0, face1);
    let next_side = neighbor_side(body, next, face0, face1);
    let (Some(prev_side), Some(next_side)) = (prev_side, next_side) else {
        return Err("the round cannot close this corner".into());
    };
    if prev_side == next_side {
        return Err("the round cannot close this corner".into());
    }
    let start_angle = if prev_side == 0 { 0.0 } else { fillet.span };
    let end_angle = if next_side == 0 { 0.0 } else { fillet.span };
    let mut positions = Vec::with_capacity(loop_.len() + divisions as usize);
    for (offset, id) in loop_.iter().enumerate() {
        if offset == index {
            for step in 0..=divisions {
                let t = f64::from(step) / f64::from(divisions);
                push_unique(&mut positions, fillet.point(along, start_angle + (end_angle - start_angle) * t));
            }
        } else {
            push_unique(&mut positions, body.vertex_position(*id).ok_or("a face vertex is missing")?);
        }
    }
    if positions.len() < 3 {
        return Err("the end face collapsed".into());
    }
    Ok(positions)
}

fn neighbor_side(body: &SolidBody, vertex: u32, face0: u32, face1: u32) -> Option<u8> {
    let faces = body.faces_of_vertex(vertex);
    match (faces.contains(&face0), faces.contains(&face1)) {
        (true, false) => Some(0),
        (false, true) => Some(1),
        _ => None,
    }
}

fn push_unique(positions: &mut Vec<[f64; 3]>, point: [f64; 3]) {
    if positions.last().is_none_or(|previous| distance(*previous, point) > 1.0e-8) {
        positions.push(point);
    }
    if positions.len() >= 2 && distance(positions[0], *positions.last().unwrap()) <= 1.0e-8 {
        positions.pop();
    }
}

fn push_caps(tris: &mut Vec<CornerTri>, fillet: &Fillet, along0: f64, along1: f64, wing: f64, divisions: u32) {
    for (along, outward) in [(along0, scale(fillet.direction, -1.0)), (along1, fillet.direction)] {
        let mut loop_ = Vec::with_capacity(divisions as usize + 4);
        loop_.push(add(fillet.point(along, 0.0), scale(fillet.inward0, wing)));
        for step in 0..=divisions {
            loop_.push(fillet.point(along, fillet.span * f64::from(step) / f64::from(divisions)));
        }
        loop_.push(add(fillet.point(along, fillet.span), scale(fillet.inward1, wing)));
        let inner = add(edge_point(fillet, along), scale(add(fillet.inward0, fillet.inward1), (fillet.tangent_m + wing) * 0.5));
        for pair in loop_.windows(2) {
            push_tri(tris, inner, pair[0], pair[1], outward);
        }
    }
}

fn push_arc_fins(tris: &mut Vec<CornerTri>, fillet: &Fillet, along0: f64, along1: f64, divisions: u32) {
    let width = CORNER_FIN_RADII * fillet.radius_m;
    for step in 0..=divisions {
        let angle = fillet.span * f64::from(step) / f64::from(divisions);
        let up = fillet_radial(fillet, angle);
        push_fin(tris, fillet.point(along0, angle), fillet.point(along1, angle), up, width);
    }
    for along in [along0, along1] {
        for step in 0..divisions {
            let angle0 = fillet.span * f64::from(step) / f64::from(divisions);
            let angle1 = fillet.span * f64::from(step + 1) / f64::from(divisions);
            push_fin(tris, fillet.point(along, angle0), fillet.point(along, angle1), fillet_radial(fillet, (angle0 + angle1) * 0.5), width);
        }
    }
}

fn push_wing_fins(tris: &mut Vec<CornerTri>, fillet: &Fillet, along0: f64, along1: f64, wing: f64) {
    let width = CORNER_FIN_RADII * fillet.radius_m;
    for (inward, normal, angle) in [(fillet.inward0, fillet.normal0, 0.0), (fillet.inward1, fillet.normal1, fillet.span)] {
        let start = fillet.point(along0, angle);
        let end = fillet.point(along1, angle);
        let outer0 = add(start, scale(inward, wing));
        let outer1 = add(end, scale(inward, wing));
        push_fin(tris, outer0, outer1, normal, width);
        push_fin(tris, start, outer0, normal, width);
        push_fin(tris, end, outer1, normal, width);
    }
}

fn push_fin(tris: &mut Vec<CornerTri>, a: [f64; 3], b: [f64; 3], up: [f64; 3], width: f64) {
    let Some(up) = unit(up) else { return };
    let Some(side) = unit(cross(sub(b, a), up)) else { return };
    let lift = scale(up, width * 0.65);
    let side = scale(side, width * 0.5);
    let a0 = add(add(a, lift), side);
    let a1 = sub(add(a, lift), side);
    let b0 = add(add(b, lift), side);
    let b1 = sub(add(b, lift), side);
    push_tri(tris, a0, b0, b1, up);
    push_tri(tris, a0, b1, a1, up);
}

fn fillet_radial(fillet: &Fillet, angle: f64) -> [f64; 3] {
    let (sin, cos) = angle.sin_cos();
    add(scale(fillet.normal0, cos), scale(fillet.wing, sin))
}

fn edge_point(fillet: &Fillet, along: f64) -> [f64; 3] {
    add(fillet.origin, scale(fillet.direction, along))
}

fn pack_corner(tris: &[CornerTri]) -> Result<PackedStrip, String> {
    if tris.is_empty() {
        return Err("the corner mesh is empty".into());
    }
    let vertices = u32::try_from(tris.len() * 3).map_err(|_| "too many corner vertices")?;
    let triangles = u32::try_from(tris.len()).map_err(|_| "too many corner triangles")?;
    let index_format = if vertices > u32::from(u16::MAX) { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 };
    let vertex_bytes = u64::from(vertices) * u64::from(CURVE_VERTEX_STRIDE);
    let index_byte_len = u64::from(triangles) * 3 * u64::from(index_format.byte_size());
    let mut bytes = Vec::with_capacity(vertex_bytes as usize);
    let mut index_bytes_buf = Vec::with_capacity(index_byte_len as usize);
    let mut position_bytes = Vec::new();
    for tri in tris {
        let normal = unit(tri.normal).ok_or("a corner normal is degenerate")?;
        let packed_normal = [normal[0] as f32, normal[1] as f32, normal[2] as f32];
        for corner in tri.position {
            let packed = [corner[0] as f32, corner[1] as f32, corner[2] as f32];
            if packed.iter().any(|axis| !axis.is_finite()) || packed_normal.iter().any(|axis| !axis.is_finite()) {
                return Err("a corner vertex was not finite".into());
            }
            for value in corner {
                position_bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
            push_vertex(&mut bytes, packed, packed_normal);
        }
    }
    for index in 0..vertices {
        match index_format {
            MeshIndexFormat::Uint16 => index_bytes_buf.extend_from_slice(&(index as u16).to_le_bytes()),
            MeshIndexFormat::Uint32 => index_bytes_buf.extend_from_slice(&index.to_le_bytes()),
        }
    }
    if bytes.len() as u64 != vertex_bytes || index_bytes_buf.len() as u64 != index_byte_len {
        return Err("packed corner bytes drifted from the triangles".into());
    }
    let mesh = create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: CURVE_VERTEX_STRIDE,
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
        index_bytes: index_bytes_buf,
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: triangles * 3,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .map_err(|error: MeshError| error.to_string())?;
    Ok(PackedStrip {
        mesh,
        cpu_bytes: vertex_bytes + index_byte_len,
        gpu_bytes: vertex_bytes + index_byte_len,
        vertex_bytes,
        index_bytes: index_byte_len,
        index_format,
        position_hash: fnv64(&position_bytes),
    })
}

pub fn curve_proof_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Temp")
        .join("jarvig-authored-curve")
}

/// Copy the eligible 65-entry tape and append one round. `Main.jarviglevel` is only read.
pub fn write_authored_curve_project(directory: &Path) -> Result<PathBuf, String> {
    let text = directory.to_string_lossy();
    if text.contains("jarvig-authored-intent") || text.contains("jarvig-intent-proof") || text.contains("jarvig-analytic-intent") || text.contains("IntentProof") {
        return Err("the curve project refuses the frozen experiment directories".into());
    }
    let main = crate::authored_main_level();
    let main_bytes = std::fs::read(&main).map_err(|error| error.to_string())?;
    if main_bytes.len() as u64 != CURVE_MAIN_LEVEL_BYTES {
        return Err(format!("Main.jarviglevel is {} bytes", main_bytes.len()));
    }
    let source = crate::parse_level(std::str::from_utf8(&main_bytes).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
    let project_file = directory.join("AuthoredCurve.jarvigproject");
    let project = crate::ProjectDocument {
        format_version: crate::PROJECT_FORMAT_VERSION,
        project_uuid: crate::EntityId::parse(PROJECT_UUID).ok_or("curve project uuid")?,
        display_name: "Authored Curve".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        startup_level: "Content/Levels/Curve.jarviglevel".into(),
        content_directory: "Content".into(),
        saved_directory: "Saved".into(),
        config_directory: "Config".into(),
        intermediate_directory: "Intermediate".into(),
        settings: "Config/Project.jarvigsettings".into(),
    };
    crate::create_project_directories(&project_file, &project).map_err(|error| error.to_string())?;
    let backup = directory.join("Saved").join("Backup");
    crate::save_project_atomic(&project_file, &backup, &project).map_err(|error| error.to_string())?;
    let level = curve_level(&source)?;
    crate::save_level_atomic(&project.startup_level_path(&project_file).map_err(|error| error.to_string())?, &backup, &level).map_err(|error| error.to_string())?;
    let settings = directory.join(&project.settings);
    if let Some(parent) = settings.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(settings, "JARVIG project settings placeholder. This is not a level and not a GPU resource.\n").map_err(|error| error.to_string())?;
    let after = std::fs::read(&main).map_err(|error| error.to_string())?;
    if after != main_bytes {
        return Err("writing the curve project changed Main.jarviglevel".into());
    }
    Ok(project_file)
}

fn curve_level(source: &crate::LevelDocument) -> Result<crate::LevelDocument, String> {
    let settings = entity_named(source, "World Settings")?;
    let mut solid = entity_named(source, "Intent Solid")?;
    let legacy = entity_named(source, "Legacy Cube")?;
    if solid.uuid.to_string() != SOURCE_SOLID_UUID {
        return Err(format!("the source Intent Solid is {}, not {SOURCE_SOLID_UUID}", solid.uuid));
    }
    let block = block_in(&solid)?;
    if block.body.is_some() || block.intent.len() != 65 || block.history.len() != 24 {
        return Err(format!("the source tape is intent {} history {} body {}", block.intent.len(), block.history.len(), block.body.is_some()));
    }
    if crate::intent_authority_diagnostic(block) != "Intent Authority: ELIGIBLE" {
        return Err(crate::intent_authority_diagnostic(block));
    }
    if block_in(&legacy)?.body.is_none() {
        return Err("the source legacy cube has no body".into());
    }
    let token = select_round_token(block, CURVE_RADIUS_M)?;
    let rounded = crate::commit_class_c_intent(block, round_entry(&token, CURVE_RADIUS_M))?;
    install_block(&mut solid, rounded);
    solid.uuid = crate::EntityId::parse(SOLID_UUID).ok_or("curve solid uuid")?;
    Ok(crate::LevelDocument {
        format_version: crate::LEVEL_BLOCK_VERSION,
        level_uuid: crate::EntityId::parse(LEVEL_UUID).ok_or("curve level uuid")?,
        name: "Authored Curve".into(),
        world_settings: source.world_settings.clone(),
        organization: crate::SceneOrganization::default(),
        entities: vec![settings, solid, legacy],
    })
}

fn choose_arc_divisions(fillet: &Fillet, camera: &RoundCamera) -> Result<u32, String> {
    for divisions in 1..=CURVE_N_CAP {
        let error = projected_sagitta_px(fillet, camera, divisions)?;
        if error <= f64::from(camera.requested_error_px) {
            return Ok(divisions);
        }
    }
    Err("no arc division met the error budget before construction".into())
}

fn projected_sagitta_px(fillet: &Fillet, camera: &RoundCamera, divisions: u32) -> Result<f64, String> {
    if divisions == 0 {
        return Err("arc divisions start at 1".into());
    }
    let half = fillet.span / (2.0 * f64::from(divisions));
    let sagitta = fillet.radius_m * (1.0 - half.cos());
    if !sagitta.is_finite() || sagitta < 0.0 {
        return Err("the sagitta was not a length".into());
    }
    let depth = closest_depth(fillet, camera)?;
    let tan_half = (camera.vertical_fov_radians * 0.5).tan();
    if !tan_half.is_finite() || tan_half <= 0.0 {
        return Err("the field of view is not usable".into());
    }
    let pixels = f64::from(projected_detail_px(sagitta as f32, depth as f32, camera.viewport_height, tan_half as f32));
    if !pixels.is_finite() {
        return Err("the projected sagitta was not finite".into());
    }
    Ok(pixels)
}

fn closest_depth(fillet: &Fillet, camera: &RoundCamera) -> Result<f64, String> {
    let center = fillet.arc_center();
    let toward = sub(camera.eye, center);
    let cosine = dot(unit(toward).ok_or("the camera sits on the arc center")?, fillet.normal0);
    let sine = dot(unit(toward).unwrap_or([0.0; 3]), fillet.wing);
    let mut angle = sine.atan2(cosine);
    if angle < 0.0 {
        angle = 0.0;
    }
    if angle > fillet.span {
        angle = fillet.span;
    }
    let closest = fillet.point(fillet.length_m * 0.5, angle);
    let depth = distance(camera.eye, closest);
    if !depth.is_finite() || depth <= 0.0 {
        return Err("the round has no positive depth".into());
    }
    Ok(depth)
}

struct EmittedStrip {
    corners: Vec<[f64; 3]>,
    indices: Vec<u32>,
    vertices: u32,
    triangles: u32,
    discarded: u32,
}

fn emit_strip(fillet: &Fillet, divisions: u32) -> Result<EmittedStrip, String> {
    let mut corners = Vec::with_capacity((divisions as usize + 1) * 2);
    for step in 0..=divisions {
        let angle = fillet.span * f64::from(step) / f64::from(divisions);
        corners.push(fillet.point(0.0, angle));
        corners.push(fillet.point(fillet.length_m, angle));
    }
    let mut indices = Vec::with_capacity(divisions as usize * 6);
    for step in 0..divisions {
        let start = step * 2;
        let (a0, b0, a1, b1) = (start, start + 1, start + 2, start + 3);
        indices.extend_from_slice(&[a0, b0, b1, a0, b1, a1]);
    }
    let radial = fillet.radial(fillet.span * 0.5);
    let sample = corners[0];
    let along = corners[1];
    let next = corners[3];
    let normal = cross(sub(along, sample), sub(next, sample));
    if dot(normal, radial) < 0.0 {
        for chunk in indices.chunks_exact_mut(3) {
            chunk.swap(1, 2);
        }
    }
    let vertices = u32::try_from(corners.len()).map_err(|_| "too many round vertices")?;
    let triangles = divisions.checked_mul(2).ok_or("too many round triangles")?;
    if vertices != divisions.saturating_add(1).saturating_mul(2) || indices.len() != triangles as usize * 3 {
        return Err("the round strip did not match the division".into());
    }
    Ok(EmittedStrip { corners, indices, vertices, triangles, discarded: 0 })
}

struct PackedStrip {
    mesh: Mesh,
    cpu_bytes: u64,
    gpu_bytes: u64,
    vertex_bytes: u64,
    index_bytes: u64,
    index_format: MeshIndexFormat,
    position_hash: u64,
}

fn pack_strip(emitted: &EmittedStrip) -> Result<PackedStrip, String> {
    let index_format = if emitted.vertices > u32::from(u16::MAX) { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 };
    let vertex_bytes = u64::from(emitted.vertices) * u64::from(CURVE_VERTEX_STRIDE);
    let index_bytes = u64::from(emitted.triangles) * 3 * u64::from(index_format.byte_size());
    let mut bytes = Vec::with_capacity(vertex_bytes as usize);
    let mut index_bytes_buf = Vec::with_capacity(index_bytes as usize);
    let mut position_bytes = Vec::new();
    for corner in &emitted.corners {
        let packed = [corner[0] as f32, corner[1] as f32, corner[2] as f32];
        if packed.iter().any(|axis| !axis.is_finite()) {
            return Err("a round vertex was not finite".into());
        }
        for value in corner {
            position_bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        let normal = packed_corner_normal(packed);
        push_vertex(&mut bytes, packed, normal);
    }
    for index in &emitted.indices {
        match index_format {
            MeshIndexFormat::Uint16 => index_bytes_buf.extend_from_slice(&(*index as u16).to_le_bytes()),
            MeshIndexFormat::Uint32 => index_bytes_buf.extend_from_slice(&index.to_le_bytes()),
        }
    }
    if bytes.len() as u64 != vertex_bytes || index_bytes_buf.len() as u64 != index_bytes {
        return Err("packed round bytes drifted from the strip".into());
    }
    let mesh = create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: CURVE_VERTEX_STRIDE,
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
        index_bytes: index_bytes_buf,
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: emitted.triangles * 3,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .map_err(|error: MeshError| error.to_string())?;
    Ok(PackedStrip {
        mesh,
        cpu_bytes: vertex_bytes + index_bytes,
        gpu_bytes: vertex_bytes + index_bytes,
        vertex_bytes,
        index_bytes,
        index_format,
        position_hash: fnv64(&position_bytes),
    })
}

fn packed_corner_normal(position: [f32; 3]) -> [f32; 3] {
    let length = (position[0] * position[0] + position[1] * position[1] + position[2] * position[2]).sqrt();
    if !length.is_finite() || length < 1.0e-8 {
        [0.0, 1.0, 0.0]
    } else {
        [position[0] / length, position[1] / length, position[2] / length]
    }
}

fn push_vertex(bytes: &mut Vec<u8>, position: [f32; 3], normal: [f32; 3]) {
    for value in position.into_iter().chain([1.0, 1.0, 1.0]).chain([0.0, 0.0]).chain(normal).chain([1.0, 0.0, 0.0, 1.0]) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn face_is_planar(body: &SolidBody, face: u32, normal: [f64; 3]) -> bool {
    let Some(loop_) = body.face_loop(face) else { return false };
    let Some(origin) = loop_.first().and_then(|id| body.vertex_position(*id)) else { return false };
    loop_.iter().all(|id| body.vertex_position(*id).is_some_and(|position| dot(sub(position, origin), normal).abs() < 1.0e-5))
}

fn inward_on_face(body: &SolidBody, face: u32, start: u32, end: u32, direction: [f64; 3]) -> Result<[f64; 3], String> {
    let normal = body.unit_normal(face).ok_or("an adjacent face is not planar")?;
    let loop_ = body.face_loop(face).ok_or("an adjacent face is missing")?;
    let mut travel = None;
    for index in 0..loop_.len() {
        let from = loop_[index];
        let to = loop_[(index + 1) % loop_.len()];
        if (from == start && to == end) || (from == end && to == start) {
            let from_pos = body.vertex_position(from).ok_or("an adjacent face is missing")?;
            let to_pos = body.vertex_position(to).ok_or("an adjacent face is missing")?;
            travel = Some(sub(to_pos, from_pos));
            break;
        }
    }
    let travel = travel.ok_or("the edge is not on both adjacent faces")?;
    let along = if dot(travel, direction) >= 0.0 { direction } else { scale(direction, -1.0) };
    unit(cross(normal, along)).ok_or_else(|| "orientation is ambiguous".to_string())
}

fn step_is_inside_face(body: &SolidBody, face: u32, origin: [f64; 3], end: [f64; 3], inward: [f64; 3]) -> bool {
    let mid = scale(add(origin, end), 0.5);
    let sample = add(mid, scale(inward, 1.0e-4));
    point_in_face(body, face, origin, unit(sub(end, origin)).unwrap_or(inward), inward, sample)
}

fn face_clearance(body: &SolidBody, face: u32, origin: [f64; 3], direction: [f64; 3], length_m: f64, inward: [f64; 3]) -> Result<f64, String> {
    let loop_ = body.face_loop(face).ok_or("an adjacent face is missing")?;
    let mut clearance = f64::MAX;
    let mut hits = 0u32;
    for index in 0..loop_.len() {
        let from = body.vertex_position(loop_[index]).ok_or("an adjacent face is missing")?;
        let to = body.vertex_position(loop_[(index + 1) % loop_.len()]).ok_or("an adjacent face is missing")?;
        let (u0, v0) = project(origin, direction, inward, from);
        let (u1, v1) = project(origin, direction, inward, to);
        if let Some(height) = min_positive_v(u0, v0, u1, v1, length_m) {
            hits = hits.saturating_add(1);
            clearance = clearance.min(height);
        }
    }
    if hits == 0 || !clearance.is_finite() {
        return Err("the adjacent face has no measurable clearance".into());
    }
    Ok(clearance)
}

fn min_positive_v(u0: f64, v0: f64, u1: f64, v1: f64, length_m: f64) -> Option<f64> {
    let inset = 1.0e-8;
    let du = u1 - u0;
    if du.abs() < 1.0e-12 {
        if u0 <= inset || u0 >= length_m - inset {
            return None;
        }
        let low = v0.min(v1);
        let high = v0.max(v1);
        if high <= 1.0e-8 {
            return None;
        }
        return Some(if low > 1.0e-8 { low } else { 0.0 });
    }
    let at = |u: f64| (u - u0) / du;
    let (s0, s1) = {
        let left = at(inset);
        let right = at(length_m - inset);
        if left < right { (left, right) } else { (right, left) }
    };
    let lo = s0.max(0.0);
    let hi = s1.min(1.0);
    if hi < lo {
        return None;
    }
    let v_at = |parameter: f64| v0 + parameter * (v1 - v0);
    let start = v_at(lo);
    let end = v_at(hi);
    if start <= 1.0e-8 && end <= 1.0e-8 {
        return None;
    }
    if start <= 1.0e-8 || end <= 1.0e-8 {
        return Some(0.0);
    }
    Some(start.min(end))
}

fn foreign_topology_crosses(
    body: &SolidBody,
    edge: u32,
    faces: &[u32],
    origin: [f64; 3],
    direction: [f64; 3],
    length_m: f64,
    normal0: [f64; 3],
    normal1: [f64; 3],
    tangent_m: f64,
) -> bool {
    let mut boundary = Vec::new();
    for face in faces {
        let Some(loop_) = body.face_loop(*face) else { return true };
        for index in 0..loop_.len() {
            let from = loop_[index];
            let to = loop_[(index + 1) % loop_.len()];
            if let Some(id) = body.edge_between(from, to) {
                if !boundary.contains(&id) {
                    boundary.push(id);
                }
            }
        }
    }
    let end = add(origin, scale(direction, length_m));
    for other in &body.edges {
        if other.id == edge || boundary.contains(&other.id) {
            continue;
        }
        let (Some(start), Some(stop)) = (body.vertex_position(other.a), body.vertex_position(other.b)) else { return true };
        let shares = other.a == body.edges.iter().find(|item| item.id == edge).map(|item| item.a).unwrap_or(0)
            || other.b == body.edges.iter().find(|item| item.id == edge).map(|item| item.a).unwrap_or(0)
            || other.a == body.edges.iter().find(|item| item.id == edge).map(|item| item.b).unwrap_or(0)
            || other.b == body.edges.iter().find(|item| item.id == edge).map(|item| item.b).unwrap_or(0);
        if shares {
            let edge_vertex = if body.vertex_position(other.a).is_some_and(|point| distance(point, origin) < 1.0e-8 || distance(point, end) < 1.0e-8) {
                other.a
            } else {
                other.b
            };
            let away = if edge_vertex == other.a { other.b } else { other.a };
            let Some(from) = body.vertex_position(edge_vertex) else { return true };
            let Some(to) = body.vertex_position(away) else { return true };
            let Some(out) = unit(sub(to, from)) else { return true };
            if dot(out, normal0) < -1.0e-6 && dot(out, normal1) < -1.0e-6 {
                return true;
            }
            continue;
        }
        let (distance_m, point) = segment_distance(origin, end, start, stop);
        if distance_m < tangent_m && dot(sub(point, origin), normal0) < -1.0e-6 && dot(sub(point, origin), normal1) < -1.0e-6 {
            return true;
        }
    }
    false
}

fn point_in_face(body: &SolidBody, face: u32, origin: [f64; 3], axis_u: [f64; 3], axis_v: [f64; 3], point: [f64; 3]) -> bool {
    let Some(loop_) = body.face_loop(face) else { return false };
    let mut polygon = Vec::new();
    for id in loop_ {
        let Some(position) = body.vertex_position(*id) else { return false };
        polygon.push(project(origin, axis_u, axis_v, position));
    }
    let sample = project(origin, axis_u, axis_v, point);
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for index in 0..polygon.len() {
        let (u0, v0) = polygon[previous];
        let (u1, v1) = polygon[index];
        let crosses = (v0 > sample.1) != (v1 > sample.1);
        if crosses {
            let at = (u1 - u0) * (sample.1 - v0) / (v1 - v0) + u0;
            if sample.0 < at {
                inside = !inside;
            }
        }
        previous = index;
    }
    inside
}

fn project(origin: [f64; 3], axis_u: [f64; 3], axis_v: [f64; 3], point: [f64; 3]) -> (f64, f64) {
    let delta = sub(point, origin);
    (dot(delta, axis_u), dot(delta, axis_v))
}

fn segment_distance(a0: [f64; 3], a1: [f64; 3], b0: [f64; 3], b1: [f64; 3]) -> (f64, [f64; 3]) {
    let d1 = sub(a1, a0);
    let d2 = sub(b1, b0);
    let r = sub(a0, b0);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let c = dot(d1, r);
    let b = dot(d1, d2);
    let denom = a * e - b * b;
    let mut s = if denom > 1.0e-12 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
    let mut t = if e > 1.0e-12 { (b * s + f) / e } else { 0.0 };
    if t < 0.0 {
        t = 0.0;
        s = if a > 1.0e-12 { (-c / a).clamp(0.0, 1.0) } else { 0.0 };
    } else if t > 1.0 {
        t = 1.0;
        s = if a > 1.0e-12 { ((b - c) / a).clamp(0.0, 1.0) } else { 0.0 };
    }
    let on_b = add(b0, scale(d2, t));
    let on_a = add(a0, scale(d1, s));
    (distance(on_a, on_b), on_b)
}

fn vertex_bounds(body: &SolidBody) -> Option<([f64; 3], [f64; 3])> {
    if body.vertices.is_empty() {
        return None;
    }
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    for vertex in &body.vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex.position[axis]);
            max[axis] = max[axis].max(vertex.position[axis]);
        }
    }
    Some((min, max))
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

fn install_block(entity: &mut crate::EntityRecord, block: BlockRecord) {
    for component in &mut entity.components {
        if let crate::ComponentRecord::ParametricBlock(slot) = component {
            *slot = block;
            return;
        }
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

fn no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
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

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let delta = sub(left, right);
    dot(delta, delta).sqrt()
}

fn unit(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = distance(value, [0.0; 3]);
    if !length.is_finite() || length < 1.0e-12 {
        None
    } else {
        Some(scale(value, 1.0 / length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_round_fits_inside_the_face_and_refuses_an_impossible_radius() {
        let body = SolidBody::from_box([2.0, 2.0, 2.0]).expect("box");
        let mut accepted = 0u32;
        for edge in &body.edges {
            if validate_fillet(&body, edge.id, 0.05).is_ok() {
                accepted += 1;
            }
            assert!(validate_fillet(&body, edge.id, 3.0).is_err(), "a radius larger than the face must refuse");
        }
        assert!(accepted >= 4, "a cube has straight manifold edges that accept 0.05 m");
        assert!(validate_fillet(&body, body.edges[0].id, 0.0).is_err());
        assert!(validate_fillet(&body, 999, 0.05).is_err());
    }

    #[test]
    fn round_keeps_one_semantic_edge_through_rename_save_and_replay() {
        let main = std::fs::read(crate::authored_main_level()).expect("main level");
        assert_eq!(main.len() as u64, CURVE_MAIN_LEVEL_BYTES);
        let source = crate::parse_level(std::str::from_utf8(&main).unwrap()).expect("parse");
        let mut record = block_named(&source, "Intent Solid");
        assert!(record.body.is_none());
        assert_eq!(record.intent.len(), 65);
        assert_eq!(record.history.len(), 24);
        assert_eq!(crate::intent_authority_diagnostic(&record), "Intent Authority: ELIGIBLE");
        let planar = crate::class_c_cache_extent(&record).expect("planar extent");
        assert_eq!(planar, record.size_m, "a tape with no round keeps the planar AABB cache");
        let token = select_round_token(&record, CURVE_RADIUS_M).expect("one semantic edge");
        assert!(token.starts_with("E:"), "{token}");
        let bindings = crate::semantic_edge_bindings(&record).expect("bindings");
        let edge_id = bindings.iter().find(|(name, _)| name == &token).map(|(_, id)| *id).expect("token binds");
        assert_eq!(bindings.iter().filter(|(name, _)| name == &token).count(), 1);
        let group_id = record.next_surface_group.max(1);
        record.next_surface_group = group_id.saturating_add(1);
        record.surface_groups.push(crate::SurfaceGroup {
            id: group_id,
            name: "Trim".into(),
            slot: 0,
            members: vec![crate::SurfaceMember { face: None, provenance: vec![token.clone()] }],
        });
        assert!(record.rename_surface_group(group_id, "Upper Rim").expect("rename"));
        assert_eq!(record.surface_groups.iter().find(|group| group.id == group_id).unwrap().name, "Upper Rim");
        let before = record.clone();
        let rounded = crate::commit_class_c_intent(&record, round_entry(&token, CURVE_RADIUS_M)).expect("commit round");
        assert!(rounded.body.is_none());
        assert_eq!(rounded.history.len(), 24);
        assert_eq!(rounded.intent.len(), 66);
        assert_eq!(rounded.intent.last().unwrap().groups, Some(vec![vec![token.clone()]]));
        assert!(matches!(rounded.intent.last().unwrap().payload, IntentPayload::Round { radius_m } if radius_m == CURVE_RADIUS_M));
        assert_eq!(crate::intent_authority_diagnostic(&rounded), "Intent Authority: ELIGIBLE");
        assert_eq!(rounded.size_m, crate::class_c_cache_extent(&rounded).unwrap());
        assert_eq!(before.intent.len(), 65, "a refused path is not this commit, and the source tape was not edited in place");
        let directory = std::env::temp_dir().join("jarvig-curve-contract");
        let _ = std::fs::remove_dir_all(&directory);
        let project = write_level_copy(&directory, &source, rounded.clone()).expect("save");
        let bytes = std::fs::read(&project).expect("bytes");
        let text = String::from_utf8(bytes.clone()).expect("utf8");
        assert!(!text.contains("\"op\": \"analytic-surface\""));
        assert!(text.contains("\"op\": \"round\""));
        assert!(text.contains("\"version\": 3"));
        let parsed = crate::parse_level(&text).expect("reparse");
        let loaded = block_named(&parsed, "Intent Solid");
        assert!(loaded.body.is_none());
        assert_eq!(loaded.intent.last().unwrap().groups, Some(vec![vec![token.clone()]]));
        let replayed = crate::replay_round(&loaded).expect("replay");
        assert_eq!(replayed.token, token);
        assert_eq!(replayed.edge_id, edge_id);
        assert_eq!(replayed.radius_m, CURVE_RADIUS_M);
        assert_eq!(feature_id(&replayed.token), feature_id(&token));
        assert_eq!(loaded.surface_groups.iter().find(|group| group.id == group_id).unwrap().name, "Upper Rim");
        let mut renamed = loaded.clone();
        assert!(!renamed.rename_surface_group(group_id, "Trim Again").expect("second rename") || renamed.surface_groups.iter().any(|group| group.name == "Trim Again"));
        assert_eq!(renamed.intent.last().unwrap().groups, Some(vec![vec![token.clone()]]));
        let mut poisoned = loaded.clone();
        poisoned.size_m = [9.0, 9.0, 9.0];
        assert!(crate::intent_authority_diagnostic(&poisoned).starts_with("Intent Authority: INELIGIBLE"));
        let far = observation_camera(&replayed.fillet, CURVE_FAR_MULTIPLE * replayed.radius_m, CURVE_FAR_WIDTH, CURVE_VIEW_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let close = observation_camera(&replayed.fillet, CURVE_CLOSE_MULTIPLE * replayed.radius_m, CURVE_CLOSE_WIDTH, CURVE_VIEW_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let far_product = realize_round_observation(&loaded, &far, "Far", 1).expect("far");
        let close_product = realize_round_observation(&loaded, &close, "Close", 1).expect("close");
        let poisoned_far = realize_round_observation(&poisoned, &far, "Far", 1).expect("poison still replays intent");
        assert_eq!(far_product.position_hash, poisoned_far.position_hash);
        assert!(!far_product.replay_consulted_size_m);
        assert_eq!(far_product.triangles_discarded_after_construction, 0);
        assert_eq!(close_product.triangles_discarded_after_construction, 0);
        assert!(far_product.measured_error_px <= CURVE_ERROR_PX + CURVE_ERROR_SLACK_PX);
        assert!(close_product.measured_error_px <= CURVE_ERROR_PX + CURVE_ERROR_SLACK_PX);
        assert_eq!(far_product.vertices_constructed, far_product.chosen_arc_divisions * 2 + 2);
        assert_eq!(far_product.triangles_constructed, far_product.chosen_arc_divisions * 2);
        assert_eq!(close_product.triangles_constructed, close_product.chosen_arc_divisions * 2);
        assert_eq!(far_product.semantic_edge_id, token);
        assert_eq!(close_product.semantic_feature_id, feature_id(&token));
        let edited = crate::commit_class_c_intent(&loaded, round_entry(&token, CURVE_EDITED_RADIUS_M)).expect("radius edit");
        assert!(edited.body.is_none());
        assert_eq!(edited.intent.len(), 67);
        assert_eq!(edited.history.len(), 24);
        assert!(matches!(edited.intent.last().unwrap().payload, IntentPayload::Round { radius_m } if radius_m == CURVE_EDITED_RADIUS_M));
        assert_eq!(edited.intent[65].groups, edited.intent[66].groups);
        assert_eq!(edited.size_m, crate::class_c_cache_extent(&edited).unwrap());
        let edited_replay = crate::replay_round(&edited).unwrap();
        assert_eq!(edited_replay.token, token);
        assert_eq!(edited_replay.edge_id, edge_id);
        assert_eq!(edited_replay.radius_m, CURVE_EDITED_RADIUS_M);
        assert_ne!(crate::authored_authority_hash(&loaded).unwrap(), crate::authored_authority_hash(&edited).unwrap());
        let edited_far_camera = observation_camera(&edited_replay.fillet, CURVE_FAR_MULTIPLE * edited_replay.radius_m, CURVE_FAR_WIDTH, CURVE_VIEW_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let edited_far = realize_round_observation(&edited, &edited_far_camera, "Far", 2).unwrap();
        assert_ne!(edited_far.position_hash, far_product.position_hash);
        assert_eq!(edited_far.semantic_feature_id, far_product.semantic_feature_id);
        assert_eq!(edited_far.semantic_edge_id, far_product.semantic_edge_id);
        let crate::IntentAuthorityCandidate::Reconstructable(planar) = crate::intent_authority_candidate(&loaded) else {
            panic!("the rounded tape still replays");
        };
        let other = bindings
            .iter()
            .find_map(|(name, id)| {
                if name == &token || *id == edge_id {
                    return None;
                }
                round_edge_closes(&planar, *id, CURVE_RADIUS_M).ok().map(|_| name.clone())
            })
            .expect("another edge can take the radius");
        let second = crate::commit_class_c_intent(&loaded, round_entry(&other, CURVE_RADIUS_M)).expect("a compatible edge stays beside the fillet");
        assert!(second.body.is_none());
        assert_eq!(loaded.intent.len(), 66, "the caller's tape stays unchanged");
        assert_eq!(second.intent.len(), 67);
        let rounds = crate::replay_rounds(&second).expect("both fillets");
        assert!(rounds.iter().any(|round| round.token == token), "{:?}", rounds.iter().map(|round| round.token.as_str()).collect::<Vec<_>>());
        assert!(rounds.iter().any(|round| round.token == other));
        assert!(rounds.iter().all(|round| (round.radius_m - CURVE_RADIUS_M).abs() < 1.0e-12));
        let undone = crate::replay_rounds(&loaded).expect("the earlier tape still has its fillet");
        assert_eq!(undone.len(), 1);
        assert_eq!(undone[0].token, token);
        let other_id = bindings.iter().find(|(name, _)| name == &other).map(|(_, id)| *id).expect("other edge");
        let first_chain = logical_edge_chain(&planar, edge_id).expect("first chain");
        let other_chain = logical_edge_chain(&planar, other_id).expect("other chain");
        if chain_endpoints_touch(&planar, &first_chain, &other_chain) {
            let clash = crate::commit_class_c_intent(&loaded, round_entry(&other, CURVE_EDITED_RADIUS_M));
            assert!(clash.is_err(), "adjacent edges with different radii must refuse");
            assert_eq!(loaded.intent.len(), 66);
        }
        let huge = crate::commit_class_c_intent(&record, round_entry(&token, 10.0));
        assert!(huge.is_err());
        assert_eq!(record.intent.len(), 65);
        let missing = crate::commit_class_c_intent(&record, IntentEntry { groups: None, payload: IntentPayload::Round { radius_m: 0.05 } });
        assert!(missing.is_err());
        assert!(text.contains(&token));
    }

    #[test]
    fn presentation_corner_replaces_the_sharp_edge() {
        let main = std::fs::read(crate::authored_main_level()).expect("main level");
        let source = crate::parse_level(std::str::from_utf8(&main).unwrap()).expect("parse");
        let record = block_named(&source, "Intent Solid");
        let token = select_round_token(&record, CURVE_RADIUS_M).expect("edge");
        let rounded = crate::commit_class_c_intent(&record, round_entry(&token, CURVE_RADIUS_M)).expect("round");
        let replayed = crate::replay_round(&rounded).expect("replay");
        let far = presentation_camera(&replayed.fillet, CURVE_PRESENT_FAR_M, CURVE_PRESENT_FAR_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let close = presentation_camera(&replayed.fillet, CURVE_PRESENT_CLOSE_M, CURVE_PRESENT_CLOSE_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        assert!((far.near_m - CURVE_PRESENT_NEAR_M).abs() < 1.0e-12);
        let far_product = realize_round_corner(&rounded, &far, "Far", 1).expect("far corner");
        let close_product = realize_round_corner(&rounded, &close, "Close", 1).expect("close corner");
        assert!(
            close_product.chosen_arc_divisions > far_product.chosen_arc_divisions,
            "close {} far {}",
            close_product.chosen_arc_divisions,
            far_product.chosen_arc_divisions
        );
        assert!(far_product.chosen_arc_divisions >= 2, "far {}", far_product.chosen_arc_divisions);
        assert_eq!(far_product.triangles_constructed, far_product.mesh.index_count() / 3);
        assert_eq!(far_product.vertices_constructed, far_product.mesh.vertex_count());
        assert!(far_product.triangles_constructed > far_product.chosen_arc_divisions * 2);
        assert_eq!(far_product.triangles_discarded_after_construction, 0);
        let sharp = sharp_corner_mesh(&replayed.fillet).expect("sharp");
        let mid = add(replayed.fillet.origin, scale(replayed.fillet.direction, replayed.fillet.length_m * 0.5));
        assert!(mesh_has_point(&sharp, mid, 1.0e-4), "the sharp corner left the edge");
        assert!(mesh_min_distance(&far_product.mesh, mid) > replayed.fillet.radius_m * 0.25, "the round still contains the sharp edge");
        let crown = replayed.fillet.point(replayed.fillet.length_m * 0.5, replayed.fillet.span * 0.5);
        assert!(mesh_has_point(&far_product.mesh, crown, 1.0e-3), "the arc crown is missing");
        let mut poisoned = rounded.clone();
        poisoned.size_m = [9.0, 9.0, 9.0];
        let poisoned_far = realize_round_corner(&poisoned, &far, "Far", 1).expect("poison");
        assert_eq!(poisoned_far.position_hash, far_product.position_hash);
        let edited = crate::commit_class_c_intent(&rounded, round_entry(&token, CURVE_EDITED_RADIUS_M)).expect("edit");
        let edited_replay = crate::replay_round(&edited).unwrap();
        let edited_far_camera = presentation_camera(&edited_replay.fillet, CURVE_PRESENT_FAR_M, CURVE_PRESENT_FAR_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let edited_close_camera = presentation_camera(&edited_replay.fillet, CURVE_PRESENT_CLOSE_M, CURVE_PRESENT_CLOSE_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let edited_far = realize_round_corner(&edited, &edited_far_camera, "Far", 2).unwrap();
        let edited_close = realize_round_corner(&edited, &edited_close_camera, "Close", 2).unwrap();
        assert_ne!(edited_far.position_hash, far_product.position_hash);
        assert!(edited_close.chosen_arc_divisions > edited_far.chosen_arc_divisions, "edited close {} far {}", edited_close.chosen_arc_divisions, edited_far.chosen_arc_divisions);
        println!(
            "presentation divisions far {} close {} edited far {} close {}",
            far_product.chosen_arc_divisions,
            close_product.chosen_arc_divisions,
            edited_far.chosen_arc_divisions,
            edited_close.chosen_arc_divisions
        );
        let wire = round_wire_mesh(&replayed.fillet, close_product.chosen_arc_divisions).expect("wire");
        assert!(wire.index_count() / 3 >= close_product.chosen_arc_divisions * 2);
    }

    #[test]
    fn the_front_edge_round_replaces_the_spliced_corner_and_the_cage() {
        let token = "E:grid(F:seed/4,1,1,2)";
        let main = std::fs::read(crate::authored_main_level()).expect("main level");
        let source = crate::parse_level(std::str::from_utf8(&main).unwrap()).expect("parse");
        let record = block_named(&source, "Intent Solid");
        let body = match crate::intent_authority_candidate(&record) {
            crate::IntentAuthorityCandidate::Reconstructable(body) => body,
            crate::IntentAuthorityCandidate::Refused(reason) => panic!("{reason}"),
        };
        let bindings = crate::semantic_edge_bindings(&record).expect("bindings");
        let edge_id = bindings.iter().find(|(name, _)| name == token).map(|(_, id)| *id).expect("token");
        assert_eq!(bindings.iter().filter(|(_, id)| *id == edge_id).count(), 1);
        let chain = logical_edge_chain(&body, edge_id).expect("logical side");
        assert!(chain.len() >= 2, "the front side is one logical edge split into {chain:?}");
        let persistent = persistent_edge_token(&body, &bindings, edge_id).expect("persistent token");
        assert_eq!(persistent, "E:seed-edge/3");
        for id in &chain {
            assert_eq!(persistent_edge_token(&body, &bindings, *id).expect("fragment token"), persistent);
            assert!(round_cage_segment(&body, &fillet_for_chain(&body, &chain, CURVE_RADIUS_M).expect("probe"), *id).is_none(), "fragment {id} stayed a sharp wire");
        }
        let rounded = crate::commit_class_c_intent(&record, round_entry(token, CURVE_RADIUS_M)).expect("round");
        assert!(rounded.body.is_none());
        let replayed = crate::replay_round(&rounded).expect("replay");
        assert_eq!(replayed.edge_id, chain[0]);
        let (chain_start, chain_end) = chain_endpoints(&body, &chain).expect("endpoints");
        let span = distance(body.vertex_position(chain_start).unwrap(), body.vertex_position(chain_end).unwrap());
        assert!((replayed.fillet.length_m - span).abs() < 1.0e-6, "the fillet covers the logical side");
        let close = presentation_camera(&replayed.fillet, CURVE_PRESENT_CLOSE_M, CURVE_PRESENT_CLOSE_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let far = presentation_camera(&replayed.fillet, 1.6, CURVE_PRESENT_FAR_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let product = realize_round_solid(&rounded, &close, "Close", 1).expect("solid");
        let far_product = realize_round_solid(&rounded, &far, "Far", 1).expect("far");
        assert!(far_product.chosen_arc_divisions < product.chosen_arc_divisions, "close {} far {}", product.chosen_arc_divisions, far_product.chosen_arc_divisions);
        let floor = replayed.fillet.radius_m * 0.25;
        let crown = replayed.fillet.point(replayed.fillet.length_m * 0.5, replayed.fillet.span * 0.5);
        assert!(mesh_has_point(&product.mesh, crown, 1.0e-3), "the arc crown is missing");
        for id in &chain {
            let edge = body.edges.iter().find(|edge| edge.id == *id).expect("fragment");
            let mid = add(body.vertex_position(edge.a).unwrap(), scale(sub(body.vertex_position(edge.b).unwrap(), body.vertex_position(edge.a).unwrap()), 0.5));
            assert!(mesh_min_distance(&product.mesh, mid) > floor, "fragment {id} stayed sharp");
            assert!(round_cage_segment(&body, &replayed.fillet, *id).is_none(), "fragment {id} kept a sharp wire");
        }
        for vertex in [chain_start, chain_end] {
            let position = body.vertex_position(vertex).unwrap();
            assert!(mesh_min_distance(&product.mesh, position) > floor, "corner {vertex} stayed sharp");
        }
        let neighbor = body
            .edges
            .iter()
            .find(|other| !chain.contains(&other.id) && (other.a == chain_end || other.b == chain_end))
            .expect("a side that meets the logical edge");
        let neighbor_mid = add(
            body.vertex_position(neighbor.a).unwrap(),
            scale(sub(body.vertex_position(neighbor.b).unwrap(), body.vertex_position(neighbor.a).unwrap()), 0.5),
        );
        assert!(mesh_has_point(&product.mesh, neighbor_mid, 1.0e-3), "the neighboring edge left the solid");
        let corner = body.vertex_position(chain_end).unwrap();
        let segment = round_cage_segment(&body, &replayed.fillet, neighbor.id).expect("neighbor cage");
        assert!(distance(segment.0, corner) > floor && distance(segment.1, corner) > floor, "the cage still ends on the sharp corner");
        let sibling = bindings
            .iter()
            .find(|(name, id)| *name != token && chain.contains(id))
            .map(|(name, _)| name.clone())
            .expect("another fragment of the same side");
        let edited = crate::commit_class_c_intent(&rounded, round_entry(&sibling, CURVE_EDITED_RADIUS_M)).expect("the fragment edits the same feature");
        let rounds = crate::replay_rounds(&edited).expect("one feature");
        assert_eq!(rounds.len(), 1);
        assert!((rounds[0].radius_m - CURVE_EDITED_RADIUS_M).abs() < 1.0e-12);
        assert_eq!(logical_edge_chain(&body, rounds[0].edge_id).unwrap(), chain);
        println!(
            "front edge close {} far {} persistent {persistent} chain {chain:?} eye {:?} forward {:?}",
            product.chosen_arc_divisions, far_product.chosen_arc_divisions, close.eye, close.forward
        );
    }

    #[test]
    fn round_solid_replaces_one_edge_and_keeps_the_body_absent() {
        let box_body = SolidBody::from_box([2.0, 2.0, 2.0]).expect("box");
        let mut box_edges = 0u32;
        for edge in &box_body.edges {
            let Ok(fillet) = validate_fillet(&box_body, edge.id, CURVE_RADIUS_M) else { continue };
            let tris = emit_round_solid(&box_body, &fillet, 6).unwrap_or_else(|error| panic!("box edge {}: {error}", edge.id));
            let packed = pack_corner(&tris).expect("pack");
            assert_round_solid(&packed.mesh, &fillet, &box_body, edge.a, edge.b, 6);
            assert!(round_cage_segment(&box_body, &fillet, edge.id).is_none(), "box edge {} kept the sharp segment", edge.id);
            let neighbor = box_body
                .edges
                .iter()
                .find(|other| other.id != edge.id && (other.a == edge.a || other.b == edge.a))
                .expect("box neighbor");
            let segment = round_cage_segment(&box_body, &fillet, neighbor.id).expect("box cage");
            let sharp = box_body.vertex_position(edge.a).expect("corner");
            assert!(
                distance(segment.0, sharp) > fillet.radius_m * 0.25 && distance(segment.1, sharp) > fillet.radius_m * 0.25,
                "box edge {} cage still meets the sharp corner",
                edge.id
            );
            box_edges += 1;
        }
        assert!(box_edges >= 12, "box edges {box_edges}");

        let main = std::fs::read(crate::authored_main_level()).expect("main level");
        let source = crate::parse_level(std::str::from_utf8(&main).unwrap()).expect("parse");
        let record = block_named(&source, "Intent Solid");
        assert!(record.body.is_none());
        let body = match crate::intent_authority_candidate(&record) {
            crate::IntentAuthorityCandidate::Reconstructable(body) => body,
            crate::IntentAuthorityCandidate::Refused(reason) => panic!("{reason}"),
        };
        let bindings = crate::semantic_edge_bindings(&record).expect("bindings");
        let mut accepted: Option<(String, u32)> = None;
        let mut refused = 0u32;
        for edge in &body.edges {
            let Ok(fillet) = validate_fillet(&body, edge.id, CURVE_RADIUS_M) else {
                refused += 1;
                continue;
            };
            match emit_round_solid(&body, &fillet, 4) {
                Ok(_) => {
                    if accepted.is_none() && bindings.iter().filter(|(_, id)| *id == edge.id).count() == 1 {
                        let token = bindings.iter().find(|(_, id)| *id == edge.id).unwrap().0.clone();
                        accepted = Some((token, edge.id));
                    }
                }
                Err(_) => refused += 1,
            }
        }
        let (token, edge_id) = accepted.expect("at least one intent edge must round");
        let rounded = crate::commit_class_c_intent(&record, round_entry(&token, CURVE_RADIUS_M)).expect("round");
        assert!(rounded.body.is_none());
        let replayed = crate::replay_round(&rounded).expect("replay");
        assert_eq!(replayed.edge_id, edge_id);
        let far = presentation_camera(&replayed.fillet, CURVE_PRESENT_FAR_M, CURVE_PRESENT_FAR_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let close = presentation_camera(&replayed.fillet, CURVE_PRESENT_CLOSE_M, CURVE_PRESENT_CLOSE_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let far_product = realize_round_solid(&rounded, &far, "Far", 1).expect("far solid");
        let close_product = realize_round_solid(&rounded, &close, "Close", 1).expect("close solid");
        assert!(rounded.body.is_none());
        assert!(close_product.chosen_arc_divisions > far_product.chosen_arc_divisions, "close {} far {}", close_product.chosen_arc_divisions, far_product.chosen_arc_divisions);
        assert_eq!(far_product.triangles_constructed, far_product.mesh.index_count() / 3);
        assert!(far_product.triangles_constructed > far_product.chosen_arc_divisions * 2);
        assert_eq!(far_product.triangles_discarded_after_construction, 0);
        assert!(!far_product.record_body_consulted);
        let mid = add(replayed.fillet.origin, scale(replayed.fillet.direction, replayed.fillet.length_m * 0.5));
        assert!(mesh_min_distance(&far_product.mesh, mid) > replayed.fillet.radius_m * 0.25, "the solid still contains the sharp edge");
        let crown = replayed.fillet.point(replayed.fillet.length_m * 0.5, replayed.fillet.span * 0.5);
        assert!(mesh_has_point(&far_product.mesh, crown, 1.0e-3), "the arc crown is missing");
        let (start_id, end_id) = body.edges.iter().find(|edge| edge.id == edge_id).map(|edge| (edge.a, edge.b)).unwrap();
        let other = body.vertices.iter().find(|vertex| vertex.id != start_id && vertex.id != end_id).expect("other vertex");
        assert!(mesh_has_point(&far_product.mesh, other.position, 1.0e-3), "the solid dropped a face away from the edge");
        let mut poisoned = rounded.clone();
        poisoned.size_m = [9.0, 9.0, 9.0];
        let poisoned_far = realize_round_solid(&poisoned, &far, "Far", 1).expect("poison");
        assert_eq!(poisoned_far.position_hash, far_product.position_hash);
        println!("round solid box_edges {box_edges} intent_refused {refused} edge {edge_id} far {} close {}", far_product.chosen_arc_divisions, close_product.chosen_arc_divisions);
    }

    fn assert_round_solid(mesh: &Mesh, fillet: &Fillet, body: &SolidBody, start_id: u32, end_id: u32, divisions: u32) {
        let mid = add(fillet.origin, scale(fillet.direction, fillet.length_m * 0.5));
        assert!(mesh_min_distance(mesh, mid) > fillet.radius_m * 0.25, "sharp edge remains");
        let crown = fillet.point(fillet.length_m * 0.5, fillet.span * 0.5);
        assert!(mesh_has_point(mesh, crown, 1.0e-3), "crown missing");
        let other = body.vertices.iter().find(|vertex| vertex.id != start_id && vertex.id != end_id).expect("other");
        assert!(mesh_has_point(mesh, other.position, 1.0e-3), "missing solid vertex");
        assert!(mesh.index_count() / 3 > divisions * 2);
    }

    #[test]
    fn the_curve_project_saves_one_round_without_a_body() {
        let main_before = std::fs::read(crate::authored_main_level()).expect("main");
        assert_eq!(main_before.len() as u64, CURVE_MAIN_LEVEL_BYTES);
        let directory = curve_proof_dir();
        let _ = std::fs::remove_dir_all(&directory);
        let project = write_authored_curve_project(&directory).expect("curve project");
        let main_after = std::fs::read(crate::authored_main_level()).expect("main after");
        assert_eq!(main_after, main_before);
        let level_path = directory.join("Content").join("Levels").join("Curve.jarviglevel");
        let bytes = std::fs::read(&level_path).expect("level bytes");
        let text = String::from_utf8(bytes).expect("utf8");
        assert!(text.contains("\"op\": \"round\""));
        assert!(text.contains("\"version\": 3"));
        assert!(!text.contains("\"op\": \"analytic-surface\""));
        let parsed = crate::parse_level(&text).expect("parse");
        assert_eq!(parsed.format_version, 6);
        let record = block_named(&parsed, "Intent Solid");
        let legacy = block_named(&parsed, "Legacy Cube");
        assert!(record.body.is_none());
        assert!(legacy.body.is_some());
        assert_eq!(record.intent.len(), 66);
        assert_eq!(record.history.len(), 24);
        assert_eq!(crate::intent_authority_diagnostic(&record), "Intent Authority: ELIGIBLE");
        let rounds: Vec<_> = record.intent.iter().filter(|entry| matches!(entry.payload, IntentPayload::Round { .. })).collect();
        assert_eq!(rounds.len(), 1);
        let IntentPayload::Round { radius_m } = rounds[0].payload else { unreachable!() };
        assert_eq!(radius_m.to_bits(), CURVE_RADIUS_M.to_bits());
        assert_eq!(rounds[0].groups, Some(vec![vec![rounds[0].groups.as_ref().unwrap()[0][0].clone()]]));
        let token = rounds[0].groups.as_ref().unwrap()[0][0].clone();
        assert!(token.starts_with("E:"));
        let replayed = crate::replay_round(&record).expect("replay");
        assert_eq!(replayed.token, token);
        assert_eq!(replayed.radius_m.to_bits(), CURVE_RADIUS_M.to_bits());
        assert_eq!(record.size_m, replayed.cache_extent);
        assert_eq!(record.size_m, crate::class_c_cache_extent(&record).unwrap());
        assert_eq!(feature_id(&token), feature_id(&replayed.token));
        assert!(project.ends_with("AuthoredCurve.jarvigproject"));
    }

    #[test]
    fn the_round_handle_edits_radius_without_a_replay() {
        let body = SolidBody::from_box([2.0, 2.0, 2.0]).expect("box");
        let fillet = body
            .edges
            .iter()
            .find_map(|edge| {
                let fillet = validate_fillet(&body, edge.id, 0.05).ok()?;
                fillet.outward_bisector().ok().filter(|bisector| bisector[1].abs() < 0.95)?;
                Some(fillet)
            })
            .expect("one edge whose crown can face the camera");
        let edge = fillet.edge_id;
        let maximum = maximum_round_radius(&body, edge).expect("maximum");
        assert!(maximum > 0.05, "{maximum}");
        assert!(round_edge_closes(&body, edge, maximum).is_ok());
        assert!(round_edge_closes(&body, edge, maximum + 0.01).is_err(), "past the clamp must refuse");
        let scaled = fillet_scaled(&fillet, 0.04).expect("scale");
        assert_eq!(scaled.radius_m, 0.04);
        assert!((scaled.tangent_m - fillet.tangent_m * 0.8).abs() < 1.0e-12);
        let (step, away) = fillet_crown_step(&fillet).expect("crown");
        let (scaled_step, scaled_away) = fillet_crown_step(&scaled).expect("scaled crown");
        assert!((step - scaled_step).abs() < 1.0e-9);
        assert!(distance(away, scaled_away) < 1.0e-9);
        let sharp = add(fillet.origin, scale(fillet.direction, fillet.length_m * 0.5));
        let crown = fillet.point(fillet.length_m * 0.5, fillet.span * 0.5);
        let smaller = scaled.point(scaled.length_m * 0.5, scaled.span * 0.5);
        assert!(distance(sharp, smaller) < distance(sharp, crown), "a smaller radius stays closer to the sharp edge");
        let grown = dragged_round_radius(0.05, step * 0.01, step, false, None, 0.001, maximum);
        assert!((grown - 0.06).abs() < 1.0e-9, "{grown}");
        let fine = dragged_round_radius(0.05, step * 0.01, step, true, None, 0.001, maximum);
        assert!((fine - 0.051).abs() < 1.0e-9, "{fine}");
        let held = dragged_round_radius(0.05, step * 0.004, step, false, Some(0.01), 0.001, maximum);
        assert!((held - 0.05).abs() < 1.0e-9, "{held}");
        let snapped = dragged_round_radius(0.05, step * 0.006, step, false, Some(0.01), 0.001, maximum);
        assert!((snapped - 0.06).abs() < 1.0e-9, "{snapped}");
        let stopped = dragged_round_radius(0.05, step * 10.0, step, false, None, 0.001, 0.08);
        assert!((stopped - 0.08).abs() < 1.0e-12);
        let camera = observation_camera(&fillet, 1.6, 960.0, 700.0, CURVE_FOV, CURVE_ERROR_PX).expect("camera");
        let (mesh, divisions) = observe_resolved_fillet(&body, &fillet, &camera).expect("observation");
        assert!(divisions >= 1);
        assert!(mesh.triangle_indices().len() > 12, "the fillet adds triangles to the box");
        let mid = add(fillet.origin, scale(fillet.direction, fillet.length_m * 0.5));
        assert!(mesh_min_distance(&mesh, mid) > fillet.radius_m * 0.25, "the observation still contains the sharp edge");
    }

    #[test]
    fn a_committed_round_is_rebuilt_without_the_preview_mesh() {
        let main = std::fs::read(crate::authored_main_level()).expect("main level");
        let source = crate::parse_level(std::str::from_utf8(&main).unwrap()).expect("parse");
        let sharp = block_named(&source, "Intent Solid");
        assert!(sharp.body.is_none());
        assert!(crate::replay_round(&sharp).is_err(), "the sharp tape has no round");
        let body = match crate::intent_authority_candidate(&sharp) {
            crate::IntentAuthorityCandidate::Reconstructable(body) => body,
            crate::IntentAuthorityCandidate::Refused(reason) => panic!("{reason}"),
        };
        let token = "E:grid(F:seed/4,1,1,2)";
        let bindings = crate::semantic_edge_bindings(&sharp).expect("bindings");
        let edge_id = bindings.iter().find(|(name, _)| name == token).map(|(_, id)| *id).expect("front edge");
        let maximum = maximum_round_radius(&body, edge_id).expect("maximum");
        let radius = if maximum >= 1.217 { 1.217 } else { maximum };
        assert!(radius > 0.05, "maximum {maximum}");
        let chain = logical_edge_chain(&body, edge_id).expect("logical side");
        let scaled = fillet_for_chain(&body, &chain, radius).expect("chain fillet");
        let camera = observation_camera(&scaled, radius * 8.0, 640.0, 360.0, CURVE_FOV, CURVE_ERROR_PX).expect("camera");
        let (preview, preview_divisions) = observe_resolved_fillets(&body, std::slice::from_ref(&scaled), &camera).expect("preview");
        let preview_positions: Vec<[f32; 3]> = (0..preview.vertex_count()).map(|index| preview.position(index).unwrap()).collect();
        drop(preview);
        let committed = crate::commit_class_c_intent(&sharp, round_entry(token, radius)).expect("commit");
        assert!(committed.body.is_none(), "the round does not store a body");
        assert!(matches!(committed.intent.last().unwrap().payload, IntentPayload::Round { radius_m } if (radius_m - radius).abs() < 1.0e-12));
        let product = realize_round_solid(&committed, &camera, "Viewport", 1).expect("committed realization");
        assert_eq!(product.semantic_edge_id, token);
        assert!((product.radius_m - radius).abs() < 1.0e-12);
        assert_eq!(product.chosen_arc_divisions, preview_divisions);
        assert_eq!(product.mesh.vertex_count() as usize, preview_positions.len());
        for index in 0..preview_positions.len() {
            let committed_position = product.mesh.position(index as u32).unwrap();
            let delta = sub(
                [f64::from(committed_position[0]), f64::from(committed_position[1]), f64::from(committed_position[2])],
                [f64::from(preview_positions[index][0]), f64::from(preview_positions[index][1]), f64::from(preview_positions[index][2])],
            );
            assert!(distance(delta, [0.0; 3]) < 1.0e-4, "vertex {index} moved after the preview was dropped");
        }
        let replayed = crate::replay_round(&committed).expect("replay");
        let sharp_point = add(replayed.fillet.origin, scale(replayed.fillet.direction, replayed.fillet.length_m * 0.5));
        let crown = replayed.fillet.point(replayed.fillet.length_m * 0.5, replayed.fillet.span * 0.5);
        assert!(mesh_min_distance(&product.mesh, sharp_point) > radius * 0.25, "the committed mesh still contains the sharp edge");
        assert!(mesh_has_point(&product.mesh, crown, 1.0e-3), "the committed mesh lost the crown");
        let directory = std::env::temp_dir().join("jarvig-round-apply-chain");
        let path = write_level_copy(&directory, &source, committed.clone()).expect("save");
        let loaded_text = std::fs::read_to_string(&path).expect("reload text");
        let loaded = block_named(&crate::parse_level(&loaded_text).expect("reload"), "Intent Solid");
        assert!(loaded.body.is_none());
        let loaded_product = realize_round_solid(&loaded, &camera, "Viewport", 2).expect("reloaded realization");
        assert_eq!(loaded_product.position_hash, product.position_hash);
        assert!((crate::replay_round(&loaded).unwrap().radius_m - radius).abs() < 1.0e-12);
        let disconnected = disjoint_semantic_edge(&body, &bindings, &chain, radius).expect("a disconnected semantic edge accepts the radius");
        let second = crate::commit_class_c_intent(&committed, round_entry(&disconnected, radius)).expect("a disconnected edge stays beside the fillet");
        assert_eq!(committed.intent.len(), sharp.intent.len() + 1, "the caller's tape stays unchanged");
        assert!(second.body.is_none());
        assert_eq!(second.intent.len(), sharp.intent.len() + 2);
        let rounds = crate::replay_rounds(&second).expect("both fillets");
        assert_eq!(rounds.len(), 2, "{:?}", rounds.iter().map(|round| round.token.as_str()).collect::<Vec<_>>());
        assert!(rounds.iter().any(|round| round.token == token));
        assert!(rounds.iter().any(|round| round.token == disconnected));
        assert!(rounds.iter().all(|round| (round.radius_m - radius).abs() < 1.0e-12));
        let both = realize_round_solid(&second, &camera, "Viewport", 3).expect("both realizations");
        assert!(both.semantic_edge_id.contains(token) && both.semantic_edge_id.contains(&disconnected));
        assert!(mesh_min_distance(&both.mesh, sharp_point) > radius * 0.25, "the first edge went sharp");
        let added = rounds.iter().find(|round| round.token == disconnected).unwrap();
        let added_sharp = add(added.fillet.origin, scale(added.fillet.direction, added.fillet.length_m * 0.5));
        assert!(mesh_min_distance(&both.mesh, added_sharp) > radius * 0.25, "the added edge stayed sharp");
        let adjacent = adjacent_semantic_edge(&body, &bindings, &chain).expect("an adjacent semantic edge");
        let clash = crate::commit_class_c_intent(&committed, round_entry(&adjacent, CURVE_RADIUS_M));
        assert!(clash.is_err(), "a different radius on an adjacent edge must refuse");
        assert_eq!(committed.intent.len(), sharp.intent.len() + 1);
        let huge = crate::commit_class_c_intent(&committed, round_entry(&disconnected, 10.0));
        assert!(huge.is_err(), "a radius of 10 m must refuse");
        assert_eq!(committed.intent.len(), sharp.intent.len() + 1);
        let missing = crate::commit_class_c_intent(&committed, IntentEntry { groups: None, payload: IntentPayload::Round { radius_m: CURVE_RADIUS_M } });
        assert!(missing.is_err(), "a missing group must refuse");
        assert_eq!(committed.intent.len(), sharp.intent.len() + 1);
    }

    #[test]
    fn a_round_set_resolves_fragments_and_names_a_conflict() {
        let main = std::fs::read(crate::authored_main_level()).expect("main level");
        let source = crate::parse_level(std::str::from_utf8(&main).unwrap()).expect("parse");
        let record = block_named(&source, "Intent Solid");
        let body = match crate::intent_authority_candidate(&record) {
            crate::IntentAuthorityCandidate::Reconstructable(body) => body,
            crate::IntentAuthorityCandidate::Refused(reason) => panic!("{reason}"),
        };
        let bindings = crate::semantic_edge_bindings(&record).expect("bindings");
        let fragment = "E:grid(F:seed/4,1,1,2)";
        let fragment_id = bindings.iter().find(|(name, _)| name == fragment).map(|(_, id)| *id).expect("fragment");
        let chain = logical_edge_chain(&body, fragment_id).expect("chain");
        assert!(chain.len() >= 2, "{chain:?}");
        let persistent = persistent_edge_token(&body, &bindings, fragment_id).expect("persistent");
        assert_eq!(persistent, "E:seed-edge/3");
        for id in &chain {
            assert_eq!(persistent_edge_token(&body, &bindings, *id).unwrap(), persistent);
        }
        let rounded = crate::commit_class_c_intent(&record, round_entry(fragment, CURVE_RADIUS_M)).expect("fragment round");
        let edited = crate::commit_class_c_intent(&rounded, round_entry(&persistent, CURVE_EDITED_RADIUS_M)).expect("same feature");
        assert_eq!(rounded.intent.len(), record.intent.len() + 1, "the caller's tape stays unchanged");
        let rounds = crate::replay_rounds(&edited).expect("one feature");
        assert_eq!(rounds.len(), 1, "{:?}", rounds.iter().map(|round| &round.token).collect::<Vec<_>>());
        assert_eq!(rounds[0].token, persistent);
        assert!((rounds[0].radius_m - CURVE_EDITED_RADIUS_M).abs() < 1.0e-12);
        assert_eq!(logical_edge_chain(&body, rounds[0].edge_id).unwrap(), chain);
        let sibling = bindings
            .iter()
            .find(|(name, id)| *name != fragment && *name != persistent && chain.contains(id))
            .map(|(name, _)| name.clone())
            .expect("sibling fragment");
        let together = crate::commit_class_c_intent(&record, round_entry_set(&[fragment.to_string(), sibling], CURVE_RADIUS_M)).expect("one gesture");
        assert_eq!(crate::replay_rounds(&together).unwrap().len(), 1, "two fragments of one side are one round");

        let features = semantic_fillets(&body, &bindings, CURVE_RADIUS_M);
        let persistent_fillet = features_fillet(&features, &persistent);
        let disconnected = features
            .iter()
            .find(|(token, other, fillet)| {
                *token != persistent && !other.iter().any(|edge| chain.contains(edge)) && !chain_endpoints_touch(&body, &chain, other) && round_fillets_close(&body, &[persistent_fillet.clone(), fillet.clone()]).is_ok()
            })
            .map(|(token, _, _)| token.clone())
            .expect("a disconnected semantic edge");
        // The edited fillet above is 0.04 m. Pair the original sharp tape at one shared radius.
        let pair = crate::commit_class_c_intent(&record, round_entry_set(&[persistent.clone(), disconnected.clone()], CURVE_RADIUS_M)).expect("two edges, one radius");
        assert_eq!(record.intent.len() + 1, pair.intent.len());
        let pair_rounds = crate::replay_rounds(&pair).expect("pair");
        assert_eq!(pair_rounds.len(), 2);
        assert!(pair_rounds.iter().all(|round| (round.radius_m - CURVE_RADIUS_M).abs() < 1.0e-12));
        assert!(pair_rounds.iter().any(|round| round.token == persistent));
        assert!(pair_rounds.iter().any(|round| round.token == disconnected));
        let camera = observation_camera(&pair_rounds[0].fillet, 1.6, 640.0, 360.0, CURVE_FOV, CURVE_ERROR_PX).expect("camera");
        let pair_mesh = realize_round_solid(&pair, &camera, "Viewport", 1).expect("pair mesh");
        for round in &pair_rounds {
            let mid = add(round.fillet.origin, scale(round.fillet.direction, round.fillet.length_m * 0.5));
            let crown = round.fillet.point(round.fillet.length_m * 0.5, round.fillet.span * 0.5);
            assert!(mesh_min_distance(&pair_mesh.mesh, mid) > CURVE_RADIUS_M * 0.25, "{} stayed sharp", round.token);
            let step = round.fillet.span / f64::from(pair_mesh.chosen_arc_divisions.max(1));
            let sagitta = round.fillet.radius_m * (1.0 - (step * 0.5).cos());
            assert!(mesh_min_distance(&pair_mesh.mesh, crown) <= sagitta + 1.0e-4, "{} lost its crown", round.token);
        }
        assert!(crate::replay_round(&record).is_err(), "undo of the one transaction removes both fillets");

        let adjacent = features
            .iter()
            .find(|(token, other, fillet)| {
                *token != persistent && !other.iter().any(|edge| chain.contains(edge)) && chain_endpoints_touch(&body, &chain, other) && round_fillets_close(&body, &[persistent_fillet.clone(), fillet.clone()]).is_ok()
            })
            .map(|(token, _, _)| token.clone())
            .expect("an adjacent semantic edge at the same radius");
        let side_by_side = crate::commit_class_c_intent(&record, round_entry_set(&[persistent.clone(), adjacent.clone()], CURVE_RADIUS_M)).expect("adjacent edges");
        let directory = std::env::temp_dir().join("jarvig-round-edge-set");
        let path = write_level_copy(&directory, &source, side_by_side.clone()).expect("save");
        let text = std::fs::read_to_string(&path).expect("text");
        assert!(text.contains("\"op\": \"round\""));
        assert!(text.contains(&persistent));
        assert!(text.contains(&adjacent));
        assert!(!text.contains("divisions"));
        assert!(!text.contains("arc_divisions"));
        let loaded = block_named(&crate::parse_level(&text).expect("reload"), "Intent Solid");
        assert!(loaded.body.is_none());
        let groups = loaded.intent.last().unwrap().groups.clone().expect("groups");
        assert_eq!(groups.len(), 2);
        for group in &groups {
            assert_eq!(group.len(), 1);
            assert!(group[0].starts_with("E:"), "{}", group[0]);
        }
        let loaded_rounds = crate::replay_rounds(&loaded).expect("reloaded set");
        assert_eq!(loaded_rounds.len(), 2);
        assert!(loaded_rounds.iter().any(|round| round.token == persistent));
        assert!(loaded_rounds.iter().any(|round| round.token == adjacent));
        let loaded_camera = presentation_camera(&loaded_rounds[0].fillet, CURVE_PRESENT_CLOSE_M, CURVE_PRESENT_CLOSE_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let loaded_mesh = realize_round_solid(&loaded, &loaded_camera, "Close", 1).expect("reloaded mesh");
        for round in &loaded_rounds {
            let mid = add(round.fillet.origin, scale(round.fillet.direction, round.fillet.length_m * 0.5));
            assert!(mesh_min_distance(&loaded_mesh.mesh, mid) > CURVE_RADIUS_M * 0.25, "{} went sharp after reload", round.token);
        }
        let far = presentation_camera(&loaded_rounds[0].fillet, CURVE_PRESENT_FAR_M, CURVE_PRESENT_FAR_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let close = presentation_camera(&loaded_rounds[0].fillet, CURVE_PRESENT_CLOSE_M, CURVE_PRESENT_CLOSE_WIDTH, CURVE_PRESENT_HEIGHT, CURVE_FOV, CURVE_ERROR_PX).unwrap();
        let far_product = realize_round_solid(&loaded, &far, "Far", 1).unwrap();
        let close_product = realize_round_solid(&loaded, &close, "Close", 1).unwrap();
        assert!(close_product.chosen_arc_divisions > far_product.chosen_arc_divisions, "close {} far {}", close_product.chosen_arc_divisions, far_product.chosen_arc_divisions);
        assert_eq!(close_product.position_hash, loaded_mesh.position_hash);

        let kept = crate::commit_class_c_intent(&record, round_entry(&persistent, CURVE_RADIUS_M)).expect("kept");
        let clash = crate::commit_class_c_intent(&kept, round_entry(&adjacent, CURVE_EDITED_RADIUS_M));
        assert!(clash.is_err(), "overlapping radii must refuse");
        assert_eq!(kept.intent.len(), record.intent.len() + 1);
        let kept_round = crate::replay_round(&kept).unwrap();
        let kept_feature = RoundFeature { token: kept_round.token.clone(), fillet: kept_round.fillet.clone(), chain: chain.clone() };
        let adjacent_feature = features.iter().find(|(token, _, _)| token == &adjacent).map(|(_, other, fillet)| RoundFeature { token: adjacent.clone(), fillet: fillet.clone(), chain: other.clone() }).unwrap();
        match assess_round_set(&body, &[kept_feature], &[adjacent_feature], CURVE_EDITED_RADIUS_M) {
            RoundAssessment::Ready { radius_m, clamped, .. } => {
                assert!(clamped, "the requested radius differs from the neighbor");
                assert!((radius_m - CURVE_RADIUS_M).abs() < 1.0e-6, "{radius_m}");
                let applied = crate::commit_class_c_intent(&kept, round_entry(&adjacent, radius_m)).expect("the clamped radius applies");
                let applied_rounds = crate::replay_rounds(&applied).unwrap();
                assert_eq!(applied_rounds.len(), 2);
                assert!(applied_rounds.iter().all(|round| (round.radius_m - CURVE_RADIUS_M).abs() < 1.0e-9));
            }
            RoundAssessment::Conflict { tokens, reason } => {
                assert!(tokens.iter().any(|token| token == &adjacent || token == &persistent), "{tokens:?} {reason}");
                assert!(reason.contains(&adjacent) || reason.contains("radii") || reason.contains("neighbor"), "{reason}");
            }
        }
    }

    fn features_fillet(features: &[(String, Vec<u32>, Fillet)], token: &str) -> Fillet {
        features.iter().find(|(name, _, _)| name == token).map(|(_, _, fillet)| fillet.clone()).expect("feature")
    }

    fn semantic_fillets(body: &SolidBody, bindings: &[(String, u32)], radius_m: f64) -> Vec<(String, Vec<u32>, Fillet)> {
        let mut seen = Vec::new();
        let mut features = Vec::new();
        for (_, id) in bindings {
            let Ok(token) = persistent_edge_token(body, bindings, *id) else { continue };
            if seen.iter().any(|have: &String| have == &token) {
                continue;
            }
            let Ok(chain) = logical_edge_chain(body, *id) else { continue };
            let Ok(fillet) = fillet_for_chain(body, &chain, radius_m) else { continue };
            seen.push(token.clone());
            features.push((token, chain, fillet));
        }
        features
    }

    fn disjoint_semantic_edge(body: &SolidBody, bindings: &[(String, u32)], chain: &[u32], radius_m: f64) -> Option<String> {
        let kept = fillet_for_chain(body, chain, radius_m).ok()?;
        let mut seen = Vec::new();
        for (_, id) in bindings {
            let Ok(token) = persistent_edge_token(body, bindings, *id) else { continue };
            if seen.iter().any(|have: &String| have == &token) {
                continue;
            }
            seen.push(token.clone());
            let Ok(other) = logical_edge_chain(body, *id) else { continue };
            if other.iter().any(|edge| chain.contains(edge)) || chain_endpoints_touch(body, chain, &other) {
                continue;
            }
            let Ok(fillet) = fillet_for_chain(body, &other, radius_m) else { continue };
            if round_fillets_close(body, &[kept.clone(), fillet]).is_ok() {
                return Some(token);
            }
        }
        None
    }

    fn adjacent_semantic_edge(body: &SolidBody, bindings: &[(String, u32)], chain: &[u32]) -> Option<String> {
        let mut seen = Vec::new();
        for (_, id) in bindings {
            let Ok(token) = persistent_edge_token(body, bindings, *id) else { continue };
            if seen.iter().any(|have: &String| have == &token) {
                continue;
            }
            seen.push(token.clone());
            let Ok(other) = logical_edge_chain(body, *id) else { continue };
            if other.iter().any(|edge| chain.contains(edge)) || !chain_endpoints_touch(body, chain, &other) {
                continue;
            }
            if fillet_for_chain(body, &other, CURVE_RADIUS_M).is_ok() {
                return Some(token);
            }
        }
        None
    }

    fn chain_endpoints_touch(body: &SolidBody, left: &[u32], right: &[u32]) -> bool {
        let Ok((left_start, left_end)) = chain_endpoints(body, left) else { return false };
        let Ok((right_start, right_end)) = chain_endpoints(body, right) else { return false };
        [left_start, left_end].into_iter().any(|vertex| vertex == right_start || vertex == right_end)
    }

    fn mesh_has_point(mesh: &Mesh, point: [f64; 3], tolerance: f64) -> bool {
        mesh_min_distance(mesh, point) <= tolerance
    }

    fn mesh_min_distance(mesh: &Mesh, point: [f64; 3]) -> f64 {
        let mut best = f64::MAX;
        for triangle in mesh.triangle_indices() {
            let Some(a) = mesh.position(triangle[0]) else { continue };
            let Some(b) = mesh.position(triangle[1]) else { continue };
            let Some(c) = mesh.position(triangle[2]) else { continue };
            best = best.min(point_triangle_distance(point, vertex_f64(a), vertex_f64(b), vertex_f64(c)));
        }
        best
    }

    fn vertex_f64(position: [f32; 3]) -> [f64; 3] {
        [f64::from(position[0]), f64::from(position[1]), f64::from(position[2])]
    }

    fn point_triangle_distance(point: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
        let ab = sub(b, a);
        let ac = sub(c, a);
        let ap = sub(point, a);
        let d1 = dot(ab, ap);
        let d2 = dot(ac, ap);
        if d1 <= 0.0 && d2 <= 0.0 {
            return distance(point, a);
        }
        let bp = sub(point, b);
        let d3 = dot(ab, bp);
        let d4 = dot(ac, bp);
        if d3 >= 0.0 && d4 <= d3 {
            return distance(point, b);
        }
        let vc = d1 * d4 - d3 * d2;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            let v = d1 / (d1 - d3);
            return distance(point, add(a, scale(ab, v)));
        }
        let cp = sub(point, c);
        let d5 = dot(ab, cp);
        let d6 = dot(ac, cp);
        if d6 >= 0.0 && d5 <= d6 {
            return distance(point, c);
        }
        let vb = d5 * d2 - d1 * d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            let w = d2 / (d2 - d6);
            return distance(point, add(a, scale(ac, w)));
        }
        let va = d3 * d6 - d5 * d4;
        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
            let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
            return distance(point, add(b, scale(sub(c, b), w)));
        }
        let denom = va + vb + vc;
        if denom.abs() < 1.0e-20 {
            return distance(point, a);
        }
        let v = vb / denom;
        let w = vc / denom;
        distance(point, add(a, add(scale(ab, v), scale(ac, w))))
    }

    fn block_named(document: &crate::LevelDocument, name: &str) -> BlockRecord {
        document
            .entities
            .iter()
            .find(|entity| entity.name == name)
            .and_then(|entity| entity.components.iter().find_map(|component| match component {
                crate::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                _ => None,
            }))
            .expect(name)
    }

    fn write_level_copy(directory: &Path, source: &crate::LevelDocument, block: BlockRecord) -> Result<PathBuf, String> {
        let _ = std::fs::remove_dir_all(directory);
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let mut solid = source.entities.iter().find(|entity| entity.name == "Intent Solid").unwrap().clone();
        install_block(&mut solid, block);
        let document = crate::LevelDocument {
            format_version: source.format_version,
            level_uuid: source.level_uuid,
            name: source.name.clone(),
            world_settings: source.world_settings.clone(),
            organization: source.organization.clone(),
            entities: vec![
                source.entities.iter().find(|entity| entity.name == "World Settings").unwrap().clone(),
                solid,
                source.entities.iter().find(|entity| entity.name == "Legacy Cube").unwrap().clone(),
            ],
        };
        let path = directory.join("Curve.jarviglevel");
        let backup = directory.join("Saved").join("Backup");
        crate::save_level_atomic(&path, &backup, &document).map_err(|error| error.to_string())?;
        Ok(path)
    }
}
