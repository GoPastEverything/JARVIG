//! CPU coverage of one camera against Leaf Truth.
//!
//! Each pipeline stage submits its own triangles. This raster uses one flat
//! depth test and ignores diagnostic colors. A parent may cover a pixel with
//! fewer triangles. A pixel Leaf Truth covers and the stage leaves empty is
//! lost coverage. This is not a GPU framebuffer and it does not read depth.

use crate::gpu_scene::{frustum_cull_meshlets, hierarchical_occlusion_cull, GeometryDiagnostic, GpuMeshletRecord, GpuSceneGeometry};
use crate::mesh::Mesh;
use crate::meshlet::MeshletSet;
use crate::meshlet_hierarchy::{cut_visible_hierarchy_for_pose, ClusterCut, ClusterHierarchy};
use crate::meshlet_parents::{submission_keeping_coverage, ParentRange};
use crate::scene::{instance_gpu_transforms, RenderInstance};
use crate::space::{ResolvedPose, SpaceError};

/// Parent positions above this stay on the GPU only. The compare then skips hierarchy stages.
pub const PARENT_POSITION_KEEP: usize = 2_000_000;

/// One stage's submission. Triangles are object-local. Colors are not stored.
#[derive(Clone, Debug)]
pub struct StageTriangles {
    pub stage: GeometryDiagnostic,
    pub submitted_meshlets: u32,
    pub submitted_triangles: u32,
    pub frustum_rejected: u32,
    pub occlusion_rejected: u32,
    pub hierarchy_parent_count: u32,
    pub hierarchy_leaf_count: u32,
    pub triangles: Vec<[[f32; 3]; 3]>,
    /// False when this stage could not be built. A zero loss on a skipped stage is not a pass.
    pub compared: bool,
}

/// Pixel result for one stage against the Leaf Truth raster.
#[derive(Clone, Debug)]
pub struct CoverageStageReport {
    pub stage: GeometryDiagnostic,
    pub submitted_meshlets: u32,
    pub submitted_triangles: u32,
    pub frustum_rejected: u32,
    pub occlusion_rejected: u32,
    pub hierarchy_parent_count: u32,
    pub hierarchy_leaf_count: u32,
    pub visible_pixel_count: u32,
    pub depth_coverage_hash: u64,
    /// Reference pixels this stage left empty. This is the red count.
    pub first_mismatching_pixel_count: u32,
    /// Pixels this stage covered where Leaf Truth has nothing.
    pub extra_pixel_count: u32,
    /// Lost pixels that are not the one-pixel silhouette of the reference.
    pub interior_lost_pixels: u32,
    pub compared: bool,
    /// Tightly packed RGB. Black matches. Red is lost. Blue is extra.
    pub rgb: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct LeafTruthComparison {
    pub width: u32,
    pub height: u32,
    pub reference_pixel_count: u32,
    pub reference_depth_hash: u64,
    pub stages: Vec<CoverageStageReport>,
    /// First compared stage, after Leaf Truth, whose red count is above zero.
    pub first_red_stage: Option<GeometryDiagnostic>,
    /// Hierarchy triangles were not available, so those stages were not compared.
    pub parent_positions_missing: bool,
}

/// Object-local parent triangles the cut may substitute for leaves.
#[derive(Clone, Copy)]
pub struct ParentCoverageInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub indices: &'a [u32],
    pub ranges: &'a [ParentRange],
    pub hierarchy: &'a ClusterHierarchy,
    pub draw_ok: &'a [bool],
    pub leaf_counts: &'a [u32],
}

struct CoverageMask {
    covered: Vec<bool>,
    depth_mm: Vec<u32>,
    pixels: u32,
    hash: u64,
}

