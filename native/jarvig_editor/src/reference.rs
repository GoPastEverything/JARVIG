//! Editor ground grid and axis triad. Not a level entity, not saved, and not drawn in Play.
//!
//! Points from [`editor_reference_segments`] are scene-local. This module lifts them
//! into the bootstrap root and turns each segment into a camera-relative box. The
//! block's analytic box and its derived mesh do not read these lines.

use jarvig_core::{camera_relative_f32, editor_reference_segments, Vec3, BOOTSTRAP_ROOT_M};
use jarvig_renderer::OverlayVertex;

/// World length of the positive axes. About 70 pixels at the scene origin, before the core clamp.
pub fn axis_length_m(camera_world: Vec3, vertical_fov_radians: f64, viewport_height: u32) -> f64 {
    let dx = camera_world.x - BOOTSTRAP_ROOT_M;
    let distance = (dx * dx + camera_world.y * camera_world.y + camera_world.z * camera_world.z).sqrt().max(0.25);
    let half = (viewport_height.max(1) as f64) * 0.5;
    let fov = if vertical_fov_radians.is_finite() && vertical_fov_radians > 0.0 { vertical_fov_radians } else { 1.0 };
    distance * (fov * 0.5).tan() / half.max(1.0) * 70.0
}

/// Depth-tested reference lines for one editor camera. Empty when the camera is not finite.
pub fn reference_vertices(camera_world: Vec3, vertical_fov_radians: f64, viewport_height: u32) -> Vec<OverlayVertex> {
    if !camera_world.x.is_finite() || !camera_world.y.is_finite() || !camera_world.z.is_finite() {
        return Vec::new();
    }
    let length = axis_length_m(camera_world, vertical_fov_radians, viewport_height);
    let segments = editor_reference_segments(camera_world.x - BOOTSTRAP_ROOT_M, camera_world.z, length);
    let mut vertices = Vec::new();
    for segment in segments {
        let from = scene_to_world(segment.from);
        let to = scene_to_world(segment.to);
        let pixels = if segment.from.y < 0.0 { 1.1 } else { 2.6 };
        let radius = line_radius(camera_world, from, to, vertical_fov_radians, viewport_height, pixels);
        push_box(&mut vertices, from, camera_world, to, radius, segment.color);
    }
    vertices
}

fn scene_to_world(point: Vec3) -> Vec3 {
    Vec3::new(point.x + BOOTSTRAP_ROOT_M, point.y, point.z)
}

fn line_radius(camera: Vec3, from: Vec3, to: Vec3, fov: f64, height: u32, pixels: f64) -> f64 {
    let mid = Vec3::new((from.x + to.x) * 0.5, (from.y + to.y) * 0.5, (from.z + to.z) * 0.5);
    let dx = mid.x - camera.x;
    let dy = mid.y - camera.y;
    let dz = mid.z - camera.z;
    let distance = (dx * dx + dy * dy + dz * dz).sqrt().max(0.25);
    let half = (height.max(1) as f64) * 0.5;
    let fov = if fov.is_finite() && fov > 0.0 { fov } else { 1.0 };
    (distance * (fov * 0.5).tan() / half.max(1.0) * pixels).clamp(0.002, 0.25)
}

fn push_box(vertices: &mut Vec<OverlayVertex>, origin: Vec3, camera: Vec3, end: Vec3, radius: f64, color: [f32; 4]) {
    let axis = normalize(sub(end, origin)).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let side = perpendicular(axis).scale(radius);
    let up = normalize(axis.cross(side)).unwrap_or(Vec3::new(1.0, 0.0, 0.0)).scale(radius);
    let corners = [
        origin + side + up,
        sub(origin, side) + up,
        sub(sub(origin, side), up),
        sub(origin + side, up),
        end + side + up,
        sub(end, side) + up,
        sub(sub(end, side), up),
        sub(end + side, up),
    ];
    for [a, b, c] in [[0, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6], [0, 4, 5], [0, 5, 1], [1, 5, 6], [1, 6, 2], [2, 6, 7], [2, 7, 3], [3, 7, 4], [3, 4, 0]] {
        push_tri(vertices, camera, corners[a], corners[b], corners[c], color);
    }
}

fn push_tri(vertices: &mut Vec<OverlayVertex>, camera: Vec3, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
    push_vertex(vertices, a, camera, color);
    push_vertex(vertices, b, camera, color);
    push_vertex(vertices, c, camera, color);
}

fn push_vertex(vertices: &mut Vec<OverlayVertex>, world: Vec3, camera: Vec3, color: [f32; 4]) {
    vertices.push(OverlayVertex { position: camera_relative_f32(world, camera), color });
}

fn perpendicular(axis: Vec3) -> Vec3 {
    let reference = if axis.y.abs() < 0.9 { Vec3::new(0.0, 1.0, 0.0) } else { Vec3::new(1.0, 0.0, 0.0) };
    normalize(axis.cross(reference)).unwrap_or(Vec3::new(1.0, 0.0, 0.0))
}

fn sub(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x - right.x, left.y - right.y, left.z - right.z)
}

fn length(value: Vec3) -> f64 {
    (value.x * value.x + value.y * value.y + value.z * value.z).sqrt()
}

fn normalize(value: Vec3) -> Option<Vec3> {
    let length = length(value);
    if length < 1.0e-12 || !length.is_finite() { None } else { Some(value.scale(1.0 / length)) }
}
