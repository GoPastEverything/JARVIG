//! Persistent face, edge, and vertex identity for one parametric solid.
//!
//! The body is the authored solid once an edge, vertex, or subdivision edit
//! exists. Derived triangles are a view of it. They do not keep it closed.

/// Ordinary edge shorter than this is a collapsed solid.
const MIN_EDGE_M: f64 = 1.0e-4;
/// Newell area below this is a collapsed face. The vector length is twice the area.
const MIN_AREA: f64 = 1.0e-8;
/// A drag shorter than this is not an edit.
const MIN_DELTA_M: f64 = 1.0e-9;
/// Hits this close to a triangle boundary are not an interior pierce.
const INTERIOR_EPS: f64 = 1.0e-4;
/// A segment parallel to a triangle is not a pierce.
const PARALLEL_EPS: f64 = 1.0e-12;
/// Subdivision factors above this are refused. One means the face is unchanged.
pub const SUBDIVIDE_MAX: u32 = 8;

/// One corner. The id survives splits and moves.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidVertex {
    pub id: u32,
    pub position: [f64; 3],
}

/// One undirected connection. The stored order is not a winding.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidEdge {
    pub id: u32,
    pub a: u32,
    pub b: u32,
}

/// One closed loop. Winding is the outward side.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidFace {
    pub id: u32,
    pub vertices: Vec<u32>,
}

/// Authored topology. Ids are not triangle or meshlet indices.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidBody {
    pub next_id: u32,
    pub vertices: Vec<SolidVertex>,
    pub edges: Vec<SolidEdge>,
    pub faces: Vec<SolidFace>,
}

/// A candidate that passed the closed-solid check. `shift` is added to the entity translation.
#[derive(Clone, Debug, PartialEq)]
pub struct TopologyEdit {
    pub body: SolidBody,
    pub shift: [f64; 3],
    pub size_m: [f64; 3],
}

/// Why a candidate was not stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TopologyError {
    /// The id is not on this solid.
    Missing,
    /// Subdivide Face was asked of a loop that is not a quad.
    NotQuad,
    /// The edit would collapse a face or an edge, or the numbers are not usable.
    Degenerate,
    /// The edit would open a crack, a T-junction, or a self-intersection.
    Torn,
}

impl std::fmt::Display for TopologyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Missing => "that element is not on the solid",
            Self::NotQuad => "Subdivide Face needs a quad",
            Self::Degenerate => "that edit would collapse the solid",
            Self::Torn => "that edit would rip the solid",
        })
    }
}

/// A viewport hit on one topological element. `ray_t` is distance along a unit ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopologyPick {
    pub id: u32,
    pub ray_t: f64,
    pub distance: f64,
}

impl SolidBody {
    /// Canonical box. Vertex ids are `1..=8`, edge ids `9..=20`, face ids `1..=6`, `next_id` is 21.
    ///
    /// Face ids are the analytic face index plus one: +X, −X, +Y, −Y, +Z, −Z.
    pub fn from_box(size_m: [f64; 3]) -> Result<Self, TopologyError> {
        if size_m.iter().any(|axis| !axis.is_finite() || *axis <= 0.0) {
            return Err(TopologyError::Degenerate);
        }
        let half = [size_m[0] * 0.5, size_m[1] * 0.5, size_m[2] * 0.5];
        let mut vertices = Vec::with_capacity(8);
        for bit in 0..8u32 {
            let position = [
                if bit & 1 == 0 { -half[0] } else { half[0] },
                if bit & 2 == 0 { -half[1] } else { half[1] },
                if bit & 4 == 0 { -half[2] } else { half[2] },
            ];
            vertices.push(SolidVertex { id: bit + 1, position });
        }
        let edges = [
            (9, 1, 2),
            (10, 3, 4),
            (11, 5, 6),
            (12, 7, 8),
            (13, 1, 3),
            (14, 2, 4),
            (15, 5, 7),
            (16, 6, 8),
            (17, 1, 5),
            (18, 2, 6),
            (19, 3, 7),
            (20, 4, 8),
        ]
        .into_iter()
        .map(|(id, a, b)| SolidEdge { id, a, b })
        .collect();
        let faces = [
            (1, vec![2, 4, 8, 6]),
            (2, vec![1, 5, 7, 3]),
            (3, vec![3, 7, 8, 4]),
            (4, vec![1, 2, 6, 5]),
            (5, vec![5, 6, 8, 7]),
            (6, vec![1, 3, 4, 2]),
        ]
        .into_iter()
        .map(|(id, vertices)| SolidFace { id, vertices })
        .collect();
        let body = Self { next_id: 21, vertices, edges, faces };
        body.validate()?;
        Ok(body)
    }

