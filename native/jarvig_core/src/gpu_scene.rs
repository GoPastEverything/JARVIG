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
        // One sphere around every cluster. Fully outside means every cluster is outside.
        if enabled {
            let (center, radius) = enclosing_sphere(records);
            if !sphere_inside(&eye_from_local, center, radius, tan_x, tan_y, near) {
                for _record in records {
                    meshlets = meshlets.saturating_add(1);
                    flags.push(0);
                }
                continue;
            }
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

struct HzTarget {
    width: usize,
    height: usize,
    /// The JRV-0027 reference: square splat and the absolute 12 px / 16 px caps.
    reference: bool,
}

/// Half the viewport, even, clamped. A full-frame buffer makes the city cull hitch.
pub fn hierarchical_depth_extent(viewport_width: u32, viewport_height: u32) -> (u32, u32) {
    (even_in(viewport_width / 2, 96, 384), even_in(viewport_height / 2, 54, 216))
}

fn even_in(value: u32, min: u32, max: u32) -> u32 {
    let even = value.clamp(min, max) & !1;
    if even < min { min } else { even }
}

/// JRV-0027 reference occlusion. Fixed 64 by 36 buffer. Callers that draw a view use
/// [`hierarchical_occlusion_cull`] so a close cluster can be tested.
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
    occlude(instances, camera, vertical_fov_radians, near_m, aspect, geometries, frustum, enabled, HzTarget {
        width: HIZ_W,
        height: HIZ_H,
        reference: true,
    })
}

