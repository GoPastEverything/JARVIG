//! Compact instance and meshlet tables for one view.
//!
//! The renderer uploads these bytes. It does not open a project, parse glTF, or
//! decide which clusters are visible. Frustum culling and occlusion are later tickets.

use crate::mesh::MeshId;
use crate::scene::{instance_gpu_transforms, RenderInstance};
use crate::space::{ResolvedPose, SpaceError};
use crate::Meshlet;

/// One cluster's bounds. Shared by every instance of that mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuMeshletRecord {
    pub center: [f32; 3],
    pub radius: f32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub cone_axis: [f32; 3],
    pub cone_cutoff: f32,
    pub triangles: u32,
    pub vertices: u32,
    /// Source submesh. Not part of the 64-byte uploaded record.
    pub submesh: u32,
}

impl GpuMeshletRecord {
    pub fn from_meshlet(meshlet: &Meshlet) -> Self {
        Self {
            center: meshlet.sphere_center,
            radius: meshlet.sphere_radius,
            bounds_min: meshlet.bounds_min,
            bounds_max: meshlet.bounds_max,
            cone_axis: meshlet.cone_axis,
            cone_cutoff: meshlet.cone_cutoff,
            triangles: meshlet.index_count / 3,
            vertices: meshlet.vertex_count,
            submesh: meshlet.submesh,
        }
    }

    pub const BYTES: usize = 64;
}

/// Meshlets for one runtime mesh. Two instances may name the same mesh.
#[derive(Clone, Copy)]
pub struct GpuSceneGeometry<'a> {
    pub mesh: MeshId,
    pub meshlets: &'a [GpuMeshletRecord],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuSceneStats {
    pub instance_count: u32,
    pub geometry_count: u32,
    pub meshlet_count: u32,
    pub instance_bytes: u64,
    pub geometry_bytes: u64,
    pub meshlet_bytes: u64,
}

/// Packed tables. Instance transforms are camera-relative float32.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuScenePack {
    pub instances: Vec<u8>,
    pub geometries: Vec<u8>,
    pub meshlets: Vec<u8>,
    pub stats: GpuSceneStats,
}

const INSTANCE_BYTES: usize = 88;
const GEOMETRY_BYTES: usize = 16;