/// Build the five submissions for one camera. Leaf Truth is first.
/// `lod_viewport_height` is the window height the parent cut uses. The raster size is separate.
pub fn assemble_leaf_truth_stages(
    mesh: &Mesh,
    meshlets: &MeshletSet,
    parents: Option<ParentCoverageInput<'_>>,
    instance: &RenderInstance,
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    lod_viewport_width: u32,
    lod_viewport_height: u32,
    error_px: f32,
) -> Result<Vec<StageTriangles>, SpaceError> {
    let records: Vec<GpuMeshletRecord> = meshlets.meshlets.iter().map(GpuMeshletRecord::from_meshlet).collect();
    let geometries = [GpuSceneGeometry { mesh: instance.mesh, meshlets: &records }];
    let instances = [instance.clone()];
    let frustum = frustum_cull_meshlets(&instances, camera, vertical_fov_radians, near_m, aspect, &geometries, true)?;
    let occlusion = hierarchical_occlusion_cull(
        &instances,
        camera,
        vertical_fov_radians,
        near_m,
        aspect,
        &geometries,
        &frustum,
        true,
        lod_viewport_width,
        lod_viewport_height,
    )?;
    let leaf_counts: Vec<u32> = meshlets.meshlets.iter().map(|meshlet| meshlet.index_count / 3).collect();
    let drawn_slots: Vec<usize> = meshlets
        .meshlets
        .iter()
        .enumerate()
        .filter(|(_, meshlet)| meshlet.submesh == 0)
        .map(|(index, _)| index)
        .collect();
    let leaf = leaf_stage(&drawn_slots, mesh, meshlets);
    let frustum_stage = flagged_leaf_stage(
        GeometryDiagnostic::FrustumOnly,
        &drawn_slots,
        mesh,
        meshlets,
        &frustum.flags,
        |flag| flag != 0,
        true,
        false,
    );
    let hierarchy_flags = frustum.flags.clone();
    let hierarchy = hierarchy_stage(
        GeometryDiagnostic::HierarchyNoOcclusion,
        mesh,
        meshlets,
        parents,
        instance,
        camera,
        vertical_fov_radians,
        near_m,
        aspect,
        lod_viewport_height,
        error_px,
        &hierarchy_flags,
        &leaf_counts,
        &drawn_slots,
        false,
    )?;
    let occluded = hierarchy_stage(
        GeometryDiagnostic::HierarchyWithOcclusion,
        mesh,
        meshlets,
        parents,
        instance,
        camera,
        vertical_fov_radians,
        near_m,
        aspect,
        lod_viewport_height,
        error_px,
        &occlusion.flags,
        &leaf_counts,
        &drawn_slots,
        true,
    )?;
    let mut normal = occluded.clone();
    normal.stage = GeometryDiagnostic::Normal;
    Ok(vec![leaf, frustum_stage, hierarchy, occluded, normal])
}

/// Raster every compared stage against Leaf Truth. Back faces are rejected on every stage.
pub fn compare_stage_triangles(
    stages: &[StageTriangles],
    instance: &RenderInstance,
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    raster_width: u32,
    raster_height: u32,
) -> Result<LeafTruthComparison, SpaceError> {
    let (width, height) = coverage_extent(raster_width, raster_height);
    let reference = stages.iter().find(|stage| stage.stage == GeometryDiagnostic::LeafTruth && stage.compared);
    let Some(reference) = reference else {
        return Ok(LeafTruthComparison {
            width,
            height,
            reference_pixel_count: 0,
            reference_depth_hash: 0,
            stages: Vec::new(),
            first_red_stage: None,
            parent_positions_missing: false,
        });
    };
    let reference_mask = raster_triangles(&reference.triangles, instance, camera, vertical_fov_radians, near_m, aspect, width, height)?;
    let mut reports = Vec::with_capacity(stages.len());
    let mut first_red_stage = None;
    let mut parent_positions_missing = false;
    for stage in stages {
        if !stage.compared {
            if matches!(stage.stage, GeometryDiagnostic::HierarchyNoOcclusion | GeometryDiagnostic::HierarchyWithOcclusion | GeometryDiagnostic::Normal) {
                parent_positions_missing = true;
            }
            reports.push(empty_report(stage, width, height));
            continue;
        }
        let mask = if stage.stage == GeometryDiagnostic::LeafTruth {
            reference_mask.clone_shallow()
        } else {
            raster_triangles(&stage.triangles, instance, camera, vertical_fov_radians, near_m, aspect, width, height)?
        };
        let mut lost = 0u32;
        let mut extra = 0u32;
        let mut rgb = vec![0u8; reference_mask.covered.len() * 3];
        let mut lost_flags = vec![false; reference_mask.covered.len()];
        for index in 0..reference_mask.covered.len() {
            let reference_on = reference_mask.covered[index];
            let stage_on = mask.covered[index];
            let pixel = &mut rgb[index * 3..index * 3 + 3];
            if reference_on && !stage_on {
                lost = lost.saturating_add(1);
                lost_flags[index] = true;
                pixel.copy_from_slice(&[255, 0, 0]);
            } else if stage_on && !reference_on {
                extra = extra.saturating_add(1);
                pixel.copy_from_slice(&[0, 0, 255]);
            }
        }
        let interior = interior_lost(&reference_mask.covered, &lost_flags, width, height);
        if first_red_stage.is_none() && stage.stage != GeometryDiagnostic::LeafTruth && lost > 0 {
            first_red_stage = Some(stage.stage);
        }
        reports.push(CoverageStageReport {
            stage: stage.stage,
            submitted_meshlets: stage.submitted_meshlets,
            submitted_triangles: stage.submitted_triangles,
            frustum_rejected: stage.frustum_rejected,
            occlusion_rejected: stage.occlusion_rejected,
            hierarchy_parent_count: stage.hierarchy_parent_count,
            hierarchy_leaf_count: stage.hierarchy_leaf_count,
            visible_pixel_count: mask.pixels,
            depth_coverage_hash: mask.hash,
            first_mismatching_pixel_count: lost,
            extra_pixel_count: extra,
            interior_lost_pixels: interior,
            compared: true,
            rgb,
        });
    }
    Ok(LeafTruthComparison {
        width,
        height,
        reference_pixel_count: reference_mask.pixels,
        reference_depth_hash: reference_mask.hash,
        stages: reports,
        first_red_stage,
        parent_positions_missing,
    })
}