/// Hierarchical depth for one view. The buffer is half the viewport, clamped.
/// An occluder writes the inside of its bounding circle. A cluster is dropped only
/// when every sample of its expanded silhouette is clearly in front of its near side.
/// Anything larger than the old 12 px cap is still tested. False-hidden is not allowed.
pub fn hierarchical_occlusion_cull(
    instances: &[RenderInstance],
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    geometries: &[GpuSceneGeometry<'_>],
    frustum: &FrustumCull,
    enabled: bool,
    viewport_width: u32,
    viewport_height: u32,
) -> Result<OcclusionCull, SpaceError> {
    let (width, height) = hierarchical_depth_extent(viewport_width, viewport_height);
    occlude(instances, camera, vertical_fov_radians, near_m, aspect, geometries, frustum, enabled, HzTarget {
        width: width as usize,
        height: height as usize,
        reference: false,
    })
}

fn occlude(
    instances: &[RenderInstance],
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    geometries: &[GpuSceneGeometry<'_>],
    frustum: &FrustumCull,
    enabled: bool,
    target: HzTarget,
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
            let cluster = project_cluster(&eye_from_local, &transforms.projection, record, near, tan_y, &target);
            projected.push(Projected { flag_index, cluster });
        }
        flag_cursor += records.len();
    }
    let mut base = vec![0f32; target.width * target.height];
    for item in &projected {
        if let Some(cluster) = item.cluster.as_ref() {
            splat(&mut base, target.width, target.height, cluster, !target.reference);
        }
    }
    let mips = build_min_mips(&base, target.width, target.height);
    let mut flags = frustum.flags.clone();
    let mut occlusion_rejected = 0u32;
    let mut conservative_visible = 0u32;
    let mut submitted = 0u32;
    let mut triangles_submitted = 0u32;
    flag_cursor = 0;
    let mut projected_cursor = 0usize;
    for instance in instances {
        let records = geometries.iter().find(|geometry| geometry.mesh == instance.mesh).map(|geometry| geometry.meshlets).unwrap_or(&[]);
        let instance_hidden = !target.reference && instance_sphere_occluded(instance, records, camera, vertical_fov_radians, near_m, aspect, near, tan_y, &target, &mips)?;
        for (index, record) in records.iter().enumerate() {
            let flag_index = flag_cursor + index;
            if flags.get(flag_index).copied().unwrap_or(0) == 0 {
                continue;
            }
            let matched = projected.get(projected_cursor).filter(|item| item.flag_index == flag_index);
            if matched.is_some() {
                projected_cursor += 1;
            }
            if instance_hidden {
                flags[flag_index] = FLAG_OCCLUDED;
                occlusion_rejected = occlusion_rejected.saturating_add(1);
                continue;
            }
            let classification = match matched.and_then(|item| item.cluster.as_ref()) {
                Some(cluster) if cluster.can_reject && proven_occluded(&mips, cluster, near, target.reference) => FLAG_OCCLUDED,
                Some(cluster) if cluster.can_reject => FLAG_VISIBLE,
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

fn instance_sphere_occluded(
    instance: &RenderInstance,
    records: &[GpuMeshletRecord],
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    near: f32,
    tan_y: f32,
    target: &HzTarget,
    mips: &[(usize, usize, Vec<f32>)],
) -> Result<bool, SpaceError> {
    if records.is_empty() {
        return Ok(false);
    }
    let (center, radius) = enclosing_sphere(records);
    let transforms = instance_gpu_transforms(instance, camera, vertical_fov_radians, near_m, aspect)?;
    let eye_from_local = transforms.view.mul(transforms.model);
    let record = GpuMeshletRecord {
        center,
        radius,
        bounds_min: [center[0] - radius, center[1] - radius, center[2] - radius],
        bounds_max: [center[0] + radius, center[1] + radius, center[2] + radius],
        cone_axis: [0.0, 0.0, 1.0],
        cone_cutoff: 0.0,
        triangles: 0,
        vertices: 0,
        submesh: 0,
    };
    let Some(cluster) = project_cluster(&eye_from_local, &transforms.projection, &record, near, tan_y, target) else {
        return Ok(false);
    };
    Ok(cluster.can_reject && proven_occluded(mips, &cluster, near, false))
}

struct ScreenCluster {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    cx: f32,
    cy: f32,
    /// Radius written into the depth buffer. Smaller than the true silhouette.
    occluder_pixels: f32,
    /// Radius that must be covered before the cluster can be dropped.
    occludee_pixels: f32,
    near_depth: f32,
    far_z: f32,
    radius: f32,
    can_occlude: bool,
    can_reject: bool,
}

struct Projected {
    flag_index: usize,
    cluster: Option<ScreenCluster>,
}

fn project_cluster(
    eye_from_local: &crate::Mat4,
    projection: &crate::Mat4,
    record: &GpuMeshletRecord,
    near: f32,
    tan_y: f32,
    target: &HzTarget,
) -> Option<ScreenCluster> {
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
    let focal = (target.height as f32 * 0.5) / tan_y;
    let safe_depth = depth.max(near);
    let occluder_pixels = radius * focal / safe_depth;
    // asin silhouette is larger than radius/depth. The occludee test uses that larger circle.
    let occludee_pixels = if target.reference || safe_depth <= radius {
        occluder_pixels
    } else {
        radius * focal / (safe_depth * safe_depth - radius * radius).sqrt()
    };
    let sx = (ndc_x * 0.5 + 0.5) * target.width as f32;
    let sy = (ndc_y * 0.5 + 0.5) * target.height as f32;
    if !occluder_pixels.is_finite() || !occludee_pixels.is_finite() || !sx.is_finite() || !sy.is_finite() || !far_z.is_finite() {
        return None;
    }
    let pixels = if target.reference { occluder_pixels } else { occludee_pixels };
    let x0 = (sx - pixels).floor() as i32;
    let y0 = (sy - pixels).floor() as i32;
    let x1 = (sx + pixels).ceil() as i32;
    let y1 = (sy + pixels).ceil() as i32;
    let span = (x1 - x0).max(y1 - y0);
    let fully_inside = ndc_x.abs() <= 1.0 && ndc_y.abs() <= 1.0 && x0 >= 0 && y0 >= 0 && x1 < target.width as i32 && y1 < target.height as i32;
    let overlaps = x1 >= 0 && y1 >= 0 && x0 < target.width as i32 && y0 < target.height as i32;
    let depth_bad = far_z >= 0.98 || far_z <= 0.0;
    let (can_occlude, can_reject) = if target.reference {
        let ambiguous = crossing_near || occluder_pixels < 2.0 || occluder_pixels > 12.0 || span > 16 || !fully_inside || depth_bad;
        (!ambiguous, !ambiguous)
    } else {
        let reliable = !crossing_near && occluder_pixels >= 2.0 && !depth_bad;
        (reliable && overlaps, reliable && fully_inside)
    };
    Some(ScreenCluster {
        x0,
        y0,
        x1,
        y1,
        cx: sx,
        cy: sy,
        occluder_pixels,
        occludee_pixels: pixels,
        near_depth,
        far_z,
        radius,
        can_occlude,
        can_reject,
    })
}

fn splat(depth: &mut [f32], width: usize, height: usize, cluster: &ScreenCluster, circle: bool) {
    if !cluster.can_occlude || width == 0 || height == 0 {
        return;
    }
    let x0 = cluster.x0.clamp(0, width as i32 - 1) as usize;
    let x1 = cluster.x1.clamp(0, width as i32 - 1) as usize;
    let y0 = cluster.y0.clamp(0, height as i32 - 1) as usize;
    let y1 = cluster.y1.clamp(0, height as i32 - 1) as usize;
    let radius_sq = cluster.occluder_pixels * cluster.occluder_pixels;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if circle {
                let dx = (x as f32 + 0.5) - cluster.cx;
                let dy = (y as f32 + 0.5) - cluster.cy;
                if dx * dx + dy * dy > radius_sq {
                    continue;
                }
            }
            let texel = &mut depth[y * width + x];
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

fn proven_occluded(mips: &[(usize, usize, Vec<f32>)], cluster: &ScreenCluster, near: f32, reference: bool) -> bool {
    if reference { rect_occluded(mips, cluster, near) } else { circle_occluded(mips, cluster, near) }
}

fn circle_occluded(mips: &[(usize, usize, Vec<f32>)], cluster: &ScreenCluster, near: f32) -> bool {
    if !cluster.can_reject || !(near > 0.0) || !(cluster.near_depth > near) {
        return false;
    }
    let (base_w, base_h, base) = &mips[0];
    let x0 = cluster.x0.max(0) as usize;
    let y0 = cluster.y0.max(0) as usize;
    let x1 = cluster.x1.max(0) as usize;
    let y1 = cluster.y1.max(0) as usize;
    if x1 >= *base_w || y1 >= *base_h {
        return false;
    }
    let separation = (cluster.near_depth * 0.02).max(cluster.radius).max(0.05);
    let radius_sq = cluster.occludee_pixels * cluster.occludee_pixels;
    let mut samples = 0u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = (x as f32 + 0.5) - cluster.cx;
            let dy = (y as f32 + 0.5) - cluster.cy;
            if dx * dx + dy * dy > radius_sq {
                continue;
            }
            samples += 1;
            if !sample_clearly_in_front(base[y * base_w + x], cluster.near_depth, separation, near) {
                return false;
            }
        }
    }
    samples > 0
}

fn rect_occluded(mips: &[(usize, usize, Vec<f32>)], cluster: &ScreenCluster, near: f32) -> bool {
    // Reversed-Z: larger stored z is closer. The pyramid value is the far side of an occluder sphere.
    // Reject only when every sample, including the min mip over the whole rect, is clearly in front of this sphere's near side.
    if !cluster.can_reject || !(near > 0.0) || !(cluster.near_depth > near) {
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

fn enclosing_sphere(records: &[GpuMeshletRecord]) -> ([f32; 3], f32) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for record in records {
        let radius = record.radius.max(0.0);
        for axis in 0..3 {
            min[axis] = min[axis].min(record.center[axis] - radius);
            max[axis] = max[axis].max(record.center[axis] + radius);
        }
    }
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
    let mut radius = 0.0f32;
    for record in records {
        let offset = [record.center[0] - center[0], record.center[1] - center[1], record.center[2] - center[2]];
        let reach = (offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2]).sqrt() + record.radius.max(0.0);
        radius = radius.max(reach);
    }
    (center, radius)
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

/// What geometry a view submits. A visualization never selects this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GeometryDiagnostic {
    #[default]
    Normal,
    /// Every leaf, in a stable color. No frustum, hierarchy, occlusion, or detail mesh.
    LeafTruth,
    /// The same leaves, rejecting only clusters outside the camera frustum.
    FrustumOnly,
    /// Parent cut with frustum on and occlusion off. Detail meshes stay off.
    HierarchyNoOcclusion,
    /// Parent cut with frustum and hierarchical-Z. Detail meshes stay off.
    HierarchyWithOcclusion,
}

impl GeometryDiagnostic {
    /// Extra Einstein meshes stay off. A visualization may still recolor this submission.
    pub fn suspends_detail(self) -> bool {
        self != Self::Normal
    }

    pub fn frustum(self, menu: bool) -> bool {
        match self {
            Self::Normal => menu,
            Self::LeafTruth => false,
            _ => true,
        }
    }

    pub fn occlusion(self, menu: bool) -> bool {
        match self {
            Self::Normal => menu,
            Self::HierarchyWithOcclusion => true,
            _ => false,
        }
    }

    pub fn parents(self, menu: bool) -> bool {
        match self {
            Self::Normal => menu,
            Self::LeafTruth | Self::FrustumOnly => false,
            _ => true,
        }
    }

    /// Stable per-leaf colors. Outside a frustum-only view is not drawn.
    pub fn identity_colors(self) -> bool {
        matches!(self, Self::LeafTruth | Self::FrustumOnly)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::LeafTruth => "leaf truth",
            Self::FrustumOnly => "frustum only",
            Self::HierarchyNoOcclusion => "hierarchy",
            Self::HierarchyWithOcclusion => "hierarchy+occlusion",
        }
    }
}

/// Recolor the pipeline submission. This does not choose which triangles are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DiagnosticVisualization {
    #[default]
    None,
    CutReasons,
    HierarchyLevels,
    EinsteinActive,
    EinsteinEligible,
    EinsteinLodError,
    EinsteinState,
    EinsteinReject,
    EinsteinDetail,
}

impl DiagnosticVisualization {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CutReasons => "cut reasons",
            Self::HierarchyLevels => "hierarchy levels",
            Self::EinsteinActive => "einstein active",
            Self::EinsteinEligible => "einstein eligible",
            Self::EinsteinLodError => "einstein lod",
            Self::EinsteinState => "einstein state",
            Self::EinsteinReject => "einstein reject",
            Self::EinsteinDetail => "einstein detail",
        }
    }

    pub fn paints_einstein(self) -> bool {
        matches!(
            self,
            Self::EinsteinActive | Self::EinsteinEligible | Self::EinsteinLodError | Self::EinsteinState | Self::EinsteinReject
        )
    }

    pub fn shows_cut_colors(self) -> bool {
        self == Self::CutReasons
    }

    pub fn shows_level_colors(self) -> bool {
        self == Self::HierarchyLevels
    }

    /// The fragment color changes. The pipeline packets stay.
    pub fn recolors_submission(self) -> bool {
        !matches!(self, Self::None | Self::EinsteinDetail)
    }

    /// Generated-cell annotation. It is not a replacement mesh.
    pub fn overlays_detail(self) -> bool {
        self == Self::EinsteinDetail
    }

    pub fn detail_mode(self) -> Option<crate::MicroDebugMode> {
        match self {
            Self::EinsteinActive => Some(crate::MicroDebugMode::Active),
            Self::EinsteinEligible => Some(crate::MicroDebugMode::Eligible),
            Self::EinsteinLodError => Some(crate::MicroDebugMode::Error),
            Self::EinsteinState => Some(crate::MicroDebugMode::State),
            Self::EinsteinReject => Some(crate::MicroDebugMode::Reject),
            _ => None,
        }
    }
}