/// `geometries` is one entry per mesh the caller wants shared. Instances select a row by `MeshId`.
pub fn pack_gpu_scene(
    instances: &[RenderInstance],
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    geometries: &[GpuSceneGeometry<'_>],
) -> Result<GpuScenePack, SpaceError> {
    let mut order: Vec<MeshId> = Vec::new();
    for instance in instances {
        if !order.contains(&instance.mesh) {
            order.push(instance.mesh);
        }
    }
    let mut meshlet_bytes = Vec::new();
    let mut geometry_bytes = Vec::with_capacity(order.len() * GEOMETRY_BYTES);
    let mut meshlet_count = 0u32;
    for mesh in &order {
        let records = geometries.iter().find(|geometry| geometry.mesh == *mesh).map(|geometry| geometry.meshlets).unwrap_or(&[]);
        let offset = meshlet_count;
        for record in records {
            write_meshlet(&mut meshlet_bytes, record);
            meshlet_count = meshlet_count.saturating_add(1);
        }
        geometry_bytes.extend_from_slice(&mesh.0.to_le_bytes());
        geometry_bytes.extend_from_slice(&offset.to_le_bytes());
        geometry_bytes.extend_from_slice(&(records.len() as u32).to_le_bytes());
    }
    let mut instance_bytes = Vec::with_capacity(instances.len() * INSTANCE_BYTES);
    for instance in instances {
        let geometry_index = order.iter().position(|mesh| *mesh == instance.mesh).unwrap_or(0) as u32;
        let transforms = instance_gpu_transforms(instance, camera, vertical_fov_radians, near_m, aspect)?;
        instance_bytes.extend_from_slice(&instance.entity.as_bytes());
        instance_bytes.extend_from_slice(&geometry_index.to_le_bytes());
        instance_bytes.extend_from_slice(&(u32::from(instance.visible)).to_le_bytes());
        let model = transforms.model.to_column_major();
        for lane in model {
            instance_bytes.extend_from_slice(&lane.to_le_bytes());
        }
    }
    debug_assert!(instance_bytes.len() == instances.len() * INSTANCE_BYTES || instances.is_empty());
    Ok(GpuScenePack {
        stats: GpuSceneStats {
            instance_count: instances.len() as u32,
            geometry_count: order.len() as u32,
            meshlet_count,
            instance_bytes: instance_bytes.len() as u64,
            geometry_bytes: geometry_bytes.len() as u64,
            meshlet_bytes: meshlet_bytes.len() as u64,
        },
        instances: instance_bytes,
        geometries: geometry_bytes,
        meshlets: meshlet_bytes,
    })
}

fn write_meshlet(bytes: &mut Vec<u8>, record: &GpuMeshletRecord) {
    write_f32x3(bytes, record.center);
    bytes.extend_from_slice(&record.radius.to_le_bytes());
    write_f32x3(bytes, record.bounds_min);
    write_f32x3(bytes, record.bounds_max);
    write_f32x3(bytes, record.cone_axis);
    bytes.extend_from_slice(&record.cone_cutoff.to_le_bytes());
    bytes.extend_from_slice(&record.triangles.to_le_bytes());
    bytes.extend_from_slice(&record.vertices.to_le_bytes());
}

fn write_f32x3(bytes: &mut Vec<u8>, value: [f32; 3]) {
    for lane in value {
        bytes.extend_from_slice(&lane.to_le_bytes());
    }
}

trait ModelBytes {
    fn to_column_major(self) -> [f32; 16];
}

impl ModelBytes for crate::Mat4 {
    fn to_column_major(self) -> [f32; 16] {
        let mut bytes = [0u8; 64];
        self.write_column_major(&mut bytes);
        let mut lanes = [0f32; 16];
        for (index, lane) in lanes.iter_mut().enumerate() {
            let start = index * 4;
            *lane = f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap());
        }
        lanes
    }
}

/// Sphere-versus-frustum result for one view. Flags are 1 when that instance's meshlet is submitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrustumCull {
    pub instances: u32,
    pub instances_visible: u32,
    pub meshlets: u32,
    pub submitted: u32,
    pub rejected: u32,
    pub triangles_submitted: u32,
    pub cpu_us: u32,
    /// Concatenated per instance, in snapshot order, one entry per meshlet of that mesh.
    pub flags: Vec<u32>,
}

/// Test meshlet spheres against the infinite reversed-Z camera. This does not read triangles.
/// `enabled == false` submits every meshlet.
pub fn frustum_cull_meshlets(
    instances: &[RenderInstance],
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    geometries: &[GpuSceneGeometry<'_>],
    enabled: bool,
) -> Result<FrustumCull, SpaceError> {
    let started = std::time::Instant::now();
    let tan_y = (vertical_fov_radians as f32 * 0.5).tan();
    let tan_x = tan_y * aspect.max(0.0001);
    let near = near_m.max(0.0);
    let mut flags = Vec::new();
    let mut submitted = 0u32;
    let mut triangles_submitted = 0u32;
    let mut meshlets = 0u32;
    let mut instances_visible = 0u32;
    for instance in instances {
        let records = geometries.iter().find(|geometry| geometry.mesh == instance.mesh).map(|geometry| geometry.meshlets).unwrap_or(&[]);
        let transforms = instance_gpu_transforms(instance, camera, vertical_fov_radians, near_m, aspect)?;
        let eye_from_local = transforms.view.mul(transforms.model);
        if records.is_empty() {
            let center = instance.bounds.sphere.center;
            let radius = instance.bounds.sphere.radius;
            if !enabled || sphere_inside(&eye_from_local, center, radius, tan_x, tan_y, near) {
                instances_visible = instances_visible.saturating_add(1);
            }
            continue;
        }
        let mut any = false;
        for record in records {
            meshlets = meshlets.saturating_add(1);
            let inside = !enabled || sphere_inside(&eye_from_local, record.center, record.radius, tan_x, tan_y, near);
            flags.push(u32::from(inside));
            if inside {
                any = true;
                submitted = submitted.saturating_add(1);
                triangles_submitted = triangles_submitted.saturating_add(record.triangles);
            }
        }
        if any {
            instances_visible = instances_visible.saturating_add(1);
        }
    }
    let rejected = meshlets.saturating_sub(submitted);
    Ok(FrustumCull {
        instances: instances.len() as u32,
        instances_visible,
        meshlets,
        submitted,
        rejected,
        triangles_submitted,
        cpu_us: u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX),
        flags,
    })
}

