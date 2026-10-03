//! Canonical parametric solids. The saved object is the solid, not a triangle mesh.
//!
//! Meshlets, hierarchy, and Einstein detail are derived views. They do not define
//! the block, its transform, or its collision. ADR-0062.

use crate::{MaterialAssetRef, Quat, Vec3, BOOTSTRAP_ROOT_M};

/// Smallest full extent on one axis. Below this the solid stops being a usable face.
pub const BLOCK_MIN_EXTENT_M: f64 = 0.05;
/// Largest full extent on one axis for this slice.
pub const BLOCK_MAX_EXTENT_M: f64 = 1000.0;
/// Free-fly body kept outside the analytic box. Not a character capsule and not JRV-0090.
pub const FLY_COLLISION_RADIUS_M: f64 = 0.3;
/// Editor ground grid, just under Y = 0 so a block resting on the ground wins reversed-Z.
pub const REFERENCE_GRID_Y_M: f64 = -0.001;
/// Face-drag increment. The Move gizmo does not use this.
pub const BLOCK_DIMENSION_SNAP_M: f64 = 0.05;
/// Translation snap for the Snap command. The Move gizmo does not use this.
pub const BLOCK_POSITION_SNAP_M: f64 = 0.1;
/// One Extrude click pushes the selected face by this distance.
pub const BLOCK_EXTRUDE_STEP_M: f64 = 0.25;
/// One Inset click adds this distance on the selected face.
pub const BLOCK_INSET_STEP_M: f64 = 0.10;
/// How many modeling edits the record keeps. Older entries drop off the front.
pub const BLOCK_HISTORY_LIMIT: usize = 24;

/// Face order matches the derived box: +X, −X, +Y, −Y, +Z, −Z.
pub fn face_name(face: u8) -> &'static str {
    match face {
        0 => "+X",
        1 => "-X",
        2 => "+Y",
        3 => "-Y",
        4 => "+Z",
        5 => "-Z",
        _ => "?",
    }
}

/// Outward unit normal in the block frame. `None` when `face` is not 0..5.
pub fn face_normal(face: u8) -> Option<Vec3> {
    match face {
        0 => Some(Vec3::new(1.0, 0.0, 0.0)),
        1 => Some(Vec3::new(-1.0, 0.0, 0.0)),
        2 => Some(Vec3::new(0.0, 1.0, 0.0)),
        3 => Some(Vec3::new(0.0, -1.0, 0.0)),
        4 => Some(Vec3::new(0.0, 0.0, 1.0)),
        5 => Some(Vec3::new(0.0, 0.0, -1.0)),
        _ => None,
    }
}

/// In-plane axis used by the face handle cross. Same tangents as the derived box.
pub fn face_tangent(face: u8) -> Option<Vec3> {
    match face {
        0 => Some(Vec3::new(0.0, 0.0, 1.0)),
        1 => Some(Vec3::new(0.0, 0.0, -1.0)),
        2 | 3 | 4 => Some(Vec3::new(1.0, 0.0, 0.0)),
        5 => Some(Vec3::new(-1.0, 0.0, 0.0)),
        _ => None,
    }
}

const MINOR_RADIUS_M: f64 = 40.0;
const MAJOR_RADIUS_M: f64 = 180.0;
const FADE_FLOOR: f32 = 0.04;

/// One logged edit. The mesh is rebuilt from [`BlockRecord`]'s parameters, not by replaying this list.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockOp {
    Size { size_m: [f64; 3] },
    ExtrudeFace { face: u8, distance_m: f64 },
    InsetFace { face: u8, distance_m: f64 },
    Bevel { distance_m: f64 },
    Mirror { axis: u8 },
    /// Both ends of one edge moved by the same vector. The log is not replayed.
    MoveEdge { edge: u32, delta_m: [f64; 3] },
    /// A new edge and the wall that joins it to the old one.
    ExtrudeEdge { edge: u32, delta_m: [f64; 3] },
    /// Midpoint of one edge. The old id stays on the first half.
    SplitEdge { edge: u32 },
    /// One quad replaced by a grid of quads.
    SubdivideFace { face: u32, u: u32, v: u32 },
    /// One vertex moved. Loops that already contain it stay connected.
    MoveVertex { vertex: u32, delta_m: [f64; 3] },
}

impl BlockOp {
    pub fn summary(&self) -> String {
        match self {
            Self::Size { size_m } => format!("Size {} × {} × {}", meters(size_m[0]), meters(size_m[1]), meters(size_m[2])),
            Self::ExtrudeFace { face, distance_m } => format!("Extrude {} {}", face_name(*face), meters(*distance_m)),
            Self::InsetFace { face, distance_m } => format!("Inset {} {}", face_name(*face), meters(*distance_m)),
            Self::Bevel { distance_m } => format!("Bevel {}", meters(*distance_m)),
            Self::Mirror { axis } => format!("Mirror {}", axis_name(*axis)),
            Self::MoveEdge { edge, delta_m } => format!("Move edge E:{edge} {}", meters(delta_span(*delta_m))),
            Self::ExtrudeEdge { edge, delta_m } => format!("Extrude edge E:{edge} {}", meters(delta_span(*delta_m))),
            Self::SplitEdge { edge } => format!("Split edge E:{edge}"),
            Self::SubdivideFace { face, u, v } => format!("Subdivide face F:{face} {u}×{v}"),
            Self::MoveVertex { vertex, delta_m } => format!("Move vertex V:{vertex} {}", meters(delta_span(*delta_m))),
        }
    }

    pub(crate) fn is_topology(&self) -> bool {
        matches!(
            self,
            Self::MoveEdge { .. } | Self::ExtrudeEdge { .. } | Self::SplitEdge { .. } | Self::SubdivideFace { .. } | Self::MoveVertex { .. }
        )
    }

    pub(crate) fn finite(&self) -> bool {
        match self {
            Self::Size { size_m } => size_m.iter().all(|axis| axis.is_finite()),
            Self::ExtrudeFace { face, distance_m } | Self::InsetFace { face, distance_m } => *face < 6 && distance_m.is_finite(),
            Self::Bevel { distance_m } => distance_m.is_finite(),
            Self::Mirror { axis } => *axis < 3,
            Self::MoveEdge { edge, delta_m } | Self::ExtrudeEdge { edge, delta_m } => *edge != 0 && delta_m.iter().all(|axis| axis.is_finite()),
            Self::SplitEdge { edge } => *edge != 0,
            Self::SubdivideFace { face, u, v } => *face != 0 && (1..=crate::topology::SUBDIVIDE_MAX).contains(u) && (1..=crate::topology::SUBDIVIDE_MAX).contains(v),
            Self::MoveVertex { vertex, delta_m } => *vertex != 0 && delta_m.iter().all(|axis| axis.is_finite()),
        }
    }
}

/// Saved block. Full extents are meters. Inset and bevel are optional parameters.
/// The derived mesh is not this record.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockRecord {
    pub size_m: [f64; 3],
    /// Meters, one per face, in [`face_name`] order. Zero keeps that face a single quad.
    pub inset_m: [f64; 6],
    /// Uniform chamfer along the outer edges, in meters. Zero keeps the sharp box.
    pub bevel_m: f64,
    pub material: MaterialAssetRef,
    pub history: Vec<BlockOp>,
    /// Present after an edge, vertex, or subdivision edit. Absent on an analytic box.
    pub body: Option<crate::topology::SolidBody>,
}

impl BlockRecord {
    pub fn standard(size_m: [f64; 3]) -> Result<Self, crate::AuthoringError> {
        Ok(Self {
            size_m: finite_size(size_m)?,
            inset_m: [0.0; 6],
            bevel_m: 0.0,
            material: default_block_material(),
            history: Vec::new(),
            body: None,
        })
    }