/// A frozen frame is reusable when the pipeline and the cluster count are the ones that were captured.
pub fn diagnostic_frame_is_reusable(
    freeze: bool,
    frozen_pipeline: Option<GeometryDiagnostic>,
    frozen_len: usize,
    pipeline: GeometryDiagnostic,
    fresh_len: usize,
) -> bool {
    freeze && frozen_pipeline == Some(pipeline) && frozen_len == fresh_len && frozen_len > 0
}

/// Leaf Truth draws every cluster. Frustum Only keeps an in-frustum leaf and does not draw an outside leaf.
/// Flag 7 is the leaf color. Flag 6 is not drawn. This does not add a parent.
pub fn stage_leaf_flags(stage: GeometryDiagnostic, flags: &[u32]) -> Vec<u32> {
    match stage {
        GeometryDiagnostic::LeafTruth => vec![7; flags.len()],
        GeometryDiagnostic::FrustumOnly => flags.iter().copied().map(|flag| if flag == 0 { 6 } else { 7 }).collect(),
        _ => flags.to_vec(),
    }
}

/// A parent packet uses this instead of a meshlet slot.
pub const DIAGNOSTIC_NO_PARENT: u32 = u32::MAX;

/// Parent LOD positions are 60-byte records. A 24-byte color copy is not that buffer.
pub const DIAGNOSTIC_PARENT_STRIDE: u32 = 60;

/// Packed diagnostic color. Values below this stay the cut-reason flags.
pub const DIAGNOSTIC_COLOR_BASE: u32 = 256;

/// One submitted draw. Visualization may change `color_key` and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticPacket {
    pub mesh_id: u32,
    pub local_meshlet_id: u32,
    pub global_meshlet_id: u32,
    pub parent_id: u32,
    pub vertex_buffer_id: u64,
    pub index_buffer_id: u64,
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    /// Stride the shader uses.
    pub vertex_stride: u32,
    /// Stride the buffer was uploaded with.
    pub buffer_stride: u32,
    pub buffer_bytes: u64,
    pub depth_write: bool,
    pub color_key: u32,
}

/// Hashes of one frozen frame. Geometry hashes ignore color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticFrameHashes {
    pub draw_list_hash: u64,
    pub index_range_hash: u64,
    pub vertex_source_hash: u64,
    pub selected_cut_hash: u64,
    pub depth_hash: u64,
    pub color_output_hash: u64,
}

impl DiagnosticFrameHashes {
    pub fn geometry_matches(self, other: Self) -> bool {
        self.draw_list_hash == other.draw_list_hash
            && self.index_range_hash == other.index_range_hash
            && self.vertex_source_hash == other.vertex_source_hash
            && self.selected_cut_hash == other.selected_cut_hash
            && self.depth_hash == other.depth_hash
    }
}

fn mix_diagnostic(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x100000001b3)
}