/// Occlusion after the frustum test.
/// `0` frustum reject, `1` visible, `3` occluded, `4` conservative visible.
/// Uncertain clusters stay submitted. The pyramid stores the far side of a sphere in reversed-Z.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcclusionCull {
    pub submitted: u32,
    pub frustum_rejected: u32,
    pub occlusion_rejected: u32,
    pub conservative_visible: u32,
    pub triangles_submitted: u32,
    pub cpu_us: u32,
    pub flags: Vec<u32>,
}

const FLAG_VISIBLE: u32 = 1;
const FLAG_OCCLUDED: u32 = 3;
const FLAG_CONSERVATIVE: u32 = 4;

const HIZ_W: usize = 64;
const HIZ_H: usize = 36;

pub fn occlusion_cull_meshlets(
    instances: &[RenderInstance],
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    geometries: &[GpuSceneGeometry<'_>],
    frustum: &FrustumCull,
    enabled: bool,
) -> Result<OcclusionCull, SpaceError> {
    let started = std::time::Instant::now();
    if !enabled {
        let frustum_rejected = frustum.flags.iter().filter(|flag| **flag == 0).count() as u32;
        return Ok(OcclusionCull {
            submitted: frustum.submitted,
            frustum_rejected,
            occlusion_rejected: 0,
            conservative_visible: 0,
            triangles_submitted: frustum.triangles_submitted,
            cpu_us: u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX),
            flags: frustum.flags.clone(),
        });
    }
    let tan_y = (vertical_fov_radians as f32 * 0.5).tan().max(0.0001);
    let near = near_m.max(0.0001);
    let mut projected: Vec<Projected> = Vec::new();
    let mut flag_cursor = 0usize;
    for instance in instances {
        let records = geometries.iter().find(|geometry| geometry.mesh == instance.mesh).map(|geometry| geometry.meshlets).unwrap_or(&[]);
        if records.is_empty() {
            continue;
        }
        let transforms = instance_gpu_transforms(instance, camera, vertical_fov_radians, near_m, aspect)?;
        let eye_from_local = transforms.view.mul(transforms.model);
        for (index, record) in records.iter().enumerate() {
            let flag_index = flag_cursor + index;
            if frustum.flags.get(flag_index).copied().unwrap_or(0) == 0 {
                continue;
            }
            let cluster = project_cluster(&eye_from_local, &transforms.projection, record, near, tan_y);
            projected.push(Projected { flag_index, cluster });
        }
        flag_cursor += records.len();
    }
    let mut base = vec![0f32; HIZ_W * HIZ_H];
    for item in &projected {
        if let Some(cluster) = item.cluster.as_ref().filter(|cluster| !cluster.ambiguous) {
            splat(&mut base, cluster);
        }
    }
    let mips = build_min_mips(&base, HIZ_W, HIZ_H);
    let mut flags = frustum.flags.clone();
    let mut occlusion_rejected = 0u32;
    let mut conservative_visible = 0u32;
    let mut submitted = 0u32;
    let mut triangles_submitted = 0u32;
    flag_cursor = 0;
    let mut projected_cursor = 0usize;
    for instance in instances {
        let records = geometries.iter().find(|geometry| geometry.mesh == instance.mesh).map(|geometry| geometry.meshlets).unwrap_or(&[]);
        for (index, record) in records.iter().enumerate() {
            let flag_index = flag_cursor + index;
            if flags.get(flag_index).copied().unwrap_or(0) == 0 {
                continue;
            }
            let Some(item) = projected.get(projected_cursor).filter(|item| item.flag_index == flag_index) else {
                flags[flag_index] = FLAG_CONSERVATIVE;
                conservative_visible = conservative_visible.saturating_add(1);
                submitted = submitted.saturating_add(1);
                triangles_submitted = triangles_submitted.saturating_add(record.triangles);
                continue;
            };
            projected_cursor += 1;
            let classification = match item.cluster.as_ref() {
                Some(cluster) if !cluster.ambiguous && rect_occluded(&mips, cluster, near) => FLAG_OCCLUDED,
                Some(cluster) if !cluster.ambiguous => FLAG_VISIBLE,
                _ => FLAG_CONSERVATIVE,
            };
            flags[flag_index] = classification;
            if classification == FLAG_OCCLUDED {
                occlusion_rejected = occlusion_rejected.saturating_add(1);
            } else {
                if classification == FLAG_CONSERVATIVE {
                    conservative_visible = conservative_visible.saturating_add(1);
                }
                submitted = submitted.saturating_add(1);
                triangles_submitted = triangles_submitted.saturating_add(record.triangles);
            }
        }
        flag_cursor += records.len();
    }
    let frustum_rejected = flags.iter().filter(|flag| **flag == 0).count() as u32;
    Ok(OcclusionCull {
        submitted,
        frustum_rejected,
        occlusion_rejected,
        conservative_visible,
        triangles_submitted,
        cpu_us: u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX),
        flags,
    })
}