    /// A plain block writes the same JSON as before this slice: size and material only.
    pub fn is_plain(&self) -> bool {
        self.body.is_none() && self.bevel_m.abs() < 1.0e-12 && self.inset_m.iter().all(|value| value.abs() < 1.0e-12) && self.history.is_empty()
    }

    /// Bevel or inset is still an analytic parameter. Edge edits wait until those are cleared.
    pub fn analytic_features(&self) -> bool {
        self.bevel_m.abs() > 1.0e-9 || self.inset_m.iter().any(|value| value.abs() > 1.0e-9)
    }

    /// Body used for picking and the overlay. A canonical box is computed and not written back.
    pub fn display_body(&self) -> Option<crate::topology::SolidBody> {
        if let Some(body) = &self.body {
            return Some(body.clone());
        }
        if self.analytic_features() {
            return None;
        }
        crate::topology::SolidBody::from_box(self.size_m).ok()
    }

    pub fn history_text(&self) -> String {
        if self.history.is_empty() {
            return "Block".into();
        }
        let start = self.history.len().saturating_sub(8);
        self.history[start..].iter().map(BlockOp::summary).collect::<Vec<_>>().join(" · ")
    }

    pub fn validate(&self) -> Result<(), crate::LevelError> {
        finite_size(self.size_m).map_err(|_| crate::LevelError::Corrupt("block size is not finite".into()))?;
        for axis in self.size_m {
            if axis < BLOCK_MIN_EXTENT_M - 1.0e-9 || axis > BLOCK_MAX_EXTENT_M + 1.0e-9 {
                return Err(crate::LevelError::Corrupt("block size is outside 0.05 m to 1000 m".into()));
            }
        }
        if !self.bevel_m.is_finite() || self.inset_m.iter().any(|value| !value.is_finite()) {
            return Err(crate::LevelError::Corrupt("block feature is not finite".into()));
        }
        let limit = feature_limit(self.size_m);
        if self.bevel_m < -1.0e-9 || self.bevel_m > limit + 1.0e-6 {
            return Err(crate::LevelError::Corrupt("block bevel is outside the solid".into()));
        }
        for inset in self.inset_m {
            if inset < -1.0e-9 || inset > limit + 1.0e-6 {
                return Err(crate::LevelError::Corrupt("block inset is outside the solid".into()));
            }
        }
        if self.history.iter().any(|op| !op.finite()) {
            return Err(crate::LevelError::Corrupt("block history is not finite".into()));
        }
        if let Some(body) = &self.body {
            body.validate().map_err(|_| crate::LevelError::Corrupt("block body is not a closed solid".into()))?;
            let size = body.aabb_size();
            if (0..3).any(|axis| (size[axis] - self.size_m[axis]).abs() > 1.0e-3) {
                return Err(crate::LevelError::Corrupt("block size does not match its body".into()));
            }
        }
        self.material.validate()
    }

    pub(crate) fn clamp_features(&mut self) {
        let limit = feature_limit(self.size_m);
        self.bevel_m = if self.bevel_m.is_finite() { self.bevel_m.clamp(0.0, limit) } else { 0.0 };
        for inset in &mut self.inset_m {
            *inset = if inset.is_finite() { inset.clamp(0.0, limit) } else { 0.0 };
        }
    }

    pub(crate) fn push_op(&mut self, op: BlockOp) {
        self.history.push(op);
        if self.history.len() > BLOCK_HISTORY_LIMIT {
            let extra = self.history.len() - BLOCK_HISTORY_LIMIT;
            self.history.drain(0..extra);
        }
    }
}

/// Largest bevel or inset that stays inside the box. 45% of the shortest side.
pub fn feature_limit(size_m: [f64; 3]) -> f64 {
    (0.45 * size_m[0].min(size_m[1]).min(size_m[2])).max(0.0)
}

/// Face-drag snap. Values under half a step become zero. This is not the Move gizmo.
pub fn snap_dimension(outward_m: f64) -> f64 {
    if !outward_m.is_finite() {
        return 0.0;
    }
    (outward_m / BLOCK_DIMENSION_SNAP_M).round() * BLOCK_DIMENSION_SNAP_M
}

/// Rounds each component to `step_m`. A non-positive step uses [`BLOCK_POSITION_SNAP_M`].
pub fn snap_translation(translation: Vec3, step_m: f64) -> Vec3 {
    let step = if step_m.is_finite() && step_m > 0.0 { step_m } else { BLOCK_POSITION_SNAP_M };
    Vec3::new(snap_step(translation.x, step), snap_step(translation.y, step), snap_step(translation.z, step))
}

/// Moves one face along its outward normal and shifts the center by half the applied change.
/// The opposite face stays where it was. `rotation` maps the block frame into `translation`'s frame.
pub fn push_face(size_m: [f64; 3], translation: Vec3, rotation: Quat, face: u8, outward_m: f64) -> Option<FacePush> {
    let normal = face_normal(face)?;
    if !outward_m.is_finite() || size_m.iter().any(|axis| !axis.is_finite()) {
        return None;
    }
    let axis = (face / 2) as usize;
    let mut size = size_m;
    let grown = (size[axis] + outward_m).clamp(BLOCK_MIN_EXTENT_M, BLOCK_MAX_EXTENT_M);
    let applied = grown - size[axis];
    size[axis] = grown;
    let delta = rotation.rotate(normal.scale(applied * 0.5));
    Some(FacePush {
        size_m: size,
        translation: Vec3::new(translation.x + delta.x, translation.y + delta.y, translation.z + delta.z),
    })
}

/// The original box face closest to a point in the block frame. Used when a click lands on the solid.
pub fn face_from_local_point(local: Vec3, size: [f64; 3]) -> Option<u8> {
    if size.iter().any(|axis| !axis.is_finite() || *axis <= 0.0) || !local.x.is_finite() || !local.y.is_finite() || !local.z.is_finite() {
        return None;
    }
    let coords = [local.x, local.y, local.z];
    let mut best_face = 0u8;
    let mut best_gap = f64::MAX;
    for face in 0..6u8 {
        let axis = (face / 2) as usize;
        let sign = if face % 2 == 0 { 1.0 } else { -1.0 };
        let plane = sign * size[axis] * 0.5;
        let gap = (coords[axis] - plane).abs();
        if gap < best_gap {
            best_gap = gap;
            best_face = face;
        }
    }
    Some(best_face)
}

/// Center of one face in the same frame as `translation`.
pub fn face_center(translation: Vec3, rotation: Quat, size: [f64; 3], face: u8) -> Option<Vec3> {
    let normal = face_normal(face)?;
    let axis = (face / 2) as usize;
    if !size[axis].is_finite() {
        return None;
    }
    let offset = rotation.rotate(normal.scale(size[axis] * 0.5));
    Some(Vec3::new(translation.x + offset.x, translation.y + offset.y, translation.z + offset.z))
}

