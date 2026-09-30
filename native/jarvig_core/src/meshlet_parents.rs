//! Simplified triangles for an accepted cluster hierarchy.
//!
//! The meshlet partition is not rebuilt. A selected parent draws this mesh
//! instead of its descendant leaves. Boundary vertices stay where the leaves
//! put them, so a neighbor still on its leaves meets this mesh on the same edge.
//! The recorded error is the farthest a vertex moved during that simplification.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::gpu_scene::GpuMeshletRecord;
use crate::mesh::Mesh;
use crate::meshlet::{Meshlet, MeshletSet};
use crate::meshlet_hierarchy::{build_cluster_hierarchy, leaves_under, ClusterCut, ClusterHierarchy};

const STRIDE: usize = 60;
const NO_ORIGIN: u32 = u32::MAX;
const INELIGIBLE_ERROR: f32 = 1.0e8;
/// Smallest parent. A pair of meshlets stays here. Larger coverage is allowed more triangles.
const PARENT_TRIANGLE_FLOOR: usize = 32;
const PARENT_TRIANGLE_CEILING: usize = 2048;
/// Bump when the simplifier or the cache bytes change.
pub const PARENT_BUILDER_VERSION: u32 = 4;
const PARENT_MAGIC: &[u8; 8] = b"JARVPAR1";