pub fn coverage_stage_line(report: &CoverageStageReport) -> String {
    if !report.compared {
        return format!(
            "{}: not compared. Parent positions were not retained, so this stage cannot be judged.",
            report.stage.label()
        );
    }
    format!(
        "{}: meshlets {} tris {} fru {} occ {} parents {} leaves {} pixels {} depth {:016x} red {} blue {} interior {}",
        report.stage.label(),
        report.submitted_meshlets,
        report.submitted_triangles,
        report.frustum_rejected,
        report.occlusion_rejected,
        report.hierarchy_parent_count,
        report.hierarchy_leaf_count,
        report.visible_pixel_count,
        report.depth_coverage_hash,
        report.first_mismatching_pixel_count,
        report.extra_pixel_count,
        report.interior_lost_pixels
    )
}

/// 24-bit BMP, bottom row first. The RGB buffer is top row first.
pub fn coverage_bmp(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let row_stride = (width as usize * 3).div_ceil(4) * 4;
    let pixels = row_stride * height as usize;
    let mut bytes = vec![0u8; 54 + pixels];
    bytes[0] = b'B';
    bytes[1] = b'M';
    bytes[2..6].copy_from_slice(&((54 + pixels) as u32).to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&width.to_le_bytes());
    bytes[22..26].copy_from_slice(&height.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    let width = width as usize;
    let height = height as usize;
    for y in 0..height {
        let src = (height - 1 - y) * width;
        let dst = 54 + y * row_stride;
        for x in 0..width {
            let pixel = (src + x) * 3;
            if pixel + 2 < rgb.len() {
                bytes[dst + x * 3] = rgb[pixel + 2];
                bytes[dst + x * 3 + 1] = rgb[pixel + 1];
                bytes[dst + x * 3 + 2] = rgb[pixel];
            }
        }
    }
    bytes
}

fn coverage_extent(width: u32, height: u32) -> (u32, u32) {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let scale = (480.0 / width.max(height)).min(1.0);
    (((width * scale).round() as u32).max(16), ((height * scale).round() as u32).max(16))
}

fn empty_report(stage: &StageTriangles, width: u32, height: u32) -> CoverageStageReport {
    CoverageStageReport {
        stage: stage.stage,
        submitted_meshlets: stage.submitted_meshlets,
        submitted_triangles: stage.submitted_triangles,
        frustum_rejected: stage.frustum_rejected,
        occlusion_rejected: stage.occlusion_rejected,
        hierarchy_parent_count: stage.hierarchy_parent_count,
        hierarchy_leaf_count: stage.hierarchy_leaf_count,
        visible_pixel_count: 0,
        depth_coverage_hash: 0,
        first_mismatching_pixel_count: 0,
        extra_pixel_count: 0,
        interior_lost_pixels: 0,
        compared: false,
        rgb: vec![0u8; width as usize * height as usize * 3],
    }
}

fn leaf_stage(slots: &[usize], mesh: &Mesh, meshlets: &MeshletSet) -> StageTriangles {
    let mut triangles = Vec::new();
    for slot in slots {
        push_meshlet(mesh, meshlets, *slot as u32, &mut triangles);
    }
    StageTriangles {
        stage: GeometryDiagnostic::LeafTruth,
        submitted_meshlets: slots.len() as u32,
        submitted_triangles: triangles.len() as u32,
        frustum_rejected: 0,
        occlusion_rejected: 0,
        hierarchy_parent_count: 0,
        hierarchy_leaf_count: slots.len() as u32,
        triangles,
        compared: true,
    }
}

fn flagged_leaf_stage(
    stage: GeometryDiagnostic,
    slots: &[usize],
    mesh: &Mesh,
    meshlets: &MeshletSet,
    flags: &[u32],
    keep: impl Fn(u32) -> bool,
    count_frustum: bool,
    count_occlusion: bool,
) -> StageTriangles {
    let mut triangles = Vec::new();
    let mut kept = 0u32;
    for slot in slots {
        let flag = flags.get(*slot).copied().unwrap_or(1);
        if keep(flag) {
            push_meshlet(mesh, meshlets, *slot as u32, &mut triangles);
            kept = kept.saturating_add(1);
        }
    }
    StageTriangles {
        stage,
        submitted_meshlets: kept,
        submitted_triangles: triangles.len() as u32,
        frustum_rejected: if count_frustum { flags.iter().filter(|flag| **flag == 0).count() as u32 } else { 0 },
        occlusion_rejected: if count_occlusion { flags.iter().filter(|flag| **flag == 3).count() as u32 } else { 0 },
        hierarchy_parent_count: 0,
        hierarchy_leaf_count: kept,
        triangles,
        compared: true,
    }
}

fn hierarchy_stage(
    stage: GeometryDiagnostic,
    mesh: &Mesh,
    meshlets: &MeshletSet,
    parents: Option<ParentCoverageInput<'_>>,
    instance: &RenderInstance,
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    lod_viewport_height: u32,
    error_px: f32,
    flags: &[u32],
    leaf_counts: &[u32],
    slots: &[usize],
    count_occlusion: bool,
) -> Result<StageTriangles, SpaceError> {
    let Some(parents) = parents else {
        return Ok(StageTriangles {
            stage,
            submitted_meshlets: 0,
            submitted_triangles: 0,
            frustum_rejected: flags.iter().filter(|flag| **flag == 0).count() as u32,
            occlusion_rejected: if count_occlusion { flags.iter().filter(|flag| **flag == 3).count() as u32 } else { 0 },
            hierarchy_parent_count: 0,
            hierarchy_leaf_count: 0,
            triangles: Vec::new(),
            compared: false,
        });
    };
    if parents.positions.is_empty() || parents.hierarchy.leaf_count == 0 {
        return Ok(StageTriangles {
            stage,
            submitted_meshlets: 0,
            submitted_triangles: 0,
            frustum_rejected: flags.iter().filter(|flag| **flag == 0).count() as u32,
            occlusion_rejected: if count_occlusion { flags.iter().filter(|flag| **flag == 3).count() as u32 } else { 0 },
            hierarchy_parent_count: 0,
            hierarchy_leaf_count: 0,
            triangles: Vec::new(),
            compared: false,
        });
    }
    let visible: Vec<bool> = (0..meshlets.meshlets.len()).map(|index| drawable_leaf(flags.get(index).copied().unwrap_or(1))).collect();
    let view = cut_visible_hierarchy_for_pose(
        parents.hierarchy,
        instance,
        camera,
        vertical_fov_radians,
        near_m,
        aspect,
        lod_viewport_height as f32,
        error_px,
        flags,
        leaf_counts,
    )?;
    let cut = ClusterCut { selected: view.selected, coarseness: view.coarseness, covered_leaves: view.covered_leaves };
    let submission = submission_keeping_coverage(
        parents.hierarchy,
        parents.ranges,
        parents.leaf_counts,
        leaf_counts.iter().fold(0u32, |sum, count| sum.saturating_add(*count)),
        &cut,
        Some(&visible),
        parents.draw_ok,
    );
    let mut triangles = Vec::new();
    let mut leaves = 0u32;
    for leaf in submission.leaf_meshlets {
        if slots.binary_search(&(leaf as usize)).is_err() && !slots.contains(&(leaf as usize)) {
            continue;
        }
        if !visible.get(leaf as usize).copied().unwrap_or(false) {
            continue;
        }
        push_meshlet(mesh, meshlets, leaf, &mut triangles);
        leaves = leaves.saturating_add(1);
    }
    let mut parents_drawn = 0u32;
    for node in submission.parent_nodes {
        let before = triangles.len();
        push_parent(parents, node, &mut triangles);
        if triangles.len() > before {
            parents_drawn = parents_drawn.saturating_add(1);
        }
    }
    Ok(StageTriangles {
        stage,
        submitted_meshlets: leaves.saturating_add(parents_drawn),
        submitted_triangles: triangles.len() as u32,
        frustum_rejected: flags.iter().filter(|flag| **flag == 0).count() as u32,
        occlusion_rejected: if count_occlusion { flags.iter().filter(|flag| **flag == 3).count() as u32 } else { 0 },
        hierarchy_parent_count: parents_drawn,
        hierarchy_leaf_count: leaves,
        triangles,
        compared: true,
    })
}

fn drawable_leaf(flag: u32) -> bool {
    flag != 0 && flag != 3 && flag != 6
}

fn push_meshlet(mesh: &Mesh, meshlets: &MeshletSet, index: u32, triangles: &mut Vec<[[f32; 3]; 3]>) {
    let Some(meshlet) = meshlets.meshlets.get(index as usize) else { return };
    if meshlet.submesh != 0 {
        return;
    }
    let vertex_end = meshlet.vertex_offset as usize + meshlet.vertex_count as usize;
    let index_end = meshlet.index_offset as usize + meshlet.index_count as usize;
    let Some(verts) = meshlets.vertex_indices.get(meshlet.vertex_offset as usize..vertex_end) else { return };
    let Some(locals) = meshlets.local_indices.get(meshlet.index_offset as usize..index_end) else { return };
    for corner in locals.chunks_exact(3) {
        let (Some(a), Some(b), Some(c)) = (
            verts.get(corner[0] as usize).and_then(|index| mesh.position(*index)),
            verts.get(corner[1] as usize).and_then(|index| mesh.position(*index)),
            verts.get(corner[2] as usize).and_then(|index| mesh.position(*index)),
        ) else {
            continue;
        };
        triangles.push([a, b, c]);
    }
}

fn push_parent(parents: ParentCoverageInput<'_>, node: u32, triangles: &mut Vec<[[f32; 3]; 3]>) {
    let Some(range) = parents.ranges.get(node as usize) else { return };
    let end = range.first_index as usize + range.index_count as usize;
    let Some(indices) = parents.indices.get(range.first_index as usize..end) else { return };
    for corner in indices.chunks_exact(3) {
        let (Some(a), Some(b), Some(c)) = (
            parents.positions.get(corner[0] as usize).copied(),
            parents.positions.get(corner[1] as usize).copied(),
            parents.positions.get(corner[2] as usize).copied(),
        ) else {
            continue;
        };
        triangles.push([a, b, c]);
    }
}

impl CoverageMask {
    fn clone_shallow(&self) -> Self {
        Self { covered: self.covered.clone(), depth_mm: self.depth_mm.clone(), pixels: self.pixels, hash: self.hash }
    }
}

fn raster_triangles(
    triangles: &[[[f32; 3]; 3]],
    instance: &RenderInstance,
    camera: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    width: u32,
    height: u32,
) -> Result<CoverageMask, SpaceError> {
    let transforms = instance_gpu_transforms(instance, camera, vertical_fov_radians, near_m, aspect)?;
    let eye_from_local = transforms.view.mul(transforms.model);
    let count = width as usize * height as usize;
    let mut depth = vec![f32::MAX; count];
    let mut covered = vec![false; count];
    let near = near_m.max(0.0001);
    for triangle in triangles {
        let eye = triangle.map(|position| {
            let point = eye_from_local.transform_point(position);
            [point[0], point[1], point[2]]
        });
        for clipped in clip_near(eye, near) {
            raster_eye(&clipped, &transforms.projection, near, width, height, &mut depth, &mut covered);
        }
    }
    let mut depth_mm = vec![0u32; count];
    let mut hash = 0xcbf29ce484222325u64;
    let mut pixels = 0u32;
    for index in 0..count {
        if !covered[index] {
            hash = mix(hash, 0);
            continue;
        }
        pixels = pixels.saturating_add(1);
        let millimeters = (depth[index] * 1000.0).round().max(1.0) as u32;
        depth_mm[index] = millimeters;
        hash = mix(hash, u64::from(millimeters));
    }
    Ok(CoverageMask { covered, depth_mm, pixels, hash })
}

fn raster_eye(eye: &[[f32; 3]; 3], projection: &crate::space::Mat4, _near: f32, width: u32, height: u32, depth: &mut [f32], covered: &mut [bool]) {
    let mut screen = [(0.0f32, 0.0f32, 0.0f32); 3];
    for (index, point) in eye.iter().enumerate() {
        let clip = projection.transform_point(*point);
        if clip[3] <= 1.0e-5 {
            return;
        }
        let ndc_x = clip[0] / clip[3];
        let ndc_y = clip[1] / clip[3];
        screen[index] = ((ndc_x * 0.5 + 0.5) * width as f32, (0.5 - ndc_y * 0.5) * height as f32, -point[2]);
    }
    let area = edge(screen[0].0, screen[0].1, screen[1].0, screen[1].1, screen[2].0, screen[2].1);
    // A camera-facing triangle is counter-clockwise in x/y and positive under this edge function.
    if area <= 1.0e-4 {
        return;
    }
    let min_x = screen.iter().map(|point| point.0).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let max_x = screen.iter().map(|point| point.0).fold(f32::MIN, f32::max).ceil().min(width as f32) as i32;
    let min_y = screen.iter().map(|point| point.1).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let max_y = screen.iter().map(|point| point.1).fold(f32::MIN, f32::max).ceil().min(height as f32) as i32;
    let inv_area = 1.0 / area;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let w0 = edge(screen[1].0, screen[1].1, screen[2].0, screen[2].1, px, py);
            let w1 = edge(screen[2].0, screen[2].1, screen[0].0, screen[0].1, px, py);
            let w2 = edge(screen[0].0, screen[0].1, screen[1].0, screen[1].1, px, py);
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let b0 = w0 * inv_area;
            let b1 = w1 * inv_area;
            let b2 = w2 * inv_area;
            let depth_here = b0 * screen[0].2 + b1 * screen[1].2 + b2 * screen[2].2;
            if !(depth_here.is_finite() && depth_here > 0.0) {
                continue;
            }
            let slot = y as usize * width as usize + x as usize;
            if depth_here < depth[slot] {
                depth[slot] = depth_here;
                covered[slot] = true;
            }
        }
    }
}

fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}

fn clip_near(triangle: [[f32; 3]; 3], near: f32) -> Vec<[[f32; 3]; 3]> {
    let inside = |point: [f32; 3]| -point[2] >= near;
    let mut polygon = Vec::with_capacity(4);
    for index in 0..3 {
        let current = triangle[index];
        let next = triangle[(index + 1) % 3];
        let current_in = inside(current);
        let next_in = inside(next);
        if current_in {
            polygon.push(current);
        }
        if current_in != next_in {
            let current_depth = -current[2];
            let next_depth = -next[2];
            let denominator = next_depth - current_depth;
            if denominator.abs() > 1.0e-8 {
                let t = ((near - current_depth) / denominator).clamp(0.0, 1.0);
                polygon.push([
                    current[0] + (next[0] - current[0]) * t,
                    current[1] + (next[1] - current[1]) * t,
                    current[2] + (next[2] - current[2]) * t,
                ]);
            }
        }
    }
    if polygon.len() < 3 {
        return Vec::new();
    }
    let mut output = Vec::new();
    for index in 1..polygon.len() - 1 {
        output.push([polygon[0], polygon[index], polygon[index + 1]]);
    }
    output
}

fn interior_lost(reference: &[bool], lost: &[bool], width: u32, height: u32) -> u32 {
    let width = width as usize;
    let height = height as usize;
    let mut count = 0u32;
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if !lost.get(index).copied().unwrap_or(false) {
                continue;
            }
            let edge = x == 0
                || y == 0
                || x + 1 == width
                || y + 1 == height
                || !reference.get(index - 1).copied().unwrap_or(false)
                || !reference.get(index + 1).copied().unwrap_or(false)
                || !reference.get(index - width).copied().unwrap_or(false)
                || !reference.get(index + width).copied().unwrap_or(false);
            if !edge {
                count = count.saturating_add(1);
            }
        }
    }
    count
}