/// Closest face handle whose sphere the ray hits. `ray_direction` need not be a unit vector.
pub fn nearest_face_handle(
    translation: Vec3,
    rotation: Quat,
    size: [f64; 3],
    ray_origin: Vec3,
    ray_direction: Vec3,
    radius: f64,
) -> Option<u8> {
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let length = (ray_direction.x * ray_direction.x + ray_direction.y * ray_direction.y + ray_direction.z * ray_direction.z).sqrt();
    if length < 1.0e-12 {
        return None;
    }
    let direction = ray_direction.scale(1.0 / length);
    let mut best: Option<(f64, u8)> = None;
    for face in 0..6u8 {
        let center = face_center(translation, rotation, size, face)?;
        let offset = Vec3::new(ray_origin.x - center.x, ray_origin.y - center.y, ray_origin.z - center.z);
        let toward = dot_vec(offset, direction);
        let discriminant = toward * toward - (dot_vec(offset, offset) - radius * radius);
        if discriminant < 0.0 {
            continue;
        }
        let root = discriminant.sqrt();
        let near = -toward - root;
        let hit = if near > 1.0e-4 { near } else { -toward + root };
        if hit > 1.0e-4 && best.map(|(distance, _)| hit < distance).unwrap_or(true) {
            best = Some((hit, face));
        }
    }
    best.map(|(_, face)| face)
}

/// Exchanges the two insets on `axis` so a second mirror restores them.
pub fn swap_insets(inset_m: [f64; 6], axis: usize) -> Option<[f64; 6]> {
    if axis > 2 {
        return None;
    }
    let mut swapped = inset_m;
    swapped.swap(axis * 2, axis * 2 + 1);
    Some(swapped)
}

/// Places a copy so it shares the positive face on `axis`. The original stays put.
pub fn mirrored_translation(translation: Vec3, rotation: Quat, size_m: [f64; 3], axis: usize) -> Option<Vec3> {
    if axis > 2 || !size_m[axis].is_finite() {
        return None;
    }
    let mut offset = Vec3::ZERO;
    match axis {
        0 => offset.x = size_m[0],
        1 => offset.y = size_m[1],
        _ => offset.z = size_m[2],
    }
    let delta = rotation.rotate(offset);
    Some(Vec3::new(translation.x + delta.x, translation.y + delta.y, translation.z + delta.z))
}

/// Drops the lowest corner onto scene Y = 0. Rotation is unchanged. Parent rotation is identity.
pub fn align_translation_to_ground(translation: Vec3, rotation: Quat, size_m: [f64; 3]) -> Vec3 {
    let half = [size_m[0] * 0.5, size_m[1] * 0.5, size_m[2] * 0.5];
    let mut min_y = f64::MAX;
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                let corner = rotation.rotate(Vec3::new(x * half[0], y * half[1], z * half[2]));
                min_y = min_y.min(translation.y + corner.y);
            }
        }
    }
    Vec3::new(translation.x, translation.y - min_y, translation.z)
}

/// Size and the local translation after one face push.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacePush {
    pub size_m: [f64; 3],
    pub translation: Vec3,
}

fn meters(value: f64) -> String {
    format!("{value:.2}")
}

fn delta_span(delta: [f64; 3]) -> f64 {
    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
}

fn axis_name(axis: u8) -> &'static str {
    match axis {
        0 => "X",
        1 => "Y",
        2 => "Z",
        _ => "?",
    }
}

fn snap_step(value: f64, step: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    (value / step).round() * step
}

fn dot_vec(left: Vec3, right: Vec3) -> f64 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

pub fn default_block_material() -> MaterialAssetRef {
    MaterialAssetRef::builtin("standard_white", [0.62, 0.62, 0.60, 1.0], 0.0, 0.75, [0.0, 0.0, 0.0, 1.0])
}

/// Rejects a non-finite axis. Clamps a finite axis into the supported range.
pub fn finite_size(size_m: [f64; 3]) -> Result<[f64; 3], crate::AuthoringError> {
    let mut out = [0.0; 3];
    for axis in 0..3 {
        if !size_m[axis].is_finite() {
            return Err(crate::AuthoringError::InvalidValue);
        }
        out[axis] = size_m[axis].clamp(BLOCK_MIN_EXTENT_M, BLOCK_MAX_EXTENT_M);
    }
    Ok(out)
}

/// One scene-local solid. Translation and rotation are the block frame in the scene.
#[derive(Clone, Copy, Debug)]
pub struct BlockSolid {
    pub translation: Vec3,
    pub rotation: Quat,
    pub size_m: [f64; 3],
}

/// Pushes `point` out of one expanded box. A point already outside stays.
///
/// The box is centered on `translation` and oriented by `rotation`. Half extents are
/// `size / 2` plus `radius`. The push is along the nearest face, then rotated back.
pub fn keep_outside_box(point: Vec3, solid: BlockSolid, radius: f64) -> Vec3 {
    let delta = Vec3::new(point.x - solid.translation.x, point.y - solid.translation.y, point.z - solid.translation.z);
    let local = solid.rotation.conjugate().rotate(delta);
    let half = [
        solid.size_m[0].abs() * 0.5 + radius,
        solid.size_m[1].abs() * 0.5 + radius,
        solid.size_m[2].abs() * 0.5 + radius,
    ];
    let abs = [local.x.abs(), local.y.abs(), local.z.abs()];
    if abs[0] >= half[0] || abs[1] >= half[1] || abs[2] >= half[2] {
        return point;
    }
    let penetration = [half[0] - abs[0], half[1] - abs[1], half[2] - abs[2]];
    let axis = if penetration[0] <= penetration[1] && penetration[0] <= penetration[2] {
        0
    } else if penetration[1] <= penetration[2] {
        1
    } else {
        2
    };
    let mut ejected = local;
    let signed = half[axis].copysign(match axis {
        0 => local.x,
        1 => local.y,
        _ => local.z,
    });
    match axis {
        0 => ejected.x = signed,
        1 => ejected.y = signed,
        _ => ejected.z = signed,
    }
    let world = solid.rotation.rotate(ejected);
    Vec3::new(solid.translation.x + world.x, solid.translation.y + world.y, solid.translation.z + world.z)
}

/// A few passes so overlapping boxes still eject a point that started inside.
pub fn keep_outside_blocks(point: Vec3, solids: &[BlockSolid], radius: f64) -> Vec3 {
    let mut point = point;
    for _ in 0..4 {
        for solid in solids {
            point = keep_outside_box(point, *solid, radius);
        }
    }
    point
}

/// One editor reference line in scene-local meters. Not an entity and not a saved mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceSegment {
    pub from: Vec3,
    pub to: Vec3,
    pub color: [f32; 4],
}

/// Ground grid on the XZ plane and a Y-up axis triad at the scene origin.
///
/// `camera_scene_x` and `camera_scene_z` are meters in the scene frame. The returned
/// points are scene-local. World X of the origin is [`BOOTSTRAP_ROOT_M`]. Lines far
/// from the camera are omitted. The grid sits at [`REFERENCE_GRID_Y_M`].
pub fn editor_reference_segments(camera_scene_x: f64, camera_scene_z: f64, axis_length_m: f64) -> Vec<ReferenceSegment> {
    let mut segments = Vec::new();
    let length = if axis_length_m.is_finite() { axis_length_m.clamp(0.35, 40.0) } else { 1.0 };
    let short = length * 0.45;
    push_axis(&mut segments, Vec3::new(length, 0.0, 0.0), [0.90, 0.16, 0.14, 1.0]);
    push_axis(&mut segments, Vec3::new(-short, 0.0, 0.0), [0.40, 0.07, 0.06, 1.0]);
    push_axis(&mut segments, Vec3::new(0.0, length, 0.0), [0.20, 0.78, 0.28, 1.0]);
    push_axis(&mut segments, Vec3::new(0.0, -short, 0.0), [0.09, 0.35, 0.12, 1.0]);
    push_axis(&mut segments, Vec3::new(0.0, 0.0, length), [0.20, 0.45, 0.95, 1.0]);
    push_axis(&mut segments, Vec3::new(0.0, 0.0, -short), [0.09, 0.20, 0.42, 1.0]);
    if !camera_scene_x.is_finite() || !camera_scene_z.is_finite() {
        return segments;
    }
    push_grid(&mut segments, camera_scene_x, camera_scene_z, 1.0, MINOR_RADIUS_M, [0.28, 0.30, 0.32], true);
    push_grid(&mut segments, camera_scene_x, camera_scene_z, 10.0, MAJOR_RADIUS_M, [0.48, 0.50, 0.52], false);
    segments
}