#[derive(Clone, Debug, PartialEq)]
pub struct ParentRange {
    pub first_index: u32,
    pub index_count: u32,
    pub triangles: u32,
    pub level: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParentLevelStats {
    pub level: u32,
    pub nodes: u32,
    pub parent_triangles: u32,
    pub child_triangles: u32,
    pub bounds_covered: u32,
    pub empty_parents: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParentGeometry {
    pub hierarchy: ClusterHierarchy,
    pub vertices: Vec<u8>,
    /// Warm debug color, one entry per vertex in `vertices`.
    pub debug_colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Parallel to `hierarchy.nodes`. Leaves and ineligible parents are empty.
    pub ranges: Vec<ParentRange>,
    pub leaf_counts: Vec<u32>,
    pub leaf_triangles: u32,
    pub parent_triangles: u32,
    pub root_triangles: u32,
    pub empty_parents: u32,
    pub levels: Vec<ParentLevelStats>,
    pub build_ms: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HierarchySubmission {
    pub leaf_meshlets: Vec<u32>,
    pub parent_nodes: Vec<u32>,
    pub triangles: u32,
    pub leaf_reference: u32,
}

#[derive(Clone)]
struct Working {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    indices: Vec<u32>,
    origin: Vec<u32>,
    locked: Vec<bool>,
    error: f32,
    level: u32,
    submesh: u32,
    slot: u32,
    leaf_triangles: u32,
}

pub fn build_parent_geometry(mesh: &Mesh, meshlets: &MeshletSet) -> ParentGeometry {
    build_parent_geometry_with(mesh, meshlets, &|| false, &|_, _| {}).expect("parent build")
}

/// `cancelled` is polled between parents. `report` receives a fraction and a stage line.
pub fn build_parent_geometry_with(
    mesh: &Mesh,
    meshlets: &MeshletSet,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(f32, &str),
) -> Result<ParentGeometry, String> {
    if cancelled() {
        return Err("cancelled".into());
    }
    let started = Instant::now();
    let records: Vec<GpuMeshletRecord> = meshlets.meshlets.iter().map(GpuMeshletRecord::from_meshlet).collect();
    let mut hierarchy = build_cluster_hierarchy(&records);
    let leaf_counts: Vec<u32> = meshlets.meshlets.iter().map(|meshlet| meshlet.index_count / 3).collect();
    let leaf_triangles = leaf_counts.iter().copied().fold(0u32, |sum, count| sum.saturating_add(count));
    let mut ranges = vec![empty_range(); hierarchy.nodes.len()];
    let mut vertices = Vec::new();
    let mut debug_colors = Vec::new();
    let mut indices = Vec::new();
    let mut alive: Vec<Option<Working>> = (0..hierarchy.nodes.len()).map(|_| None).collect();
    let node_count = hierarchy.nodes.len();
    let parents_total = node_count.saturating_sub(hierarchy.leaf_count as usize).max(1);
    let mut parents_done = 0u32;
    for node in 0..node_count as u32 {
        let child_count = hierarchy.nodes.get(node as usize).map(|record| record.child_count).unwrap_or(0);
        if child_count == 0 {
            continue;
        }
        if cancelled() {
            return Err("cancelled".into());
        }
        parents_done = parents_done.saturating_add(1);
        if parents_done == 1 || parents_done % 32 == 0 {
            let fraction = parents_done as f32 / parents_total as f32;
            report(
                fraction,
                &format!("Simplifying parents {parents_done}/{parents_total} · leaf triangles {leaf_triangles}"),
            );
        }
        let children = hierarchy.nodes[node as usize].children;
        let left = surface_of(mesh, meshlets, &alive, children[0]);
        let right = if child_count > 1 { surface_of(mesh, meshlets, &alive, children[1]) } else { None };
        let merged = match (left, right) {
            (Some(left), Some(right)) => merge_surfaces(left, right),
            (Some(only), None) | (None, Some(only)) => Some(only),
            (None, None) => None,
        };
        if child_count > 0 {
            alive[children[0] as usize] = None;
        }
        if child_count > 1 {
            alive[children[1] as usize] = None;
        }
        let Some(mut working) = merged else {
            hierarchy.nodes[node as usize].error = INELIGIBLE_ERROR;
            continue;
        };
        let child_bounds = mesh_bounds(&working);
        let fallback = if working.indices.len() / 3 <= 8192 { Some(working.clone()) } else { None };
        simplify(&mut working);
        let finite = working.positions.iter().all(|position| position.iter().all(|lane| lane.is_finite()));
        if !finite || working.indices.len() < 3 {
            if let Some(saved) = fallback {
                working = saved;
            }
        }
        if working.indices.len() / 3 > PARENT_TRIANGLE_CEILING {
            cluster_unlocked(&mut working, PARENT_TRIANGLE_CEILING);
            compact(&mut working);
            if working.indices.len() < 3 {
                keep_first_triangles(&mut working, PARENT_TRIANGLE_CEILING);
            }
        }
        if working.indices.len() < 3 {
            if let Some(bounds) = child_bounds {
                working = box_cover(&working, bounds);
            }
        }
        let covers = match child_bounds {
            Some(child) => mesh_bounds(&working).is_some_and(|parent| bounds_cover(parent, child)),
            None => working.indices.len() >= 3,
        };
        if !covers {
            working.error = working.error.max(0.02);
        }
        let geometric = working.error.max(0.003 * working.level.max(1) as f32);
        if working.indices.len() >= 3 && geometric.is_finite() {
            ranges[node as usize] = pack(&working, &mut vertices, &mut debug_colors, &mut indices);
            hierarchy.nodes[node as usize].error = geometric;
        } else {
            hierarchy.nodes[node as usize].error = INELIGIBLE_ERROR;
        }
        alive[node as usize] = Some(working);
    }
    for record in hierarchy.nodes.iter_mut().take(hierarchy.leaf_count as usize) {
        record.error = 0.0;
    }
    report(1.0, &format!("Parent meshes ready · leaf triangles {leaf_triangles}"));
    let parent_triangles = ranges.iter().fold(0u32, |sum, range| sum.saturating_add(range.triangles));
    let root_triangles = hierarchy.roots.iter().map(|node| ranges.get(*node as usize).map(|range| range.triangles).unwrap_or(0)).fold(0u32, |sum, count| sum.saturating_add(count));
    let levels = level_stats(&hierarchy, &vertices, &indices, &ranges, &leaf_counts);
    let empty_parents = levels.iter().fold(0u32, |sum, level| sum.saturating_add(level.empty_parents));
    Ok(ParentGeometry {
        hierarchy,
        vertices,
        debug_colors,
        indices,
        ranges,
        leaf_counts,
        leaf_triangles,
        parent_triangles,
        root_triangles,
        empty_parents,
        levels,
        build_ms: started.elapsed().as_secs_f32() * 1000.0,
    })
}

pub fn submission_for_cut(
    hierarchy: &ClusterHierarchy,
    ranges: &[ParentRange],
    leaf_counts: &[u32],
    leaf_triangles: u32,
    cut: &ClusterCut,
    visible: Option<&[bool]>,
) -> HierarchySubmission {
    let mut leaf_meshlets = Vec::new();
    let mut parent_nodes = Vec::new();
    let mut triangles = 0u32;
    for &node in &cut.selected {
        let Some(record) = hierarchy.nodes.get(node as usize) else { continue };
        if record.child_count == 0 {
            push_leaf(leaf_counts, record.leaf, visible, &mut leaf_meshlets, &mut triangles);
            continue;
        }
        let range = ranges.get(node as usize).cloned().unwrap_or_else(empty_range);
        let shown = visible.map(|flags| subtree_visible(hierarchy, node, flags)).unwrap_or(true);
        if range.index_count > 0 && range.triangles > 0 && shown {
            parent_nodes.push(node);
            triangles = triangles.saturating_add(range.triangles);
            continue;
        }
        for leaf in leaves_under(hierarchy, node) {
            push_leaf(leaf_counts, leaf, visible, &mut leaf_meshlets, &mut triangles);
        }
    }
    leaf_meshlets.sort_unstable();
    leaf_meshlets.dedup();
    HierarchySubmission { leaf_meshlets, parent_nodes, triangles, leaf_reference: leaf_triangles }
}

pub fn parent_draw_indices(indices: &[u32], ranges: &[ParentRange], nodes: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    for &node in nodes {
        let Some(range) = ranges.get(node as usize) else { continue };
        let start = range.first_index as usize;
        let end = start.saturating_add(range.index_count as usize).min(indices.len());
        if start < end {
            out.extend_from_slice(&indices[start..end]);
        }
    }
    out
}

fn push_leaf(leaf_counts: &[u32], leaf: u32, visible: Option<&[bool]>, leaves: &mut Vec<u32>, triangles: &mut u32) {
    let shown = visible.map(|flags| flags.get(leaf as usize).copied().unwrap_or(false)).unwrap_or(true);
    if !shown {
        return;
    }
    let count = leaf_counts.get(leaf as usize).copied().unwrap_or(0);
    leaves.push(leaf);
    *triangles = triangles.saturating_add(count);
}

fn subtree_visible(hierarchy: &ClusterHierarchy, node: u32, visible: &[bool]) -> bool {
    let Some(record) = hierarchy.nodes.get(node as usize) else { return false };
    if record.child_count == 0 {
        return visible.get(record.leaf as usize).copied().unwrap_or(false);
    }
    (0..record.child_count).any(|child| subtree_visible(hierarchy, record.children[child as usize], visible))
}

fn surface_of(mesh: &Mesh, meshlets: &MeshletSet, alive: &[Option<Working>], node: u32) -> Option<Working> {
    if let Some(Some(working)) = alive.get(node as usize) {
        return Some(working.clone());
    }
    let record = meshlets.meshlets.get(node as usize)?;
    expand_leaf(mesh, meshlets, record)
}

fn expand_leaf(mesh: &Mesh, meshlets: &MeshletSet, meshlet: &Meshlet) -> Option<Working> {
    let verts = meshlets.vertex_indices.get(meshlet.vertex_offset as usize..meshlet.vertex_offset as usize + meshlet.vertex_count as usize)?;
    let locals = meshlets.local_indices.get(meshlet.index_offset as usize..meshlet.index_offset as usize + meshlet.index_count as usize)?;
    if verts.is_empty() || locals.len() < 3 {
        return None;
    }
    let mut working = Working {
        positions: Vec::with_capacity(verts.len()),
        normals: Vec::with_capacity(verts.len()),
        uvs: Vec::with_capacity(verts.len()),
        tangents: Vec::with_capacity(verts.len()),
        indices: Vec::with_capacity(locals.len()),
        origin: Vec::with_capacity(verts.len()),
        locked: Vec::new(),
        error: 0.0,
        level: 0,
        submesh: meshlet.submesh,
        slot: meshlet.material_slot,
        leaf_triangles: meshlet.index_count / 3,
    };
    for vertex in verts {
        working.positions.push(read3(mesh, *vertex, 0).unwrap_or([0.0, 0.0, 0.0]));
        working.normals.push(read3(mesh, *vertex, 3).unwrap_or([0.0, 1.0, 0.0]));
        working.uvs.push(read2(mesh, *vertex, 2).unwrap_or([0.0, 0.0]));
        working.tangents.push(read4(mesh, *vertex, 4).unwrap_or([1.0, 0.0, 0.0, 1.0]));
        working.origin.push(*vertex);
    }
    for local in locals {
        working.indices.push(*local as u32);
    }
    working.locked = lock_boundary(&working.indices, working.positions.len());
    Some(working)
}

fn merge_surfaces(mut left: Working, right: Working) -> Option<Working> {
    if left.submesh != right.submesh || left.slot != right.slot {
        return None;
    }
    let mut weld: BTreeMap<u32, u32> = BTreeMap::new();
    for (index, origin) in left.origin.iter().enumerate() {
        if *origin != NO_ORIGIN {
            weld.entry(*origin).or_insert(index as u32);
        }
    }
    let mut remap = vec![0u32; right.positions.len()];
    for (index, origin) in right.origin.iter().enumerate() {
        if *origin != NO_ORIGIN {
            if let Some(existing) = weld.get(origin) {
                remap[index] = *existing;
                continue;
            }
        }
        remap[index] = left.positions.len() as u32;
        left.positions.push(right.positions[index]);
        left.normals.push(right.normals[index]);
        left.uvs.push(right.uvs[index]);
        left.tangents.push(right.tangents[index]);
        left.origin.push(*origin);
        if *origin != NO_ORIGIN {
            weld.insert(*origin, remap[index]);
        }
    }
    for index in right.indices {
        left.indices.push(remap[index as usize]);
    }
    left.error = left.error.max(right.error);
    left.level = left.level.max(right.level).saturating_add(1);
    left.leaf_triangles = left.leaf_triangles.saturating_add(right.leaf_triangles);
    left.locked = lock_boundary(&left.indices, left.positions.len());
    Some(left)
}

fn mesh_bounds(work: &Working) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for index in &work.indices {
        let Some(position) = work.positions.get(*index as usize) else { continue };
        if position.iter().any(|lane| !lane.is_finite()) {
            continue;
        }
        any = true;
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    if any { Some((min, max)) } else { None }
}

fn bounds_cover(parent: ([f32; 3], [f32; 3]), child: ([f32; 3], [f32; 3])) -> bool {
    let mut diagonal_sq = 0.0f32;
    for axis in 0..3 {
        let edge = child.1[axis] - child.0[axis];
        diagonal_sq += edge * edge;
    }
    let tolerance = diagonal_sq.sqrt() * 0.02;
    let tolerance = tolerance.max(0.01);
    (0..3).all(|axis| parent.0[axis] <= child.0[axis] + tolerance && parent.1[axis] >= child.1[axis] - tolerance)
}

fn keep_first_triangles(work: &mut Working, cap: usize) {
    let max_indices = cap.saturating_mul(3);
    if work.indices.len() > max_indices {
        work.indices.truncate(max_indices);
    }
    compact(work);
}

fn box_cover(template: &Working, bounds: ([f32; 3], [f32; 3])) -> Working {
    let (min, max) = bounds;
    let corners = [
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], max[1], min[2]],
        [min[0], max[1], min[2]],
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ];
    let mut diagonal_sq = 0.0f32;
    for axis in 0..3 {
        let edge = max[axis] - min[axis];
        diagonal_sq += edge * edge;
    }
    Working {
        positions: corners.to_vec(),
        normals: vec![[0.0, 1.0, 0.0]; 8],
        uvs: vec![[0.0, 0.0]; 8],
        tangents: vec![[1.0, 0.0, 0.0, 1.0]; 8],
        indices: vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 3, 2, 6, 3, 6, 7, 0, 3, 7, 0, 7, 4, 1, 5, 6, 1, 6, 2],
        origin: vec![NO_ORIGIN; 8],
        locked: vec![true; 8],
        error: diagonal_sq.sqrt() * 0.5,
        level: template.level,
        submesh: template.submesh,
        slot: template.slot,
        leaf_triangles: template.leaf_triangles,
    }
}