struct ScreenCluster {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    near_depth: f32,
    far_z: f32,
    radius: f32,
    ambiguous: bool,
}

struct Projected {
    flag_index: usize,
    cluster: Option<ScreenCluster>,
}

fn project_cluster(eye_from_local: &crate::Mat4, projection: &crate::Mat4, record: &GpuMeshletRecord, near: f32, tan_y: f32) -> Option<ScreenCluster> {
    let eye = eye_from_local.transform_point(record.center);
    if !eye[0].is_finite() || !eye[1].is_finite() || !eye[2].is_finite() {
        return None;
    }
    let depth = -eye[2];
    let radius = record.radius.max(0.0);
    let crossing_near = !(depth.is_finite() && radius.is_finite()) || depth <= radius + near;
    let clip = projection.transform_point([eye[0], eye[1], eye[2]]);
    if clip[3] <= 0.05 || !clip[0].is_finite() || !clip[1].is_finite() || !clip[3].is_finite() {
        return None;
    }
    let ndc_x = clip[0] / clip[3];
    let ndc_y = clip[1] / clip[3];
    let near_depth = (depth - radius).max(near);
    let far_depth = depth + radius;
    if near_depth <= near || far_depth <= near_depth {
        return None;
    }
    let far_z = near / far_depth;
    let focal = (HIZ_H as f32 * 0.5) / tan_y;
    let pixels = radius * focal / depth.max(near);
    let sx = (ndc_x * 0.5 + 0.5) * HIZ_W as f32;
    let sy = (ndc_y * 0.5 + 0.5) * HIZ_H as f32;
    if !pixels.is_finite() || !sx.is_finite() || !sy.is_finite() || !far_z.is_finite() {
        return None;
    }
    let x0 = (sx - pixels).floor() as i32;
    let y0 = (sy - pixels).floor() as i32;
    let x1 = (sx + pixels).ceil() as i32;
    let y1 = (sy + pixels).ceil() as i32;
    let span = (x1 - x0).max(y1 - y0);
    let offscreen = ndc_x.abs() > 1.0 || ndc_y.abs() > 1.0 || x0 < 0 || y0 < 0 || x1 >= HIZ_W as i32 || y1 >= HIZ_H as i32;
    let ambiguous = crossing_near || pixels < 2.0 || pixels > 12.0 || span > 16 || offscreen || far_z >= 0.98 || far_z <= 0.0;
    Some(ScreenCluster { x0, y0, x1, y1, near_depth, far_z, radius, ambiguous })
}