fn push_axis(segments: &mut Vec<ReferenceSegment>, end: Vec3, color: [f32; 4]) {
    segments.push(ReferenceSegment { from: Vec3::ZERO, to: end, color });
}

fn push_grid(segments: &mut Vec<ReferenceSegment>, camera_x: f64, camera_z: f64, step: f64, radius: f64, color: [f32; 3], skip_major: bool) {
    let y = REFERENCE_GRID_Y_M;
    let min_x = ((camera_x - radius) / step).floor() as i32;
    let max_x = ((camera_x + radius) / step).ceil() as i32;
    let min_z = ((camera_z - radius) / step).floor() as i32;
    let max_z = ((camera_z + radius) / step).ceil() as i32;
    let z0 = camera_z - radius;
    let z1 = camera_z + radius;
    let x0 = camera_x - radius;
    let x1 = camera_x + radius;
    for index in min_x..=max_x {
        let x = index as f64 * step;
        if skip_major && (x / 10.0).round() * 10.0 == snapped(x) {
            continue;
        }
        let fade = fade_of((x - camera_x).abs(), radius);
        if fade < FADE_FLOOR {
            continue;
        }
        segments.push(ReferenceSegment {
            from: Vec3::new(x, y, z0),
            to: Vec3::new(x, y, z1),
            color: [color[0] * fade, color[1] * fade, color[2] * fade, 1.0],
        });
    }
    for index in min_z..=max_z {
        let z = index as f64 * step;
        if skip_major && (z / 10.0).round() * 10.0 == snapped(z) {
            continue;
        }
        let fade = fade_of((z - camera_z).abs(), radius);
        if fade < FADE_FLOOR {
            continue;
        }
        segments.push(ReferenceSegment {
            from: Vec3::new(x0, y, z),
            to: Vec3::new(x1, y, z),
            color: [color[0] * fade, color[1] * fade, color[2] * fade, 1.0],
        });
    }
}