fn vertex_position(vertices: &[u8], index: u32) -> Option<[f32; 3]> {
    let start = index as usize * STRIDE;
    let bytes = vertices.get(start..start + 12)?;
    Some([
        f32::from_le_bytes(bytes[0..4].try_into().ok()?),
        f32::from_le_bytes(bytes[4..8].try_into().ok()?),
        f32::from_le_bytes(bytes[8..12].try_into().ok()?),
    ])
}

/// A parent covers its children when every child center sits inside the parent mesh bounds.
/// The pad is the stored geometric error, at least 2 cm, and never more than 25 cm.
fn parent_covers_child_centers(hierarchy: &ClusterHierarchy, vertices: &[u8], indices: &[u32], ranges: &[ParentRange], node: usize) -> bool {
    let Some(range) = ranges.get(node) else { return false };
    if range.index_count < 3 {
        return false;
    }
    let end = range.first_index as usize + range.index_count as usize;
    let Some(slice) = indices.get(range.first_index as usize..end) else { return false };
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for index in slice {
        let Some(position) = vertex_position(vertices, *index) else { continue };
        if position.iter().any(|lane| !lane.is_finite()) {
            continue;
        }
        any = true;
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    if !any {
        return false;
    }
    let Some(record) = hierarchy.nodes.get(node) else { return false };
    let pad = record.error.min(0.25).max(0.02);
    for child_index in 0..record.child_count {
        let Some(child) = hierarchy.nodes.get(record.children[child_index as usize] as usize) else { return false };
        for axis in 0..3 {
            if child.center[axis] < min[axis] - pad || child.center[axis] > max[axis] + pad {
                return false;
            }
        }
    }
    true
}

fn level_stats(hierarchy: &ClusterHierarchy, vertices: &[u8], indices: &[u32], ranges: &[ParentRange], leaf_counts: &[u32]) -> Vec<ParentLevelStats> {
    let mut stats = Vec::new();
    for (node, record) in hierarchy.nodes.iter().enumerate() {
        if record.child_count == 0 {
            continue;
        }
        let level = ranges.get(node).map(|range| range.level.max(1)).unwrap_or(1);
        if !stats.iter().any(|row: &ParentLevelStats| row.level == level) {
            stats.push(ParentLevelStats { level, nodes: 0, parent_triangles: 0, child_triangles: 0, bounds_covered: 0, empty_parents: 0 });
        }
        let row = stats.iter_mut().find(|row| row.level == level).expect("level row");
        row.nodes = row.nodes.saturating_add(1);
        let triangles = ranges.get(node).map(|range| range.triangles).unwrap_or(0);
        row.parent_triangles = row.parent_triangles.saturating_add(triangles);
        for child_index in 0..record.child_count {
            let child = record.children[child_index as usize] as usize;
            let child_triangles = if hierarchy.nodes.get(child).is_some_and(|node| node.child_count == 0) {
                hierarchy.nodes.get(child).and_then(|node| leaf_counts.get(node.leaf as usize).copied()).unwrap_or(0)
            } else {
                ranges.get(child).map(|range| range.triangles).unwrap_or(0)
            };
            row.child_triangles = row.child_triangles.saturating_add(child_triangles);
        }
        if triangles == 0 {
            row.empty_parents = row.empty_parents.saturating_add(1);
        } else if parent_covers_child_centers(hierarchy, vertices, indices, ranges, node) {
            row.bounds_covered = row.bounds_covered.saturating_add(1);
        }
    }
    stats.sort_by_key(|row| row.level);
    stats
}

fn triangle_budget(leaf_triangles: u32) -> usize {
    ((leaf_triangles as usize) / 16).clamp(PARENT_TRIANGLE_FLOOR, PARENT_TRIANGLE_CEILING)
}

fn simplify(work: &mut Working) {
    let cap = triangle_budget(work.leaf_triangles);
    let target = (work.indices.len() / 6).max(1).min(cap);
    if work.indices.len() / 3 > target {
        if work.indices.len() / 3 > 512 {
            cluster_unlocked(work, target);
        } else {
            collapse_once(work, target);
        }
        compact(work);
    }
    let mut spins = 0usize;
    while work.indices.len() / 3 > cap && spins < 4 {
        let before = work.indices.len();
        cluster_unlocked(work, cap.min(spins + 1).max(1));
        compact(work);
        spins += 1;
        if work.indices.len() >= before {
            break;
        }
    }
    recompute_shading(work);
}

fn collapse_once(work: &mut Working, target: usize) {
    let edges = sorted_edges(work);
    let mut alive = vec![true; work.positions.len()];
    for (a, b) in edges {
        if work.indices.len() / 3 <= target {
            break;
        }
        let (a, b) = (a as usize, b as usize);
        if a >= alive.len() || b >= alive.len() || !alive[a] || !alive[b] || a == b {
            continue;
        }
        let Some((remove, keep, position)) = collapse_plan(work, a, b) else { continue };
        if flips(work, remove, keep, position) {
            continue;
        }
        let moved = distance(work.positions[remove], position).max(distance(work.positions[keep], position));
        work.positions[keep] = position;
        if !work.locked[keep] && !work.locked[remove] {
            work.uvs[keep] = average2(work.uvs[keep], work.uvs[remove]);
            work.normals[keep] = unit3(add3(work.normals[keep], work.normals[remove]), work.normals[keep]);
        }
        work.error = work.error.max(moved);
        for index in &mut work.indices {
            if *index as usize == remove {
                *index = keep as u32;
            }
        }
        drop_degenerates(work);
        alive[remove] = false;
    }
}

fn collapse_plan(work: &Working, a: usize, b: usize) -> Option<(usize, usize, [f32; 3])> {
    if nearly_same(work.positions[a], work.positions[b]) && distance2(work.uvs[a], work.uvs[b]) > 0.02 {
        return None;
    }
    let (remove, keep) = match (work.locked[a], work.locked[b]) {
        (true, true) => return None,
        (true, false) => (b, a),
        (false, true) => (a, b),
        (false, false) => {
            if a < b {
                (b, a)
            } else {
                (a, b)
            }
        }
    };
    let position = if work.locked[keep] { work.positions[keep] } else { average3(work.positions[a], work.positions[b]) };
    Some((remove, keep, position))
}

fn cluster_unlocked(work: &mut Working, target: usize) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut unlocked = Vec::new();
    for (index, locked) in work.locked.iter().enumerate() {
        if *locked {
            continue;
        }
        unlocked.push(index);
        for axis in 0..3 {
            min[axis] = min[axis].min(work.positions[index][axis]);
            max[axis] = max[axis].max(work.positions[index][axis]);
        }
    }
    if unlocked.len() < 2 {
        return;
    }
    let bins = (target as f32).cbrt().ceil().max(1.0) as i32;
    let cell = [
        ((max[0] - min[0]) / bins as f32).max(1.0e-6),
        ((max[1] - min[1]) / bins as f32).max(1.0e-6),
        ((max[2] - min[2]) / bins as f32).max(1.0e-6),
    ];
    let mut buckets: BTreeMap<(i32, i32, i32), Vec<usize>> = BTreeMap::new();
    for index in unlocked {
        let key = (
            ((work.positions[index][0] - min[0]) / cell[0]).floor() as i32,
            ((work.positions[index][1] - min[1]) / cell[1]).floor() as i32,
            ((work.positions[index][2] - min[2]) / cell[2]).floor() as i32,
        );
        buckets.entry(key).or_default().push(index);
    }
    let mut remap: Vec<u32> = (0..work.positions.len() as u32).collect();
    for group in buckets.values() {
        if group.len() < 2 {
            continue;
        }
        let keep = group[0];
        let mut average = [0.0f32; 3];
        for index in group {
            average = add3(average, work.positions[*index]);
        }
        let scale = 1.0 / group.len() as f32;
        average = [average[0] * scale, average[1] * scale, average[2] * scale];
        for index in group {
            work.error = work.error.max(distance(work.positions[*index], average));
            remap[*index] = keep as u32;
        }
        work.positions[keep] = average;
    }
    for index in &mut work.indices {
        *index = remap[*index as usize];
    }
    drop_degenerates(work);
}