fn splat(depth: &mut [f32], cluster: &ScreenCluster) {
    let x0 = cluster.x0.clamp(0, HIZ_W as i32 - 1) as usize;
    let x1 = cluster.x1.clamp(0, HIZ_W as i32 - 1) as usize;
    let y0 = cluster.y0.clamp(0, HIZ_H as i32 - 1) as usize;
    let y1 = cluster.y1.clamp(0, HIZ_H as i32 - 1) as usize;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let texel = &mut depth[y * HIZ_W + x];
            if cluster.far_z > *texel {
                *texel = cluster.far_z;
            }
        }
    }
}

fn build_min_mips(base: &[f32], width: usize, height: usize) -> Vec<(usize, usize, Vec<f32>)> {
    let mut mips = vec![(width, height, base.to_vec())];
    let mut w = width;
    let mut h = height;
    while w > 1 || h > 1 {
        let nw = (w / 2).max(1);
        let nh = (h / 2).max(1);
        let previous = &mips[mips.len() - 1].2;
        let mut next = vec![1f32; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                let mut value = 1f32;
                for oy in 0..2 {
                    for ox in 0..2 {
                        let sx = (x * 2 + ox).min(w - 1);
                        let sy = (y * 2 + oy).min(h - 1);
                        value = value.min(previous[sy * w + sx]);
                    }
                }
                next[y * nw + x] = value;
            }
        }
        mips.push((nw, nh, next));
        w = nw;
        h = nh;
    }
    mips
}

fn rect_occluded(mips: &[(usize, usize, Vec<f32>)], cluster: &ScreenCluster, near: f32) -> bool {
    // Reversed-Z: larger stored z is closer. The pyramid value is the far side of an occluder sphere.
    // Reject only when every sample, including the min mip over the whole rect, is clearly in front of this sphere's near side.
    if cluster.ambiguous || !(near > 0.0) || !(cluster.near_depth > near) {
        return false;
    }
    let (base_w, base_h, base) = &mips[0];
    let x0 = cluster.x0.max(0) as usize;
    let x1 = cluster.x1.max(0) as usize;
    let y0 = cluster.y0.max(0) as usize;
    let y1 = cluster.y1.max(0) as usize;
    if x1 >= *base_w || y1 >= *base_h {
        return false;
    }
    let separation = (cluster.near_depth * 0.02).max(cluster.radius).max(0.05);
    let mut samples = 0u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            samples += 1;
            if !sample_clearly_in_front(base[y * base_w + x], cluster.near_depth, separation, near) {
                return false;
            }
        }
    }
    if samples == 0 {
        return false;
    }
    let width = (cluster.x1 - cluster.x0).max(1) as f32;
    let height = (cluster.y1 - cluster.y0).max(1) as f32;
    let level = (width.max(height).log2().floor() as usize).min(mips.len() - 1);
    let (mip_w, mip_h, texels) = &mips[level];
    let scale = 1usize << level;
    let mx0 = (x0 / scale).min(mip_w - 1);
    let mx1 = (x1 / scale).min(mip_w - 1);
    let my0 = (y0 / scale).min(mip_h - 1);
    let my1 = (y1 / scale).min(mip_h - 1);
    for y in my0..=my1 {
        for x in mx0..=mx1 {
            if !sample_clearly_in_front(texels[y * mip_w + x], cluster.near_depth, separation, near) {
                return false;
            }
        }
    }
    true
}

fn sample_clearly_in_front(stored_z: f32, near_depth: f32, separation: f32, near: f32) -> bool {
    if !(stored_z > 0.0) {
        return false;
    }
    let stored_depth = near / stored_z;
    stored_depth + separation < near_depth
}