    /// Closed manifold: two faces per ordinary edge, opposite windings, no crack and no collapse.
    pub fn validate(&self) -> Result<(), TopologyError> {
        if self.next_id == 0 || self.vertices.is_empty() || self.edges.is_empty() || self.faces.is_empty() {
            return Err(TopologyError::Torn);
        }
        let mut vertex_ids = Vec::new();
        let mut edge_ids = Vec::new();
        let mut face_ids = Vec::new();
        let claim = |seen: &mut Vec<u32>, id: u32| -> Result<(), TopologyError> {
            if id == 0 || id >= self.next_id || seen.contains(&id) {
                return Err(TopologyError::Torn);
            }
            seen.push(id);
            Ok(())
        };
        for vertex in &self.vertices {
            claim(&mut vertex_ids, vertex.id)?;
            if vertex.position.iter().any(|axis| !axis.is_finite()) {
                return Err(TopologyError::Degenerate);
            }
        }
        for edge in &self.edges {
            claim(&mut edge_ids, edge.id)?;
            if edge.a == edge.b || self.vertex_position(edge.a).is_none() || self.vertex_position(edge.b).is_none() {
                return Err(TopologyError::Torn);
            }
            let length = distance(self.vertex_position(edge.a).unwrap_or([0.0; 3]), self.vertex_position(edge.b).unwrap_or([0.0; 3]));
            if length < MIN_EDGE_M {
                return Err(TopologyError::Degenerate);
            }
        }
        for face in &self.faces {
            claim(&mut face_ids, face.id)?;
            if face.vertices.len() < 3 {
                return Err(TopologyError::Torn);
            }
            for (index, vertex) in face.vertices.iter().copied().enumerate() {
                if self.vertex_position(vertex).is_none() || face.vertices[..index].contains(&vertex) {
                    return Err(TopologyError::Torn);
                }
            }
            let area = newell_length(&self.loop_positions(face));
            if area < MIN_AREA * 2.0 {
                return Err(TopologyError::Degenerate);
            }
        }
        for vertex in &self.vertices {
            if !self.faces.iter().any(|face| face.vertices.contains(&vertex.id)) {
                return Err(TopologyError::Torn);
            }
        }
        let mut directed: Vec<(u32, u32)> = Vec::new();
        for face in &self.faces {
            let count = face.vertices.len();
            for index in 0..count {
                let start = face.vertices[index];
                let end = face.vertices[(index + 1) % count];
                if self.edge_between(start, end).is_none() {
                    return Err(TopologyError::Torn);
                }
                if directed.contains(&(start, end)) {
                    return Err(TopologyError::Torn);
                }
                directed.push((start, end));
            }
        }
        for edge in &self.edges {
            let forward = directed.iter().filter(|(a, b)| *a == edge.a && *b == edge.b).count();
            let reverse = directed.iter().filter(|(a, b)| *a == edge.b && *b == edge.a).count();
            if forward != 1 || reverse != 1 {
                return Err(TopologyError::Torn);
            }
        }
        let (min, max) = self.bounds().ok_or(TopologyError::Degenerate)?;
        let center = midpoint(min, max);
        if center.iter().any(|axis| !axis.is_finite() || axis.abs() > 1.0e-6) {
            return Err(TopologyError::Torn);
        }
        self.reject_t_junctions()?;
        self.reject_pierces()?;
        Ok(())
    }

    /// Full extents of the vertex box. Collision uses this box, not the slanted faces inside it.
    pub fn aabb_size(&self) -> [f64; 3] {
        let Some((min, max)) = self.bounds() else { return [0.0; 3] };
        sub(max, min)
    }

    pub fn vertex_position(&self, id: u32) -> Option<[f64; 3]> {
        self.vertices.iter().find(|vertex| vertex.id == id).map(|vertex| vertex.position)
    }

    pub fn edge_endpoints(&self, id: u32) -> Option<([f64; 3], [f64; 3])> {
        let edge = self.edges.iter().find(|edge| edge.id == id)?;
        Some((self.vertex_position(edge.a)?, self.vertex_position(edge.b)?))
    }

    pub fn edge_length(&self, id: u32) -> Option<f64> {
        let (start, end) = self.edge_endpoints(id)?;
        Some(distance(start, end))
    }

    pub fn edge_between(&self, a: u32, b: u32) -> Option<u32> {
        self.edges.iter().find(|edge| (edge.a == a && edge.b == b) || (edge.a == b && edge.b == a)).map(|edge| edge.id)
    }

    pub fn face_loop(&self, id: u32) -> Option<&[u32]> {
        self.faces.iter().find(|face| face.id == id).map(|face| face.vertices.as_slice())
    }

    pub fn face_len(&self, id: u32) -> Option<usize> {
        self.face_loop(id).map(|loop_| loop_.len())
    }

    pub fn face_positions(&self, id: u32) -> Option<Vec<[f64; 3]>> {
        let loop_ = self.face_loop(id)?;
        loop_.iter().map(|id| self.vertex_position(*id)).collect()
    }

    /// Outward unit normal from the loop winding. `None` when the face has no area.
    pub fn unit_normal(&self, face: u32) -> Option<[f64; 3]> {
        let positions = self.face_positions(face)?;
        unit(newell(&positions))
    }

    pub fn edge_segments(&self) -> Vec<(u32, [f64; 3], [f64; 3])> {
        self.edges
            .iter()
            .filter_map(|edge| {
                let (start, end) = self.edge_endpoints(edge.id)?;
                Some((edge.id, start, end))
            })
            .collect()
    }