fn compact(work: &mut Working) {
    let mut used = vec![false; work.positions.len()];
    for index in &work.indices {
        if let Some(slot) = used.get_mut(*index as usize) {
            *slot = true;
        }
    }
    let mut remap = vec![0u32; work.positions.len()];
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut tangents = Vec::new();
    let mut origin = Vec::new();
    let mut locked = Vec::new();
    for (index, keep) in used.into_iter().enumerate() {
        if !keep {
            continue;
        }
        remap[index] = positions.len() as u32;
        positions.push(work.positions[index]);
        normals.push(work.normals[index]);
        uvs.push(work.uvs[index]);
        tangents.push(work.tangents[index]);
        origin.push(work.origin[index]);
        locked.push(work.locked.get(index).copied().unwrap_or(false));
    }
    for index in &mut work.indices {
        *index = remap[*index as usize];
    }
    work.positions = positions;
    work.normals = normals;
    work.uvs = uvs;
    work.tangents = tangents;
    work.origin = origin;
    work.locked = locked;
}

fn recompute_shading(work: &mut Working) {
    let mut normals = vec![[0.0f32; 3]; work.positions.len()];
    for triangle in work.indices.chunks_exact(3) {
        let (a, b, c) = (triangle[0] as usize, triangle[1] as usize, triangle[2] as usize);
        let face = face_normal(work.positions[a], work.positions[b], work.positions[c]);
        for index in [a, b, c] {
            normals[index] = add3(normals[index], face);
        }
    }
    for (index, normal) in normals.iter_mut().enumerate() {
        *normal = unit3(*normal, work.normals.get(index).copied().unwrap_or([0.0, 1.0, 0.0]));
    }
    work.normals = normals;
    work.tangents = generated_tangents(&work.positions, &work.normals, &work.uvs, &work.indices);
}

