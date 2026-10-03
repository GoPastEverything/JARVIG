//! Parent and child links over an accepted meshlet set.
//!
//! The meshlet partition is not rebuilt. A selected parent stands for every
//! descendant leaf, so a cut cannot drop a region. When parent meshes exist,
//! that parent is drawn instead of the leaves under it.

use crate::gpu_scene::GpuMeshletRecord;

/// One node in the cluster hierarchy. Leaves occupy `0..leaf_count`.
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterNode {
    pub parent: u32,
    pub children: [u32; 2],
    pub child_count: u32,
    pub leaf: u32,
    pub center: [f32; 3],
    pub radius: f32,
    /// Object-space error. A leaf is 0. A parent is at least as large as its children.
    pub error: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterHierarchy {
    pub nodes: Vec<ClusterNode>,
    pub roots: Vec<u32>,
    pub leaf_count: u32,
}

/// Nodes whose screen-space error is small enough, covering every leaf once.
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterCut {
    pub selected: Vec<u32>,
    /// For each leaf, how many parents sit between it and the selected node. 0 means the leaf itself.
    pub coarseness: Vec<u32>,
    /// Descendant leaves of the selected nodes. A complete cut equals `leaf_count`.
    pub covered_leaves: u32,
}

const NO_PARENT: u32 = u32::MAX;
const NO_LEAF: u32 = u32::MAX;

pub fn build_cluster_hierarchy(records: &[GpuMeshletRecord]) -> ClusterHierarchy {
    let leaf_count = records.len() as u32;
    if records.is_empty() {
        return ClusterHierarchy { nodes: Vec::new(), roots: Vec::new(), leaf_count: 0 };
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for record in records {
        for axis in 0..3 {
            min[axis] = min[axis].min(record.center[axis]);
            max[axis] = max[axis].max(record.center[axis]);
        }
    }
    let extent = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let mut order: Vec<u32> = (0..leaf_count).collect();
    order.sort_by(|&left, &right| {
        let left_key = morton(records[left as usize].center, min, extent);
        let right_key = morton(records[right as usize].center, min, extent);
        left_key.cmp(&right_key).then(left.cmp(&right))
    });
    let mut nodes: Vec<ClusterNode> = records
        .iter()
        .enumerate()
        .map(|(index, record)| ClusterNode {
            parent: NO_PARENT,
            children: [0, 0],
            child_count: 0,
            leaf: index as u32,
            center: record.center,
            radius: record.radius.max(0.0),
            error: 0.0,
        })
        .collect();
    let mut level = order;
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut index = 0;
        while index + 1 < level.len() {
            let parent = push_parent(&mut nodes, level[index], level[index + 1]);
            next.push(parent);
            index += 2;
        }
        if index < level.len() {
            next.push(level[index]);
        }
        level = next;
    }
    ClusterHierarchy { nodes, roots: level, leaf_count }
}

pub fn select_cluster_cut_for_pose(
    hierarchy: &ClusterHierarchy,
    instance: &crate::scene::RenderInstance,
    camera: &crate::space::ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    viewport_height: f32,
    threshold_px: f32,
) -> Result<ClusterCut, crate::space::SpaceError> {
    let transforms = crate::scene::instance_gpu_transforms(instance, camera, vertical_fov_radians, near_m, aspect)?;
    let eye_from_local = transforms.view.mul(transforms.model);
    let depth_x = eye_from_local.cols[0][2];
    let depth_y = eye_from_local.cols[1][2];
    let depth_z = eye_from_local.cols[2][2];
    let depth_w = eye_from_local.cols[3][2];
    let tan_y = (vertical_fov_radians as f32 * 0.5).tan().max(0.0001);
    Ok(select_cluster_cut(
        hierarchy,
        move |center| -(depth_x * center[0] + depth_y * center[1] + depth_z * center[2] + depth_w),
        tan_y,
        viewport_height,
        threshold_px,
    ))
}

/// One camera's cluster cut. The hierarchy and the mesh are not modified.
///
/// A parent is selected only when every leaf it stands for is visible to this
/// view. A hidden or off-screen subtree is not selected, even when another
/// part of the same mesh is. `0` is outside the frustum, `3` is occluded.
/// Any other flag stays drawable. False-visible is allowed. A drawable leaf
/// is not dropped.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewVisibility {
    pub selected: Vec<u32>,
    pub coarseness: Vec<u32>,
    pub covered_leaves: u32,
    pub candidate_nodes: u32,
    pub frustum_rejected: u32,
    pub occlusion_rejected: u32,
    pub visible_nodes: u32,
    pub leaf_clusters: u32,
    pub parent_clusters: u32,
    pub submitted_triangles: u32,
    pub selection_us: u32,
}

