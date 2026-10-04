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
/// Step used to see whether a sweep walks into a neighbor or off its boundary.
const SWEEP_PROBE_M: f64 = 1.0e-3;
/// A pick this close to a stored edge still belongs to that face. Shared grid lines are not misses.
const PICK_BOUNDARY_M: f64 = 1.0e-5;

std::thread_local! {
    static LAST_SOLID_VALIDATE_US: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Microseconds spent in the last `validate` on this thread. The editor reads it after an edit.
pub fn last_solid_validate_us() -> u32 {
    LAST_SOLID_VALIDATE_US.with(|slot| slot.get())
}

/// Where a point sits relative to one face.
enum FacePlace {
    Outside,
    Boundary,
    Inside,
}
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
        let started = std::time::Instant::now();
        let result = self.validate_manifold();
        let micros = started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        LAST_SOLID_VALIDATE_US.with(|slot| slot.set(micros));
        result
    }

    fn validate_manifold(&self) -> Result<(), TopologyError> {
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

    /// Moves the selected faces by `delta` and keeps one closed solid.
    ///
    /// The selected ids stay on the caps. Each boundary vertex is duplicated once and shared by
    /// every selected face that used it. A vertex whose whole fan is selected moves in place.
    /// A sweep that leaves the neighbor grows a wall quad. A sweep that stays on that neighbor
    /// trims the neighbor to the new edge instead, so the two faces share it and the overlapped
    /// strip is not a second polygon.
    pub fn extrude_faces(&self, faces: &[u32], delta: [f64; 3]) -> Result<TopologyEdit, TopologyError> {
        if !usable_delta(delta) {
            return Err(TopologyError::Degenerate);
        }
        if faces.is_empty() {
            return Err(TopologyError::Missing);
        }
        let mut selected = Vec::with_capacity(faces.len());
        for id in faces {
            if *id == 0 || selected.contains(id) {
                return Err(TopologyError::Degenerate);
            }
            if self.faces.iter().all(|face| face.id != *id) {
                return Err(TopologyError::Missing);
            }
            selected.push(*id);
        }
        let mut body = self.clone();
        let mut boundary: Vec<(u32, u32, u32)> = Vec::new();
        let mut interior: Vec<u32> = Vec::new();
        for edge in &body.edges {
            let incidents = body.incident_face_indexes(edge.a, edge.b);
            let selected_hits = incidents.iter().filter(|index| selected.contains(&body.faces[**index].id)).count();
            if selected_hits == 0 {
                continue;
            }
            if selected_hits > 2 || incidents.len() != 2 {
                return Err(TopologyError::Torn);
            }
            if selected_hits == 2 {
                interior.push(edge.id);
                continue;
            }
            let face_index = incidents.iter().copied().find(|index| selected.contains(&body.faces[*index].id)).ok_or(TopologyError::Torn)?;
            let neighbor_index = incidents.into_iter().find(|index| !selected.contains(&body.faces[*index].id)).ok_or(TopologyError::Torn)?;
            let neighbor = body.faces[neighbor_index].id;
            let loop_ = &body.faces[face_index].vertices;
            let count = loop_.len();
            let directed = (0..count).find_map(|index| {
                let start = loop_[index];
                let end = loop_[(index + 1) % count];
                ((start == edge.a && end == edge.b) || (start == edge.b && end == edge.a)).then_some((start, end, neighbor))
            });
            boundary.push(directed.ok_or(TopologyError::Torn)?);
        }
        let mut degree: Vec<(u32, u32)> = Vec::new();
        for (start, end, _) in &boundary {
            for vertex in [*start, *end] {
                if let Some(slot) = degree.iter_mut().find(|(id, _)| *id == vertex) {
                    slot.1 += 1;
                } else {
                    degree.push((vertex, 1));
                }
            }
        }
        // A boundary vertex with any count other than two is a branch, a touch, or a crack.
        // One duplicate would then be asked to close more than two walls.
        if degree.iter().any(|(_, count)| *count != 2) {
            return Err(TopologyError::Torn);
        }
        let mut used = Vec::new();
        for id in &selected {
            let loop_ = body.face_loop(*id).ok_or(TopologyError::Missing)?;
            for vertex in loop_ {
                if !used.contains(vertex) {
                    used.push(*vertex);
                }
            }
        }
        let mut map: Vec<(u32, u32)> = Vec::with_capacity(used.len());
        for vertex in used {
            if degree.iter().any(|(id, _)| *id == vertex) {
                let position = add(body.vertex_position(vertex).ok_or(TopologyError::Missing)?, delta);
                // Landing on a corner that is already there reuses it. A fresh vertex on that
                // spot would be a zero-length edge or a T-junction.
                let existing = body
                    .vertices
                    .iter()
                    .find(|entry| entry.id != vertex && distance(entry.position, position) < MIN_EDGE_M)
                    .map(|entry| entry.id);
                if let Some(id) = existing {
                    map.push((vertex, id));
                } else {
                    let id = body.alloc()?;
                    body.vertices.push(SolidVertex { id, position });
                    map.push((vertex, id));
                }
            } else {
                body.translate(vertex, delta)?;
                map.push((vertex, vertex));
            }
        }
        let mapped = |map: &[(u32, u32)], id: u32| map.iter().find(|(old, _)| *old == id).map(|(_, new)| *new).ok_or(TopologyError::Torn);
        for face in &mut body.faces {
            if !selected.contains(&face.id) {
                continue;
            }
            for vertex in &mut face.vertices {
                *vertex = mapped(&map, *vertex)?;
            }
        }
        // Interior edges move with the caps. Leaving the old endpoints would orphan the record.
        for edge_id in interior {
            let edge = body.edges.iter_mut().find(|edge| edge.id == edge_id).ok_or(TopologyError::Torn)?;
            edge.a = mapped(&map, edge.a)?;
            edge.b = mapped(&map, edge.b)?;
        }
        let mut carve = Vec::new();
        let mut walls = Vec::new();
        for (start, end, neighbor) in boundary {
            let start_new = mapped(&map, start)?;
            let end_new = mapped(&map, end)?;
            if start_new == start || end_new == end || start_new == end_new || start_new == end || end_new == start {
                return Err(TopologyError::Degenerate);
            }
            let start_at = body.vertex_position(start).ok_or(TopologyError::Missing)?;
            let end_at = body.vertex_position(end).ok_or(TopologyError::Missing)?;
            let start_new_at = body.vertex_position(start_new).ok_or(TopologyError::Missing)?;
            let end_new_at = body.vertex_position(end_new).ok_or(TopologyError::Missing)?;
            if body.sweep_enters_face(neighbor, start_at, end_at, start_new_at, end_new_at) {
                let start_place = body.place_on_face(neighbor, start_new_at);
                let end_place = body.place_on_face(neighbor, end_new_at);
                if !matches!(start_place, FacePlace::Inside | FacePlace::Boundary) || !matches!(end_place, FacePlace::Inside | FacePlace::Boundary) {
                    // The sweep leaves this neighbor and enters the next face. Clipping across
                    // that next face is a different edit, so this one refuses.
                    return Err(TopologyError::Torn);
                }
                carve.push((neighbor, start, end, start_new, end_new));
            } else {
                walls.push((start, end, start_new, end_new));
            }
        }
        for (neighbor, start, end, start_new, end_new) in carve {
            body.carve_strip(neighbor, start, end, start_new, end_new)?;
        }
        body.remove_unused_edges();
        for (start, end, start_new, end_new) in walls {
            body.ensure_edge(start, end)?;
            body.ensure_edge(end, end_new)?;
            body.ensure_edge(end_new, start_new)?;
            body.ensure_edge(start_new, start)?;
            // [start, end, end', start'] opposes the neighbor on the old edge and the cap on the new one.
            // Reversing it would give both faces the same direction, which the closed-solid check rejects.
            let wall_id = body.alloc()?;
            body.faces.push(SolidFace { id: wall_id, vertices: vec![start, end, end_new, start_new] });
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

    /// Closest face the unit ray enters. A planar loop uses the polygon, so a concave deck does not fill its notch. A bent loop keeps the fan.
    pub fn pick_face(&self, origin: [f64; 3], direction: [f64; 3]) -> Option<TopologyPick> {
        let direction = unit(direction)?;
        let mut best: Option<TopologyPick> = None;
        for face in &self.faces {
            let positions = self.loop_positions(face);
            if positions.len() < 3 {
                continue;
            }
            let consider = |best: &mut Option<TopologyPick>, ray_t: f64, id: u32| {
                let pick = TopologyPick { id, ray_t, distance: 0.0 };
                let replace = best.as_ref().is_none_or(|current| {
                    let order = pick_order(&pick, current);
                    order.is_lt() || (order.is_eq() && pick.id < current.id)
                });
                if replace {
                    *best = Some(pick);
                }
            };
            let normal = newell(&positions);
            let span = length(normal);
            if span >= 1.0e-12 {
                let normal = scale(normal, 1.0 / span);
                if nearly_planar(&positions, normal) {
                    if let Some(ray_t) = ray_polygon(origin, direction, &positions, normal) {
                        consider(&mut best, ray_t, face.id);
                    }
                    continue;
                }
            }
            for index in 1..positions.len() - 1 {
                if let Some(ray_t) = ray_triangle(origin, direction, [positions[0], positions[index], positions[index + 1]]) {
                    consider(&mut best, ray_t, face.id);
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

    /// Drops the strip a coplanar sweep covers and leaves the neighbor on the new edge.
    fn carve_strip(&mut self, neighbor: u32, start: u32, end: u32, start_new: u32, end_new: u32) -> Result<(), TopologyError> {
        self.split_vertex_onto_edge(start_new)?;
        self.split_vertex_onto_edge(end_new)?;
        let current = self.faces.iter().find(|face| face.id == neighbor).ok_or(TopologyError::Missing)?.vertices.clone();
        let start_on = current.contains(&start_new);
        let end_on = current.contains(&end_new);
        let next = if start_on && end_on {
            remove_strip(&current, start_new, end_new, start, end)?
        } else if !start_on && !end_on {
            splice_detour(&current, end, start, end_new, start_new)?
        } else {
            return Err(TopologyError::Torn);
        };
        if next.len() < 3 {
            return Err(TopologyError::Torn);
        }
        let face = self.faces.iter_mut().find(|face| face.id == neighbor).ok_or(TopologyError::Missing)?;
        face.vertices = next;
        self.ensure_edge(start_new, end_new)?;
        if !start_on {
            self.ensure_edge(end, end_new)?;
            self.ensure_edge(start, start_new)?;
        }
        Ok(())
    }

    /// Inserts `vertex` into the one edge it already lies on. A corner match is left alone.
    fn split_vertex_onto_edge(&mut self, vertex: u32) -> Result<(), TopologyError> {
        let position = self.vertex_position(vertex).ok_or(TopologyError::Missing)?;
        let mut host = None;
        for edge in &self.edges {
            if edge.a == vertex || edge.b == vertex {
                continue;
            }
            let (Some(start), Some(end)) = (self.vertex_position(edge.a), self.vertex_position(edge.b)) else {
                continue;
            };
            let along = project_segment(position, start, end);
            let span = distance(start, end);
            if along > MIN_EDGE_M && along < span - MIN_EDGE_M && point_segment_distance(position, start, end) < MIN_EDGE_M {
                if host.is_some() {
                    return Err(TopologyError::Torn);
                }
                host = Some((edge.a, edge.b));
            }
        }
        if let Some((start, end)) = host {
            self.split_edge_chain(start, end, &[vertex], None)?;
        }
        Ok(())
    }

    fn remove_unused_edges(&mut self) {
        let mut used = Vec::new();
        for face in &self.faces {
            let count = face.vertices.len();
            for index in 0..count {
                let start = face.vertices[index];
                let end = face.vertices[(index + 1) % count];
                used.push((start.min(end), start.max(end)));
            }
        }
        self.edges.retain(|edge| used.contains(&(edge.a.min(edge.b), edge.a.max(edge.b))));
    }

    /// The sweep steps into `face` rather than off its boundary into empty space.
    fn sweep_enters_face(&self, face: u32, start: [f64; 3], end: [f64; 3], start_new: [f64; 3], end_new: [f64; 3]) -> bool {
        let old_mid = midpoint(start, end);
        let new_mid = midpoint(start_new, end_new);
        let step = sub(new_mid, old_mid);
        let span = length(step);
        if span < MIN_DELTA_M {
            return false;
        }
        let probe = if span <= SWEEP_PROBE_M { new_mid } else { add(old_mid, scale(step, SWEEP_PROBE_M / span)) };
        matches!(self.place_on_face(face, probe), FacePlace::Inside | FacePlace::Boundary)
    }

    fn place_on_face(&self, face: u32, point: [f64; 3]) -> FacePlace {
        let Some(positions) = self.face_positions(face) else {
            return FacePlace::Outside;
        };
        if positions.len() < 3 {
            return FacePlace::Outside;
        }
        let normal = newell(&positions);
        let span = length(normal);
        if span < 1.0e-12 {
            return FacePlace::Outside;
        }
        let normal = scale(normal, 1.0 / span);
        if dot(sub(point, positions[0]), normal).abs() > MIN_EDGE_M {
            return FacePlace::Outside;
        }
        let on_edge = (0..positions.len()).any(|index| {
            let start = positions[index];
            let end = positions[(index + 1) % positions.len()];
            let along = project_segment(point, start, end);
            let span = distance(start, end);
            along >= -MIN_EDGE_M && along <= span + MIN_EDGE_M && point_segment_distance(point, start, end) <= MIN_EDGE_M
        });
        if on_edge {
            return FacePlace::Boundary;
        }
        if polygon_contains(&positions, point, normal) { FacePlace::Inside } else { FacePlace::Outside }
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

/// Replaces the path `new_a ... old vertices ... new_b` with the direct edge, keeping the other way around.
fn remove_strip(loop_: &[u32], new_a: u32, new_b: u32, old_a: u32, old_b: u32) -> Result<Vec<u32>, TopologyError> {
    let count = loop_.len();
    for index in 0..count {
        if loop_[index] != new_a && loop_[index] != new_b {
            continue;
        }
        let mut cursor = (index + 1) % count;
        let mut seen_old = 0u32;
        let mut guard = 0;
        while loop_[cursor] != new_a && loop_[cursor] != new_b {
            if loop_[cursor] == old_a || loop_[cursor] == old_b {
                seen_old += 1;
            }
            cursor = (cursor + 1) % count;
            guard += 1;
            if guard > count {
                break;
            }
        }
        if guard > count || loop_[cursor] == loop_[index] || seen_old < 2 {
            continue;
        }
        let mut next = Vec::new();
        let mut walk = cursor;
        loop {
            next.push(loop_[walk]);
            if walk == index {
                break;
            }
            walk = (walk + 1) % count;
            if next.len() > count {
                return Err(TopologyError::Torn);
            }
        }
        return Ok(next);
    }
    Err(TopologyError::Torn)
}

/// Replaces neighbor edge `end → start` with the three edges that walk around an interior strip.
fn splice_detour(loop_: &[u32], end: u32, start: u32, end_new: u32, start_new: u32) -> Result<Vec<u32>, TopologyError> {
    let count = loop_.len();
    let slot = (0..count).find(|index| loop_[*index] == end && loop_[(*index + 1) % count] == start).ok_or(TopologyError::Torn)?;
    let mut next = Vec::with_capacity(count + 2);
    for (index, vertex) in loop_.iter().copied().enumerate() {
        next.push(vertex);
        if index == slot {
            next.push(end_new);
            next.push(start_new);
        }
    }
    Ok(next)
}

fn nearly_planar(positions: &[[f64; 3]], normal: [f64; 3]) -> bool {
    let origin = positions[0];
    positions.iter().all(|point| dot(sub(*point, origin), normal).abs() <= 1.0e-4)
}

fn ray_polygon(origin: [f64; 3], direction: [f64; 3], positions: &[[f64; 3]], normal: [f64; 3]) -> Option<f64> {
    let denom = dot(normal, direction);
    if denom.abs() < PARALLEL_EPS {
        return None;
    }
    let ray_t = dot(sub(positions[0], origin), normal) / denom;
    if ray_t <= 1.0e-6 {
        return None;
    }
    let point = add(origin, scale(direction, ray_t));
    face_pick_contains(positions, point, normal).then_some(ray_t)
}

fn face_pick_contains(positions: &[[f64; 3]], point: [f64; 3], normal: [f64; 3]) -> bool {
    if polygon_contains(positions, point, normal) {
        return true;
    }
    let count = positions.len();
    (0..count).any(|index| point_segment_distance(point, positions[index], positions[(index + 1) % count]) <= PICK_BOUNDARY_M)
}

fn polygon_contains(positions: &[[f64; 3]], point: [f64; 3], normal: [f64; 3]) -> bool {
    let (axis_u, axis_v) = dominant_axes(normal);
    let target = [point[axis_u], point[axis_v]];
    let mut inside = false;
    let count = positions.len();
    for index in 0..count {
        let start = [positions[index][axis_u], positions[index][axis_v]];
        let end = [positions[(index + 1) % count][axis_u], positions[(index + 1) % count][axis_v]];
        let crosses = (start[1] > target[1]) != (end[1] > target[1]);
        if !crosses || (end[1] - start[1]).abs() < 1.0e-15 {
            continue;
        }
        let crossing = start[0] + (end[0] - start[0]) * (target[1] - start[1]) / (end[1] - start[1]);
        if target[0] < crossing {
            inside = !inside;
        }
    }
    inside
}

fn dominant_axes(normal: [f64; 3]) -> (usize, usize) {
    let ax = normal[0].abs();
    let ay = normal[1].abs();
    let az = normal[2].abs();
    if ax >= ay && ax >= az { (1, 2) } else if ay >= az { (0, 2) } else { (0, 1) }
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

    fn centroid(body: &SolidBody, face: u32) -> [f64; 3] {
        let positions = body.face_positions(face).unwrap();
        let mut sum = [0.0; 3];
        for position in &positions {
            sum = add(sum, *position);
        }
        scale(sum, 1.0 / positions.len() as f64)
    }

    fn top_faces(body: &SolidBody) -> Vec<u32> {
        body.faces.iter().filter(|face| body.unit_normal(face.id).unwrap()[1] > 0.9).map(|face| face.id).collect()
    }

    fn interior_top(body: &SolidBody) -> Vec<u32> {
        top_faces(body)
            .into_iter()
            .filter(|face| {
                body.face_positions(*face).unwrap().iter().all(|position| position[0].abs() <= 0.5 + 1.0e-6 && position[2].abs() <= 0.5 + 1.0e-6)
            })
            .collect()
    }

    fn shared_vertices(body: &SolidBody, left: u32, right: u32) -> usize {
        let loop_ = body.face_loop(left).unwrap();
        body.face_loop(right).unwrap().iter().filter(|vertex| loop_.contains(vertex)).count()
    }

    #[test]
    fn picking_a_subdivided_cell_hits_that_cell_and_its_shared_edge() {
        let divided = box_body().subdivide_face(3, 4, 4).unwrap();
        assert!(last_solid_validate_us() > 0);
        let cap = interior_top(&divided.body)[0];
        let center = centroid(&divided.body, cap);
        let hit = divided.body.pick_face([center[0], center[1] + 3.0, center[2]], [0.0, -1.0, 0.0]).unwrap();
        assert_eq!(hit.id, cap);
        let neighbor = top_faces(&divided.body).into_iter().find(|face| *face != cap && shared_vertices(&divided.body, cap, *face) == 2).unwrap();
        let shared: Vec<_> = divided
            .body
            .face_loop(cap)
            .unwrap()
            .iter()
            .copied()
            .filter(|vertex| divided.body.face_loop(neighbor).unwrap().contains(vertex))
            .collect();
        assert_eq!(shared.len(), 2);
        let start = divided.body.vertex_position(shared[0]).unwrap();
        let end = divided.body.vertex_position(shared[1]).unwrap();
        let mid = [(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5, (start[2] + end[2]) * 0.5];
        let edge_hit = divided.body.pick_face([mid[0], mid[1] + 3.0, mid[2]], [0.0, -1.0, 0.0]).unwrap();
        assert!(edge_hit.id == cap || edge_hit.id == neighbor, "{}", edge_hit.id);
        let raised = divided.body.extrude_faces(&[cap], [0.0, 0.4, 0.0]).unwrap();
        let cap_at = centroid(&raised.body, cap);
        assert_eq!(raised.body.pick_face([cap_at[0], cap_at[1] + 3.0, cap_at[2]], [0.0, -1.0, 0.0]).unwrap().id, cap);
        let side = raised.body.faces.iter().find(|face| face.id >= 21).unwrap().id;
        let side_at = centroid(&raised.body, side);
        let normal = raised.body.unit_normal(side).unwrap();
        let origin = [side_at[0] + normal[0] * 2.0, side_at[1] + normal[1] * 2.0, side_at[2] + normal[2] * 2.0];
        assert_eq!(raised.body.pick_face(origin, [-normal[0], -normal[1], -normal[2]]).unwrap().id, side);
    }

    #[test]
    fn extruding_one_face_of_a_box_adds_a_closed_cap_and_four_walls() {
        let body = box_body();
        let edit = body.extrude_faces(&[3], [0.0, 0.4, 0.0]).unwrap();
        assert_eq!(edit.body.vertices.len(), 12);
        assert_eq!(edit.body.edges.len(), 20);
        assert_eq!(edit.body.faces.len(), 10);
        assert!((edit.size_m[1] - 2.4).abs() < 1.0e-9, "{:?}", edit.size_m);
        assert!((edit.shift[1] - 0.2).abs() < 1.0e-9, "{:?}", edit.shift);
        edit.body.validate().unwrap();
        assert!(edit.body.face_loop(3).is_some());
        let side = edit.body.faces.iter().find(|face| face.id >= 21).unwrap().clone();
        assert_eq!(shared_vertices(&edit.body, 3, side.id), 2);
        let neighbor = edit.body.faces.iter().find(|face| face.id != 3 && face.id < 21 && shared_vertices(&edit.body, side.id, face.id) == 2).unwrap().id;
        assert_eq!(shared_vertices(&edit.body, 3, neighbor), 0);
        let boundary_y = side
            .vertices
            .iter()
            .filter(|vertex| edit.body.face_loop(neighbor).unwrap().contains(vertex))
            .map(|vertex| edit.body.vertex_position(*vertex).unwrap()[1])
            .sum::<f64>()
            / 2.0;
        assert!((centroid(&edit.body, 3)[1] - boundary_y - 0.4).abs() < 1.0e-9, "{}", centroid(&edit.body, 3)[1] - boundary_y);
        let normal = edit.body.unit_normal(side.id).unwrap();
        assert!(normal[1].abs() < 0.1 && dot(normal, centroid(&edit.body, side.id)) > 0.2, "{normal:?}");
        assert_eq!(body.extrude_faces(&[], [0.0, 0.4, 0.0]), Err(TopologyError::Missing));
        assert_eq!(body.extrude_faces(&[3, 3], [0.0, 0.4, 0.0]), Err(TopologyError::Degenerate));
        assert_eq!(body.extrude_faces(&[99], [0.0, 0.4, 0.0]), Err(TopologyError::Missing));
        assert_eq!(body.extrude_faces(&[3], [0.0, 0.0, 0.0]), Err(TopologyError::Degenerate));
        assert_eq!(body.extrude_faces(&[3], [0.0, -3.0, 0.0]), Err(TopologyError::Torn));
    }

    #[test]
    fn extruding_an_interior_cell_then_a_new_side_keeps_shared_edges() {
        let divided = box_body().subdivide_face(3, 4, 4).unwrap();
        assert_eq!(divided.body.faces.len(), 21);
        let interior = interior_top(&divided.body);
        assert_eq!(interior.len(), 4);
        let cap = interior[0];
        let neighbor = top_faces(&divided.body).into_iter().find(|face| *face != cap && shared_vertices(&divided.body, cap, *face) == 2).unwrap();
        let raised = divided.body.extrude_faces(&[cap], [0.0, 0.4, 0.0]).unwrap();
        assert_eq!(raised.body.faces.len(), 25);
        raised.body.validate().unwrap();
        assert!(raised.body.face_loop(cap).is_some());
        assert!((centroid(&raised.body, cap)[1] - centroid(&raised.body, neighbor)[1] - 0.4).abs() < 1.0e-6);
        let side = raised
            .body
            .faces
            .iter()
            .find(|face| shared_vertices(&raised.body, cap, face.id) == 2 && shared_vertices(&raised.body, neighbor, face.id) == 2)
            .unwrap();
        assert_eq!(shared_vertices(&raised.body, cap, neighbor), 0);
        let normal = raised.body.unit_normal(side.id).unwrap();
        let again = raised.body.extrude_faces(&[side.id], scale(normal, 0.4)).unwrap();
        again.body.validate().unwrap();
        assert!(again.body.faces.len() > raised.body.faces.len());
        assert!(again.body.face_loop(cap).is_some());
        assert!(again.body.face_loop(side.id).is_some());
        let deck = again
            .body
            .faces
            .iter()
            .find(|face| {
                face.id != side.id
                    && shared_vertices(&again.body, side.id, face.id) == 2
                    && again.body.unit_normal(face.id).unwrap()[1] > 0.9
                    && shared_vertices(&again.body, cap, face.id) == 0
            })
            .unwrap()
            .id;
        assert_eq!(shared_vertices(&again.body, side.id, deck), 2);
        assert_eq!(shared_vertices(&again.body, cap, side.id), 0);
        let shelf = again.body.faces.iter().find(|face| shared_vertices(&again.body, cap, face.id) == 2 && shared_vertices(&again.body, side.id, face.id) == 2).unwrap();
        assert!(shelf.id != side.id && shelf.id != cap);
        assert!(again.body.unit_normal(deck).unwrap()[1] > 0.9);
        assert_eq!(divided.body.extrude_faces(&[cap], [0.0, -3.0, 0.0]), Err(TopologyError::Torn));
        assert_eq!(raised.body.extrude_faces(&[side.id], scale(normal, -0.4)), Err(TopologyError::Torn));
    }

    #[test]
    fn extruding_a_new_side_of_a_raised_box_adds_another_shelf() {
        let raised = box_body().extrude_faces(&[3], [0.0, 0.4, 0.0]).unwrap();
        let side = raised.body.faces.iter().find(|face| face.id >= 21).unwrap().id;
        let normal = raised.body.unit_normal(side).unwrap();
        let again = raised.body.extrude_faces(&[side], scale(normal, 0.4)).unwrap();
        again.body.validate().unwrap();
        assert!(again.body.face_loop(3).is_some());
        assert!(again.body.face_loop(side).is_some());
        assert!(again.body.faces.len() > raised.body.faces.len());
        assert_eq!(shared_vertices(&again.body, 3, side), 0);
    }

    #[test]
    fn extruding_the_four_central_cells_moves_their_shared_vertex_once() {
        let divided = box_body().subdivide_face(3, 4, 4).unwrap();
        let caps = interior_top(&divided.body);
        assert_eq!(caps.len(), 4);
        let shared = divided
            .body
            .vertices
            .iter()
            .find(|vertex| caps.iter().all(|face| divided.body.face_loop(*face).unwrap().contains(&vertex.id)))
            .unwrap()
            .id;
        let before = divided.body.vertex_position(1).unwrap()[1];
        let edit = divided.body.extrude_faces(&caps, [0.0, 0.4, 0.0]).unwrap();
        edit.body.validate().unwrap();
        assert!(caps.iter().all(|face| edit.body.face_loop(*face).unwrap().contains(&shared)));
        let rise = edit.body.vertex_position(shared).unwrap()[1] - edit.body.vertex_position(1).unwrap()[1];
        let original = divided.body.vertex_position(shared).unwrap()[1] - before;
        assert!((rise - original - 0.4).abs() < 1.0e-6, "{rise} {original}");
    }

    #[test]
    fn extruding_two_cells_that_only_share_a_vertex_is_refused() {
        let divided = box_body().subdivide_face(3, 2, 2).unwrap();
        let tops = top_faces(&divided.body);
        let pair = tops.iter().copied().find_map(|left| {
            tops.iter().copied().find(|right| *right != left && shared_vertices(&divided.body, left, *right) == 1).map(|right| (left, right))
        });
        let (left, right) = pair.unwrap();
        assert_eq!(divided.body.extrude_faces(&[left, right], [0.0, 0.4, 0.0]), Err(TopologyError::Torn));
    }
}