fn pack(work: &Working, vertices: &mut Vec<u8>, debug_colors: &mut Vec<[f32; 3]>, indices: &mut Vec<u32>) -> ParentRange {
    let base = (vertices.len() / STRIDE) as u32;
    let color = level_color(work.level.max(1));
    for index in 0..work.positions.len() {
        push_vertex(vertices, work.positions[index], [1.0, 1.0, 1.0], work.uvs[index], work.normals[index], work.tangents[index]);
        debug_colors.push(color);
    }
    let first_index = indices.len() as u32;
    for index in &work.indices {
        indices.push(base.saturating_add(*index));
    }
    ParentRange {
        first_index,
        index_count: work.indices.len() as u32,
        triangles: work.indices.len() as u32 / 3,
        level: work.level.max(1),
    }
}

fn sorted_edges(work: &Working) -> Vec<(u32, u32)> {
    let mut edges = Vec::new();
    for triangle in work.indices.chunks_exact(3) {
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
            continue;
        }
        for pair in [(triangle[0], triangle[1]), (triangle[1], triangle[2]), (triangle[2], triangle[0])] {
            edges.push((pair.0.min(pair.1), pair.0.max(pair.1)));
        }
    }
    edges.sort_unstable();
    edges.dedup();
    edges.sort_by_key(|(a, b)| {
        let length = distance(work.positions[*a as usize], work.positions[*b as usize]);
        (((length * 1.0e6).max(0.0) as u64).min(u64::from(u32::MAX)) as u32, *a, *b)
    });
    edges
}

fn lock_boundary(indices: &[u32], vertices: usize) -> Vec<bool> {
    let mut edges = Vec::new();
    for triangle in indices.chunks_exact(3) {
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
            continue;
        }
        for pair in [(triangle[0], triangle[1]), (triangle[1], triangle[2]), (triangle[2], triangle[0])] {
            edges.push((pair.0.min(pair.1), pair.0.max(pair.1)));
        }
    }
    edges.sort_unstable();
    let mut locked = vec![false; vertices];
    let mut cursor = 0;
    while cursor < edges.len() {
        let mut end = cursor + 1;
        while end < edges.len() && edges[end] == edges[cursor] {
            end += 1;
        }
        if end - cursor == 1 {
            let (a, b) = edges[cursor];
            if let Some(slot) = locked.get_mut(a as usize) {
                *slot = true;
            }
            if let Some(slot) = locked.get_mut(b as usize) {
                *slot = true;
            }
        }
        cursor = end;
    }
    locked
}

fn drop_degenerates(work: &mut Working) {
    let mut kept = Vec::with_capacity(work.indices.len());
    for triangle in work.indices.chunks_exact(3) {
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
            continue;
        }
        let normal = face_normal(
            work.positions[triangle[0] as usize],
            work.positions[triangle[1] as usize],
            work.positions[triangle[2] as usize],
        );
        if length3(normal) < 1.0e-12 {
            continue;
        }
        kept.extend_from_slice(triangle);
    }
    work.indices = kept;
}

fn flips(work: &Working, remove: usize, keep: usize, position: [f32; 3]) -> bool {
    for triangle in work.indices.chunks_exact(3) {
        let ids = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
        if !ids.contains(&remove) {
            continue;
        }
        let old = [work.positions[ids[0]], work.positions[ids[1]], work.positions[ids[2]]];
        let mut next = old;
        for corner in 0..3 {
            if ids[corner] == remove || ids[corner] == keep {
                next[corner] = position;
            }
        }
        if next[0] == next[1] || next[1] == next[2] || next[2] == next[0] {
            continue;
        }
        let before = face_normal(old[0], old[1], old[2]);
        let after = face_normal(next[0], next[1], next[2]);
        if length3(before) < 1.0e-12 || length3(after) < 1.0e-12 {
            continue;
        }
        if dot(before, after) < -1.0e-5 {
            return true;
        }
    }
    false
}

fn empty_range() -> ParentRange {
    ParentRange { first_index: 0, index_count: 0, triangles: 0, level: 0 }
}

fn level_color(level: u32) -> [f32; 3] {
    let warm = (level.min(8) as f32) / 8.0;
    [warm, 0.85 - warm * 0.45, 1.0 - warm]
}

fn read3(mesh: &Mesh, vertex: u32, location: u32) -> Option<[f32; 3]> {
    let values = read_floats(mesh, vertex, location)?;
    if values.len() < 3 {
        return None;
    }
    Some([values[0], values[1], values[2]])
}

fn read2(mesh: &Mesh, vertex: u32, location: u32) -> Option<[f32; 2]> {
    let values = read_floats(mesh, vertex, location)?;
    if values.len() < 2 {
        return None;
    }
    Some([values[0], values[1]])
}

fn read4(mesh: &Mesh, vertex: u32, location: u32) -> Option<[f32; 4]> {
    let values = read_floats(mesh, vertex, location)?;
    if values.len() < 4 {
        return None;
    }
    Some([values[0], values[1], values[2], values[3]])
}

fn read_floats(mesh: &Mesh, vertex: u32, location: u32) -> Option<Vec<f32>> {
    let stream = mesh.streams().first()?;
    let attribute = stream.attributes.iter().find(|attribute| attribute.shader_location == location)?;
    let start = vertex as usize * stream.stride as usize + attribute.offset as usize;
    let width = attribute.format.byte_size() as usize;
    let slice = stream.bytes.get(start..start + width)?;
    let mut values = Vec::with_capacity(width / 4);
    for lane in slice.chunks_exact(4) {
        values.push(f32::from_le_bytes(lane.try_into().ok()?));
    }
    Some(values)
}