pub fn cut_visible_hierarchy_for_pose(
    hierarchy: &ClusterHierarchy,
    instance: &crate::scene::RenderInstance,
    camera: &crate::space::ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
    viewport_height: f32,
    threshold_px: f32,
    leaf_flags: &[u32],
    leaf_triangles: &[u32],
) -> Result<ViewVisibility, crate::space::SpaceError> {
    let transforms = crate::scene::instance_gpu_transforms(instance, camera, vertical_fov_radians, near_m, aspect)?;
    let eye_from_local = transforms.view.mul(transforms.model);
    let depth_x = eye_from_local.cols[0][2];
    let depth_y = eye_from_local.cols[1][2];
    let depth_z = eye_from_local.cols[2][2];
    let depth_w = eye_from_local.cols[3][2];
    let tan_y = (vertical_fov_radians as f32 * 0.5).tan().max(0.0001);
    Ok(cut_visible_hierarchy(
        hierarchy,
        leaf_flags,
        leaf_triangles,
        move |center| -(depth_x * center[0] + depth_y * center[1] + depth_z * center[2] + depth_w),
        tan_y,
        viewport_height,
        threshold_px,
    ))
}

pub fn cut_visible_hierarchy(
    hierarchy: &ClusterHierarchy,
    leaf_flags: &[u32],
    leaf_triangles: &[u32],
    depth_of: impl Fn([f32; 3]) -> f32,
    tan_y: f32,
    viewport_height: f32,
    threshold_px: f32,
) -> ViewVisibility {
    let started = std::time::Instant::now();
    let mut selected = Vec::new();
    let mut coarseness = vec![0u32; hierarchy.leaf_count as usize];
    let mut covered_leaves = 0u32;
    let mut candidate_nodes = 0u32;
    let mut frustum_rejected = 0u32;
    let mut occlusion_rejected = 0u32;
    let mut submitted_triangles = 0u32;
    let mut leaf_clusters = 0u32;
    let mut parent_clusters = 0u32;
    let tan_y = tan_y.max(0.0001);
    let height = viewport_height.max(1.0);
    let sums = summarize_flags(hierarchy, leaf_flags);
    for &root in &hierarchy.roots {
        walk_visible(
            hierarchy,
            root,
            &sums,
            leaf_triangles,
            &depth_of,
            tan_y,
            height,
            threshold_px,
            &mut selected,
            &mut coarseness,
            &mut covered_leaves,
            &mut candidate_nodes,
            &mut frustum_rejected,
            &mut occlusion_rejected,
            &mut submitted_triangles,
            &mut leaf_clusters,
            &mut parent_clusters,
        );
    }
    ViewVisibility {
        visible_nodes: selected.len() as u32,
        selected,
        coarseness,
        covered_leaves,
        candidate_nodes,
        frustum_rejected,
        occlusion_rejected,
        leaf_clusters,
        parent_clusters,
        submitted_triangles,
        selection_us: u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX),
    }
}

#[derive(Clone, Copy)]
struct FlagSum {
    drawn: u32,
    frustum: u32,
    occluded: u32,
    leaves: u32,
}

fn summarize_flags(hierarchy: &ClusterHierarchy, leaf_flags: &[u32]) -> Vec<FlagSum> {
    let mut sums = vec![FlagSum { drawn: 0, frustum: 0, occluded: 0, leaves: 0 }; hierarchy.nodes.len()];
    for (index, node) in hierarchy.nodes.iter().enumerate() {
        if node.child_count == 0 {
            let flag = leaf_flags.get(node.leaf as usize).copied().unwrap_or(1);
            if flag == 0 {
                sums[index].frustum = 1;
            } else if flag == 3 {
                sums[index].occluded = 1;
            } else {
                sums[index].drawn = 1;
            }
            sums[index].leaves = 1;
        } else {
            for child in 0..node.child_count {
                let child_sum = sums.get(node.children[child as usize] as usize).copied().unwrap_or(FlagSum { drawn: 0, frustum: 0, occluded: 0, leaves: 0 });
                sums[index].drawn = sums[index].drawn.saturating_add(child_sum.drawn);
                sums[index].frustum = sums[index].frustum.saturating_add(child_sum.frustum);
                sums[index].occluded = sums[index].occluded.saturating_add(child_sum.occluded);
                sums[index].leaves = sums[index].leaves.saturating_add(child_sum.leaves);
            }
        }
    }
    sums
}