fn snapped(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn fade_of(distance: f64, radius: f64) -> f32 {
    (1.0 - distance / radius).clamp(0.0, 1.0) as f32
}

/// Scene-local origin, expressed in the root frame. The scene sits under the bootstrap root.
pub fn scene_origin_world_x() -> f64 {
    BOOTSTRAP_ROOT_M
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        empty_world_level, parse_level, ComponentRecord, GameSettings, LevelDocument, LevelError, PawnSelection, PhysicalControl, PlayControl,
        Quat, StartupCameraPolicy, Vec3, LEVEL_BLOCK_VERSION, LEVEL_FORMAT_VERSION,
    };

    fn solid(translation: Vec3, rotation: Quat, size: [f64; 3]) -> BlockSolid {
        BlockSolid { translation, rotation, size_m: size }
    }

    #[test]
    fn an_inside_point_is_pushed_out_and_an_outside_point_stays() {
        let box_solid = solid(Vec3::ZERO, Quat::IDENTITY, [2.0, 2.0, 2.0]);
        let inside = keep_outside_box(Vec3::ZERO, box_solid, FLY_COLLISION_RADIUS_M);
        assert!((inside.x - 1.3).abs() < 1.0e-9, "{inside:?}");
        assert!(inside.y.abs() < 1.0e-9 && inside.z.abs() < 1.0e-9, "{inside:?}");
        let outside = Vec3::new(2.0, 0.0, 0.0);
        let stayed = keep_outside_box(outside, box_solid, FLY_COLLISION_RADIUS_M);
        assert!((stayed.x - 2.0).abs() < 1.0e-9 && stayed.y.abs() < 1.0e-9 && stayed.z.abs() < 1.0e-9, "{stayed:?}");
    }

    #[test]
    fn a_rotated_box_pushes_along_the_rotated_face() {
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        let box_solid = solid(Vec3::ZERO, yaw, [2.0, 2.0, 2.0]);
        let pushed = keep_outside_box(Vec3::new(0.0, 0.0, -0.2), box_solid, FLY_COLLISION_RADIUS_M);
        assert!(pushed.x.abs() < 1.0e-6 && pushed.y.abs() < 1.0e-6, "{pushed:?}");
        assert!((pushed.z + 1.3).abs() < 1.0e-6, "{pushed:?}");
    }

    #[test]
    fn empty_level_stays_world_settings_and_a_block_round_trips_without_triangles() {
        let empty = empty_world_level();
        assert_eq!(empty.format_version, LEVEL_FORMAT_VERSION);
        assert_eq!(empty.entities.len(), 1);
        assert!(empty.entities[0].components.iter().all(|component| matches!(component, ComponentRecord::WorldSettings)));
        let mut world = empty.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 3.0, 4.0]).unwrap()).unwrap();
        assert_eq!(world.entity_outline().iter().find(|row| row.uuid == id).unwrap().class, crate::AuthoringClass::Block);
        assert!(!world.entity_ownership(id).unwrap().capabilities.mesh_renderer);
        let mesh_id = world.object_mesh(id).unwrap();
        let set = world.derived_meshlets(mesh_id).unwrap();
        let coverage = crate::meshlet_coverage(world.meshes().get(mesh_id).unwrap(), set);
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert_eq!(coverage.canonical_triangles, 12);
        let cluster_count = set.meshlets.len();
        assert_eq!(cluster_count, 12);
        let captured = LevelDocument::capture(&world, empty.level_uuid, "Empty").unwrap();
        assert_eq!(captured.format_version, LEVEL_BLOCK_VERSION);
        let json = captured.to_json();
        assert!(json.contains("ParametricBlock"));
        assert!(json.contains("2") && json.contains("3") && json.contains("4"));
        assert!(!json.contains("bevel_m") && !json.contains("inset_m") && !json.contains("history") && !json.contains("\"body\""), "a plain block keeps the size-only record");
        for forbidden in ["meshlet", "einstein", "Einstein", "indices", "triangle"] {
            assert!(!json.contains(forbidden), "{forbidden} leaked into the block level");
        }
        let mut loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let record = loaded.authored_block(id).unwrap();
        assert_eq!(record.size_m, [2.0, 3.0, 4.0]);
        let reloaded = loaded.object_mesh(id).unwrap();
        assert_eq!(loaded.derived_meshlets(reloaded).unwrap().meshlets.len(), cluster_count);
        let facts = crate::SurfaceDetailFacts {
            opaque: true,
            skinned: false,
            ui: false,
            particle: false,
            bounds_min: loaded.meshes().get(reloaded).unwrap().bounds().aabb.min,
            bounds_max: loaded.meshes().get(reloaded).unwrap().bounds().aabb.max,
            exact: true,
        };
        assert!(crate::microgeometry_active(crate::MicrogeometryMode::Auto, &facts));
        let flat = vec![
            crate::DetailCluster { flag: 1, projected_px: 0.2, compatible: true, has_anchor: true, exact: false };
            cluster_count
        ];
        let reasons = crate::select_detail_clusters(&flat, 256);
        assert!(reasons.iter().all(|reason| *reason == crate::DetailReject::BelowThreshold));
        let exact = vec![
            crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: true };
            cluster_count
        ];
        let exact_reasons = crate::select_detail_clusters(&exact, 256);
        assert!(exact_reasons.iter().all(|reason| *reason == crate::DetailReject::Exact));
        assert_eq!(exact_reasons.iter().filter(|reason| **reason == crate::DetailReject::Selected).count(), 0);
        let pose = loaded.authored_local_pose(id).unwrap().0;
        assert!((pose.y - 1.0).abs() < 1.0e-9 && (pose.z + 4.0).abs() < 1.0e-9, "{pose:?}");
        assert_eq!(loaded.set_block_extent(id, 0, 6.0).unwrap(), crate::AuthoringResult::Applied);
        assert!(loaded.derived_meshlets(reloaded).is_none());
        let ownership = loaded.entity_ownership(id).unwrap();
        let resized = ownership.mesh.unwrap();
        assert_ne!(resized, reloaded);
        let resized_set = loaded.derived_meshlets(resized).unwrap();
        let resized_coverage = crate::meshlet_coverage(loaded.meshes().get(resized).unwrap(), resized_set);
        assert_eq!(resized_coverage.missing_triangles, 0);
        assert_eq!(resized_coverage.duplicate_triangles, 0);
        assert_eq!(resized_coverage.canonical_triangles, 12);
        let bounds = loaded.meshes().get(resized).unwrap().bounds();
        assert!((bounds.aabb.max[0] - 3.0).abs() < 1.0e-4, "{:?}", bounds.aabb);
        assert!((bounds.aabb.max[1] - 1.5).abs() < 1.0e-4, "{:?}", bounds.aabb);
        assert!((bounds.aabb.min[2] + 2.0).abs() < 1.0e-4, "{:?}", bounds.aabb);
        let again = LevelDocument::capture(&loaded, empty.level_uuid, "Empty").unwrap();
        assert_eq!(parse_level(&again.to_json()).unwrap().instantiate().unwrap().authored_block(id).unwrap().size_m[0], 6.0);
        let solids = loaded.block_solids();
        assert_eq!(solids.len(), 1);
        assert!((solids[0].size_m[0] - 6.0).abs() < 1.0e-9);
    }

    fn lane3(mesh: &crate::Mesh, index: u32, offset: usize) -> [f32; 3] {
        let stream = &mesh.streams()[0];
        let start = index as usize * stream.stride as usize + offset;
        let bytes = &stream.bytes;
        [
            f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()),
            f32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()),
            f32::from_le_bytes(bytes[start + 8..start + 12].try_into().unwrap()),
        ]
    }

    fn quantize(position: [f32; 3]) -> [i32; 3] {
        position.map(|lane| (lane * 10_000.0).round() as i32)
    }

    fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
        [
            left[1] * right[2] - left[2] * right[1],
            left[2] * right[0] - left[0] * right[2],
            left[0] * right[1] - left[1] * right[0],
        ]
    }

    fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
        left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
    }

    fn distance(left: [f32; 3], right: [f32; 3]) -> f32 {
        let delta = [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
        (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
    }

    /// Derived triangles, meshlets, and hierarchy spheres. Einstein is not part of this audit.
    fn audit_block_surface(mesh: &crate::Mesh, set: &crate::MeshletSet, half: [f32; 3]) {
        let triangles = mesh.triangle_indices();
        assert_eq!(triangles.len(), 12);
        assert_eq!(mesh.vertex_count(), 24);
        let mut corners = Vec::new();
        for index in 0..mesh.vertex_count() {
            let position = mesh.position(index).unwrap();
            for axis in 0..3 {
                assert!((position[axis].abs() - half[axis]).abs() < 1.0e-4, "{position:?} is not a corner of {half:?}");
            }
            let key = quantize(position);
            if !corners.contains(&key) {
                corners.push(key);
            }
        }
        assert_eq!(corners.len(), 8, "a box has eight logical corners");
        let mut face_keys = Vec::new();
        let mut face_counts = Vec::new();
        let mut edges: Vec<(([i32; 3], [i32; 3]), Vec<[i32; 3]>)> = Vec::new();
        for triangle in &triangles {
            let positions = [mesh.position(triangle[0]).unwrap(), mesh.position(triangle[1]).unwrap(), mesh.position(triangle[2]).unwrap()];
            let stored = lane3(mesh, triangle[0], 32);
            let geometric = cross(
                [positions[1][0] - positions[0][0], positions[1][1] - positions[0][1], positions[1][2] - positions[0][2]],
                [positions[2][0] - positions[0][0], positions[2][1] - positions[0][1], positions[2][2] - positions[0][2]],
            );
            assert!(dot(geometric, stored) > 0.0, "winding {positions:?} against {stored:?}");
            let normal_key = stored.map(|lane| lane.round() as i32);
            for position in positions {
                for axis in 0..3 {
                    if stored[axis].abs() > 0.5 {
                        assert!((position[axis] - stored[axis] * half[axis]).abs() < 1.0e-4, "triangle leaves its face");
                    }
                }
            }
            if let Some(slot) = face_keys.iter().position(|key| *key == normal_key) {
                face_counts[slot] += 1;
            } else {
                face_keys.push(normal_key);
                face_counts.push(1);
            }
            let keys = [quantize(positions[0]), quantize(positions[1]), quantize(positions[2])];
            for pair in [(0, 1), (1, 2), (2, 0)] {
                let mut ends = [keys[pair.0], keys[pair.1]];
                if ends[0] > ends[1] {
                    ends.swap(0, 1);
                }
                let key = (ends[0], ends[1]);
                if let Some((_, uses)) = edges.iter_mut().find(|(found, _)| *found == key) {
                    uses.push(normal_key);
                } else {
                    edges.push((key, vec![normal_key]));
                }
            }
        }
        assert_eq!(face_keys.len(), 6);
        assert!(face_counts.iter().all(|count| *count == 2), "{face_counts:?}");
        let mut cube_edges = 0;
        let mut diagonals = 0;
        for ((start, end), uses) in &edges {
            let differ = (0..3).filter(|axis| start[*axis] != end[*axis]).count();
            if differ == 1 {
                cube_edges += 1;
                assert_eq!(uses.len(), 2, "a shared face boundary is one coincident edge");
                assert_ne!(uses[0], uses[1]);
            } else {
                assert_eq!(differ, 2);
                diagonals += 1;
                assert_eq!(uses.len(), 2);
                assert_eq!(uses[0], uses[1]);
            }
        }
        assert_eq!(cube_edges, 12);
        assert_eq!(diagonals, 6);
        let coverage = crate::meshlet_coverage(mesh, set);
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert_eq!(coverage.canonical_triangles, 12);
        assert_eq!(set.meshlets.len(), 12);
        let draw = crate::meshlet_draw(set);
        assert_eq!(draw.indices.len(), 36);
        for index in &draw.indices {
            let position = mesh.position(*index).expect("meshlet index");
            for axis in 0..3 {
                assert!(position[axis].abs() <= half[axis] + 1.0e-4, "{position:?} outside {half:?}");
            }
        }
        for meshlet in &set.meshlets {
            for axis in 0..3 {
                assert!(meshlet.bounds_min[axis] >= -half[axis] - 1.0e-4);
                assert!(meshlet.bounds_max[axis] <= half[axis] + 1.0e-4);
            }
        }
        let records: Vec<_> = set.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        let hierarchy = crate::build_cluster_hierarchy(&records);
        assert_eq!(hierarchy.leaf_count, 12);
        for node in &hierarchy.nodes {
            for slot in 0..node.child_count as usize {
                let child = &hierarchy.nodes[node.children[slot] as usize];
                let reach = distance(node.center, child.center) + child.radius;
                assert!(reach <= node.radius + 1.0e-4, "parent sphere {reach} does not contain {child:?}");
            }
        }
    }

    #[test]
    fn a_two_meter_block_stays_inside_its_bounds_until_einstein_relief() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mesh_id = world.object_mesh(id).unwrap();
        let mesh = world.meshes().get(mesh_id).unwrap();
        let set = world.derived_meshlets(mesh_id).unwrap();
        audit_block_surface(mesh, set, [1.0, 1.0, 1.0]);
        let first = mesh.triangle_indices()[0][0];
        let corner = mesh.position(first).unwrap();
        assert!((corner[0] - 1.0).abs() < 1.0e-4 && (corner[1] + 1.0).abs() < 1.0e-4 && (corner[2] + 1.0).abs() < 1.0e-4, "{corner:?}");
        let relief = crate::build_procedural_microtriangles(
            &[crate::SurfaceAnchor {
                position: corner,
                normal: [1.0, 0.0, 0.0],
                tangent: [0.0, 0.0, 1.0, 1.0],
                uv: [0.0, 0.0],
                depth_m: 1.0,
            }],
            1,
            true,
            940.0,
            0.5,
            &crate::MicroBudget::surface(),
            crate::DetailProvider::EinsteinSurface,
        );
        assert!(relief.triangle_count > 0, "submitting the anchor still builds the public reference marker");
        let mut past_face = false;
        for vertex in 0..relief.vertex_count as usize {
            let start = vertex * 60;
            let x = f32::from_le_bytes(relief.vertices[start..start + 4].try_into().unwrap());
            past_face |= x > 1.0 + 1.0e-4;
        }
        assert!(!past_face, "the public provider stays on the face");
        let close = crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: true };
        let reasons = crate::select_detail_clusters(&[close], 256);
        assert_eq!(reasons, vec![crate::DetailReject::Exact]);
        assert_eq!(world.set_block_extent(id, 0, 6.0).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 1, 3.0).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 2, 4.0).unwrap(), crate::AuthoringResult::Applied);
        let resized = world.entity_ownership(id).unwrap().mesh.unwrap();
        audit_block_surface(world.meshes().get(resized).unwrap(), world.derived_meshlets(resized).unwrap(), [3.0, 1.5, 2.0]);
    }

    #[test]
    fn pushing_a_face_keeps_the_opposite_face_and_the_center() {
        let pushed = push_face([2.0, 2.0, 2.0], Vec3::ZERO, Quat::IDENTITY, 0, 1.0).unwrap();
        assert_eq!(pushed.size_m, [3.0, 2.0, 2.0]);
        assert!((pushed.translation.x - 0.5).abs() < 1.0e-9, "{:?}", pushed.translation);
        let opposite = pushed.translation.x - pushed.size_m[0] * 0.5;
        assert!((opposite + 1.0).abs() < 1.0e-9, "opposite face moved to {opposite}");
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        let turned = push_face([2.0, 2.0, 2.0], Vec3::ZERO, yaw, 0, 1.0).unwrap();
        assert!(turned.translation.x.abs() < 1.0e-6 && turned.translation.y.abs() < 1.0e-6, "{:?}", turned.translation);
        assert!((turned.translation.z + 0.5).abs() < 1.0e-6, "{:?}", turned.translation);
        let local_opposite = yaw.rotate(Vec3::new(-turned.size_m[0] * 0.5, 0.0, 0.0));
        let world_opposite = Vec3::new(turned.translation.x + local_opposite.x, turned.translation.y + local_opposite.y, turned.translation.z + local_opposite.z);
        assert!(world_opposite.x.abs() < 1.0e-6 && (world_opposite.z - 1.0).abs() < 1.0e-6, "{world_opposite:?}");
        assert_eq!(snap_dimension(0.02), 0.0);
        assert!((snap_dimension(0.03) - 0.05).abs() < 1.0e-12);
        assert!((snap_dimension(-0.06) + 0.05).abs() < 1.0e-12);
    }

    #[test]
    fn align_snap_and_mirror_math_stay_on_the_solid() {
        let seated = align_translation_to_ground(Vec3::new(1.0, 3.0, -2.0), Quat::IDENTITY, [2.0, 2.0, 2.0]);
        assert!((seated.y - 1.0).abs() < 1.0e-9 && (seated.x - 1.0).abs() < 1.0e-9, "{seated:?}");
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        let turned = align_translation_to_ground(Vec3::new(0.0, 0.0, 0.0), yaw, [4.0, 2.0, 2.0]);
        assert!(turned.y > 0.9, "lowest corner was below the ground: {turned:?}");
        let snapped = snap_translation(Vec3::new(1.06, -0.04, 2.0), BLOCK_POSITION_SNAP_M);
        assert!((snapped.x - 1.1).abs() < 1.0e-9 && snapped.y.abs() < 1.0e-9 && (snapped.z - 2.0).abs() < 1.0e-9, "{snapped:?}");
        let copy = mirrored_translation(Vec3::new(0.0, 1.0, -4.0), Quat::IDENTITY, [2.0, 3.0, 4.0], 0).unwrap();
        assert!((copy.x - 2.0).abs() < 1.0e-9 && (copy.y - 1.0).abs() < 1.0e-9, "{copy:?}");
        let swapped = swap_insets([0.2, 0.0, 0.0, 0.0, 0.0, 0.0], 0).unwrap();
        assert_eq!(swapped, [0.0, 0.2, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(swap_insets(swapped, 0).unwrap()[0], 0.2);
    }

    #[test]
    fn a_face_handle_hit_ignores_a_ray_that_misses_the_sphere() {
        let size = [2.0, 2.0, 2.0];
        let hit = nearest_face_handle(Vec3::ZERO, Quat::IDENTITY, size, Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0), 0.2);
        assert_eq!(hit, Some(4), "+Z is the face the ray meets");
        let miss = nearest_face_handle(Vec3::ZERO, Quat::IDENTITY, size, Vec3::new(0.0, 3.0, 5.0), Vec3::new(0.0, 0.0, -1.0), 0.2);
        assert_eq!(miss, None);
        let on_face = face_from_local_point(Vec3::new(1.0, 0.2, -0.1), size).unwrap();
        assert_eq!(on_face, 0);
    }

    #[test]
    fn a_block_advertises_solid_tools_and_a_mesh_does_not() {
        let world = crate::SceneWorld::bootstrap();
        let near = world.entity_outline().iter().find(|row| row.name == "Near Triangle").unwrap().uuid;
        let light = world.entity_outline().iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let settings = world.entity_outline().iter().find(|row| row.name == "World Settings").unwrap().uuid;
        let near_caps = crate::AuthoringCapabilities::from_entity(world.entity_ownership(near).unwrap().capabilities);
        let light_caps = crate::AuthoringCapabilities::from_entity(world.entity_ownership(light).unwrap().capabilities);
        let settings_caps = crate::AuthoringCapabilities::from_entity(world.entity_ownership(settings).unwrap().capabilities);
        assert!(near_caps.material_assignable);
        assert!(!near_caps.parametric_solid && !near_caps.face_editable && !near_caps.patternable && !near_caps.boolean_operand);
        assert!(!light_caps.parametric_solid && !light_caps.face_editable && !light_caps.collidable);
        assert!(light_caps.component_host);
        assert!(!settings_caps.component_host && !settings_caps.parametric_solid);
        let mut empty = empty_world_level().instantiate().unwrap();
        let block = empty.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let block_caps = crate::AuthoringCapabilities::from_entity(empty.entity_ownership(block).unwrap().capabilities);
        assert!(block_caps.transformable && block_caps.parametric_solid && block_caps.face_editable && block_caps.patternable);
        assert!(block_caps.material_assignable && block_caps.collidable && block_caps.component_host);
        assert!(!block_caps.boolean_operand);
    }

    #[test]
    fn a_face_push_moves_the_center_and_commit_logs_once() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let baseline = world.entity_local_pose(id).unwrap().translation;
        assert_eq!(
            world.push_block_face(id, 0, [2.0, 2.0, 2.0], baseline, 1.0).unwrap(),
            crate::AuthoringResult::Applied
        );
        assert_eq!(world.authored_block(id).unwrap().size_m, [3.0, 2.0, 2.0]);
        assert!(world.authored_block(id).unwrap().history.is_empty());
        let moved = world.entity_local_pose(id).unwrap().translation;
        assert!((moved.x - 0.5).abs() < 1.0e-9 && (moved.y - 1.0).abs() < 1.0e-9 && (moved.z + 4.0).abs() < 1.0e-9, "{moved:?}");
        assert_eq!(world.commit_block_face(id, 0, [2.0, 2.0, 2.0]).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().history.len(), 1);
        assert_eq!(world.restore_block_drag(id, [2.0, 2.0, 2.0], [0.0; 6], 0.0, baseline).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.restore_block_drag(id, [2.0, 2.0, 2.0], [0.0; 6], 0.0, baseline).unwrap(), crate::AuthoringResult::Unchanged);
        let restored = world.authored_block(id).unwrap();
        assert_eq!(restored.size_m, [2.0, 2.0, 2.0]);
        assert_eq!(restored.history.len(), 1, "cancel does not append or erase the log");
        assert_eq!(world.entity_local_pose(id).unwrap().translation.x.abs(), 0.0);
    }

    #[test]
    fn a_bevel_preview_logs_once_on_commit_and_cancel_restores_it() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(world.preview_block_bevel(id, 0.2).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.preview_block_bevel(id, 0.4).unwrap(), crate::AuthoringResult::Applied);
        assert!(world.authored_block(id).unwrap().history.is_empty());
        assert!((world.authored_block(id).unwrap().bevel_m - 0.4).abs() < 1.0e-9);
        assert_eq!(world.commit_block_bevel(id, 0.0).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().history.len(), 1);
        assert_eq!(world.commit_block_bevel(id, 0.4).unwrap(), crate::AuthoringResult::Unchanged);
        let pose = world.entity_local_pose(id).unwrap().translation;
        assert_eq!(world.restore_block_drag(id, [2.0, 2.0, 2.0], [0.0; 6], 0.0, pose).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().bevel_m, 0.0);
        assert_eq!(world.authored_block(id).unwrap().history.len(), 1);
        assert_eq!(world.preview_block_inset(id, 2, 0.15).unwrap(), crate::AuthoringResult::Applied);
        assert!(world.authored_block(id).unwrap().history.len() == 1);
        assert_eq!(world.commit_block_inset(id, 2, 0.0).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().history.len(), 2);
    }

    #[test]
    fn inset_and_bevel_stay_inside_the_analytic_box_and_round_trip() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let plain = world.object_mesh(id).unwrap();
        assert_eq!(world.derived_meshlets(plain).unwrap().meshlets.len(), 12);
        assert_eq!(world.set_block_bevel(id, 0.1).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.set_block_inset(id, 0, 0.1).unwrap(), crate::AuthoringResult::Applied);
        let featured = world.object_mesh(id).unwrap();
        let mesh = world.meshes().get(featured).unwrap();
        assert!(mesh.index_count() > 36);
        for index in 0..mesh.vertex_count() {
            let position = mesh.position(index).unwrap();
            assert!(position.iter().all(|axis| axis.abs() <= 1.0 + 1.0e-3), "{position:?}");
        }
        let coverage = crate::meshlet_coverage(mesh, world.derived_meshlets(featured).unwrap());
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert!(coverage.canonical_triangles > 12);
        let captured = LevelDocument::capture(&world, empty_world_level().level_uuid, "Featured").unwrap();
        let json = captured.to_json();
        assert!(json.contains("bevel_m") && json.contains("inset_m") && json.contains("history"));
        assert!(json.contains("\"op\":\"bevel\"") || json.contains("\"op\": \"bevel\""));
        for forbidden in ["meshlet", "einstein", "Einstein", "indices", "triangle"] {
            assert!(!json.contains(forbidden), "{forbidden} leaked into the featured block");
        }
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let record = loaded.authored_block(id).unwrap();
        assert!((record.bevel_m - 0.1).abs() < 1.0e-9);
        assert!((record.inset_m[0] - 0.1).abs() < 1.0e-9);
        assert!(record.inset_m[1..].iter().all(|value| value.abs() < 1.0e-9));
        assert!(record.history.len() >= 2);
        let reloaded_count = loaded.derived_meshlets(loaded.object_mesh(id).unwrap()).unwrap().meshlets.len();
        assert_eq!(reloaded_count, world.derived_meshlets(featured).unwrap().meshlets.len());
    }

    #[test]
    fn mirror_copies_the_solid_and_leaves_the_original() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 3.0, 4.0]).unwrap()).unwrap();
        world.set_block_inset(id, 0, 0.2).unwrap();
        let before = world.entity_local_pose(id).unwrap().translation;
        let copy = world.mirror_block(id, 0).unwrap();
        assert_ne!(copy, id);
        let original = world.authored_block(id).unwrap();
        let mirrored = world.authored_block(copy).unwrap();
        assert!((original.inset_m[0] - 0.2).abs() < 1.0e-9 && original.inset_m[1].abs() < 1.0e-9);
        assert!(mirrored.inset_m[0].abs() < 1.0e-9 && (mirrored.inset_m[1] - 0.2).abs() < 1.0e-9);
        assert!(matches!(mirrored.history.last(), Some(BlockOp::Mirror { axis: 0 })));
        assert!(!original.history.iter().any(|op| matches!(op, BlockOp::Mirror { .. })));
        let stayed = world.entity_local_pose(id).unwrap().translation;
        let moved = world.entity_local_pose(copy).unwrap().translation;
        assert!((stayed.x - before.x).abs() < 1.0e-9 && (stayed.z - before.z).abs() < 1.0e-9);
        assert!((moved.x - (before.x + 2.0)).abs() < 1.0e-6 && (moved.y - before.y).abs() < 1.0e-6, "{moved:?}");
        let grounded = world.align_block_to_ground(id).unwrap();
        assert_eq!(grounded, crate::AuthoringResult::Applied);
        assert!((world.entity_local_pose(id).unwrap().translation.y - 1.5).abs() < 1.0e-6);
        world.set_entity_local_translation(copy, Vec3::new(1.06, 1.0, -4.04)).unwrap();
        assert_eq!(world.snap_block_translation(copy, BLOCK_POSITION_SNAP_M).unwrap(), crate::AuthoringResult::Applied);
        let snapped = world.entity_local_pose(copy).unwrap().translation;
        assert!((snapped.x - 1.1).abs() < 1.0e-9 && (snapped.z + 4.0).abs() < 1.0e-9, "{snapped:?}");
        let duplicated = world.duplicate_authored(id).unwrap();
        assert!((world.authored_block(duplicated).unwrap().inset_m[0] - 0.2).abs() < 1.0e-9);
        assert_eq!(world.reset_block_shape(id).unwrap(), crate::AuthoringResult::Applied);
        let reset = world.authored_block(id).unwrap();
        assert!(reset.is_plain() && reset.size_m == [2.0, 2.0, 2.0]);
        assert_eq!(world.derived_meshlets(world.object_mesh(id).unwrap()).unwrap().meshlets.len(), 12);
    }

    #[test]
    fn a_version_1_block_is_corrupt_and_a_future_level_is_rejected() {
        let mut document = empty_world_level();
        document.entities.push(crate::EntityRecord {
            uuid: crate::EntityId::parse("44444444-4444-4444-8444-444444444444").unwrap(),
            name: "Block".into(),
            parent_uuid: None,
            components: vec![
                ComponentRecord::Transform { translation: Vec3::new(0.0, 1.0, 0.0), rotation: Quat::IDENTITY, scale: Vec3::new(1.0, 1.0, 1.0) },
                ComponentRecord::ParametricBlock(BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()),
            ],
        });
        assert!(matches!(document.validate(), Err(LevelError::Corrupt(_))));
        document.format_version = LEVEL_BLOCK_VERSION;
        document.validate().unwrap();
        document.format_version = LEVEL_BLOCK_VERSION + 1;
        assert!(matches!(document.validate(), Err(LevelError::UnsupportedVersion(7))));
    }

    #[test]
    fn reference_axes_sit_on_the_scene_origin_and_far_grid_lines_are_omitted() {
        let segments = editor_reference_segments(0.0, 0.0, 2.0);
        assert!(segments.iter().any(|segment| segment.from == Vec3::ZERO && (segment.to.x - 2.0).abs() < 1.0e-9 && segment.to.y == 0.0));
        assert!(segments.iter().any(|segment| segment.from == Vec3::ZERO && (segment.to.y - 2.0).abs() < 1.0e-9));
        assert!(segments.iter().any(|segment| segment.from == Vec3::ZERO && (segment.to.z - 2.0).abs() < 1.0e-9));
        assert!((scene_origin_world_x() - BOOTSTRAP_ROOT_M).abs() < 1.0e-6);
        assert!(segments.iter().filter(|segment| segment.from.y < 0.0).all(|segment| (segment.from.y - REFERENCE_GRID_Y_M).abs() < 1.0e-9));
        assert!(segments.iter().any(|segment| segment.from.y < 0.0));
        let far = editor_reference_segments(10_000.0, 0.0, 2.0);
        assert!(far.iter().filter(|segment| segment.from.y < 0.0).all(|segment| segment.from.x > 9_000.0));
    }

    #[test]
    fn free_fly_stops_short_of_the_analytic_box() {
        let document = empty_world_level();
        let mut runtime = document.instantiate().unwrap();
        runtime.create_block(Vec3::new(0.0, 1.0, 0.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut settings = GameSettings::inactive();
        settings.player_controller = true;
        settings.pawn = PawnSelection::DefaultFreeFly;
        settings.mapping_context = Some("JARVIG.Default".into());
        settings.startup_camera = StartupCameraPolicy::Pawn;
        let mut control = PlayControl::attach(&mut runtime, &settings).unwrap();
        let pawn = control.pawn().unwrap();
        control.place_free_fly_view(&mut runtime, Vec3::new(0.0, 1.6, 2.0), 0.0, 0.0).unwrap();
        control.device_mut().held.insert(PhysicalControl::KeyW);
        for _ in 0..20 {
            control.tick(&mut runtime, 0.1).unwrap();
        }
        let stopped = runtime.authored_local_pose(pawn).unwrap().0;
        assert!(stopped.z > 1.2 && stopped.z < 1.45, "{stopped:?}");
        assert!(stopped.z > 0.0, "{stopped:?}");
    }

    #[test]
    fn free_fly_inside_a_block_is_ejected() {
        let document = empty_world_level();
        let mut runtime = document.instantiate().unwrap();
        runtime.create_block(Vec3::new(0.0, 1.0, 0.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut settings = GameSettings::inactive();
        settings.player_controller = true;
        settings.pawn = PawnSelection::DefaultFreeFly;
        settings.mapping_context = Some("JARVIG.Default".into());
        settings.startup_camera = StartupCameraPolicy::Pawn;
        let mut control = PlayControl::attach(&mut runtime, &settings).unwrap();
        let pawn = control.pawn().unwrap();
        control.place_free_fly_view(&mut runtime, Vec3::new(0.0, 1.0, 0.0), 0.0, 0.0).unwrap();
        control.tick(&mut runtime, 0.1).unwrap();
        let ejected = runtime.authored_local_pose(pawn).unwrap().0;
        assert!((ejected.x - 1.3).abs() < 1.0e-6, "{ejected:?}");
        assert!((ejected.y - 1.0).abs() < 1.0e-6 && ejected.z.abs() < 1.0e-6, "{ejected:?}");
    }

    #[test]
    fn a_split_edge_round_trips_and_a_body_is_not_a_plain_box() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let plain = LevelDocument::capture(&world, document.level_uuid, "Split").unwrap().to_json();
        assert!(!plain.contains("\"body\""));
        assert!(world.authored_block(id).unwrap().is_plain());
        assert_eq!(world.split_block_edge(id, 12).unwrap(), crate::AuthoringResult::Applied);
        let record = world.authored_block(id).unwrap();
        assert!(!record.is_plain());
        assert_eq!(record.body.as_ref().unwrap().vertices.len(), 9);
        assert!(matches!(record.history.last(), Some(BlockOp::SplitEdge { edge: 12 })));
        let mesh = world.object_mesh(id).unwrap();
        assert_eq!(world.meshes().get(mesh).unwrap().triangle_indices().len(), 14);
        let json = LevelDocument::capture(&world, document.level_uuid, "Split").unwrap().to_json();
        assert!(json.contains("\"body\"") && json.contains("split-edge"));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let again = loaded.authored_block(id).unwrap();
        assert_eq!(again.body.as_ref().unwrap().vertices.len(), 9);
        assert!(again.body.as_ref().unwrap().edges.iter().any(|edge| edge.id == 12));
        assert!(world.push_block_face(id, 0, [2.0, 2.0, 2.0], Vec3::new(0.0, 1.0, -4.0), 0.25).is_err());
        assert!(world.set_block_bevel(id, 0.1).is_err());
    }

    #[test]
    fn sizing_a_body_scales_about_the_center_and_keeps_ids() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::ZERO, BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        world.split_block_edge(id, 12).unwrap();
        assert_eq!(world.set_block_extent(id, 0, 4.0).unwrap(), crate::AuthoringResult::Applied);
        let record = world.authored_block(id).unwrap();
        assert!((record.size_m[0] - 4.0).abs() < 1.0e-6, "{:?}", record.size_m);
        let body = record.body.unwrap();
        assert!(body.edges.iter().any(|edge| edge.id == 12));
        assert!((body.vertex_position(2).unwrap()[0] - 2.0).abs() < 1.0e-6);
        let pose = world.authored_local_pose(id).unwrap().0;
        assert!(pose.x.abs() < 1.0e-6 && pose.y.abs() < 1.0e-6, "{pose:?}");
    }

    #[test]
    fn an_invalid_body_preview_leaves_the_record_alone() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, 0.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut bad = crate::SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        bad.vertices.clear();
        assert!(world.preview_block_body(id, bad, Vec3::new(0.0, 1.0, 0.0)).is_err());
        assert!(world.authored_block(id).unwrap().body.is_none());
        assert_eq!(world.authored_block(id).unwrap().size_m, [2.0, 2.0, 2.0]);
        let baked = crate::SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        world.preview_block_body(id, baked, Vec3::new(0.0, 1.0, 0.0)).unwrap();
        let mesh = world.object_mesh(id).unwrap();
        assert_eq!(world.meshes().get(mesh).unwrap().triangle_indices().len(), 12);
    }
}