/// `draw_list_hash`, `index_range_hash`, and `vertex_source_hash` ignore `color_key`.
pub fn hash_diagnostic_frame(packets: &[DiagnosticPacket], cut: &[u32], depth: &[u32]) -> DiagnosticFrameHashes {
    let mut draw_list = 0xcbf29ce484222325u64;
    let mut index_range = 0xcbf29ce484222325u64;
    let mut vertex_source = 0xcbf29ce484222325u64;
    let mut color = 0xcbf29ce484222325u64;
    for packet in packets {
        draw_list = mix_diagnostic(draw_list, u64::from(packet.mesh_id));
        draw_list = mix_diagnostic(draw_list, u64::from(packet.local_meshlet_id));
        draw_list = mix_diagnostic(draw_list, u64::from(packet.global_meshlet_id));
        draw_list = mix_diagnostic(draw_list, u64::from(packet.parent_id));
        draw_list = mix_diagnostic(draw_list, packet.vertex_buffer_id);
        draw_list = mix_diagnostic(draw_list, packet.index_buffer_id);
        draw_list = mix_diagnostic(draw_list, u64::from(packet.first_index));
        draw_list = mix_diagnostic(draw_list, u64::from(packet.index_count));
        draw_list = mix_diagnostic(draw_list, packet.base_vertex as u32 as u64);
        draw_list = mix_diagnostic(draw_list, u64::from(packet.first_vertex));
        draw_list = mix_diagnostic(draw_list, u64::from(packet.vertex_count));
        draw_list = mix_diagnostic(draw_list, u64::from(packet.vertex_stride));
        draw_list = mix_diagnostic(draw_list, u64::from(packet.depth_write));
        index_range = mix_diagnostic(index_range, packet.index_buffer_id);
        index_range = mix_diagnostic(index_range, u64::from(packet.first_index));
        index_range = mix_diagnostic(index_range, u64::from(packet.index_count));
        index_range = mix_diagnostic(index_range, packet.base_vertex as u32 as u64);
        vertex_source = mix_diagnostic(vertex_source, packet.vertex_buffer_id);
        vertex_source = mix_diagnostic(vertex_source, u64::from(packet.vertex_stride));
        vertex_source = mix_diagnostic(vertex_source, u64::from(packet.buffer_stride));
        vertex_source = mix_diagnostic(vertex_source, packet.buffer_bytes);
        vertex_source = mix_diagnostic(vertex_source, u64::from(packet.first_vertex));
        vertex_source = mix_diagnostic(vertex_source, u64::from(packet.vertex_count));
        color = mix_diagnostic(color, u64::from(packet.color_key));
    }
    let mut selected_cut = 0xcbf29ce484222325u64;
    for id in cut {
        selected_cut = mix_diagnostic(selected_cut, u64::from(*id));
    }
    let mut depth_hash = 0xcbf29ce484222325u64;
    for pixel in depth {
        depth_hash = mix_diagnostic(depth_hash, u64::from(*pixel));
    }
    DiagnosticFrameHashes {
        draw_list_hash: draw_list,
        index_range_hash: index_range,
        vertex_source_hash: vertex_source,
        selected_cut_hash: selected_cut,
        depth_hash,
        color_output_hash: color,
    }
}

/// Same packets, different color keys. A longer or shorter list is not a recolor.
pub fn recolor_packets(packets: &[DiagnosticPacket], color_keys: &[u32]) -> Vec<DiagnosticPacket> {
    packets
        .iter()
        .enumerate()
        .map(|(index, packet)| {
            let mut colored = *packet;
            if let Some(color) = color_keys.get(index) {
                colored.color_key = *color;
            }
            colored
        })
        .collect()
}

pub fn diagnostic_geometry_matches(left: &[DiagnosticPacket], right: &[DiagnosticPacket]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right).all(|(left, right)| {
        left.mesh_id == right.mesh_id
            && left.local_meshlet_id == right.local_meshlet_id
            && left.global_meshlet_id == right.global_meshlet_id
            && left.parent_id == right.parent_id
            && left.vertex_buffer_id == right.vertex_buffer_id
            && left.index_buffer_id == right.index_buffer_id
            && left.first_index == right.first_index
            && left.index_count == right.index_count
            && left.base_vertex == right.base_vertex
            && left.first_vertex == right.first_vertex
            && left.vertex_count == right.vertex_count
            && left.vertex_stride == right.vertex_stride
            && left.buffer_stride == right.buffer_stride
            && left.buffer_bytes == right.buffer_bytes
            && left.depth_write == right.depth_write
    })
}

/// The pipeline stride and the upload stride are the same buffer. The index lands in that vertex range.
pub fn packet_index_in_range(packet: &DiagnosticPacket, index: u32) -> bool {
    if packet.vertex_stride == 0 || packet.buffer_stride == 0 || packet.vertex_stride != packet.buffer_stride {
        return false;
    }
    if packet.vertex_count == 0 || packet.buffer_bytes < u64::from(packet.vertex_count) * u64::from(packet.buffer_stride) {
        return false;
    }
    let vertex = i64::from(index) + i64::from(packet.base_vertex);
    if vertex < i64::from(packet.first_vertex) {
        return false;
    }
    let local = vertex as u64 - u64::from(packet.first_vertex);
    if local >= u64::from(packet.vertex_count) {
        return false;
    }
    local * u64::from(packet.vertex_stride) + 12 <= packet.buffer_bytes
}

/// A leaf packet uses the leaf buffers. A parent packet uses its own, at the 60-byte upload stride.
pub fn packet_uses_its_own_buffers(packet: &DiagnosticPacket, leaf_vertices: u64, leaf_indices: u64) -> bool {
    if packet.parent_id == DIAGNOSTIC_NO_PARENT {
        packet.vertex_buffer_id == leaf_vertices && packet.index_buffer_id == leaf_indices && packet.local_meshlet_id < u32::MAX
    } else {
        packet.vertex_buffer_id != leaf_vertices
            && packet.index_buffer_id != leaf_indices
            && packet.vertex_stride == DIAGNOSTIC_PARENT_STRIDE
            && packet.buffer_stride == DIAGNOSTIC_PARENT_STRIDE
    }
}

/// Flag and span slots are this mesh's own range. A scene-global meshlet id is not a slot here.
pub fn meshlet_slot_is_local(local_meshlet_id: u32, mesh_meshlet_count: u32) -> bool {
    mesh_meshlet_count > 0 && local_meshlet_id < mesh_meshlet_count
}

/// Detail Debug may add draws. They do not write depth and they stay inside a base packet's range.
pub fn detail_overlay_keeps_base(base: &[DiagnosticPacket], overlay: &[DiagnosticPacket]) -> bool {
    overlay.iter().all(|item| {
        !item.depth_write
            && item.vertex_stride == item.buffer_stride
            && base.iter().any(|source| {
                source.depth_write
                    && source.vertex_buffer_id == item.vertex_buffer_id
                    && source.index_buffer_id == item.index_buffer_id
                    && source.vertex_stride == item.vertex_stride
                    && source.base_vertex == item.base_vertex
                    && item.first_index >= source.first_index
                    && item.first_index.saturating_add(item.index_count) <= source.first_index.saturating_add(source.index_count)
            })
    })
}

