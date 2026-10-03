//! Editor transform gizmo. Not a scene entity.
//!
//! Translate and rotate write absolute local values through the engine.
//! Scale is not represented. The spatial frame has no scale property.
//! Geometry is camera-relative float32. The billion-meter origin stays on the CPU.

use jarvig_core::{camera_relative_f32, face_center, face_normal, face_tangent, Quat, Vec3};
use jarvig_renderer::OverlayVertex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformSpace {
    World,
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GizmoHandle {
    AxisX,
    AxisY,
    AxisZ,
    RingX,
    RingY,
    RingZ,
}

impl GizmoHandle {
    pub fn axis(self) -> Vec3 {
        match self {
            Self::AxisX | Self::RingX => Vec3::new(1.0, 0.0, 0.0),
            Self::AxisY | Self::RingY => Vec3::new(0.0, 1.0, 0.0),
            Self::AxisZ | Self::RingZ => Vec3::new(0.0, 0.0, 1.0),
        }
    }

    pub fn is_rotation(self) -> bool {
        matches!(self, Self::RingX | Self::RingY | Self::RingZ)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AxisX => "translate-x",
            Self::AxisY => "translate-y",
            Self::AxisZ => "translate-z",
            Self::RingX => "rotate-x",
            Self::RingY => "rotate-y",
            Self::RingZ => "rotate-z",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GizmoDrag {
    pub entity: jarvig_core::EntityUuid,
    pub handle: GizmoHandle,
    pub origin: Vec3,
    pub axis: Vec3,
    pub original_local: Vec3,
    pub original_rotation: Quat,
    pub original_world_rotation: Quat,
    pub start_hit: Vec3,
}

pub fn visual_length(distance_m: f64, vertical_fov_radians: f64, viewport_height: u32) -> f64 {
    let distance = distance_m.abs().max(0.25);
    let half = (viewport_height.max(1) as f64) * 0.5;
    let world_per_pixel = distance * (vertical_fov_radians * 0.5).tan() / half;
    (world_per_pixel * 90.0).clamp(0.05, 1.0e6)
}

pub fn axis_in_space(handle: GizmoHandle, space: TransformSpace, entity_rotation: Quat) -> Vec3 {
    let local = handle.axis();
    match space {
        TransformSpace::World => local,
        TransformSpace::Local => normalize(entity_rotation.rotate(local)).unwrap_or(local),
    }
}

pub fn hit_gizmo(origin: Vec3, ray_origin: Vec3, ray_direction: Vec3, length: f64, space: TransformSpace, entity_rotation: Quat, rotate: bool) -> Option<GizmoHandle> {
    let handles = if rotate {
        [GizmoHandle::RingX, GizmoHandle::RingY, GizmoHandle::RingZ]
    } else {
        [GizmoHandle::AxisX, GizmoHandle::AxisY, GizmoHandle::AxisZ]
    };
    let mut best: Option<(GizmoHandle, f64)> = None;
    for handle in handles {
        let axis = axis_in_space(handle, space, entity_rotation);
        let distance = if rotate {
            ray_ring(ray_origin, ray_direction, origin, axis, length * 0.85, length * 0.08)
        } else {
            ray_segment(ray_origin, ray_direction, origin, origin + axis.scale(length), length * 0.12)
        };
        let Some(distance) = distance else { continue };
        if best.as_ref().is_some_and(|(_, previous)| *previous <= distance) {
            continue;
        }
        best = Some((handle, distance));
    }
    best.map(|(handle, _)| handle)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn translation_world_point(drag: &GizmoDrag, hit: Vec3) -> Vec3 {
    drag.origin + translation_delta(drag, hit)
}

/// Axis movement from the drag start. The scalar is the constraint.
/// Callers add it to the original local translation. They do not add it to the world origin.
pub fn translation_delta(drag: &GizmoDrag, hit: Vec3) -> Vec3 {
    let start = dot(sub(drag.start_hit, drag.origin), drag.axis);
    let now = dot(sub(hit, drag.origin), drag.axis);
    drag.axis.scale(now - start)
}

/// The visible modeling arrow, from the face center through the tip.
pub fn hit_operation_handle(ray_origin: Vec3, ray_direction: Vec3, center: Vec3, direction: Vec3, length: f64) -> bool {
    let Some(direction) = normalize(direction) else { return false };
    let tip = center + direction.scale(length * 0.92);
    ray_segment(ray_origin, ray_direction, center, tip, length * 0.14).is_some()
}

pub fn axis_drag_plane(axis: Vec3, view_direction: Vec3) -> Vec3 {
    let projected = sub(view_direction, axis.scale(dot(view_direction, axis)));
    normalize(projected).unwrap_or(Vec3::new(0.0, 1.0, 0.0))
}

/// Ring plane when the ring faces the camera. Otherwise the view plane, so an
/// edge-on ring still has a hit. The angle is measured after projecting off the axis.
pub fn rotation_constraint_normal(axis: Vec3, view_direction: Vec3) -> Vec3 {
    let axis = normalize(axis).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let view = normalize(view_direction).unwrap_or(axis);
    if dot(view, axis).abs() > 0.2 {
        axis
    } else {
        view
    }
}

pub fn ray_plane(ray_origin: Vec3, ray_direction: Vec3, point: Vec3, normal: Vec3) -> Option<Vec3> {
    let denom = dot(ray_direction, normal);
    if denom.abs() < 1.0e-8 {
        return None;
    }
    let t = dot(sub(point, ray_origin), normal) / denom;
    if t < 0.0 || !t.is_finite() {
        None
    } else {
        Some(ray_origin + ray_direction.scale(t))
    }
}

pub fn rotation_angle(origin: Vec3, axis: Vec3, start: Vec3, current: Vec3) -> Option<f64> {
    let axis = normalize(axis)?;
    let start = project_plane(sub(start, origin), axis)?;
    let current = project_plane(sub(current, origin), axis)?;
    let cross = start.cross(current);
    let sin = dot(cross, axis);
    let cos = dot(start, current).clamp(-1.0, 1.0);
    let angle = sin.atan2(cos);
    if angle.is_finite() {
        Some(angle)
    } else {
        None
    }
}

fn project_plane(value: Vec3, axis: Vec3) -> Option<Vec3> {
    normalize(sub(value, axis.scale(dot(value, axis))))
}

pub fn world_rotation_after(original: Quat, axis: Vec3, angle: f64) -> Option<Quat> {
    let delta = Quat::from_axis_angle(axis, angle).ok()?;
    Some(unit(delta.mul(original))?)
}

pub fn local_rotation_after(original: Quat, axis: Vec3, angle: f64) -> Option<Quat> {
    let delta = Quat::from_axis_angle(axis, angle).ok()?;
    Some(unit(original.mul(delta))?)
}

pub fn build_gizmo(
    origin: Vec3,
    camera: Vec3,
    length: f64,
    space: TransformSpace,
    entity_rotation: Quat,
    rotate: bool,
    hover: Option<GizmoHandle>,
    active: Option<GizmoHandle>,
) -> Vec<OverlayVertex> {
    let mut vertices = Vec::new();
    let handles = if rotate {
        [GizmoHandle::RingX, GizmoHandle::RingY, GizmoHandle::RingZ]
    } else {
        [GizmoHandle::AxisX, GizmoHandle::AxisY, GizmoHandle::AxisZ]
    };
    for handle in handles {
        let axis = axis_in_space(handle, space, entity_rotation);
        let color = color_for(handle, hover, active);
        if rotate {
            push_ring(&mut vertices, origin, camera, axis, length * 0.85, length * 0.035, color);
        } else {
            let end = origin + axis.scale(length * 0.78);
            let tip = origin + axis.scale(length);
            push_box(&mut vertices, origin, camera, end, length * 0.025, color);
            push_cone(&mut vertices, origin, camera, end, tip, length * 0.07, color);
        }
    }
    vertices
}

fn face_half_spans(face: u8, size: [f64; 3]) -> (f64, f64) {
    match face {
        0 | 1 => (size[2] * 0.5, size[1] * 0.5),
        2 | 3 => (size[0] * 0.5, size[2] * 0.5),
        _ => (size[0] * 0.5, size[1] * 0.5),
    }
}

fn face_corners(translation: Vec3, rotation: Quat, size: [f64; 3], face: u8) -> Option<[Vec3; 4]> {
    let center = face_center(translation, rotation, size, face)?;
    let normal = rotation.rotate(face_normal(face)?);
    let tangent = rotation.rotate(face_tangent(face)?);
    let bitangent = normal.cross(tangent);
    let (along, across) = face_half_spans(face, size);
    let u = tangent.scale(along);
    let v = bitangent.scale(across);
    Some([
        center + u + v,
        sub(center + u, v),
        sub(sub(center, u), v),
        sub(center, u) + v,
    ])
}

/// All twelve edges of the solid. One color, so a whole-object selection is not a face.
pub fn box_outline_vertices(
    translation: Vec3,
    rotation: Quat,
    size: [f64; 3],
    camera: Vec3,
    length: f64,
) -> Vec<OverlayVertex> {
    let mut vertices = Vec::new();
    let color = [0.75, 0.88, 1.0, 1.0];
    let radius = length * 0.012;
    let half = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
    let mut corners = [Vec3::ZERO; 8];
    for index in 0..8 {
        let local = Vec3::new(
            if index & 1 == 0 { -half[0] } else { half[0] },
            if index & 2 == 0 { -half[1] } else { half[1] },
            if index & 4 == 0 { -half[2] } else { half[2] },
        );
        corners[index] = translation + rotation.rotate(local);
    }
    for (start, end) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        push_segment(&mut vertices, camera, corners[start], corners[end], radius, color);
    }
    vertices
}

/// One overlay line. The marquee is four of these, placed along viewport rays.
pub fn push_segment(vertices: &mut Vec<OverlayVertex>, camera: Vec3, start: Vec3, end: Vec3, radius: f64, color: [f32; 4]) {
    push_box(vertices, start, camera, end, radius, color);
}

/// The edges of the faces under the cursor or already chosen. Not a scene entity.
pub fn face_outline_vertices(
    translation: Vec3,
    rotation: Quat,
    size: [f64; 3],
    camera: Vec3,
    length: f64,
    faces: &[u8],
    hot: Option<u8>,
) -> Vec<OverlayVertex> {
    let mut vertices = Vec::new();
    let radius = length * 0.012;
    for face in faces {
        let Some(corners) = face_corners(translation, rotation, size, *face) else { continue };
        let color = face_color(*face, hot, None);
        for index in 0..4 {
            push_box(&mut vertices, corners[index], camera, corners[(index + 1) % 4], radius, color);
        }
    }
    vertices
}

/// One arrow for the open modeling operation. `inward` points the arrow into the face.
pub fn operation_handle_vertices(
    translation: Vec3,
    rotation: Quat,
    size: [f64; 3],
    camera: Vec3,
    length: f64,
    face: u8,
    inward: bool,
    active: bool,
) -> Vec<OverlayVertex> {
    let mut vertices = Vec::new();
    let Some(center) = face_center(translation, rotation, size, face) else { return vertices };
    let Some(normal) = face_normal(face) else { return vertices };
    let mut direction = rotation.rotate(normal);
    if inward {
        direction = direction.scale(-1.0);
    }
    let color = if active { [1.0, 0.95, 0.55, 1.0] } else { face_color(face, None, None) };
    let end = center + direction.scale(length * 0.62);
    let tip = center + direction.scale(length * 0.92);
    push_box(&mut vertices, center, camera, end, length * 0.02, color);
    push_cone(&mut vertices, center, camera, end, tip, length * 0.07, color);
    vertices
}

pub fn scalar_along(delta: Vec3, axis: Vec3) -> f64 {
    dot(delta, axis)
}

fn face_color(face: u8, hover: Option<u8>, active: Option<u8>) -> [f32; 4] {
    if active == Some(face) {
        return [1.0, 0.95, 0.55, 1.0];
    }
    let base = match face / 2 {
        0 => [0.90, 0.16, 0.14, 1.0],
        1 => [0.20, 0.78, 0.28, 1.0],
        _ => [0.20, 0.45, 0.95, 1.0],
    };
    if hover == Some(face) {
        [(base[0] + 1.0) * 0.5, (base[1] + 1.0) * 0.5, (base[2] + 1.0) * 0.5, 1.0]
    } else {
        base
    }
}

/// Joint pivots, axes, and the selected joint's limit marks. Not a scene entity.
pub fn joint_debug_vertices(segments: &[jarvig_core::JointDebugSegment], camera: Vec3) -> Vec<OverlayVertex> {
    let mut vertices = Vec::new();
    for segment in segments {
        push_box(&mut vertices, segment.start, camera, segment.end, 0.0025, segment.color);
    }
    vertices
}

fn color_for(handle: GizmoHandle, hover: Option<GizmoHandle>, active: Option<GizmoHandle>) -> [f32; 4] {
    let base = match handle {
        GizmoHandle::AxisX | GizmoHandle::RingX => [0.90, 0.16, 0.14, 1.0],
        GizmoHandle::AxisY | GizmoHandle::RingY => [0.20, 0.78, 0.28, 1.0],
        GizmoHandle::AxisZ | GizmoHandle::RingZ => [0.20, 0.45, 0.95, 1.0],
    };
    if active == Some(handle) {
        [1.0, 0.95, 0.55, 1.0]
    } else if hover == Some(handle) {
        [ (base[0] + 1.0) * 0.5, (base[1] + 1.0) * 0.5, (base[2] + 1.0) * 0.5, 1.0 ]
    } else {
        base
    }
}

fn push_vertex(vertices: &mut Vec<OverlayVertex>, world: Vec3, camera: Vec3, color: [f32; 4]) {
    vertices.push(OverlayVertex { position: camera_relative_f32(world, camera), color });
}

fn push_tri(vertices: &mut Vec<OverlayVertex>, camera: Vec3, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
    push_vertex(vertices, a, camera, color);
    push_vertex(vertices, b, camera, color);
    push_vertex(vertices, c, camera, color);
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

fn push_cone(vertices: &mut Vec<OverlayVertex>, _origin: Vec3, camera: Vec3, base: Vec3, tip: Vec3, radius: f64, color: [f32; 4]) {
    let axis = normalize(sub(tip, base)).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let side = perpendicular(axis);
    let up = normalize(axis.cross(side)).unwrap_or(Vec3::new(1.0, 0.0, 0.0));
    let mut ring = Vec::new();
    for step in 0..8 {
        let angle = step as f64 / 8.0 * std::f64::consts::TAU;
        ring.push(base + side.scale(angle.cos() * radius) + up.scale(angle.sin() * radius));
    }
    for step in 0..8 {
        let next = (step + 1) % 8;
        push_tri(vertices, camera, ring[step], ring[next], tip, color);
        push_tri(vertices, camera, ring[next], ring[step], base, color);
    }
}

fn push_ring(vertices: &mut Vec<OverlayVertex>, origin: Vec3, camera: Vec3, axis: Vec3, radius: f64, tube: f64, color: [f32; 4]) {
    let axis = normalize(axis).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let side = perpendicular(axis);
    let up = normalize(axis.cross(side)).unwrap_or(Vec3::new(1.0, 0.0, 0.0));
    let steps = 40;
    for step in 0..steps {
        let a0 = step as f64 / steps as f64 * std::f64::consts::TAU;
        let a1 = (step + 1) as f64 / steps as f64 * std::f64::consts::TAU;
        let c0 = origin + side.scale(a0.cos() * radius) + up.scale(a0.sin() * radius);
        let c1 = origin + side.scale(a1.cos() * radius) + up.scale(a1.sin() * radius);
        let radial0 = normalize(sub(c0, origin)).unwrap_or(side);
        let radial1 = normalize(sub(c1, origin)).unwrap_or(side);
        let inner0 = sub(c0, radial0.scale(tube));
        let outer0 = c0 + radial0.scale(tube);
        let inner1 = sub(c1, radial1.scale(tube));
        let outer1 = c1 + radial1.scale(tube);
        push_tri(vertices, camera, inner0, outer0, outer1, color);
        push_tri(vertices, camera, inner0, outer1, inner1, color);
    }
}

fn ray_segment(ray_origin: Vec3, ray_direction: Vec3, start: Vec3, end: Vec3, radius: f64) -> Option<f64> {
    let segment = sub(end, start);
    let ray_to_start = sub(start, ray_origin);
    let a = dot(segment, segment);
    let b = dot(segment, ray_direction);
    let c = dot(ray_direction, ray_direction);
    let d = dot(segment, ray_to_start);
    let e = dot(ray_direction, ray_to_start);
    let denom = a * c - b * b;
    if denom.abs() < 1.0e-10 || a < 1.0e-12 {
        return None;
    }
    let along_segment = (b * e - c * d) / denom;
    let along_ray = (a * e - b * d) / denom;
    if !(0.0..=1.0).contains(&along_segment) || along_ray < 0.0 {
        return None;
    }
    let point = start + segment.scale(along_segment);
    let ray_point = ray_origin + ray_direction.scale(along_ray);
    if length(sub(point, ray_point)) <= radius {
        Some(along_ray)
    } else {
        None
    }
}

fn ray_ring(ray_origin: Vec3, ray_direction: Vec3, origin: Vec3, axis: Vec3, radius: f64, thickness: f64) -> Option<f64> {
    let axis = normalize(axis)?;
    let side = perpendicular(axis);
    let up = normalize(axis.cross(side))?;
    let steps = 48;
    let mut best = None;
    for step in 0..steps {
        let a0 = step as f64 / steps as f64 * std::f64::consts::TAU;
        let a1 = (step + 1) as f64 / steps as f64 * std::f64::consts::TAU;
        let p0 = origin + side.scale(a0.cos() * radius) + up.scale(a0.sin() * radius);
        let p1 = origin + side.scale(a1.cos() * radius) + up.scale(a1.sin() * radius);
        let Some(distance) = ray_segment(ray_origin, ray_direction, p0, p1, thickness) else { continue };
        if best.is_none_or(|previous: f64| distance < previous) {
            best = Some(distance);
        }
    }
    best
}

fn perpendicular(axis: Vec3) -> Vec3 {
    let reference = if axis.y.abs() < 0.9 { Vec3::new(0.0, 1.0, 0.0) } else { Vec3::new(1.0, 0.0, 0.0) };
    normalize(axis.cross(reference)).unwrap_or(Vec3::new(1.0, 0.0, 0.0))
}

fn dot(left: Vec3, right: Vec3) -> f64 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

fn sub(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x - right.x, left.y - right.y, left.z - right.z)
}

fn length(value: Vec3) -> f64 {
    (value.x * value.x + value.y * value.y + value.z * value.z).sqrt()
}

fn normalize(value: Vec3) -> Option<Vec3> {
    let length = length(value);
    if length < 1.0e-12 || !length.is_finite() {
        None
    } else {
        Some(value.scale(1.0 / length))
    }
}

fn unit(rotation: Quat) -> Option<Quat> {
    let length = (rotation.x * rotation.x + rotation.y * rotation.y + rotation.z * rotation.z + rotation.w * rotation.w).sqrt();
    if length < 1.0e-8 || !length.is_finite() {
        None
    } else {
        Some(Quat { x: rotation.x / length, y: rotation.y / length, z: rotation.z / length, w: rotation.w / length })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag_x() -> GizmoDrag {
        GizmoDrag {
            entity: jarvig_core::EntityUuid::nil(),
            handle: GizmoHandle::AxisX,
            origin: Vec3::new(1.0e9, 2.0, -4.0),
            axis: Vec3::new(1.0, 0.0, 0.0),
            original_local: Vec3::new(0.0, 0.0, -2.0),
            original_rotation: Quat::IDENTITY,
            original_world_rotation: Quat::IDENTITY,
            start_hit: Vec3::new(1.0e9, 2.0, -4.0),
        }
    }

    #[test]
    fn axis_drag_keeps_the_other_components_and_a_centimeter() {
        let drag = drag_x();
        let moved = translation_world_point(&drag, Vec3::new(drag.origin.x + 0.25, drag.origin.y + 4.0, drag.origin.z - 3.0));
        assert!((moved.x - (drag.origin.x + 0.25)).abs() < 1.0e-6);
        assert!((moved.y - drag.origin.y).abs() < 1.0e-6);
        assert!((moved.z - drag.origin.z).abs() < 1.0e-6);
        let centimeter = translation_delta(&drag, Vec3::new(drag.origin.x + 0.01, drag.origin.y, drag.origin.z));
        assert!((centimeter.x - 0.01).abs() < 1.0e-6);
        assert!(centimeter.y.abs() < 1.0e-6 && centimeter.z.abs() < 1.0e-6);
        let exact = drag.axis.scale(0.01);
        assert!((exact.x - 0.01).abs() < 1.0e-15);
        let y = GizmoDrag { handle: GizmoHandle::AxisY, axis: Vec3::new(0.0, 1.0, 0.0), ..drag };
        let up = translation_world_point(&y, Vec3::new(y.origin.x, y.origin.y + 0.4, y.origin.z));
        assert!((up.y - (y.origin.y + 0.4)).abs() < 1.0e-6);
        assert!((up.x - y.origin.x).abs() < 1.0e-6);
    }

    #[test]
    fn rotation_about_y_stays_normalized_and_x_is_local() {
        let angle = 0.35;
        let world = world_rotation_after(Quat::IDENTITY, Vec3::new(0.0, 1.0, 0.0), angle).unwrap();
        let length = (world.x * world.x + world.y * world.y + world.z * world.z + world.w * world.w).sqrt();
        assert!((length - 1.0).abs() < 1.0e-6);
        assert!(world.y.abs() > 0.1);
        let local = local_rotation_after(Quat::IDENTITY, Vec3::new(1.0, 0.0, 0.0), angle).unwrap();
        assert!(local.x.abs() > 0.1);
        assert!(local.y.abs() < 1.0e-6);
        let origin = Vec3::ZERO;
        let axis = Vec3::new(0.0, 1.0, 0.0);
        let start = Vec3::new(1.0, 0.0, 0.0);
        let current = Vec3::new(0.0, 0.0, -1.0);
        let measured = rotation_angle(origin, axis, start, current).unwrap();
        assert!((measured - std::f64::consts::FRAC_PI_2).abs() < 1.0e-4 || (measured + std::f64::consts::FRAC_PI_2).abs() < 1.0e-4);
    }

    #[test]
    fn the_x_axis_is_hit_before_a_miss_and_screen_size_grows_with_distance() {
        let origin = Vec3::new(1.0e9, 0.0, -2.0);
        let length = 1.0;
        let ray_origin = Vec3::new(1.0e9, 0.5, 2.0);
        let toward = Vec3::new(origin.x + 0.4, origin.y, origin.z);
        let direction = normalize_test(Vec3::new(toward.x - ray_origin.x, toward.y - ray_origin.y, toward.z - ray_origin.z));
        let hit = hit_gizmo(origin, ray_origin, direction, length, TransformSpace::World, Quat::IDENTITY, false);
        assert_eq!(hit, Some(GizmoHandle::AxisX));
        let miss = hit_gizmo(origin, ray_origin, Vec3::new(0.0, 1.0, 0.0), length, TransformSpace::World, Quat::IDENTITY, false);
        assert!(miss.is_none());
        let near = visual_length(2.0, 60.0_f64.to_radians(), 100);
        let far = visual_length(20.0, 60.0_f64.to_radians(), 100);
        assert!(far > near);
        let vertices = build_gizmo(origin, ray_origin, length, TransformSpace::World, Quat::IDENTITY, false, None, None);
        assert!(vertices.len() > 30);
        assert!(vertices.iter().all(|vertex| vertex.position.iter().all(|component| component.is_finite() && component.abs() < 50.0)));
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        let local_x = axis_in_space(GizmoHandle::AxisX, TransformSpace::Local, yaw);
        assert!(local_x.x.abs() < 0.1);
        assert!(local_x.z.abs() > 0.9);
        let radius = 0.85;
        let ring_point = Vec3::new(radius * 0.6_f64.cos(), 0.0, radius * 0.6_f64.sin());
        let eye = Vec3::new(0.0, 0.0, 4.0);
        let toward = normalize_test(Vec3::new(ring_point.x - eye.x, ring_point.y - eye.y, ring_point.z - eye.z));
        let ring = hit_gizmo(Vec3::ZERO, eye, toward, 1.0, TransformSpace::World, Quat::IDENTITY, true);
        assert_eq!(ring, Some(GizmoHandle::RingY));
    }

    #[test]
    fn an_operation_arrow_drag_moves_along_the_arrow() {
        let center = Vec3::new(0.0, 1.0, -4.0);
        let arrow = Vec3::new(0.0, 0.0, -1.0);
        let eye = Vec3::new(1.5, 2.4, 0.5);
        let view = normalize(sub(center, eye)).unwrap();
        let plane = axis_drag_plane(arrow, view);
        let start = ray_plane(eye, view, center, plane).unwrap();
        let target = center + arrow.scale(0.35);
        let later = normalize(sub(target, eye)).unwrap();
        let end = ray_plane(eye, later, center, plane).unwrap();
        let along = scalar_along(sub(end, start), arrow);
        assert!(along > 0.2, "along {along}");
        let face_start = ray_plane(eye, view, center, arrow).unwrap();
        let face_end = ray_plane(eye, later, center, arrow).unwrap();
        let face_along = scalar_along(sub(face_end, face_start), arrow);
        assert!(face_along.abs() < 1.0e-6, "face {face_along}");
    }

    #[test]
    fn the_operation_arrow_tip_is_a_hit() {
        let center = Vec3::new(0.0, 1.0, -4.0);
        let arrow = Vec3::new(0.0, 1.0, 0.0);
        let length = 1.0;
        let tip = center + arrow.scale(length * 0.9);
        let eye = Vec3::new(2.0, tip.y, -2.0);
        let direction = normalize(sub(tip, eye)).unwrap();
        assert!(hit_operation_handle(eye, direction, center, arrow, length));
        assert!(!hit_operation_handle(eye, Vec3::new(0.0, 1.0, 0.0), center, arrow, length));
    }

    fn normalize_test(value: Vec3) -> Vec3 {
        let length = (value.x * value.x + value.y * value.y + value.z * value.z).sqrt();
        value.scale(1.0 / length)
    }
}