fn mix(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x100000001b3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::mesh_from_surfaces;
    use crate::meshlet::{Meshlet, MeshletSet, MeshletStats};
    use crate::meshlet_hierarchy::ClusterNode;
    use crate::meshlet_parents::ParentRange;
    use crate::{CanonicalSurface, EntityId, LocalBounds, MeshId, ObjectId, Quat, RenderInstanceId, Vec3};

    fn camera_at(z: f64) -> ResolvedPose {
        ResolvedPose { translation: Vec3::new(0.0, 0.0, z), rotation: Quat::IDENTITY }
    }

    fn instance_at(mesh: MeshId, translation: Vec3) -> RenderInstance {
        RenderInstance {
            id: RenderInstanceId(1),
            entity: EntityId::new(),
            source: ObjectId(1),
            mesh,
            pose: ResolvedPose { translation, rotation: Quat::IDENTITY },
            scale: Vec3::new(1.0, 1.0, 1.0),
            bounds: LocalBounds {
                aabb: crate::Aabb { min: [-2.0, -2.0, -2.0], max: [2.0, 2.0, 2.0] },
                sphere: crate::BoundingSphere { center: [0.0, 0.0, 0.0], radius: 2.0 },
            },
            visible: true,
            cast_shadows: true,
            receive_shadows: true,
            material_bindings: Vec::new(),
        }
    }

    fn facing_quad() -> (Mesh, MeshletSet) {
        let positions = vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0], [6.0, -0.2, 0.0], [7.5, -0.2, 0.0], [7.5, 1.2, 0.0]];
        let mesh = mesh_from_surfaces(&[CanonicalSurface {
            positions: positions.clone(),
            normals: Vec::new(),
            texcoords: vec![[0.0, 0.0]; positions.len()],
            tangents: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6],
            material_slot: 0,
        }])
        .expect("quad");
        let meshlets = vec![
            Meshlet {
                vertex_offset: 0,
                vertex_count: 4,
                index_offset: 0,
                index_count: 6,
                material_slot: 0,
                submesh: 0,
                bounds_min: [-1.0, -1.0, 0.0],
                bounds_max: [1.0, 1.0, 0.0],
                sphere_center: [0.0, 0.0, 0.0],
                sphere_radius: 1.5,
                cone_axis: [0.0, 0.0, 1.0],
                cone_cutoff: 1.0,
            },
            Meshlet {
                vertex_offset: 4,
                vertex_count: 3,
                index_offset: 6,
                index_count: 3,
                material_slot: 0,
                submesh: 0,
                bounds_min: [6.0, -0.2, 0.0],
                bounds_max: [7.5, 1.2, 0.0],
                sphere_center: [6.75, 0.5, 0.0],
                sphere_radius: 1.2,
                cone_axis: [0.0, 0.0, 1.0],
                cone_cutoff: 1.0,
            },
        ];
        let set = MeshletSet {
            max_vertices: 128,
            max_triangles: 128,
            grid_resolution: 1,
            builder_version: 1,
            importer_version: 0,
            source_vertices: 7,
            source_fingerprint: String::new(),
            meshlets,
            vertex_indices: vec![0, 1, 2, 3, 4, 5, 6],
            local_indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2],
            stats: MeshletStats {
                source_triangles: 3,
                meshlet_count: 2,
                average_triangles: 1.5,
                average_vertices: 3.5,
                min_triangles: 1,
                max_triangles: 2,
                min_vertices: 3,
                max_vertices: 4,
                derived_bytes: 0,
                build_ms: 0.0,
                load_ms: 0.0,
                write_ms: 0.0,
            },
        };
        (mesh, set)
    }

    fn cover_parents(positions: Vec<[f32; 3]>, indices: Vec<u32>, draw_ok: bool) -> (ClusterHierarchy, Vec<ParentRange>, Vec<u32>, Vec<bool>, Vec<[f32; 3]>, Vec<u32>) {
        let hierarchy = ClusterHierarchy {
            nodes: vec![
                ClusterNode { parent: 2, children: [0, 0], child_count: 0, leaf: 0, center: [0.0, 0.0, 0.0], radius: 1.5, error: 0.0 },
                ClusterNode { parent: u32::MAX, children: [0, 0], child_count: 0, leaf: 1, center: [6.75, 0.5, 0.0], radius: 1.2, error: 0.0 },
                ClusterNode { parent: u32::MAX, children: [0, 0], child_count: 1, leaf: u32::MAX, center: [0.0, 0.0, 0.0], radius: 1.5, error: 0.01 },
            ],
            roots: vec![2, 1],
            leaf_count: 2,
        };
        let ranges = vec![
            ParentRange { first_index: 0, index_count: 0, triangles: 0, level: 0 },
            ParentRange { first_index: 0, index_count: 0, triangles: 0, level: 0 },
            ParentRange { first_index: 0, index_count: indices.len() as u32, triangles: (indices.len() / 3) as u32, level: 1 },
        ];
        (hierarchy, ranges, vec![2, 1], vec![false, false, draw_ok], positions, indices)
    }

    fn stage_of(stages: &[StageTriangles], stage: GeometryDiagnostic) -> &StageTriangles {
        stages.iter().find(|item| item.stage == stage).expect("stage")
    }

    fn report_of(comparison: &LeafTruthComparison, stage: GeometryDiagnostic) -> &CoverageStageReport {
        comparison.stages.iter().find(|item| item.stage == stage).expect("report")
    }

    #[test]
    fn leaf_truth_matches_itself_and_a_covering_parent_is_not_red() {
        let (mesh, set) = facing_quad();
        let instance = instance_at(MeshId(3), Vec3::new(0.0, 0.0, -4.0));
        let camera = camera_at(0.0);
        let (hierarchy, ranges, counts, draw_ok, positions, indices) = cover_parents(
            vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]],
            vec![0, 1, 2, 0, 2, 3],
            true,
        );
        let parents = ParentCoverageInput {
            positions: &positions,
            indices: &indices,
            ranges: &ranges,
            hierarchy: &hierarchy,
            draw_ok: &draw_ok,
            leaf_counts: &counts,
        };
        let stages = assemble_leaf_truth_stages(&mesh, &set, Some(parents), &instance, &camera, 60.0_f64.to_radians(), 0.1, 1.0, 64, 64, 8.0).unwrap();
        let leaf = stage_of(&stages, GeometryDiagnostic::LeafTruth);
        let frustum = stage_of(&stages, GeometryDiagnostic::FrustumOnly);
        let hierarchy_stage = stage_of(&stages, GeometryDiagnostic::HierarchyNoOcclusion);
        assert!(leaf.submitted_triangles >= 2);
        assert!(frustum.submitted_triangles < leaf.submitted_triangles, "off-screen leaf stays in leaf truth");
        assert!(frustum.frustum_rejected >= 1);
        assert!(hierarchy_stage.hierarchy_parent_count >= 1);
        assert!(hierarchy_stage.submitted_triangles < leaf.submitted_triangles);
        let comparison = compare_stage_triangles(&stages, &instance, &camera, 60.0_f64.to_radians(), 0.1, 1.0, 64, 64).unwrap();
        let leaf_report = report_of(&comparison, GeometryDiagnostic::LeafTruth);
        let frustum_report = report_of(&comparison, GeometryDiagnostic::FrustumOnly);
        let hierarchy_report = report_of(&comparison, GeometryDiagnostic::HierarchyNoOcclusion);
        assert_eq!(leaf_report.first_mismatching_pixel_count, 0);
        assert_eq!(leaf_report.extra_pixel_count, 0);
        assert!(leaf_report.visible_pixel_count > 20);
        assert_eq!(frustum_report.first_mismatching_pixel_count, 0, "an off-screen leaf is not red");
        assert_eq!(frustum_report.interior_lost_pixels, 0);
        assert_eq!(hierarchy_report.first_mismatching_pixel_count, 0, "a parent that covers the quad is not red");
        assert_eq!(hierarchy_report.interior_lost_pixels, 0);
        assert!(hierarchy_report.submitted_triangles < leaf_report.submitted_triangles);
        assert_eq!(comparison.first_red_stage, None);
    }

    #[test]
    fn a_parent_that_misses_the_quad_is_red_at_hierarchy() {
        let (mesh, set) = facing_quad();
        let instance = instance_at(MeshId(3), Vec3::new(0.0, 0.0, -4.0));
        let camera = camera_at(0.0);
        let (hierarchy, ranges, counts, draw_ok, positions, indices) = cover_parents(vec![[-0.05, -0.05, 0.0], [0.05, -0.05, 0.0], [0.0, 0.05, 0.0]], vec![0, 1, 2], true);
        let parents = ParentCoverageInput {
            positions: &positions,
            indices: &indices,
            ranges: &ranges,
            hierarchy: &hierarchy,
            draw_ok: &draw_ok,
            leaf_counts: &counts,
        };
        let stages = assemble_leaf_truth_stages(&mesh, &set, Some(parents), &instance, &camera, 60.0_f64.to_radians(), 0.1, 1.0, 64, 64, 8.0).unwrap();
        let comparison = compare_stage_triangles(&stages, &instance, &camera, 60.0_f64.to_radians(), 0.1, 1.0, 64, 64).unwrap();
        let hierarchy_report = report_of(&comparison, GeometryDiagnostic::HierarchyNoOcclusion);
        assert!(hierarchy_report.first_mismatching_pixel_count > 10, "red {}", hierarchy_report.first_mismatching_pixel_count);
        assert!(hierarchy_report.interior_lost_pixels > 0);
        assert_eq!(comparison.first_red_stage, Some(GeometryDiagnostic::HierarchyNoOcclusion));
        assert_eq!(report_of(&comparison, GeometryDiagnostic::FrustumOnly).first_mismatching_pixel_count, 0);
        let red = hierarchy_report.rgb.chunks_exact(3).any(|pixel| pixel == [255, 0, 0]);
        assert!(red);
    }

    #[test]
    fn dropping_a_hidden_surface_is_not_red_and_dropping_the_front_is() {
        let instance = instance_at(MeshId(3), Vec3::ZERO);
        let camera = camera_at(0.0);
        let front = [[-1.0, -1.0, -4.0], [1.0, -1.0, -4.0], [1.0, 1.0, -4.0]];
        let front_b = [[-1.0, -1.0, -4.0], [1.0, 1.0, -4.0], [-1.0, 1.0, -4.0]];
        let back = [[-0.6, -0.6, -8.0], [0.6, -0.6, -8.0], [0.6, 0.6, -8.0]];
        let leaf = StageTriangles {
            stage: GeometryDiagnostic::LeafTruth,
            submitted_meshlets: 2,
            submitted_triangles: 3,
            frustum_rejected: 0,
            occlusion_rejected: 0,
            hierarchy_parent_count: 0,
            hierarchy_leaf_count: 2,
            triangles: vec![front, front_b, back],
            compared: true,
        };
        let mut hidden = leaf.clone();
        hidden.stage = GeometryDiagnostic::HierarchyWithOcclusion;
        hidden.triangles = vec![front, front_b];
        hidden.occlusion_rejected = 1;
        hidden.submitted_meshlets = 1;
        hidden.submitted_triangles = 2;
        let mut front_lost = hidden.clone();
        front_lost.stage = GeometryDiagnostic::HierarchyNoOcclusion;
        front_lost.triangles = vec![back];
        front_lost.occlusion_rejected = 0;
        let comparison = compare_stage_triangles(&[leaf, front_lost, hidden], &instance, &camera, 60.0_f64.to_radians(), 0.1, 1.0, 64, 64).unwrap();
        let hidden_report = report_of(&comparison, GeometryDiagnostic::HierarchyWithOcclusion);
        let lost_report = report_of(&comparison, GeometryDiagnostic::HierarchyNoOcclusion);
        assert_eq!(hidden_report.first_mismatching_pixel_count, 0);
        assert!(lost_report.interior_lost_pixels > 0);
        assert_eq!(comparison.first_red_stage, Some(GeometryDiagnostic::HierarchyNoOcclusion));
    }

    #[test]
    fn missing_parent_positions_do_not_count_as_a_clean_hierarchy() {
        let (mesh, set) = facing_quad();
        let instance = instance_at(MeshId(3), Vec3::new(0.0, 0.0, -4.0));
        let camera = camera_at(0.0);
        let stages = assemble_leaf_truth_stages(&mesh, &set, None, &instance, &camera, 60.0_f64.to_radians(), 0.1, 1.0, 64, 64, 1.0).unwrap();
        assert!(!stage_of(&stages, GeometryDiagnostic::HierarchyNoOcclusion).compared);
        let comparison = compare_stage_triangles(&stages, &instance, &camera, 60.0_f64.to_radians(), 0.1, 1.0, 64, 64).unwrap();
        assert!(comparison.parent_positions_missing);
        assert!(!report_of(&comparison, GeometryDiagnostic::HierarchyNoOcclusion).compared);
        assert_eq!(comparison.first_red_stage, None);
        assert!(coverage_stage_line(report_of(&comparison, GeometryDiagnostic::HierarchyNoOcclusion)).contains("not compared"));
    }
}