fn walk_visible(
    hierarchy: &ClusterHierarchy,
    node: u32,
    sums: &[FlagSum],
    leaf_triangles: &[u32],
    depth_of: &impl Fn([f32; 3]) -> f32,
    tan_y: f32,
    height: f32,
    threshold_px: f32,
    selected: &mut Vec<u32>,
    coarseness: &mut [u32],
    covered_leaves: &mut u32,
    candidate_nodes: &mut u32,
    frustum_rejected: &mut u32,
    occlusion_rejected: &mut u32,
    submitted_triangles: &mut u32,
    leaf_clusters: &mut u32,
    parent_clusters: &mut u32,
) {
    let Some(record) = hierarchy.nodes.get(node as usize) else { return };
    let Some(sum) = sums.get(node as usize).copied() else { return };
    *candidate_nodes = candidate_nodes.saturating_add(1);
    if sum.leaves == 0 {
        return;
    }
    if sum.drawn == 0 {
        if sum.occluded > 0 {
            *occlusion_rejected = occlusion_rejected.saturating_add(1);
        } else {
            *frustum_rejected = frustum_rejected.saturating_add(1);
        }
        return;
    }
    let depth = depth_of(record.center);
    let error = record.error.max(0.02);
    let pixels = if depth <= record.radius + 0.05 { f32::MAX } else { error * (height * 0.5) / (depth * tan_y) };
    let whole = sum.drawn == sum.leaves;
    if record.child_count == 0 || (whole && pixels <= threshold_px) {
        selected.push(node);
        if record.child_count == 0 {
            *leaf_clusters = leaf_clusters.saturating_add(1);
            *covered_leaves = covered_leaves.saturating_add(1);
            *submitted_triangles = submitted_triangles.saturating_add(leaf_triangles.get(record.leaf as usize).copied().unwrap_or(0));
        } else {
            *parent_clusters = parent_clusters.saturating_add(1);
            add_leaf_triangles(hierarchy, node, leaf_triangles, submitted_triangles);
            mark_coarseness(hierarchy, node, coarseness, covered_leaves);
        }
        return;
    }
    for child in 0..record.child_count {
        walk_visible(
            hierarchy,
            record.children[child as usize],
            sums,
            leaf_triangles,
            depth_of,
            tan_y,
            height,
            threshold_px,
            selected,
            coarseness,
            covered_leaves,
            candidate_nodes,
            frustum_rejected,
            occlusion_rejected,
            submitted_triangles,
            leaf_clusters,
            parent_clusters,
        );
    }
}

fn add_leaf_triangles(hierarchy: &ClusterHierarchy, node: u32, leaf_triangles: &[u32], submitted: &mut u32) {
    visit_leaves(hierarchy, node, &mut |leaf| {
        *submitted = submitted.saturating_add(leaf_triangles.get(leaf as usize).copied().unwrap_or(0));
    });
}

pub fn select_cluster_cut(hierarchy: &ClusterHierarchy, depth_of: impl Fn([f32; 3]) -> f32, tan_y: f32, viewport_height: f32, threshold_px: f32) -> ClusterCut {
    let mut selected = Vec::new();
    let mut coarseness = vec![0u32; hierarchy.leaf_count as usize];
    let mut covered_leaves = 0u32;
    let tan_y = tan_y.max(0.0001);
    let height = viewport_height.max(1.0);
    for &root in &hierarchy.roots {
        select_node(hierarchy, root, &depth_of, tan_y, height, threshold_px, &mut selected, &mut coarseness, &mut covered_leaves);
    }
    ClusterCut { selected, coarseness, covered_leaves }
}

/// Leaves represented by one selected node. A parent yields every descendant.
pub fn leaves_under(hierarchy: &ClusterHierarchy, node: u32) -> Vec<u32> {
    let mut leaves = Vec::new();
    visit_leaves(hierarchy, node, &mut |leaf| leaves.push(leaf));
    leaves.sort_unstable();
    leaves
}