fn sphere_inside(eye_from_local: &crate::Mat4, center: [f32; 3], radius: f32, tan_x: f32, tan_y: f32, near: f32) -> bool {
    let eye = eye_from_local.transform_point(center);
    if !eye[0].is_finite() || !eye[1].is_finite() || !eye[2].is_finite() {
        return true;
    }
    let point = [eye[0], eye[1], eye[2]];
    let radius = radius.max(0.0);
    let depth = -point[2];
    let nearest = depth - radius;
    let farthest = depth + radius;
    // A sphere that crosses the near plane, or the camera plane, does not have a trustworthy
    // screen rectangle. The side planes pass through the camera, so they reject that sphere
    // even when part of it is in front of the near plane. Submit it.
    if nearest <= near && farthest >= 0.0 {
        return true;
    }
    // Inward normals for the four side planes through the origin, plus the near plane.
    let planes = [
        normalize([-1.0, 0.0, -tan_x]),
        normalize([1.0, 0.0, -tan_x]),
        normalize([0.0, -1.0, -tan_y]),
        normalize([0.0, 1.0, -tan_y]),
    ];
    for normal in planes {
        let distance = normal[0] * point[0] + normal[1] * point[1] + normal[2] * point[2];
        if distance < -radius {
            return false;
        }
    }
    // Camera looks down -Z. The near plane is z = -near. Inward distance is -z - near.
    if -point[2] - near < -radius {
        return false;
    }
    true
}