fn push_vertex(bytes: &mut Vec<u8>, position: [f32; 3], color: [f32; 3], uv: [f32; 2], normal: [f32; 3], tangent: [f32; 4]) {
    for value in position.iter().chain(color.iter()).chain(uv.iter()).chain(normal.iter()).chain(tangent.iter()) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn generated_tangents(positions: &[[f32; 3]], normals: &[[f32; 3]], uvs: &[[f32; 2]], indices: &[u32]) -> Vec<[f32; 4]> {
    let mut tangent = vec![[0.0f32; 3]; positions.len()];
    let mut bitangent = vec![[0.0f32; 3]; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let ids = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
        if ids.iter().any(|index| *index >= positions.len()) {
            continue;
        }
        let edge1 = sub3(positions[ids[1]], positions[ids[0]]);
        let edge2 = sub3(positions[ids[2]], positions[ids[0]]);
        let duv1 = [uvs[ids[1]][0] - uvs[ids[0]][0], uvs[ids[1]][1] - uvs[ids[0]][1]];
        let duv2 = [uvs[ids[2]][0] - uvs[ids[0]][0], uvs[ids[2]][1] - uvs[ids[0]][1]];
        let denom = duv1[0] * duv2[1] - duv1[1] * duv2[0];
        let (t, b) = if denom.abs() < 1.0e-12 {
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
        } else {
            let scale = 1.0 / denom;
            (
                [
                    (edge1[0] * duv2[1] - edge2[0] * duv1[1]) * scale,
                    (edge1[1] * duv2[1] - edge2[1] * duv1[1]) * scale,
                    (edge1[2] * duv2[1] - edge2[2] * duv1[1]) * scale,
                ],
                [
                    (edge2[0] * duv1[0] - edge1[0] * duv2[0]) * scale,
                    (edge2[1] * duv1[0] - edge1[1] * duv2[0]) * scale,
                    (edge2[2] * duv1[0] - edge1[2] * duv2[0]) * scale,
                ],
            )
        };
        for index in ids {
            tangent[index] = add3(tangent[index], t);
            bitangent[index] = add3(bitangent[index], b);
        }
    }
    normals
        .iter()
        .zip(tangent.iter())
        .zip(bitangent.iter())
        .map(|((normal, tangent), bitangent)| {
            let n = unit3(*normal, [0.0, 1.0, 0.0]);
            let projection = dot(*tangent, n);
            let mut t = sub3(*tangent, [n[0] * projection, n[1] * projection, n[2] * projection]);
            t = unit3(t, perpendicular(n));
            let cross = [n[1] * t[2] - n[2] * t[1], n[2] * t[0] - n[0] * t[2], n[0] * t[1] - n[1] * t[0]];
            let handed = dot(cross, *bitangent);
            [t[0], t[1], t[2], if handed < 0.0 { -1.0 } else { 1.0 }]
        })
        .collect()
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let edge1 = sub3(b, a);
    let edge2 = sub3(c, a);
    [edge1[1] * edge2[2] - edge1[2] * edge2[1], edge1[2] * edge2[0] - edge1[0] * edge2[2], edge1[0] * edge2[1] - edge1[1] * edge2[0]]
}

fn perpendicular(normal: [f32; 3]) -> [f32; 3] {
    let axis = if normal[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    unit3(
        [normal[1] * axis[2] - normal[2] * axis[1], normal[2] * axis[0] - normal[0] * axis[2], normal[0] * axis[1] - normal[1] * axis[0]],
        [1.0, 0.0, 0.0],
    )
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn average3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
}

fn average2(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length3(value: [f32; 3]) -> f32 {
    dot(value, value).sqrt()
}

fn unit3(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let length = length3(value);
    if length < 1.0e-8 { fallback } else { [value[0] / length, value[1] / length, value[2] / length] }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    length3(sub3(a, b))
}

fn distance2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1]];
    d[0] * d[0] + d[1] * d[1]
}

fn nearly_same(a: [f32; 3], b: [f32; 3]) -> bool {
    distance(a, b) < 1.0e-5
}

pub fn load_parent_cache(path: &std::path::Path, fingerprint: &str, meshlet_builder: u32) -> Option<ParentGeometry> {
    let bytes = std::fs::read(path).ok()?;
    decode_parent_cache(&bytes, fingerprint, meshlet_builder)
}

pub fn save_parent_cache(path: &std::path::Path, fingerprint: &str, meshlet_builder: u32, geometry: &ParentGeometry) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let bytes = encode_parent_cache(fingerprint, meshlet_builder, geometry);
    let temporary = path.with_extension("jarvigparents.partial");
    std::fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