/// Visit descendant leaves without allocating or sorting.
pub fn visit_leaves(hierarchy: &ClusterHierarchy, node: u32, visit: &mut impl FnMut(u32)) {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        let Some(record) = hierarchy.nodes.get(current as usize) else { continue };
        if record.child_count == 0 {
            if record.leaf != NO_LEAF {
                visit(record.leaf);
            }
        } else {
            for child in 0..record.child_count {
                stack.push(record.children[child as usize]);
            }
        }
    }
}

fn select_node(
    hierarchy: &ClusterHierarchy,
    node: u32,
    depth_of: &impl Fn([f32; 3]) -> f32,
    tan_y: f32,
    height: f32,
    threshold_px: f32,
    selected: &mut Vec<u32>,
    coarseness: &mut [u32],
    covered_leaves: &mut u32,
) {
    let Some(record) = hierarchy.nodes.get(node as usize) else { return };
    let depth = depth_of(record.center);
    // A parent smaller than 2 cm of error is still the leaves for any view that can see the shop.
    let error = record.error.max(0.02);
    let pixels = if depth <= record.radius + 0.05 { f32::MAX } else { error * (height * 0.5) / (depth * tan_y) };
    if record.child_count == 0 || pixels <= threshold_px {
        selected.push(node);
        if record.child_count == 0 {
            *covered_leaves = covered_leaves.saturating_add(1);
        } else {
            mark_coarseness(hierarchy, node, coarseness, covered_leaves);
        }
        return;
    }
    for child in 0..record.child_count {
        select_node(hierarchy, record.children[child as usize], depth_of, tan_y, height, threshold_px, selected, coarseness, covered_leaves);
    }
}

fn mark_coarseness(hierarchy: &ClusterHierarchy, node: u32, coarseness: &mut [u32], covered_leaves: &mut u32) {
    let mut stack = Vec::new();
    if let Some(record) = hierarchy.nodes.get(node as usize) {
        for child in 0..record.child_count {
            stack.push((record.children[child as usize], 1u32));
        }
    }
    while let Some((current, steps)) = stack.pop() {
        let Some(record) = hierarchy.nodes.get(current as usize) else { continue };
        if record.child_count == 0 {
            if let Some(slot) = coarseness.get_mut(record.leaf as usize) {
                *slot = steps;
            }
            *covered_leaves = covered_leaves.saturating_add(1);
        } else {
            let next = steps.saturating_add(1);
            for child in 0..record.child_count {
                stack.push((record.children[child as usize], next));
            }
        }
    }
}

fn push_parent(nodes: &mut Vec<ClusterNode>, left: u32, right: u32) -> u32 {
    let a = nodes[left as usize].clone();
    let b = nodes[right as usize].clone();
    let (center, radius) = enclose(a.center, a.radius, b.center, b.radius);
    let separation = distance(a.center, b.center);
    let error = a.error.max(b.error) + separation.max((a.radius + b.radius) * 0.05);
    let parent = nodes.len() as u32;
    nodes.push(ClusterNode {
        parent: NO_PARENT,
        children: [left, right],
        child_count: 2,
        leaf: NO_LEAF,
        center,
        radius,
        error,
    });
    nodes[left as usize].parent = parent;
    nodes[right as usize].parent = parent;
    parent
}

fn enclose(a: [f32; 3], a_radius: f32, b: [f32; 3], b_radius: f32) -> ([f32; 3], f32) {
    let delta = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let dist = distance(a, b);
    if dist + b_radius <= a_radius {
        return (a, a_radius);
    }
    if dist + a_radius <= b_radius {
        return (b, b_radius);
    }
    if dist <= 1.0e-8 {
        return (a, a_radius.max(b_radius));
    }
    let radius = (dist + a_radius + b_radius) * 0.5;
    let scale = (radius - a_radius) / dist;
    ([a[0] + delta[0] * scale, a[1] + delta[1] * scale, a[2] + delta[2] * scale], radius)
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let delta = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
}

fn morton(center: [f32; 3], min: [f32; 3], extent: [f32; 3]) -> u64 {
    let quantize = |value: f32, lower: f32, span: f32| -> u32 {
        if span <= 1.0e-8 {
            0
        } else {
            (((value - lower) / span).clamp(0.0, 0.999) * 1024.0) as u32
        }
    };
    let x = expand(quantize(center[0], min[0], extent[0]));
    let y = expand(quantize(center[1], min[1], extent[1]));
    let z = expand(quantize(center[2], min[2], extent[2]));
    x | (y << 1) | (z << 2)
}

