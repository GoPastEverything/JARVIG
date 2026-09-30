//! Local-space triangle BVH. Built with the mesh. A selection does not rebuild it.
//!
//! The editor asks which actor a click hit. It does not walk every triangle of a
//! multi-million triangle import. Leaves hold a few triangles. The query prunes
//! by box, then tests those triangles.

use crate::mesh::Mesh;

const LEAF_TRIANGLES: usize = 16;

#[derive(Clone, PartialEq)]
pub struct TriangleBvh {
    nodes: Vec<BvhNode>,
    triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Copy, PartialEq)]
struct BvhNode {
    min: [f32; 3],
    max: [f32; 3],
    /// Child index. `u32::MAX` is a leaf.
    left: u32,
    /// Right child, or the first triangle when this node is a leaf.
    right_or_start: u32,
    /// Triangle count for a leaf. Zero for an internal node.
    count: u32,
}

impl std::fmt::Debug for TriangleBvh {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "TriangleBvh {{ nodes: {}, triangles: {} }}", self.nodes.len(), self.triangles.len())
    }
}

struct Item {
    min: [f32; 3],
    max: [f32; 3],
    centroid: [f32; 3],
    tri: [u32; 3],
}

impl TriangleBvh {
    pub fn empty() -> Self {
        Self { nodes: Vec::new(), triangles: Vec::new() }
    }

    pub fn write_bytes(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"JARVBVH1");
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.triangles.len() as u32).to_le_bytes());
        for node in &self.nodes {
            for lane in node.min.iter().chain(node.max.iter()) {
                out.extend_from_slice(&lane.to_le_bytes());
            }
            out.extend_from_slice(&node.left.to_le_bytes());
            out.extend_from_slice(&node.right_or_start.to_le_bytes());
            out.extend_from_slice(&node.count.to_le_bytes());
        }
        for triangle in &self.triangles {
            for index in triangle {
                out.extend_from_slice(&index.to_le_bytes());
            }
        }
    }

    pub fn read_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 16 || &bytes[..8] != b"JARVBVH1" {
            return Err("pick structure is missing".into());
        }
        let node_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let triangle_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let node_bytes = node_count * 36;
        let triangle_bytes = triangle_count * 12;
        if bytes.len() < 16 + node_bytes + triangle_bytes {
            return Err("pick structure is truncated".into());
        }
        let mut nodes = Vec::with_capacity(node_count);
        let mut cursor = 16;
        for _ in 0..node_count {
            let mut min = [0.0; 3];
            let mut max = [0.0; 3];
            for lane in 0..3 {
                min[lane] = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
                cursor += 4;
            }
            for lane in 0..3 {
                max[lane] = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
                cursor += 4;
            }
            let left = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
            let right_or_start = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
            let count = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
            nodes.push(BvhNode { min, max, left, right_or_start, count });
        }
        let mut triangles = Vec::with_capacity(triangle_count);
        for _ in 0..triangle_count {
            let mut triangle = [0u32; 3];
            for lane in 0..3 {
                triangle[lane] = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
                cursor += 4;
            }
            triangles.push(triangle);
        }
        Ok(Self { nodes, triangles })
    }

    pub fn build(mesh: &Mesh) -> Self {
        let mut items = Vec::new();
        for triangle in mesh.triangle_indices() {
            let Some(a) = mesh.position(triangle[0]) else { continue };
            let Some(b) = mesh.position(triangle[1]) else { continue };
            let Some(c) = mesh.position(triangle[2]) else { continue };
            let min = [a[0].min(b[0]).min(c[0]), a[1].min(b[1]).min(c[1]), a[2].min(b[2]).min(c[2])];
            let max = [a[0].max(b[0]).max(c[0]), a[1].max(b[1]).max(c[1]), a[2].max(b[2]).max(c[2])];
            items.push(Item {
                min,
                max,
                centroid: [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5],
                tri: triangle,
            });
        }
        if items.is_empty() {
            return Self::empty();
        }
        let mut nodes = Vec::new();
        let mut triangles = Vec::with_capacity(items.len());
        build_range(&mut items, &mut nodes, &mut triangles);
        Self { nodes, triangles }
    }

    /// `t` along `direction`. The direction is not required to be unit length.
    pub fn intersect(&self, origin: [f64; 3], direction: [f64; 3], position: impl Fn(u32) -> Option<[f32; 3]>) -> Option<f64> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<f64> = None;
        let mut stack = vec![0u32];
        while let Some(index) = stack.pop() {
            let Some(node) = self.nodes.get(index as usize) else { continue };
            let limit = best.unwrap_or(f64::MAX);
            if !ray_aabb(origin, direction, node.min, node.max, limit) {
                continue;
            }
            if node.count > 0 {
                let start = node.right_or_start as usize;
                let end = start + node.count as usize;
                for triangle in self.triangles.get(start..end).unwrap_or(&[]) {
                    let [Some(a), Some(b), Some(c)] = [position(triangle[0]), position(triangle[1]), position(triangle[2])] else { continue };
                    if let Some(distance) = ray_triangle(origin, direction, a, b, c) {
                        if best.map_or(true, |current| distance < current) {
                            best = Some(distance);
                        }
                    }
                }
            } else {
                stack.push(node.left);
                stack.push(node.right_or_start);
            }
        }
        best
    }
}