fn encode_parent_cache(fingerprint: &str, meshlet_builder: u32, geometry: &ParentGeometry) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(PARENT_MAGIC);
    for value in [1u32, PARENT_BUILDER_VERSION, meshlet_builder, fingerprint.len() as u32] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(fingerprint.as_bytes());
    for value in [
        geometry.hierarchy.leaf_count,
        geometry.hierarchy.nodes.len() as u32,
        geometry.ranges.len() as u32,
        geometry.vertices.len() as u32,
        geometry.indices.len() as u32,
        geometry.leaf_triangles,
        geometry.parent_triangles,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for node in &geometry.hierarchy.nodes {
        for value in [node.parent, node.children[0], node.children[1], node.child_count, node.leaf] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for lane in node.center.iter().chain(std::iter::once(&node.radius)).chain(std::iter::once(&node.error)) {
            bytes.extend_from_slice(&lane.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&(geometry.hierarchy.roots.len() as u32).to_le_bytes());
    for root in &geometry.hierarchy.roots {
        bytes.extend_from_slice(&root.to_le_bytes());
    }
    for range in &geometry.ranges {
        for value in [range.first_index, range.index_count, range.triangles, range.level] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    for count in &geometry.leaf_counts {
        bytes.extend_from_slice(&count.to_le_bytes());
    }
    bytes.extend_from_slice(&geometry.vertices);
    for color in &geometry.debug_colors {
        for lane in color {
            bytes.extend_from_slice(&lane.to_le_bytes());
        }
    }
    for index in &geometry.indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes.extend_from_slice(&geometry.build_ms.to_le_bytes());
    bytes
}

fn decode_parent_cache(bytes: &[u8], fingerprint: &str, meshlet_builder: u32) -> Option<ParentGeometry> {
    if bytes.len() < 24 || &bytes[0..8] != PARENT_MAGIC {
        return None;
    }
    let mut cursor = 8usize;
    let take_u32 = |cursor: &mut usize| -> Option<u32> {
        let slice = bytes.get(*cursor..*cursor + 4)?;
        *cursor += 4;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    };
    let take_f32 = |cursor: &mut usize| -> Option<f32> {
        let slice = bytes.get(*cursor..*cursor + 4)?;
        *cursor += 4;
        Some(f32::from_le_bytes(slice.try_into().ok()?))
    };
    let format = take_u32(&mut cursor)?;
    let builder = take_u32(&mut cursor)?;
    let stored_meshlet_builder = take_u32(&mut cursor)?;
    let fingerprint_len = take_u32(&mut cursor)? as usize;
    if format != 1 || builder != PARENT_BUILDER_VERSION || stored_meshlet_builder != meshlet_builder {
        return None;
    }
    let stored_fingerprint = std::str::from_utf8(bytes.get(cursor..cursor + fingerprint_len)?).ok()?;
    if stored_fingerprint != fingerprint {
        return None;
    }
    cursor += fingerprint_len;
    let leaf_count = take_u32(&mut cursor)?;
    let node_count = take_u32(&mut cursor)? as usize;
    let range_count = take_u32(&mut cursor)? as usize;
    let vertex_bytes = take_u32(&mut cursor)? as usize;
    let index_count = take_u32(&mut cursor)? as usize;
    let leaf_triangles = take_u32(&mut cursor)?;
    let parent_triangles = take_u32(&mut cursor)?;
    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let parent = take_u32(&mut cursor)?;
        let child0 = take_u32(&mut cursor)?;
        let child1 = take_u32(&mut cursor)?;
        let child_count = take_u32(&mut cursor)?;
        let leaf = take_u32(&mut cursor)?;
        let center = [take_f32(&mut cursor)?, take_f32(&mut cursor)?, take_f32(&mut cursor)?];
        let radius = take_f32(&mut cursor)?;
        let error = take_f32(&mut cursor)?;
        if !center.iter().all(|lane| lane.is_finite()) || !radius.is_finite() || !error.is_finite() {
            return None;
        }
        nodes.push(crate::meshlet_hierarchy::ClusterNode { parent, children: [child0, child1], child_count, leaf, center, radius, error });
    }
    let root_count = take_u32(&mut cursor)? as usize;
    let mut roots = Vec::with_capacity(root_count);
    for _ in 0..root_count {
        roots.push(take_u32(&mut cursor)?);
    }
    let mut ranges = Vec::with_capacity(range_count);
    for _ in 0..range_count {
        ranges.push(ParentRange {
            first_index: take_u32(&mut cursor)?,
            index_count: take_u32(&mut cursor)?,
            triangles: take_u32(&mut cursor)?,
            level: take_u32(&mut cursor)?,
        });
    }
    let mut leaf_counts = Vec::with_capacity(leaf_count as usize);
    for _ in 0..leaf_count {
        leaf_counts.push(take_u32(&mut cursor)?);
    }
    let vertices = bytes.get(cursor..cursor + vertex_bytes)?.to_vec();
    cursor += vertex_bytes;
    let color_count = vertex_bytes / 60;
    let mut debug_colors = Vec::with_capacity(color_count);
    for _ in 0..color_count {
        debug_colors.push([take_f32(&mut cursor)?, take_f32(&mut cursor)?, take_f32(&mut cursor)?]);
    }
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        indices.push(take_u32(&mut cursor)?);
    }
    let build_ms = take_f32(&mut cursor).unwrap_or(0.0);
    let hierarchy = crate::meshlet_hierarchy::ClusterHierarchy { nodes, roots, leaf_count };
    let root_triangles = hierarchy.roots.iter().map(|node| ranges.get(*node as usize).map(|range| range.triangles).unwrap_or(0)).fold(0u32, |sum, count| sum.saturating_add(count));
    let levels = level_stats(&hierarchy, &vertices, &indices, &ranges, &leaf_counts);
    let empty_parents = levels.iter().fold(0u32, |sum, level| sum.saturating_add(level.empty_parents));
    Some(ParentGeometry {
        hierarchy,
        vertices,
        debug_colors,
        indices,
        ranges,
        leaf_counts,
        leaf_triangles,
        parent_triangles,
        root_triangles,
        empty_parents,
        levels,
        build_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{mesh_from_surfaces, CanonicalSurface};
    use crate::meshlet_hierarchy::select_cluster_cut;

    fn fan(ids: [u32; 4], center: u32) -> Vec<u32> {
        vec![ids[0], ids[1], center, ids[1], ids[2], center, ids[2], ids[3], center, ids[3], ids[0], center]
    }

    fn two_quads() -> (Mesh, MeshletSet) {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [0.5, 0.0, 0.5],
            [2.0, 0.0, 0.0],
            [2.0, 0.0, 1.0],
            [1.5, 0.0, 0.5],
        ];
        let uvs: Vec<[f32; 2]> = positions.iter().map(|position| [position[0] / 2.0, position[2]]).collect();
        let indices = {
            let mut indices = fan([0, 1, 3, 2], 4);
            indices.extend(fan([1, 5, 6, 3], 7));
            indices
        };
        let mesh = mesh_from_surfaces(&[CanonicalSurface {
            positions: positions.clone(),
            normals: Vec::new(),
            texcoords: uvs,
            tangents: Vec::new(),
            indices,
            material_slot: 0,
        }])
        .expect("quads");
        let meshlets = vec![
            patch(&positions, &[0, 1, 2, 3, 4], &[0, 1, 4, 1, 3, 4, 3, 2, 4, 2, 0, 4], 0),
            patch(&positions, &[1, 5, 3, 6, 7], &[1, 5, 7, 5, 6, 7, 6, 3, 7, 3, 1, 7], 1),
        ];
        let set = set_from(meshlets);
        (mesh, set)
    }

    fn patch(positions: &[[f32; 3]], verts: &[u32], locals: &[u8], ordinal: u32) -> Meshlet {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for vertex in verts {
            let position = positions[*vertex as usize];
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }
        let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
        Meshlet {
            vertex_offset: if ordinal == 0 { 0 } else { 5 },
            vertex_count: verts.len() as u32,
            index_offset: ordinal * 12,
            index_count: locals.len() as u32,
            material_slot: 0,
            submesh: 0,
            bounds_min: min,
            bounds_max: max,
            sphere_center: center,
            sphere_radius: 1.0,
            cone_axis: [0.0, 1.0, 0.0],
            cone_cutoff: 1.0,
        }
    }

    fn set_from(meshlets: Vec<Meshlet>) -> MeshletSet {
        MeshletSet {
            max_vertices: 128,
            max_triangles: 128,
            grid_resolution: 1,
            builder_version: 1,
            importer_version: 0,
            source_vertices: 8,
            source_fingerprint: String::new(),
            meshlets,
            vertex_indices: vec![0, 1, 2, 3, 4, 1, 5, 3, 6, 7],
            local_indices: vec![0, 1, 4, 1, 3, 4, 3, 2, 4, 2, 0, 4, 0, 1, 4, 1, 3, 4, 3, 2, 4, 2, 0, 4],
            stats: crate::meshlet::MeshletStats {
                source_triangles: 8,
                meshlet_count: 2,
                average_triangles: 4.0,
                average_vertices: 5.0,
                min_triangles: 4,
                max_triangles: 4,
                min_vertices: 5,
                max_vertices: 5,
                derived_bytes: 0,
                build_ms: 0.0,
                load_ms: 0.0,
                write_ms: 0.0,
            },
        }
    }

    fn position_at(geometry: &ParentGeometry, index: u32) -> [f32; 3] {
        let start = index as usize * STRIDE;
        [
            f32::from_le_bytes(geometry.vertices[start..start + 4].try_into().unwrap()),
            f32::from_le_bytes(geometry.vertices[start + 4..start + 8].try_into().unwrap()),
            f32::from_le_bytes(geometry.vertices[start + 8..start + 12].try_into().unwrap()),
        ]
    }

    #[test]
    fn a_parent_replaces_both_quads_and_keeps_the_outer_corners() {
        let (mesh, meshlets) = two_quads();
        let geometry = build_parent_geometry(&mesh, &meshlets);
        assert_eq!(geometry.leaf_triangles, 8);
        assert_eq!(geometry.hierarchy.leaf_count, 2);
        assert_eq!(geometry.hierarchy.roots.len(), 1);
        let parent = geometry.hierarchy.roots[0];
        let plain = build_cluster_hierarchy(&[
            GpuMeshletRecord::from_meshlet(&meshlets.meshlets[0]),
            GpuMeshletRecord::from_meshlet(&meshlets.meshlets[1]),
        ]);
        assert_eq!(geometry.hierarchy.nodes[parent as usize].children, plain.nodes[parent as usize].children);
        let range = &geometry.ranges[parent as usize];
        assert!(range.triangles > 0, "parent mesh missing");
        assert!(range.triangles < 8, "parent triangles {}", range.triangles);
        assert!(geometry.root_triangles > 0, "root triangles {}", geometry.root_triangles);
        assert_eq!(geometry.empty_parents, 0);
        assert!(geometry.levels.iter().all(|level| level.empty_parents == 0 && level.bounds_covered == level.nodes));
        assert!(geometry.hierarchy.nodes[parent as usize].error > 0.0);
        assert!(geometry.hierarchy.nodes[parent as usize].error < 10.0);
        let mut found = [false; 4];
        let corners = [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [2.0, 0.0, 0.0], [2.0, 0.0, 1.0]];
        let count = geometry.vertices.len() / STRIDE;
        for index in 0..count {
            let position = position_at(&geometry, index as u32);
            for (corner, hit) in corners.iter().zip(found.iter_mut()) {
                if distance(position, *corner) < 1.0e-4 {
                    *hit = true;
                }
            }
        }
        assert_eq!(found, [true, true, true, true]);
        let again = build_parent_geometry(&mesh, &meshlets);
        assert_eq!(again.ranges[parent as usize], geometry.ranges[parent as usize]);
        assert_eq!(again.indices, geometry.indices);
        assert_eq!(again.hierarchy.nodes[parent as usize].error, geometry.hierarchy.nodes[parent as usize].error);
    }

    #[test]
    fn a_close_cut_submits_every_leaf_and_a_far_cut_submits_only_the_parent() {
        let (mesh, meshlets) = two_quads();
        let geometry = build_parent_geometry(&mesh, &meshlets);
        let parent = geometry.hierarchy.roots[0];
        let close = select_cluster_cut(&geometry.hierarchy, |_| 0.3, 0.577, 1080.0, 1.0);
        let close_draw = submission_for_cut(&geometry.hierarchy, &geometry.ranges, &geometry.leaf_counts, geometry.leaf_triangles, &close, None);
        assert_eq!(close_draw.leaf_meshlets, vec![0, 1]);
        assert!(close_draw.parent_nodes.is_empty());
        assert_eq!(close_draw.triangles, 8);
        let far = select_cluster_cut(&geometry.hierarchy, |_| 4000.0, 0.577, 1080.0, 1.0);
        let far_draw = submission_for_cut(&geometry.hierarchy, &geometry.ranges, &geometry.leaf_counts, geometry.leaf_triangles, &far, None);
        assert_eq!(far_draw.parent_nodes, vec![parent]);
        assert!(far_draw.leaf_meshlets.is_empty());
        assert!(far_draw.triangles < geometry.leaf_triangles);
        assert!(parent_draw_indices(&geometry.indices, &geometry.ranges, &far_draw.parent_nodes).len() as u32 / 3 == far_draw.triangles);
        let mut covered = far_draw.leaf_meshlets.clone();
        for node in &far_draw.parent_nodes {
            covered.extend(leaves_under(&geometry.hierarchy, *node));
        }
        covered.sort_unstable();
        assert_eq!(covered, vec![0, 1]);
        for leaf in &far_draw.leaf_meshlets {
            for node in &far_draw.parent_nodes {
                assert!(!leaves_under(&geometry.hierarchy, *node).contains(leaf));
            }
        }
    }

    #[test]
    fn a_mixed_material_parent_keeps_the_leaves() {
        let (mesh, mut meshlets) = two_quads();
        meshlets.meshlets[1].submesh = 1;
        meshlets.meshlets[1].material_slot = 1;
        let geometry = build_parent_geometry(&mesh, &meshlets);
        let parent = geometry.hierarchy.roots[0];
        assert_eq!(geometry.ranges[parent as usize].index_count, 0);
        let far = select_cluster_cut(&geometry.hierarchy, |_| 4000.0, 0.577, 1080.0, 1.0);
        let draw = submission_for_cut(&geometry.hierarchy, &geometry.ranges, &geometry.leaf_counts, geometry.leaf_triangles, &far, None);
        assert_eq!(draw.leaf_meshlets, vec![0, 1]);
        assert!(draw.parent_nodes.is_empty());
        assert_eq!(draw.triangles, 8);
    }

    #[test]
    fn a_cancel_request_stops_the_parent_build() {
        let (mesh, meshlets) = two_quads();
        let error = build_parent_geometry_with(&mesh, &meshlets, &|| true, &|_, _| {}).expect_err("cancel");
        assert_eq!(error, "cancelled");
    }

    #[test]
    fn a_parent_that_covers_the_shop_is_allowed_more_than_32_triangles() {
        assert_eq!(triangle_budget(8), 32);
        assert_eq!(triangle_budget(200), 32);
        assert!(triangle_budget(6_122_214) > 32);
        assert!(triangle_budget(6_122_214) <= 2048);
    }

    #[test]
    fn a_parent_sidecar_reloads_only_when_the_fingerprint_matches() {
        let (mesh, meshlets) = two_quads();
        let geometry = build_parent_geometry(&mesh, &meshlets);
        let path = std::env::temp_dir().join("jarvig-parent-cache-test.jarvigparents");
        save_parent_cache(&path, "finger", 1, &geometry).expect("write");
        let loaded = load_parent_cache(&path, "finger", 1).expect("load");
        assert_eq!(loaded.indices, geometry.indices);
        assert_eq!(loaded.hierarchy.nodes.len(), geometry.hierarchy.nodes.len());
        assert_eq!(loaded.leaf_triangles, geometry.leaf_triangles);
        assert!(load_parent_cache(&path, "other", 1).is_none());
        assert!(load_parent_cache(&path, "finger", 9).is_none());
        let _ = std::fs::remove_file(path);
    }
}