/// RGB in 8 bits, with the cut-reason flags left below 256.
pub fn pack_diagnostic_color(color: [f32; 3]) -> u32 {
    let channel = |lane: f32| (lane.clamp(0.0, 1.0) * 255.0).round() as u32;
    DIAGNOSTIC_COLOR_BASE + channel(color[0]) + (channel(color[1]) << 8) + (channel(color[2]) << 16)
}

/// One pipeline stage, counted from a CPU coverage mask of projected cluster bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageCoverage {
    pub visible_pixels: u32,
    pub submitted_triangles: u32,
    pub selected_leaves: u32,
    pub selected_parents: u32,
    pub frustum_rejected: u32,
    pub occlusion_rejected: u32,
}

/// Leaf, frustum, hierarchy-without-occlusion, and occlusion, from one camera.
/// Hierarchy here is the in-frustum leaf set. A parent cut is a separate mask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineCoverage {
    pub leaf: StageCoverage,
    pub frustum: StageCoverage,
    pub hierarchy: StageCoverage,
    pub occlusion: StageCoverage,
    /// In-frustum leaf pixels that the hierarchy mask dropped.
    pub hierarchy_hole_pixels: u32,
    /// Hierarchy pixels that occlusion dropped without a nearer drawn occluder.
    pub occlusion_false_hidden_pixels: u32,
    pub width: u32,
    pub height: u32,
}