    /// Translate both ends. Adjacency stays. A collapsed or folded result is refused.
    pub fn move_edge(&self, edge: u32, delta: [f64; 3]) -> Result<TopologyEdit, TopologyError> {
        if !usable_delta(delta) {
            return Err(TopologyError::Degenerate);
        }
        let mut body = self.clone();
        let (start, end) = body.edge_ids(edge).ok_or(TopologyError::Missing)?;
        body.translate(start, delta)?;
        body.translate(end, delta)?;
        body.finish()
    }

    /// Translate one vertex. The loops that already contain it stay connected.
    pub fn move_vertex(&self, vertex: u32, delta: [f64; 3]) -> Result<TopologyEdit, TopologyError> {
        if !usable_delta(delta) {
            return Err(TopologyError::Degenerate);
        }
        let mut body = self.clone();
        body.translate(vertex, delta)?;
        body.finish()
    }

    /// New edge plus the wall that joins it. Side edges gain the new vertices so the solid stays closed.
    pub fn extrude_edge(&self, edge: u32, delta: [f64; 3]) -> Result<TopologyEdit, TopologyError> {
        if !usable_delta(delta) {
            return Err(TopologyError::Degenerate);
        }
        let mut body = self.clone();
        let (a, b) = body.edge_ids(edge).ok_or(TopologyError::Missing)?;
        let incidents = body.incident_face_indexes(a, b);
        if incidents.len() != 2 {
            return Err(TopologyError::Torn);
        }
        let normal_a = unit(newell(&body.loop_positions(&body.faces[incidents[0]]))).ok_or(TopologyError::Degenerate)?;
        let normal_b = unit(newell(&body.loop_positions(&body.faces[incidents[1]]))).ok_or(TopologyError::Degenerate)?;
        let chosen = if dot(normal_a, delta) > dot(normal_b, delta) { 0 } else { 1 };
        let face_index = incidents[chosen];
        let face_normal = if chosen == 0 { normal_a } else { normal_b };
        let face_id = body.faces[face_index].id;
        let loop_ = body.faces[face_index].vertices.clone();
        let count = loop_.len();
        if count < 4 {
            return Err(TopologyError::Degenerate);
        }
        let slot = (0..count)
            .find(|index| {
                let start = loop_[*index];
                let end = loop_[(*index + 1) % count];
                (start == a && end == b) || (start == b && end == a)
            })
            .ok_or(TopologyError::Torn)?;
        let u = loop_[slot];
        let v = loop_[(slot + 1) % count];
        let d = loop_[(slot + count - 1) % count];
        let c = loop_[(slot + 2) % count];
        if d == c || body.edge_between(d, u) == body.edge_between(v, c) {
            return Err(TopologyError::Degenerate);
        }
        let p = add(body.vertex_position(u).ok_or(TopologyError::Missing)?, delta);
        let q = add(body.vertex_position(v).ok_or(TopologyError::Missing)?, delta);
        let p_id = body.alloc()?;
        let q_id = body.alloc()?;
        body.vertices.push(SolidVertex { id: p_id, position: p });
        body.vertices.push(SolidVertex { id: q_id, position: q });
        body.split_edge_chain(d, u, &[p_id], Some(face_id))?;
        body.split_edge_chain(v, c, &[q_id], Some(face_id))?;
        body.ensure_edge(p_id, q_id)?;
        let mut rest = Vec::new();
        let mut cursor = (slot + 2) % count;
        while cursor != slot {
            rest.push(loop_[cursor]);
            cursor = (cursor + 1) % count;
        }
        let mut remainder = vec![p_id, q_id];
        remainder.extend(rest);
        body.faces[face_index].vertices = remainder;
        let mut wall = vec![u, v, q_id, p_id];
        let outward = cross(sub(body.vertex_position(v).unwrap_or(q), body.vertex_position(u).unwrap_or(p)), face_normal);
        if dot(newell(&body.positions_of(&wall)), outward) < 0.0 {
            wall.reverse();
        }
        let wall_id = body.alloc()?;
        body.faces.push(SolidFace { id: wall_id, vertices: wall });
        body.finish()
    }

    /// Inserts the midpoint. The old edge id stays on the half that begins at the stored start vertex.
    pub fn split_edge(&self, edge: u32) -> Result<TopologyEdit, TopologyError> {
        let mut body = self.clone();
        let (start, end) = body.edge_ids(edge).ok_or(TopologyError::Missing)?;
        let mid = midpoint(body.vertex_position(start).ok_or(TopologyError::Missing)?, body.vertex_position(end).ok_or(TopologyError::Missing)?);
        let id = body.alloc()?;
        body.vertices.push(SolidVertex { id, position: mid });
        body.split_edge_chain(start, end, &[id], None)?;
        body.finish()
    }