/// Closest meshlet sphere along a ray in that mesh's local space. `None` when the ray misses every sphere.
pub fn closest_meshlet_on_ray(records: &[GpuMeshletRecord], origin: [f32; 3], direction: [f32; 3]) -> Option<u32> {
    let length = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
    if !(length > 1.0e-8) {
        return None;
    }
    let direction = [direction[0] / length, direction[1] / length, direction[2] / length];
    let mut best_hit = f32::MAX;
    let mut best_index = None;
    for (index, record) in records.iter().enumerate() {
        let radius = record.radius.max(0.0);
        let offset = [origin[0] - record.center[0], origin[1] - record.center[1], origin[2] - record.center[2]];
        let along = offset[0] * direction[0] + offset[1] * direction[1] + offset[2] * direction[2];
        let closest = offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2] - along * along;
        let radius_sq = radius * radius;
        if closest > radius_sq {
            continue;
        }
        let span = (radius_sq - closest).sqrt();
        let hit = if -along - span >= 0.0 { -along - span } else { -along + span };
        if hit >= 0.0 && hit < best_hit {
            best_hit = hit;
            best_index = Some(index as u32);
        }
    }
    best_index
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length < 1.0e-8 {
        [0.0, 0.0, -1.0]
    } else {
        [value[0] / length, value[1] / length, value[2] / length]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::mesh_from_surfaces;
    use crate::meshlet::build_meshlets;
    use crate::{CanonicalSurface, EntityId, LocalBounds, ObjectId, Quat, RenderInstance, RenderInstanceId, Vec3};

    fn instance(mesh: MeshId, entity: EntityId, translation: Vec3, visible: bool) -> RenderInstance {
        RenderInstance {
            id: RenderInstanceId(1),
            entity,
            source: ObjectId(1),
            mesh,
            pose: ResolvedPose { translation, rotation: Quat::IDENTITY },
            scale: Vec3::new(1.0, 1.0, 1.0),
            bounds: LocalBounds {
                aabb: crate::Aabb { min: [-1.0, -1.0, -1.0], max: [1.0, 1.0, 1.0] },
                sphere: crate::BoundingSphere { center: [0.0, 0.0, 0.0], radius: 1.0 },
            },
            visible,
            cast_shadows: true,
            receive_shadows: true,
            material_bindings: Vec::new(),
        }
    }

    fn tiny_meshlets() -> (MeshId, Vec<GpuMeshletRecord>) {
        let mesh = mesh_from_surfaces(&[CanonicalSurface {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            texcoords: vec![[0.0, 0.0]; 3],
            tangents: Vec::new(),
            indices: vec![0, 1, 2],
            material_slot: 0,
        }])
        .expect("triangle");
        let set = build_meshlets(&mesh);
        let records: Vec<_> = set.meshlets.iter().map(GpuMeshletRecord::from_meshlet).collect();
        (MeshId(7), records)
    }

    #[test]
    fn two_instances_share_one_meshlet_table_and_transforms_are_camera_relative() {
        let (mesh, records) = tiny_meshlets();
        let camera = ResolvedPose { translation: Vec3::new(1_000_000.0, 0.0, 0.0), rotation: Quat::IDENTITY };
        let first = instance(mesh, EntityId::new(), Vec3::new(1_000_000.25, 2.0, 0.0), true);
        let second = instance(mesh, EntityId::new(), Vec3::new(1_000_000.5, 0.0, 0.0), false);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let pack = pack_gpu_scene(&[first, second], &camera, 1.0, 0.1, 1.0, &geometries).unwrap();
        assert_eq!(pack.stats.instance_count, 2);
        assert_eq!(pack.stats.geometry_count, 1);
        assert_eq!(pack.stats.meshlet_count, records.len() as u32);
        assert_eq!(pack.stats.meshlet_bytes, u64::from(pack.stats.meshlet_count) * GpuMeshletRecord::BYTES as u64);
        let geometry_mesh = u64::from_le_bytes(pack.geometries[0..8].try_into().unwrap());
        assert_eq!(geometry_mesh, mesh.0);
        let meshlet_count = u32::from_le_bytes(pack.geometries[12..16].try_into().unwrap());
        assert_eq!(meshlet_count, records.len() as u32);
        let relative_x = f32::from_le_bytes(pack.instances[72..76].try_into().unwrap());
        assert!((relative_x - 0.25).abs() < 0.02, "relative {relative_x}");
        let second_visible = u32::from_le_bytes(pack.instances[INSTANCE_BYTES + 20..INSTANCE_BYTES + 24].try_into().unwrap());
        assert_eq!(second_visible, 0);
        let other = MeshId(9);
        let lone = instance(other, EntityId::new(), Vec3::ZERO, true);
        let plain = pack_gpu_scene(&[lone], &camera, 1.0, 0.1, 1.0, &[]).unwrap();
        assert_eq!(plain.stats.geometry_count, 1);
        assert_eq!(plain.stats.meshlet_count, 0);
    }

    #[test]
    fn a_side_cluster_is_rejected_and_disabling_the_frustum_submits_every_meshlet() {
        let mesh = MeshId(4);
        let records = [
            GpuMeshletRecord {
                center: [0.0, 0.0, 0.0],
                radius: 0.2,
                bounds_min: [-0.2, -0.2, -0.2],
                bounds_max: [0.2, 0.2, 0.2],
                cone_axis: [0.0, 0.0, 1.0],
                cone_cutoff: 0.0,
                triangles: 40,
                vertices: 30,
                submesh: 0,
            },
            GpuMeshletRecord {
                center: [80.0, 0.0, 0.0],
                radius: 0.2,
                bounds_min: [79.8, -0.2, -0.2],
                bounds_max: [80.2, 0.2, 0.2],
                cone_axis: [0.0, 0.0, 1.0],
                cone_cutoff: 0.0,
                triangles: 10,
                vertices: 12,
                submesh: 0,
            },
        ];
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -8.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let culled = frustum_cull_meshlets(&[object.clone()], &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        assert_eq!(culled.meshlets, 2);
        assert_eq!(culled.submitted, 1);
        assert_eq!(culled.rejected, 1);
        assert_eq!(culled.triangles_submitted, 40);
        assert_eq!(culled.flags, vec![1, 0]);
        let all = frustum_cull_meshlets(&[object], &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, false).unwrap();
        assert_eq!(all.submitted, 2);
        assert_eq!(all.rejected, 0);
        assert_eq!(all.triangles_submitted, 50);
    }

    #[test]
    fn a_cluster_crossing_the_near_plane_stays_submitted() {
        let mesh = MeshId(4);
        let records = [record([0.0, 0.5, -0.15], 0.12, 16), record([80.0, 0.0, -8.0], 0.2, 8)];
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::ZERO, true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let frustum = frustum_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        assert_eq!(frustum.flags[0], 1, "a sphere that crosses the near plane stays submitted");
        assert_eq!(frustum.flags[1], 0, "a cluster far to the side stays rejected");
        let occlusion = occlusion_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true).unwrap();
        assert_ne!(occlusion.flags[0], 0);
        assert_ne!(occlusion.flags[0], 3, "a near-plane cluster is not occluded");
    }

    fn record(center: [f32; 3], radius: f32, triangles: u32) -> GpuMeshletRecord {
        GpuMeshletRecord {
            center,
            radius,
            bounds_min: [center[0] - radius, center[1] - radius, center[2] - radius],
            bounds_max: [center[0] + radius, center[1] + radius, center[2] + radius],
            cone_axis: [0.0, 0.0, 1.0],
            cone_cutoff: 0.0,
            triangles,
            vertices: 16,
            submesh: 0,
        }
    }

    #[test]
    fn a_cluster_behind_a_closer_cluster_is_occluded_and_a_small_camera_move_stays_stable() {
        let mesh = MeshId(4);
        let records = [
            record([0.0, 0.0, 0.0], 1.2, 80),
            record([0.0, 0.0, -3.0], 0.70, 20),
            record([1.2, 0.0, 0.0], 0.50, 12),
        ];
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -6.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let frustum = frustum_cull_meshlets(&[object.clone()], &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        let hidden = occlusion_cull_meshlets(&[object.clone()], &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true).unwrap();
        assert_eq!(hidden.flags[0], 1, "the front cluster stays submitted");
        assert_eq!(hidden.flags[1], 3, "the cluster behind it is occluded");
        assert_eq!(hidden.flags[2], 1, "the cluster to the side stays submitted");
        assert_eq!(hidden.occlusion_rejected, 1);
        assert_eq!(hidden.submitted, 2);
        assert_eq!(hidden.triangles_submitted, 92);
        let nudged = ResolvedPose { translation: Vec3::new(0.02, 0.0, 0.0), rotation: Quat::IDENTITY };
        let frustum_near = frustum_cull_meshlets(std::slice::from_ref(&object), &nudged, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        let hidden_near = occlusion_cull_meshlets(std::slice::from_ref(&object), &nudged, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum_near, true).unwrap();
        assert_eq!(hidden_near.flags, hidden.flags, "a 2 cm camera move keeps the same occlusion result");
        let off = occlusion_cull_meshlets(&[object], &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, false).unwrap();
        assert_eq!(off.occlusion_rejected, 0);
        assert_eq!(off.submitted, frustum.submitted);
    }

    #[test]
    fn a_visible_facade_is_not_occluded_by_a_closer_protruding_sphere() {
        let mesh = MeshId(4);
        let awning = record([0.0, 0.15, 0.45], 0.35, 40);
        let facade = record([0.0, -0.05, 0.0], 0.05, 30);
        let interior = record([0.0, 0.15, -1.6], 0.40, 18);
        let records = [awning, facade, interior];
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -4.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let frustum = frustum_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        assert_eq!(frustum.flags, vec![1, 1, 1], "all three clusters are inside the frustum");
        let culled = occlusion_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true).unwrap();
        assert_ne!(culled.flags[1], 3, "the visible facade must not be classified occluded");
        assert!(culled.flags[1] == 1 || culled.flags[1] == 4, "facade flag {}", culled.flags[1]);
        assert_eq!(culled.flags[2], 3, "a cluster buried behind the protrusion can still be occluded");
        assert_ne!(culled.flags[0], 3, "the protruding cluster does not occlude itself");
        let nudged = ResolvedPose { translation: Vec3::new(0.02, 0.0, 0.0), rotation: Quat::IDENTITY };
        let frustum_near = frustum_cull_meshlets(std::slice::from_ref(&object), &nudged, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        let culled_near = occlusion_cull_meshlets(std::slice::from_ref(&object), &nudged, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum_near, true).unwrap();
        assert_eq!(culled_near.flags, culled.flags, "a 2 cm move keeps the facade classification");
    }
}