/// Project cluster bounds for one fixed camera and compare the four pipeline submissions.
/// Frustum may drop a cluster only when its bounds miss the camera. Occlusion may drop a
/// pixel only when a nearer drawn cluster covers it. This does not build parents or Einstein.
pub fn compare_pipeline_coverage(
    instances: &[RenderInstance],
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    geometries: &[GpuSceneGeometry<'_>],
    viewport_width: u32,
    viewport_height: u32,
) -> Result<PipelineCoverage, SpaceError> {
    let width = viewport_width.clamp(16, 768);
    let height = viewport_height.clamp(16, 768);
    let frustum = frustum_cull_meshlets(instances, camera, vertical_fov_radians, near_m, aspect, geometries, true)?;
    let occlusion = hierarchical_occlusion_cull(
        instances,
        camera,
        vertical_fov_radians,
        near_m,
        aspect,
        geometries,
        &frustum,
        true,
        viewport_width,
        viewport_height,
    )?;
    let projected = project_cluster_bounds(instances, camera, vertical_fov_radians, near_m, aspect, geometries)?;
    let leaf_draw = vec![true; projected.len()];
    let frustum_draw: Vec<bool> = frustum.flags.iter().map(|flag| *flag != 0).collect();
    let occlusion_draw: Vec<bool> = occlusion.flags.iter().map(|flag| *flag == 1 || *flag == 4).collect();
    let leaf_mask = raster_bounds(&projected, &leaf_draw, width, height);
    let frustum_mask = raster_bounds(&projected, &frustum_draw, width, height);
    let occlusion_mask = raster_bounds(&projected, &occlusion_draw, width, height);
    let mut hierarchy_hole_pixels = 0u32;
    let mut occlusion_false_hidden_pixels = 0u32;
    for index in 0..leaf_mask.len() {
        let leaf_id = leaf_mask[index];
        if leaf_id == 0 {
            continue;
        }
        let cluster = (leaf_id - 1) as usize;
        let inside = frustum.flags.get(cluster).copied().unwrap_or(0) != 0;
        if inside && frustum_mask[index] == 0 {
            hierarchy_hole_pixels = hierarchy_hole_pixels.saturating_add(1);
        }
        if frustum_mask[index] != 0 && occlusion_mask[index] == 0 {
            occlusion_false_hidden_pixels = occlusion_false_hidden_pixels.saturating_add(1);
        }
    }
    let count = |draw: &[bool], rejected_frustum: bool, rejected_occlusion: bool| StageCoverage {
        visible_pixels: 0,
        submitted_triangles: draw
            .iter()
            .enumerate()
            .filter(|(_, keep)| **keep)
            .map(|(index, _)| projected.get(index).map(|item| item.triangles).unwrap_or(0))
            .fold(0u32, |sum, triangles| sum.saturating_add(triangles)),
        selected_leaves: draw.iter().filter(|keep| **keep).count() as u32,
        selected_parents: 0,
        frustum_rejected: if rejected_frustum { frustum.flags.iter().filter(|flag| **flag == 0).count() as u32 } else { 0 },
        occlusion_rejected: if rejected_occlusion { occlusion.flags.iter().filter(|flag| **flag == 3).count() as u32 } else { 0 },
    };
    let mut leaf = count(&leaf_draw, false, false);
    let mut frustum_stage = count(&frustum_draw, true, false);
    let mut hierarchy = count(&frustum_draw, true, false);
    let mut occlusion_stage = count(&occlusion_draw, true, true);
    leaf.visible_pixels = leaf_mask.iter().filter(|pixel| **pixel != 0).count() as u32;
    frustum_stage.visible_pixels = frustum_mask.iter().filter(|pixel| **pixel != 0).count() as u32;
    hierarchy.visible_pixels = frustum_stage.visible_pixels;
    occlusion_stage.visible_pixels = occlusion_mask.iter().filter(|pixel| **pixel != 0).count() as u32;
    Ok(PipelineCoverage {
        leaf,
        frustum: frustum_stage,
        hierarchy,
        occlusion: occlusion_stage,
        hierarchy_hole_pixels,
        occlusion_false_hidden_pixels,
        width,
        height,
    })
}

struct ProjectedBound {
    x: f32,
    y: f32,
    radius_px: f32,
    depth: f32,
    triangles: u32,
}

fn project_cluster_bounds(
    instances: &[RenderInstance],
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    geometries: &[GpuSceneGeometry<'_>],
) -> Result<Vec<ProjectedBound>, SpaceError> {
    let tan_y = (vertical_fov_radians as f32 * 0.5).tan().max(0.0001);
    let tan_x = tan_y * aspect.max(0.0001);
    let mut projected = Vec::new();
    for instance in instances {
        let records = geometries.iter().find(|geometry| geometry.mesh == instance.mesh).map(|geometry| geometry.meshlets).unwrap_or(&[]);
        let transforms = crate::instance_gpu_transforms(instance, camera, vertical_fov_radians, near_m, aspect)?;
        let eye_from_local = transforms.view.mul(transforms.model);
        for record in records {
            let eye = eye_from_local.transform_point(record.center);
            let depth = -eye[2];
            if !(depth > near_m.max(0.0001)) {
                projected.push(ProjectedBound { x: 0.0, y: 0.0, radius_px: 0.0, depth: f32::MAX, triangles: record.triangles });
                continue;
            }
            projected.push(ProjectedBound {
                x: eye[0] / (depth * tan_x),
                y: eye[1] / (depth * tan_y),
                radius_px: record.radius.max(0.0) / (depth * tan_y),
                depth,
                triangles: record.triangles,
            });
        }
    }
    Ok(projected)
}

fn raster_bounds(projected: &[ProjectedBound], draw: &[bool], width: u32, height: u32) -> Vec<u32> {
    let mut mask = vec![0u32; (width as usize).saturating_mul(height as usize)];
    let mut depth = vec![f32::MAX; mask.len()];
    let width_f = width as f32;
    let height_f = height as f32;
    for (index, bound) in projected.iter().enumerate() {
        if !draw.get(index).copied().unwrap_or(false) || !(bound.radius_px > 0.0) || !bound.depth.is_finite() {
            continue;
        }
        let cx = (bound.x * 0.5 + 0.5) * width_f;
        let cy = (0.5 - bound.y * 0.5) * height_f;
        let radius = bound.radius_px * height_f * 0.5;
        let min_x = (cx - radius).floor().max(0.0) as u32;
        let max_x = (cx + radius).ceil().min(width_f) as u32;
        let min_y = (cy - radius).floor().max(0.0) as u32;
        let max_y = (cy + radius).ceil().min(height_f) as u32;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let slot = (y as usize) * (width as usize) + x as usize;
                if bound.depth < depth[slot] {
                    depth[slot] = bound.depth;
                    mask[slot] = (index as u32).saturating_add(1);
                }
            }
        }
    }
    mask
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
        let staged = stage_leaf_flags(GeometryDiagnostic::FrustumOnly, &culled.flags);
        assert_eq!(staged, vec![7, 6]);
        assert_eq!(staged.len(), records.len());
        assert!(staged.iter().all(|flag| *flag < 16));
        assert_eq!(stage_leaf_flags(GeometryDiagnostic::LeafTruth, &culled.flags), vec![7, 7]);
        assert!(!GeometryDiagnostic::FrustumOnly.parents(true));
        assert!(GeometryDiagnostic::FrustumOnly.frustum(false));
        assert!(!GeometryDiagnostic::FrustumOnly.occlusion(true));
        assert!(GeometryDiagnostic::FrustumOnly.suspends_detail());
        assert!(!GeometryDiagnostic::HierarchyNoOcclusion.occlusion(true));
        assert!(GeometryDiagnostic::HierarchyWithOcclusion.occlusion(false));
        assert!(!GeometryDiagnostic::Normal.suspends_detail());
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

    #[test]
    fn a_second_view_of_the_same_mesh_does_not_inherit_the_first_view_cut() {
        let mesh = MeshId(4);
        let records = [record([0.0, 0.0, 0.0], 1.2, 80), record([0.0, 0.0, -3.0], 0.70, 20), record([1.2, 0.0, 0.0], 0.50, 12)];
        let hierarchy = crate::build_cluster_hierarchy(&records);
        let before = hierarchy.clone();
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -6.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let frustum = frustum_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        let hidden = occlusion_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true).unwrap();
        assert_eq!(hidden.flags[1], 3, "the buried cluster is occluded for this camera");
        let triangles: Vec<u32> = records.iter().map(|item| item.triangles).collect();
        let first = crate::cut_visible_hierarchy(&hierarchy, &hidden.flags, &triangles, |_| 40.0, 0.577, 1080.0, 1.0);
        let mut shown = Vec::new();
        for node in &first.selected {
            shown.extend(crate::leaves_under(&hierarchy, *node));
        }
        assert!(!shown.contains(&1), "the occluded cluster contributes no draw for this view");
        assert!(shown.contains(&0) && shown.contains(&2), "the visible clusters of the same mesh stay");
        assert_eq!(first.submitted_triangles, 92);
        let second = crate::cut_visible_hierarchy(&hierarchy, &[1, 1, 1], &triangles, |_| 40.0, 0.577, 1080.0, 1.0);
        let mut shown_again = Vec::new();
        for node in &second.selected {
            shown_again.extend(crate::leaves_under(&hierarchy, *node));
        }
        assert!(shown_again.contains(&1), "another view can draw the cluster the first view hid");
        assert_eq!(second.submitted_triangles, 112);
        assert_eq!(hierarchy, before, "the cut does not write visibility onto the mesh");
    }

    #[test]
    fn hierarchical_depth_is_half_the_view_and_clamped() {
        assert_eq!(hierarchical_depth_extent(1920, 1080), (384, 216));
        assert_eq!(hierarchical_depth_extent(768, 432), (384, 216));
        assert_eq!(hierarchical_depth_extent(200, 120), (100, 60));
        assert_eq!(hierarchical_depth_extent(0, 0), (96, 54));
    }

    #[test]
    fn a_large_cluster_stays_conservative_on_the_reference_and_is_tested_on_the_view_buffer() {
        let mesh = MeshId(4);
        let records = [record([0.0, 0.0, 1.5], 1.8, 40)];
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -5.5), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let frustum = frustum_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        assert_eq!(frustum.flags, vec![1]);
        let reference = occlusion_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true).unwrap();
        assert_eq!(reference.flags, vec![4], "the 64 by 36 cap still refuses a large cluster");
        let sized = hierarchical_occlusion_cull(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true, 768, 432).unwrap();
        assert_eq!(sized.flags, vec![1], "a large on-screen cluster is tested and stays visible when nothing is in front");
    }

    #[test]
    fn a_visible_facade_stays_visible_on_the_hierarchical_depth_buffer() {
        let mesh = MeshId(4);
        let records = [record([0.0, 0.15, 0.45], 0.35, 40), record([0.0, -0.05, 0.0], 0.05, 30), record([0.0, 0.15, -1.6], 0.40, 18)];
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -4.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let frustum = frustum_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        let culled = hierarchical_occlusion_cull(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true, 768, 432).unwrap();
        assert_ne!(culled.flags[1], 3, "the visible facade must not be classified occluded, flags {:?}", culled.flags);
        assert_ne!(culled.flags[0], 3, "the protruding cluster does not occlude itself");
        assert_eq!(culled.flags[2], 3, "a cluster buried behind the protrusion can still be occluded, flags {:?}", culled.flags);
    }

    #[test]
    fn a_close_neighbor_is_not_hidden_by_a_large_occluder() {
        let mesh = MeshId(4);
        let records = [record([0.0, 0.0, 2.2], 1.1, 80), record([2.2, 0.0, 0.2], 0.2, 24)];
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -5.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let frustum = frustum_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        assert_eq!(frustum.flags, vec![1, 1], "both clusters are inside the frustum");
        let culled = hierarchical_occlusion_cull(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true, 768, 432).unwrap();
        assert_ne!(culled.flags[0], 3, "the occluder stays");
        assert_ne!(culled.flags[1], 3, "a neighbor outside the occluder circle stays drawn, flags {:?}", culled.flags);
    }

    #[test]
    fn an_alley_behind_a_wall_submits_no_triangles_and_the_side_stays() {
        let mesh = MeshId(4);
        let mut records = Vec::new();
        for y in [-0.55_f32, 0.0, 0.55] {
            for x in [-0.55_f32, 0.0, 0.55] {
                records.push(record([x, y, 0.0], 0.38, 12));
            }
        }
        records.push(record([0.0, 0.0, -1.8], 0.25, 30));
        records.push(record([3.2, 0.0, -0.2], 0.25, 18));
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -6.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let frustum = frustum_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        assert!(frustum.flags.iter().all(|flag| *flag == 1), "the wall, the alley, and the side cluster are in frame");
        let culled = hierarchical_occlusion_cull(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, &frustum, true, 768, 432).unwrap();
        assert_eq!(culled.flags[9], 3, "the alley behind the wall is occluded, flags {:?}", culled.flags);
        assert_ne!(culled.flags[10], 3, "the cluster beside the wall stays drawn, flags {:?}", culled.flags);
        assert!(culled.flags[..9].iter().all(|flag| *flag != 3), "the wall is not hidden by itself");
        let alley_triangles = records[9].triangles;
        assert!(culled.triangles_submitted + alley_triangles <= records.iter().map(|item| item.triangles).sum::<u32>());
        assert_eq!(culled.triangles_submitted, records.iter().enumerate().filter(|(index, _)| culled.flags[*index] != 3 && culled.flags[*index] != 0).map(|(_, item)| item.triangles).sum::<u32>());
        assert_eq!(culled.flags[9], 3);
        let drawn: u32 = records.iter().enumerate().filter(|(index, _)| culled.flags[*index] == 1 || culled.flags[*index] == 4).map(|(_, item)| item.triangles).sum();
        assert_eq!(culled.triangles_submitted, drawn);
        assert!(drawn < records.iter().map(|item| item.triangles).sum::<u32>());
    }

    #[test]
    fn an_actor_fully_outside_the_frustum_rejects_every_cluster() {
        let mesh = MeshId(4);
        let records = [record([-0.2, 0.0, 0.0], 0.2, 10), record([0.2, 0.4, 0.1], 0.15, 8), record([0.0, -0.3, -0.2], 0.18, 6)];
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(40.0, 0.0, -6.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let culled = frustum_cull_meshlets(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, true).unwrap();
        assert_eq!(culled.flags, vec![0, 0, 0]);
        assert_eq!(culled.submitted, 0);
        assert_eq!(culled.triangles_submitted, 0);
        assert_eq!(culled.instances_visible, 0);
    }

    #[test]
    fn a_frozen_frame_stays_on_the_captured_pipeline() {
        assert!(diagnostic_frame_is_reusable(true, Some(GeometryDiagnostic::HierarchyNoOcclusion), 4, GeometryDiagnostic::HierarchyNoOcclusion, 4));
        assert!(!diagnostic_frame_is_reusable(true, Some(GeometryDiagnostic::HierarchyNoOcclusion), 4, GeometryDiagnostic::HierarchyWithOcclusion, 4));
        assert!(!diagnostic_frame_is_reusable(false, Some(GeometryDiagnostic::LeafTruth), 4, GeometryDiagnostic::LeafTruth, 4));
        assert_eq!(DiagnosticVisualization::CutReasons.label(), "cut reasons");
        assert!(DiagnosticVisualization::EinsteinEligible.paints_einstein());
        assert!(!DiagnosticVisualization::CutReasons.paints_einstein());
        assert!(DiagnosticVisualization::HierarchyLevels.shows_level_colors());
        assert!(DiagnosticVisualization::CutReasons.shows_cut_colors());
        assert!(!DiagnosticVisualization::None.shows_cut_colors());
    }

    #[test]
    fn pipeline_coverage_keeps_in_frustum_pixels_and_occlusion_keeps_a_nearer_occluder() {
        let mesh = MeshId(4);
        let mut records = Vec::new();
        for y in [-0.55_f32, 0.0, 0.55] {
            for x in [-0.55_f32, 0.0, 0.55] {
                records.push(record([x, y, 0.0], 0.38, 12));
            }
        }
        records.push(record([0.0, 0.0, -1.8], 0.25, 30));
        records.push(record([3.2, 0.0, -0.2], 0.25, 18));
        let camera = ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let object = instance(mesh, EntityId::new(), Vec3::new(0.0, 0.0, -6.0), true);
        let geometries = [GpuSceneGeometry { mesh, meshlets: &records }];
        let coverage = compare_pipeline_coverage(std::slice::from_ref(&object), &camera, 60.0_f64.to_radians(), 0.1, 1.0, &geometries, 768, 432).unwrap();
        assert_eq!(coverage.hierarchy_hole_pixels, 0, "an in-frustum leaf pixel must survive hierarchy without occlusion");
        assert_eq!(
            coverage.occlusion_false_hidden_pixels,
            0,
            "occlusion may drop a pixel only where a nearer cluster is drawn, rejected {}",
            coverage.occlusion.occlusion_rejected
        );
        assert!(coverage.leaf.visible_pixels > 0);
        assert_eq!(coverage.hierarchy.visible_pixels, coverage.frustum.visible_pixels);
        assert_eq!(coverage.hierarchy.selected_parents, 0);
        assert!(coverage.occlusion.occlusion_rejected > 0);
        assert!(coverage.occlusion.submitted_triangles < coverage.leaf.submitted_triangles);
        assert_eq!(coverage.frustum.frustum_rejected, 0);
    }

    fn sample_packet(parent: bool) -> DiagnosticPacket {
        if parent {
            DiagnosticPacket {
                mesh_id: 4,
                local_meshlet_id: 0,
                global_meshlet_id: 0,
                parent_id: 2,
                vertex_buffer_id: 20,
                index_buffer_id: 21,
                first_index: 0,
                index_count: 6,
                base_vertex: 0,
                first_vertex: 0,
                vertex_count: 4,
                vertex_stride: DIAGNOSTIC_PARENT_STRIDE,
                buffer_stride: DIAGNOSTIC_PARENT_STRIDE,
                buffer_bytes: u64::from(4 * DIAGNOSTIC_PARENT_STRIDE),
                depth_write: true,
                color_key: 1,
            }
        } else {
            DiagnosticPacket {
                mesh_id: 4,
                local_meshlet_id: 1,
                global_meshlet_id: 8,
                parent_id: DIAGNOSTIC_NO_PARENT,
                vertex_buffer_id: 10,
                index_buffer_id: 11,
                first_index: 0,
                index_count: 6,
                base_vertex: 0,
                first_vertex: 0,
                vertex_count: 4,
                vertex_stride: 60,
                buffer_stride: 60,
                buffer_bytes: 4 * 60,
                depth_write: true,
                color_key: 1,
            }
        }
    }

    fn raster_surface(tris: &[((f32, f32), (f32, f32), (f32, f32), u32)]) -> Vec<u32> {
        let size = 32i32;
        let mut pixels = vec![0u32; (size * size) as usize];
        let edge = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0);
        for &(a, b, c, id) in tris {
            let area = edge(a, b, c);
            if area.abs() < 1.0e-5 {
                continue;
            }
            let min_x = a.0.min(b.0).min(c.0).floor().max(0.0) as i32;
            let max_x = a.0.max(b.0).max(c.0).ceil().min((size - 1) as f32) as i32;
            let min_y = a.1.min(b.1).min(c.1).floor().max(0.0) as i32;
            let max_y = a.1.max(b.1).max(c.1).ceil().min((size - 1) as f32) as i32;
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let point = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = edge(b, c, point);
                    let w1 = edge(c, a, point);
                    let w2 = edge(a, b, point);
                    if w0 * area >= 0.0 && w1 * area >= 0.0 && w2 * area >= 0.0 {
                        pixels[(y * size + x) as usize] = id;
                    }
                }
            }
        }
        pixels
    }

    #[test]
    fn visualization_keeps_one_draw_list_and_rejects_a_strided_parent_fan() {
        let leaf = sample_packet(false);
        let parent = sample_packet(true);
        let base = vec![leaf, parent];
        let cut = [1u32, 6, 4, 7];
        let wall = [
            ((2.0, 18.0), (14.0, 18.0), (8.0, 28.0), 1u32),
            ((16.0, 18.0), (28.0, 18.0), (22.0, 28.0), 2u32),
        ];
        let depth = raster_surface(&wall);
        let modes = [
            DiagnosticVisualization::None,
            DiagnosticVisualization::CutReasons,
            DiagnosticVisualization::HierarchyLevels,
            DiagnosticVisualization::EinsteinActive,
            DiagnosticVisualization::EinsteinEligible,
            DiagnosticVisualization::EinsteinLodError,
            DiagnosticVisualization::EinsteinState,
            DiagnosticVisualization::EinsteinReject,
        ];
        let none = hash_diagnostic_frame(&base, &cut, &depth);
        for (index, mode) in modes.iter().enumerate() {
            assert!(mode.recolors_submission() || *mode == DiagnosticVisualization::None);
            let colors = vec![pack_diagnostic_color([0.2, index as f32 / 8.0, 0.4]); base.len()];
            let colored = recolor_packets(&base, &colors);
            assert!(diagnostic_geometry_matches(&base, &colored), "{mode:?} changed a packet");
            let hashes = hash_diagnostic_frame(&colored, &cut, &depth);
            assert!(hashes.geometry_matches(none), "{mode:?} changed a geometry hash");
            if *mode != DiagnosticVisualization::None {
                assert_ne!(hashes.color_output_hash, none.color_output_hash);
            }
            for packet in &colored {
                assert!(packet_uses_its_own_buffers(packet, 10, 11));
                for index in [0u32, 1, 2, 3] {
                    assert!(packet_index_in_range(packet, index), "{mode:?} index {index}");
                }
            }
        }
        let overlay = DiagnosticPacket {
            first_index: 0,
            index_count: 3,
            depth_write: false,
            color_key: 9,
            ..leaf
        };
        assert!(detail_overlay_keeps_base(&base, &[overlay]));
        assert!(DiagnosticVisualization::EinsteinDetail.overlays_detail());
        assert!(!DiagnosticVisualization::EinsteinDetail.recolors_submission());
        let detail_base = hash_diagnostic_frame(&base, &cut, &depth);
        assert!(detail_base.geometry_matches(none));

        let mut fan = parent;
        fan.vertex_stride = 24;
        assert!(!packet_index_in_range(&fan, 1), "a stride-24 read of the 60-byte parent buffer is the stretched fan");
        assert!(!packet_uses_its_own_buffers(&fan, 10, 11));
        let mut crossed = parent;
        crossed.index_buffer_id = leaf.index_buffer_id;
        crossed.vertex_buffer_id = leaf.vertex_buffer_id;
        assert!(!packet_uses_its_own_buffers(&crossed, 10, 11));
        assert!(meshlet_slot_is_local(1, 4));
        assert!(!meshlet_slot_is_local(7, 4), "a scene-global meshlet id is not this mesh's slot");
        let fan_depth = raster_surface(&[
            wall[0],
            wall[1],
            ((0.0, 0.0), (31.0, 0.0), (16.0, 31.0), 9u32),
        ]);
        let fan_hash = hash_diagnostic_frame(&[leaf, fan], &cut, &fan_depth);
        assert!(!fan_hash.geometry_matches(none));
        assert_ne!(fan_hash.depth_hash, none.depth_hash);
        assert_ne!(fan_hash.vertex_source_hash, none.vertex_source_hash);
    }
}