    /// Replaces one quad with a `u` by `v` grid of quads. Corners and the original face id stay.
    pub fn subdivide_face(&self, face: u32, u: u32, v: u32) -> Result<TopologyEdit, TopologyError> {
        if u == 0 || v == 0 || u > SUBDIVIDE_MAX || v > SUBDIVIDE_MAX {
            return Err(TopologyError::Degenerate);
        }
        let mut body = self.clone();
        let face_index = body.faces.iter().position(|entry| entry.id == face).ok_or(TopologyError::Missing)?;
        let loop_ = body.faces[face_index].vertices.clone();
        if loop_.len() != 4 {
            return Err(TopologyError::NotQuad);
        }
        if u == 1 && v == 1 {
            return body.finish();
        }
        let corners = [loop_[0], loop_[1], loop_[2], loop_[3]];
        let corner_at = [
            body.vertex_position(corners[0]).ok_or(TopologyError::Missing)?,
            body.vertex_position(corners[1]).ok_or(TopologyError::Missing)?,
            body.vertex_position(corners[2]).ok_or(TopologyError::Missing)?,
            body.vertex_position(corners[3]).ok_or(TopologyError::Missing)?,
        ];
        let mut grid = vec![0u32; ((u + 1) * (v + 1)) as usize];
        let at = |i: u32, j: u32| (i + j * (u + 1)) as usize;
        grid[at(0, 0)] = corners[0];
        grid[at(u, 0)] = corners[1];
        grid[at(u, v)] = corners[2];
        grid[at(0, v)] = corners[3];
        let sample = |i: u32, j: u32| bilinear(corner_at, i as f64 / u as f64, j as f64 / v as f64);
        let born = |body: &mut SolidBody, i: u32, j: u32| -> Result<u32, TopologyError> {
            let id = body.alloc()?;
            body.vertices.push(SolidVertex { id, position: sample(i, j) });
            Ok(id)
        };
        let mut bottom = Vec::new();
        for i in 1..u {
            let id = born(&mut body, i, 0)?;
            grid[at(i, 0)] = id;
            bottom.push(id);
        }
        let mut right = Vec::new();
        for j in 1..v {
            let id = born(&mut body, u, j)?;
            grid[at(u, j)] = id;
            right.push(id);
        }
        let mut top = Vec::new();
        for i in (1..u).rev() {
            let id = born(&mut body, i, v)?;
            grid[at(i, v)] = id;
            top.push(id);
        }
        let mut left = Vec::new();
        for j in (1..v).rev() {
            let id = born(&mut body, 0, j)?;
            grid[at(0, j)] = id;
            left.push(id);
        }
        for j in 1..v {
            for i in 1..u {
                grid[at(i, j)] = born(&mut body, i, j)?;
            }
        }
        body.split_edge_chain(corners[0], corners[1], &bottom, Some(face))?;
        body.split_edge_chain(corners[1], corners[2], &right, Some(face))?;
        body.split_edge_chain(corners[2], corners[3], &top, Some(face))?;
        body.split_edge_chain(corners[3], corners[0], &left, Some(face))?;
        for j in 0..v {
            for i in 0..u {
                let cell = [grid[at(i, j)], grid[at(i + 1, j)], grid[at(i + 1, j + 1)], grid[at(i, j + 1)]];
                body.ensure_edge(cell[0], cell[1])?;
                body.ensure_edge(cell[1], cell[2])?;
                body.ensure_edge(cell[2], cell[3])?;
                body.ensure_edge(cell[3], cell[0])?;
                if i == 0 && j == 0 {
                    body.faces[face_index].vertices = cell.to_vec();
                } else {
                    let id = body.alloc()?;
                    body.faces.push(SolidFace { id, vertices: cell.to_vec() });
                }
            }
        }
        body.finish()
    }

    /// Scales about the current center, then recenters. Ids stay.
    pub fn scale_to(&self, size_m: [f64; 3]) -> Result<TopologyEdit, TopologyError> {
        if size_m.iter().any(|axis| !axis.is_finite() || *axis <= 0.0) {
            return Err(TopologyError::Degenerate);
        }
        let mut body = self.clone();
        let (min, max) = body.bounds().ok_or(TopologyError::Degenerate)?;
        let center = midpoint(min, max);
        let old = sub(max, min);
        if old.iter().any(|axis| *axis < MIN_EDGE_M) {
            return Err(TopologyError::Degenerate);
        }
        for vertex in &mut body.vertices {
            for axis in 0..3 {
                vertex.position[axis] = center[axis] + (vertex.position[axis] - center[axis]) * (size_m[axis] / old[axis]);
            }
        }
        body.finish()
    }

    /// Flips one axis and reverses every loop so the outside stays outside.
    pub fn mirrored(&self, axis: usize) -> Result<TopologyEdit, TopologyError> {
        if axis > 2 {
            return Err(TopologyError::Degenerate);
        }
        let mut body = self.clone();
        for vertex in &mut body.vertices {
            vertex.position[axis] = -vertex.position[axis];
        }
        for face in &mut body.faces {
            face.vertices.reverse();
        }
        body.finish()
    }

    /// Closest edge to a unit ray, inside `slack` meters. Smaller `ray_t` wins, then smaller distance.
    pub fn pick_edge(&self, origin: [f64; 3], direction: [f64; 3], slack: f64) -> Option<TopologyPick> {
        let direction = unit(direction)?;
        if !slack.is_finite() || slack < 0.0 {
            return None;
        }
        self.edges.iter().filter_map(|edge| {
            let (start, end) = self.edge_endpoints(edge.id)?;
            let (ray_t, distance) = ray_segment(origin, direction, start, end);
            (distance <= slack).then_some(TopologyPick { id: edge.id, ray_t, distance })
        }).min_by(pick_order)
    }