fn expand(value: u32) -> u64 {
    let mut value = value as u64 & 0x3ff;
    value = (value | (value << 16)) & 0x030000ff;
    value = (value | (value << 8)) & 0x0300f00f;
    value = (value | (value << 4)) & 0x030c30c3;
    value = (value | (value << 2)) & 0x09249249;
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(center: [f32; 3], radius: f32) -> GpuMeshletRecord {
        GpuMeshletRecord {
            center,
            radius,
            bounds_min: center,
            bounds_max: center,
            cone_axis: [0.0, 1.0, 0.0],
            cone_cutoff: 1.0,
            triangles: 4,
            vertices: 4,
            submesh: 0,
        }
    }

    #[test]
    fn a_close_camera_selects_leaves_and_a_far_camera_selects_the_parent() {
        let records = [record([0.0, 0.0, 0.0], 0.1), record([1.0, 0.0, 0.0], 0.1)];
        let hierarchy = build_cluster_hierarchy(&records);
        assert_eq!(hierarchy.leaf_count, 2);
        assert_eq!(hierarchy.roots.len(), 1);
        let parent = hierarchy.roots[0];
        assert!(hierarchy.nodes[parent as usize].error > hierarchy.nodes[0].error);
        assert!(hierarchy.nodes[parent as usize].error >= hierarchy.nodes[1].error);
        let close = select_cluster_cut(&hierarchy, |_| 0.2, 0.577, 1080.0, 1.0);
        let mut close_leaves = Vec::new();
        for node in &close.selected {
            close_leaves.extend(leaves_under(&hierarchy, *node));
        }
        close_leaves.sort_unstable();
        assert_eq!(close_leaves, vec![0, 1]);
        assert!(close.selected.iter().all(|node| hierarchy.nodes[*node as usize].child_count == 0));
        let far = select_cluster_cut(&hierarchy, |_| 4000.0, 0.577, 1080.0, 1.0);
        assert_eq!(far.selected, vec![parent]);
        assert_eq!(far.coarseness, vec![1, 1]);
        assert_eq!(far.covered_leaves, 2);
        assert_eq!(close.covered_leaves, 2);
        assert_eq!(leaves_under(&hierarchy, parent), vec![0, 1]);
        let again = build_cluster_hierarchy(&records);
        assert_eq!(again.nodes.len(), hierarchy.nodes.len());
        assert_eq!(again.nodes[parent as usize].children, hierarchy.nodes[parent as usize].children);
    }

    #[test]
    fn a_hidden_branch_is_omitted_while_its_sibling_on_the_same_mesh_stays() {
        let records = [record([0.0, 0.0, 0.0], 0.1), record([1.0, 0.0, 0.0], 0.1)];
        let hierarchy = build_cluster_hierarchy(&records);
        let before = hierarchy.clone();
        let parent = hierarchy.roots[0];
        let hidden = cut_visible_hierarchy(&hierarchy, &[1, 3], &[40, 10], |_| 4000.0, 0.577, 1080.0, 1.0);
        assert!(hidden.selected.iter().all(|node| *node != parent), "a parent that also covers a hidden leaf is not the cut");
        let mut shown = Vec::new();
        for node in &hidden.selected {
            shown.extend(leaves_under(&hierarchy, *node));
        }
        shown.sort_unstable();
        assert_eq!(shown, vec![0]);
        assert_eq!(hidden.submitted_triangles, 40);
        assert!(hidden.occlusion_rejected >= 1);
        assert_eq!(hidden.parent_clusters, 0);
        let other_view = cut_visible_hierarchy(&hierarchy, &[1, 1], &[40, 10], |_| 4000.0, 0.577, 1080.0, 1.0);
        assert_eq!(other_view.selected, vec![parent]);
        assert_eq!(other_view.submitted_triangles, 50);
        assert_eq!(other_view.parent_clusters, 1);
        let offscreen = cut_visible_hierarchy(&hierarchy, &[0, 0], &[40, 10], |_| 4000.0, 0.577, 1080.0, 1.0);
        assert!(offscreen.selected.is_empty());
        assert_eq!(offscreen.submitted_triangles, 0);
        assert!(offscreen.frustum_rejected >= 1);
        assert_eq!(hierarchy, before);
    }
}