fn build_range(items: &mut [Item], nodes: &mut Vec<BvhNode>, triangles: &mut Vec<[u32; 3]>) -> u32 {
    let index = nodes.len() as u32;
    nodes.push(BvhNode { min: [0.0; 3], max: [0.0; 3], left: u32::MAX, right_or_start: 0, count: 0 });
    if items.len() <= LEAF_TRIANGLES {
        let (min, max) = item_bounds(items);
        let start = triangles.len() as u32;
        for item in items.iter() {
            triangles.push(item.tri);
        }
        nodes[index as usize] = BvhNode { min, max, left: u32::MAX, right_or_start: start, count: items.len() as u32 };
        return index;
    }
    let axis = longest_centroid_axis(items);
    let mid = items.len() / 2;
    items.select_nth_unstable_by(mid, |left, right| left.centroid[axis].total_cmp(&right.centroid[axis]));
    let (left_items, right_items) = items.split_at_mut(mid);
    let left = build_range(left_items, nodes, triangles);
    let right = build_range(right_items, nodes, triangles);
    let min = vmin(nodes[left as usize].min, nodes[right as usize].min);
    let max = vmax(nodes[left as usize].max, nodes[right as usize].max);
    nodes[index as usize] = BvhNode { min, max, left, right_or_start: right, count: 0 };
    index
}

fn item_bounds(items: &[Item]) -> ([f32; 3], [f32; 3]) {
    let mut min = items[0].min;
    let mut max = items[0].max;
    for item in items.iter().skip(1) {
        min = vmin(min, item.min);
        max = vmax(max, item.max);
    }
    (min, max)
}

fn longest_centroid_axis(items: &[Item]) -> usize {
    let mut min = items[0].centroid;
    let mut max = items[0].centroid;
    for item in items.iter().skip(1) {
        min = vmin(min, item.centroid);
        max = vmax(max, item.centroid);
    }
    let span = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    if span[0] >= span[1] && span[0] >= span[2] { 0 } else if span[1] >= span[2] { 1 } else { 2 }
}

fn vmin(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0].min(right[0]), left[1].min(right[1]), left[2].min(right[2])]
}

fn vmax(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0].max(right[0]), left[1].max(right[1]), left[2].max(right[2])]
}

fn ray_aabb(origin: [f64; 3], direction: [f64; 3], min: [f32; 3], max: [f32; 3], limit: f64) -> bool {
    let mut tmin: f64 = 0.0;
    let mut tmax: f64 = limit;
    for axis in 0..3 {
        let origin = origin[axis];
        let direction = direction[axis];
        if direction.abs() < 1.0e-15 {
            if origin < f64::from(min[axis]) || origin > f64::from(max[axis]) {
                return false;
            }
            continue;
        }
        let mut enter = (f64::from(min[axis]) - origin) / direction;
        let mut exit = (f64::from(max[axis]) - origin) / direction;
        if enter > exit {
            std::mem::swap(&mut enter, &mut exit);
        }
        tmin = tmin.max(enter);
        tmax = tmax.min(exit);
        if tmin > tmax {
            return false;
        }
    }
    tmax >= 0.0
}

fn ray_triangle(origin: [f64; 3], direction: [f64; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<f64> {
    let a = [f64::from(a[0]), f64::from(a[1]), f64::from(a[2])];
    let b = [f64::from(b[0]), f64::from(b[1]), f64::from(b[2])];
    let c = [f64::from(c[0]), f64::from(c[1]), f64::from(c[2])];
    let edge1 = sub(b, a);
    let edge2 = sub(c, a);
    let p = cross(direction, edge2);
    let det = dot(edge1, p);
    if det.abs() < 1.0e-12 {
        return None;
    }
    let inverse = 1.0 / det;
    let t = sub(origin, a);
    let u = dot(t, p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(t, edge1);
    let v = dot(direction, q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = dot(edge2, q) * inverse;
    if distance > 1.0e-4 { Some(distance) } else { None }
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