    /// Closest vertex to a unit ray, inside `slack` meters.
    pub fn pick_vertex(&self, origin: [f64; 3], direction: [f64; 3], slack: f64) -> Option<TopologyPick> {
        let direction = unit(direction)?;
        if !slack.is_finite() || slack < 0.0 {
            return None;
        }
        self.vertices.iter().filter_map(|vertex| {
            let offset = sub(vertex.position, origin);
            let ray_t = dot(offset, direction);
            if ray_t < 0.0 {
                return None;
            }
            let distance = distance(vertex.position, add(origin, scale(direction, ray_t)));
            (distance <= slack).then_some(TopologyPick { id: vertex.id, ray_t, distance })
        }).min_by(pick_order)
    }

    /// First face whose triangle the unit ray enters.
    pub fn pick_face(&self, origin: [f64; 3], direction: [f64; 3]) -> Option<TopologyPick> {
        let direction = unit(direction)?;
        let mut best: Option<TopologyPick> = None;
        for face in &self.faces {
            let positions = self.loop_positions(face);
            if positions.len() < 3 {
                continue;
            }
            for index in 1..positions.len() - 1 {
                if let Some(ray_t) = ray_triangle(origin, direction, [positions[0], positions[index], positions[index + 1]]) {
                    let pick = TopologyPick { id: face.id, ray_t, distance: 0.0 };
                    if best.map(|current| pick_order(&pick, &current).is_lt()).unwrap_or(true) {
                        best = Some(pick);
                    }
                }
            }
        }
        best
    }

    fn finish(mut self) -> Result<TopologyEdit, TopologyError> {
        let shift = self.recenter();
        let size_m = self.aabb_size();
        if size_m.iter().any(|axis| !axis.is_finite() || *axis < MIN_EDGE_M) {
            return Err(TopologyError::Degenerate);
        }
        self.validate()?;
        Ok(TopologyEdit { body: self, shift, size_m })
    }

    fn recenter(&mut self) -> [f64; 3] {
        let Some((min, max)) = self.bounds() else { return [0.0; 3] };
        let center = midpoint(min, max);
        if center.iter().all(|axis| axis.abs() < 1.0e-15) {
            return [0.0; 3];
        }
        for vertex in &mut self.vertices {
            vertex.position = sub(vertex.position, center);
        }
        center
    }

    fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        if self.vertices.is_empty() {
            return None;
        }
        for vertex in &self.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.position[axis]);
                max[axis] = max[axis].max(vertex.position[axis]);
            }
        }
        Some((min, max))
    }

    fn alloc(&mut self) -> Result<u32, TopologyError> {
        if self.next_id == 0 || self.next_id == u32::MAX {
            return Err(TopologyError::Degenerate);
        }
        let id = self.next_id;
        self.next_id += 1;
        Ok(id)
    }

    fn edge_ids(&self, id: u32) -> Option<(u32, u32)> {
        self.edges.iter().find(|edge| edge.id == id).map(|edge| (edge.a, edge.b))
    }

    fn translate(&mut self, id: u32, delta: [f64; 3]) -> Result<(), TopologyError> {
        let vertex = self.vertices.iter_mut().find(|vertex| vertex.id == id).ok_or(TopologyError::Missing)?;
        vertex.position = add(vertex.position, delta);
        Ok(())
    }

    fn loop_positions(&self, face: &SolidFace) -> Vec<[f64; 3]> {
        self.positions_of(&face.vertices)
    }

    fn positions_of(&self, ids: &[u32]) -> Vec<[f64; 3]> {
        ids.iter().filter_map(|id| self.vertex_position(*id)).collect()
    }

    fn incident_face_indexes(&self, a: u32, b: u32) -> Vec<usize> {
        self.faces
            .iter()
            .enumerate()
            .filter(|(_, face)| {
                let count = face.vertices.len();
                (0..count).any(|index| {
                    let start = face.vertices[index];
                    let end = face.vertices[(index + 1) % count];
                    (start == a && end == b) || (start == b && end == a)
                })
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn split_edge_chain(&mut self, from: u32, to: u32, points: &[u32], skip: Option<u32>) -> Result<(), TopologyError> {
        if points.is_empty() {
            return Ok(());
        }
        let edge_index = self
            .edges
            .iter()
            .position(|edge| (edge.a == from && edge.b == to) || (edge.a == to && edge.b == from))
            .ok_or(TopologyError::Missing)?;
        let stored_a = self.edges[edge_index].a;
        let stored_b = self.edges[edge_index].b;
        let mut ordered = points.to_vec();
        if stored_a == to && stored_b == from {
            ordered.reverse();
        } else if !(stored_a == from && stored_b == to) {
            return Err(TopologyError::Torn);
        }
        let mut sequence = Vec::with_capacity(ordered.len() + 2);
        sequence.push(stored_a);
        sequence.extend(ordered.iter().copied());
        sequence.push(stored_b);
        self.edges[edge_index].a = sequence[0];
        self.edges[edge_index].b = sequence[1];
        for index in 1..sequence.len() - 1 {
            let id = self.alloc()?;
            self.edges.push(SolidEdge { id, a: sequence[index], b: sequence[index + 1] });
        }
        for face in &mut self.faces {
            if Some(face.id) == skip {
                continue;
            }
            insert_chain(&mut face.vertices, stored_a, stored_b, &ordered);
        }
        Ok(())
    }

    fn ensure_edge(&mut self, a: u32, b: u32) -> Result<u32, TopologyError> {
        if a == b {
            return Err(TopologyError::Degenerate);
        }
        if let Some(id) = self.edge_between(a, b) {
            return Ok(id);
        }
        let id = self.alloc()?;
        self.edges.push(SolidEdge { id, a, b });
        Ok(id)
    }

    fn reject_t_junctions(&self) -> Result<(), TopologyError> {
        for vertex in &self.vertices {
            for edge in &self.edges {
                if edge.a == vertex.id || edge.b == vertex.id {
                    continue;
                }
                let (start, end) = (self.vertex_position(edge.a).unwrap_or([0.0; 3]), self.vertex_position(edge.b).unwrap_or([0.0; 3]));
                let along = project_segment(vertex.position, start, end);
                if along > MIN_EDGE_M && along < distance(start, end) - MIN_EDGE_M && point_segment_distance(vertex.position, start, end) < MIN_EDGE_M {
                    return Err(TopologyError::Torn);
                }
            }
        }
        Ok(())
    }

    fn reject_pierces(&self) -> Result<(), TopologyError> {
        for edge in &self.edges {
            let (start, end) = (self.vertex_position(edge.a).unwrap_or([0.0; 3]), self.vertex_position(edge.b).unwrap_or([0.0; 3]));
            self.segment_clear(edge.a, edge.b, start, end)?;
        }
        for face in &self.faces {
            let count = face.vertices.len();
            if count < 4 {
                continue;
            }
            let origin = face.vertices[0];
            let origin_at = self.vertex_position(origin).unwrap_or([0.0; 3]);
            for index in 2..count - 1 {
                let other = face.vertices[index];
                let other_at = self.vertex_position(other).unwrap_or([0.0; 3]);
                self.segment_clear(origin, other, origin_at, other_at)?;
            }
        }
        Ok(())
    }

    fn segment_clear(&self, a: u32, b: u32, start: [f64; 3], end: [f64; 3]) -> Result<(), TopologyError> {
        for face in &self.faces {
            if face.vertices.contains(&a) || face.vertices.contains(&b) {
                continue;
            }
            if segment_hits_face(start, end, &self.loop_positions(face)) {
                return Err(TopologyError::Torn);
            }
        }
        Ok(())
    }
}

fn insert_chain(loop_: &mut Vec<u32>, a: u32, b: u32, points_a_to_b: &[u32]) {
    let count = loop_.len();
    let mut slot = None;
    let mut forward = true;
    for index in 0..count {
        let start = loop_[index];
        let end = loop_[(index + 1) % count];
        if start == a && end == b {
            slot = Some(index);
            forward = true;
            break;
        }
        if start == b && end == a {
            slot = Some(index);
            forward = false;
            break;
        }
    }
    let Some(index) = slot else { return };
    let mut points = points_a_to_b.to_vec();
    if !forward {
        points.reverse();
    }
    for (offset, id) in points.into_iter().enumerate() {
        loop_.insert(index + 1 + offset, id);
    }
}

fn usable_delta(delta: [f64; 3]) -> bool {
    delta.iter().all(|axis| axis.is_finite()) && length(delta) >= MIN_DELTA_M
}

fn bilinear(corners: [[f64; 3]; 4], s: f64, t: f64) -> [f64; 3] {
    let weight = [(1.0 - s) * (1.0 - t), s * (1.0 - t), s * t, (1.0 - s) * t];
    let mut position = [0.0; 3];
    for corner in 0..4 {
        for axis in 0..3 {
            position[axis] += weight[corner] * corners[corner][axis];
        }
    }
    position
}

fn newell(positions: &[[f64; 3]]) -> [f64; 3] {
    let mut normal = [0.0; 3];
    if positions.len() < 3 {
        return normal;
    }
    for index in 0..positions.len() {
        let current = positions[index];
        let next = positions[(index + 1) % positions.len()];
        normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
        normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
        normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
    }
    normal
}

fn newell_length(positions: &[[f64; 3]]) -> f64 {
    length(newell(positions))
}

fn ray_segment(origin: [f64; 3], direction: [f64; 3], a: [f64; 3], b: [f64; 3]) -> (f64, f64) {
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

fn ray_triangle(origin: [f64; 3], direction: [f64; 3], triangle: [[f64; 3]; 3]) -> Option<f64> {
    let edge_u = sub(triangle[1], triangle[0]);
    let edge_v = sub(triangle[2], triangle[0]);
    let p = cross(direction, edge_v);
    let det = dot(edge_u, p);
    if det.abs() < PARALLEL_EPS {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = sub(origin, triangle[0]);
    let u = dot(tvec, p) * inv;
    if u < 0.0 || u > 1.0 {
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

/// A segment through the face, including a fan diagonal. A graze of the polygon boundary is not a pierce.
fn segment_hits_face(start: [f64; 3], end: [f64; 3], positions: &[[f64; 3]]) -> bool {
    if positions.len() < 3 {
        return false;
    }
    let direction = sub(end, start);
    for index in 1..positions.len() - 1 {
        let triangle = [positions[0], positions[index], positions[index + 1]];
        let edge_u = sub(triangle[1], triangle[0]);
        let edge_v = sub(triangle[2], triangle[0]);
        let p = cross(direction, edge_v);
        let det = dot(edge_u, p);
        if det.abs() < PARALLEL_EPS {
            continue;
        }
        let inv = 1.0 / det;
        let tvec = sub(start, triangle[0]);
        let u = dot(tvec, p) * inv;
        let q = cross(tvec, edge_u);
        let v = dot(direction, q) * inv;
        let t = dot(edge_v, q) * inv;
        if t <= INTERIOR_EPS || t >= 1.0 - INTERIOR_EPS {
            continue;
        }
        if u < -INTERIOR_EPS || v < -INTERIOR_EPS || u + v > 1.0 + INTERIOR_EPS {
            continue;
        }
        let point = add(start, scale(direction, t));
        let on_boundary = (0..positions.len()).any(|slot| {
            point_segment_distance(point, positions[slot], positions[(slot + 1) % positions.len()]) <= INTERIOR_EPS
        });
        if !on_boundary {
            return true;
        }
    }
    false
}

fn point_segment_distance(point: [f64; 3], start: [f64; 3], end: [f64; 3]) -> f64 {
    let edge = sub(end, start);
    let len2 = dot(edge, edge);
    if len2 < 1.0e-18 {
        return distance(point, start);
    }
    let t = (dot(sub(point, start), edge) / len2).clamp(0.0, 1.0);
    distance(point, add(start, scale(edge, t)))
}

fn project_segment(point: [f64; 3], start: [f64; 3], end: [f64; 3]) -> f64 {
    let edge = sub(end, start);
    let len = length(edge);
    if len < 1.0e-12 {
        return 0.0;
    }
    dot(sub(point, start), scale(edge, 1.0 / len))
}

fn pick_order(left: &TopologyPick, right: &TopologyPick) -> std::cmp::Ordering {
    left.ray_t.partial_cmp(&right.ray_t).unwrap_or(std::cmp::Ordering::Equal).then_with(|| left.distance.partial_cmp(&right.distance).unwrap_or(std::cmp::Ordering::Equal))
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
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn length(value: [f64; 3]) -> f64 {
    dot(value, value).sqrt()
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    length(sub(left, right))
}

fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    scale(add(left, right), 0.5)
}

fn unit(value: [f64; 3]) -> Option<[f64; 3]> {
    let span = length(value);
    if !span.is_finite() || span < 1.0e-12 {
        None
    } else {
        Some(scale(value, 1.0 / span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_body() -> SolidBody {
        SolidBody::from_box([2.0, 2.0, 2.0]).unwrap()
    }

    #[test]
    fn box_faces_point_outward() {
        let body = box_body();
        assert_eq!(body.vertices.len(), 8);
        assert_eq!(body.edges.len(), 12);
        assert_eq!(body.faces.len(), 6);
        assert_eq!(body.next_id, 21);
        let expected = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
        for (index, normal) in expected.iter().enumerate() {
            let got = body.unit_normal((index + 1) as u32).unwrap();
            for axis in 0..3 {
                assert!((got[axis] - normal[axis]).abs() < 1.0e-9, "face {} {got:?}", index + 1);
            }
        }
    }

    #[test]
    fn move_edge_turns_the_face_into_a_trapezoid_and_stays_closed() {
        let body = box_body();
        let edit = body.move_edge(12, [0.0, 0.0, -0.4]).unwrap();
        assert_eq!(edit.body.faces.len(), 6);
        assert_eq!(edit.body.edges.len(), 12);
        edit.body.validate().unwrap();
        let z7 = edit.body.vertex_position(7).unwrap()[2];
        let z5 = edit.body.vertex_position(5).unwrap()[2];
        assert!((z5 - z7 - 0.4).abs() < 1.0e-9, "{z5} {z7}");
        assert!((edit.shift[2]).abs() < 1.0e-9);
    }

    #[test]
    fn a_zero_move_or_a_collapsed_edge_is_refused_without_changing_the_body() {
        let body = box_body();
        assert_eq!(body.move_edge(12, [0.0, 0.0, 0.0]), Err(TopologyError::Degenerate));
        assert_eq!(body.move_edge(12, [0.0, -2.0, 0.0]), Err(TopologyError::Degenerate));
        assert_eq!(body.move_edge(99, [0.0, 0.2, 0.0]), Err(TopologyError::Missing));
        assert_eq!(body.vertices.len(), 8);
        assert!(body.validate().is_ok());
    }

    #[test]
    fn folding_an_edge_through_the_solid_is_refused() {
        let body = box_body();
        assert_eq!(body.move_edge(12, [0.0, 0.0, -3.0]), Err(TopologyError::Torn));
    }

    #[test]
    fn extrude_edge_adds_one_face_and_keeps_every_edge_shared() {
        let body = box_body();
        let edit = body.extrude_edge(12, [0.0, 0.4, 0.0]).unwrap();
        assert_eq!(edit.body.vertices.len(), 10);
        assert_eq!(edit.body.edges.len(), 15);
        assert_eq!(edit.body.faces.len(), 7);
        assert!((edit.size_m[1] - 2.4).abs() < 1.0e-9, "{:?}", edit.size_m);
        assert!((edit.shift[1] - 0.2).abs() < 1.0e-9, "{:?}", edit.shift);
        edit.body.validate().unwrap();
        let wall = edit.body.faces.iter().find(|face| face.id >= 21).unwrap();
        let normal = edit.body.unit_normal(wall.id).unwrap();
        assert!(normal[2] > 0.9, "{normal:?}");
    }

    #[test]
    fn split_edge_keeps_the_old_id_and_updates_both_faces() {
        let body = box_body();
        let edit = body.split_edge(12).unwrap();
        assert_eq!(edit.body.vertices.len(), 9);
        let edge = edit.body.edges.iter().find(|edge| edge.id == 12).unwrap();
        assert_eq!(edge.a, 7);
        assert!(edit.body.faces.iter().find(|face| face.id == 3).unwrap().vertices.contains(&edge.b));
        assert!(edit.body.faces.iter().find(|face| face.id == 5).unwrap().vertices.contains(&edge.b));
        assert_eq!(edit.body.faces.iter().find(|face| face.id == 2).unwrap().vertices, vec![1, 5, 7, 3]);
        edit.body.validate().unwrap();
        assert_eq!(edit.body.subdivide_face(3, 2, 2), Err(TopologyError::NotQuad));
    }

    #[test]
    fn subdivide_face_keeps_the_original_ids_and_the_boundary() {
        let body = box_body();
        let edit = body.subdivide_face(1, 2, 2).unwrap();
        assert_eq!(edit.body.faces.len(), 9);
        assert_eq!(edit.body.faces.iter().find(|face| face.id == 1).unwrap().vertices.len(), 4);
        assert!(edit.body.faces.iter().any(|face| face.id == 2));
        assert!(edit.body.faces.iter().any(|face| face.id == 6));
        let start = body.vertex_position(2).unwrap();
        let end = body.vertex_position(4).unwrap();
        let mid = midpoint(start, end);
        assert!(edit.body.vertices.iter().any(|vertex| distance(vertex.position, mid) < 1.0e-9));
        let untouched: Vec<u32> = edit.body.faces.iter().find(|face| face.id == 2).unwrap().vertices.clone();
        assert_eq!(untouched, vec![1, 5, 7, 3]);
        edit.body.validate().unwrap();
        assert_eq!(body.subdivide_face(1, 0, 2), Err(TopologyError::Degenerate));
        assert_eq!(body.subdivide_face(1, 9, 2), Err(TopologyError::Degenerate));
        let same = body.subdivide_face(1, 1, 1).unwrap();
        assert_eq!(same.body.vertices.len(), 8);
        assert_eq!(same.body.faces.len(), 6);
    }

    #[test]
    fn mirror_and_scale_keep_ids_and_the_outside() {
        let body = box_body();
        let mirrored = body.mirrored(0).unwrap();
        mirrored.body.validate().unwrap();
        let normal = mirrored.body.unit_normal(1).unwrap();
        assert!(normal[0] < -0.9, "{normal:?}");
        assert_eq!(mirrored.body.faces.len(), 6);
        let scaled = body.scale_to([4.0, 2.0, 2.0]).unwrap();
        assert_eq!(scaled.body.vertices.len(), 8);
        assert!((scaled.body.vertex_position(2).unwrap()[0] - 2.0).abs() < 1.0e-9);
        assert_eq!(scaled.body.edges.iter().find(|edge| edge.id == 12).unwrap().a, 7);
    }

    #[test]
    fn picking_hits_the_face_edge_and_vertex_under_the_ray() {
        let body = box_body();
        let face = body.pick_face([5.0, 0.0, 0.0], [-1.0, 0.0, 0.0]).unwrap();
        assert_eq!(face.id, 1);
        assert!((face.ray_t - 4.0).abs() < 1.0e-6);
        let edge = body.pick_edge([3.0, 0.0, -1.0], [-1.0, 0.0, 0.0], 0.05).unwrap();
        assert_eq!(edge.id, 14);
        assert!(edge.distance < 1.0e-6);
        let vertex = body.pick_vertex([1.0, -1.0, -5.0], [0.0, 0.0, 1.0], 0.05).unwrap();
        assert_eq!(vertex.id, 2);
    }

    #[test]
    fn derived_triangles_follow_the_closed_body() {
        let body = box_body();
        assert_eq!(crate::mesh_from_body(&body).triangle_indices().len(), 12);
        let moved = body.move_edge(12, [0.0, 0.0, -0.4]).unwrap();
        assert_eq!(crate::mesh_from_body(&moved.body).triangle_indices().len(), 12);
        let extruded = body.extrude_edge(12, [0.0, 0.4, 0.0]).unwrap();
        // The new quad is two triangles. Each side face gains the new vertex, so each gains one.
        assert_eq!(crate::mesh_from_body(&extruded.body).triangle_indices().len(), 16);
        let divided = body.subdivide_face(1, 2, 2).unwrap();
        // Four new quads replace two triangles, and each of the four neighbors becomes a pentagon.
        assert_eq!(crate::mesh_from_body(&divided.body).triangle_indices().len(), 22);
    }
}
