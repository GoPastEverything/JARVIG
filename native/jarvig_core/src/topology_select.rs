//! Read-only walks on an authored solid.
//!
//! Loop, ring, connected, boundary, grow, and shrink name stored faces, edges,
//! and vertices. They do not read triangles, meshlets, or a camera, and they
//! do not write the body.

use crate::topology::SolidBody;

/// Ids a walk accepted, in visit order, plus why it stopped when it did not finish cleanly.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementWalk {
    pub ids: Vec<u32>,
    pub note: Option<WalkNote>,
}

/// Why a walk stopped or refused to choose. A clean closed walk has no note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkNote {
    /// The vertex does not have four edges, so there is no opposite.
    Pole,
    /// More than one edge could continue, or the edge is non-manifold.
    Ambiguous,
    /// The ring crossed a face that does not have four edges.
    NotQuad,
    /// The edge has fewer than two faces.
    OpenEdge,
    /// Every selected edge has two faces inside the region.
    NoBoundary,
    /// A boundary vertex is not in exactly two boundary edges.
    BranchedBoundary,
    /// Two loops enclose almost the same area.
    AmbiguousOuter,
    /// The selected faces do not agree on an outward direction.
    FoldedRegion,
    /// The largest loop was kept. The count is the loops that were left out.
    HolesLeft(u32),
    /// None of the named elements are on this solid.
    Missing,
}

impl WalkNote {
    /// One sentence for the editor log.
    pub fn sentence(&self) -> String {
        match self {
            Self::Pole => "The loop stopped at a pole.".into(),
            Self::Ambiguous => "The walk stopped where the next edge was not unique.".into(),
            Self::NotQuad => "The ring stopped at a face that is not a quad.".into(),
            Self::OpenEdge => "The walk stopped at an open edge.".into(),
            Self::NoBoundary => "That region has no boundary.".into(),
            Self::BranchedBoundary => "The boundary does not form a closed loop, so every boundary edge is selected.".into(),
            Self::AmbiguousOuter => "More than one loop could be the outer boundary, so every boundary edge is selected.".into(),
            Self::FoldedRegion => "The region folds, so every boundary edge is selected.".into(),
            Self::HolesLeft(1) => "The outer boundary is selected. 1 hole stayed out.".into(),
            Self::HolesLeft(count) => format!("The outer boundary is selected. {count} holes stayed out."),
            Self::Missing => "That element is not on the solid.".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Closed,
    Pole,
    Ambiguous,
    NotQuad,
    OpenEdge,
}

fn stop_note(stop: Stop) -> Option<WalkNote> {
    match stop {
        Stop::Closed => None,
        Stop::Pole => Some(WalkNote::Pole),
        Stop::Ambiguous => Some(WalkNote::Ambiguous),
        Stop::NotQuad => Some(WalkNote::NotQuad),
        Stop::OpenEdge => Some(WalkNote::OpenEdge),
    }
}

fn note_rank(note: WalkNote) -> u8 {
    match note {
        WalkNote::Missing => 10,
        WalkNote::Ambiguous => 9,
        WalkNote::BranchedBoundary => 8,
        WalkNote::AmbiguousOuter => 7,
        WalkNote::FoldedRegion => 6,
        WalkNote::NoBoundary => 5,
        WalkNote::OpenEdge => 4,
        WalkNote::NotQuad => 3,
        WalkNote::Pole => 2,
        WalkNote::HolesLeft(_) => 1,
    }
}

fn absorb(note: &mut Option<WalkNote>, extra: Option<WalkNote>) {
    let Some(extra) = extra else { return };
    match note {
        Some(current) if note_rank(extra) <= note_rank(*current) => {}
        _ => *note = Some(extra),
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(value: [f64; 3]) -> f64 {
    dot(value, value).sqrt()
}

fn newell(points: &[[f64; 3]]) -> [f64; 3] {
    let mut normal = [0.0; 3];
    for index in 0..points.len() {
        let start = points[index];
        let end = points[(index + 1) % points.len()];
        normal[0] += (start[1] - end[1]) * (start[2] + end[2]);
        normal[1] += (start[2] - end[2]) * (start[0] + end[0]);
        normal[2] += (start[0] - end[0]) * (start[1] + end[1]);
    }
    normal
}

impl SolidBody {
    /// Continuous edge loop through valence-4 vertices. A cube corner is valence 3 and stops.
    pub fn edge_loop(&self, seeds: &[u32]) -> ElementWalk {
        let (present, missing) = self.present_edges(seeds);
        if present.is_empty() {
            return empty_walk(missing);
        }
        let mut ids = Vec::new();
        let mut note = None;
        for seed in present {
            if ids.contains(&seed) {
                continue;
            }
            let (part, stop) = self.one_loop(seed);
            for id in part {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            absorb(&mut note, stop_note(stop));
        }
        ElementWalk { ids, note }
    }

    /// Opposite edges across faces that have exactly four edges.
    pub fn edge_ring(&self, seeds: &[u32]) -> ElementWalk {
        let (present, missing) = self.present_edges(seeds);
        if present.is_empty() {
            return empty_walk(missing);
        }
        let mut ids = Vec::new();
        let mut note = None;
        for seed in present {
            if ids.contains(&seed) {
                continue;
            }
            let (part, stop) = self.one_ring(seed);
            for id in part {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            absorb(&mut note, stop_note(stop));
        }
        ElementWalk { ids, note }
    }

    /// Faces reachable across a shared edge.
    pub fn connected_faces(&self, seeds: &[u32]) -> ElementWalk {
        self.connected(seeds, |body, id| body.face_loop(id).is_some(), |body, id, ids| {
            let Some(loop_) = body.face_loop(id) else { return };
            let count = loop_.len();
            for index in 0..count {
                let Some(edge) = body.edge_between(loop_[index], loop_[(index + 1) % count]) else { continue };
                for face in body.faces_of_edge(edge) {
                    if !ids.contains(&face) {
                        ids.push(face);
                    }
                }
            }
        })
    }

    /// Edges reachable across a shared vertex.
    pub fn connected_edges(&self, seeds: &[u32]) -> ElementWalk {
        self.connected(seeds, |body, id| body.edge_record(id).is_some(), |body, id, ids| {
            let Some((start, end)) = body.edge_vertex_ids(id) else { return };
            for vertex in [start, end] {
                for edge in body.incident_edges(vertex) {
                    if !ids.contains(&edge) {
                        ids.push(edge);
                    }
                }
            }
        })
    }

    /// Vertices reachable across an edge.
    pub fn connected_vertices(&self, seeds: &[u32]) -> ElementWalk {
        self.connected(seeds, |body, id| body.vertex_position(id).is_some(), |body, id, ids| {
            for edge in body.incident_edges(id) {
                let Some((start, end)) = body.edge_vertex_ids(edge) else { continue };
                for vertex in [start, end] {
                    if !ids.contains(&vertex) {
                        ids.push(vertex);
                    }
                }
            }
        })
    }

    /// Boundary edges of a face region. One outer loop when the holes are smaller and the region does not fold.
    pub fn region_boundary(&self, faces: &[u32]) -> ElementWalk {
        let (selected, missing) = self.present_faces(faces);
        if selected.is_empty() {
            return empty_walk(missing);
        }
        let boundary = self.boundary_edges(&selected);
        if boundary.is_empty() {
            return ElementWalk { ids: Vec::new(), note: Some(WalkNote::NoBoundary) };
        }
        if !self.boundary_is_cycles(&boundary) {
            return ElementWalk { ids: boundary, note: Some(WalkNote::BranchedBoundary) };
        }
        let Some(loops) = self.chain_loops(&boundary) else {
            return ElementWalk { ids: boundary, note: Some(WalkNote::BranchedBoundary) };
        };
        if loops.len() == 1 {
            return ElementWalk { ids: loops.into_iter().next().unwrap_or_default(), note: None };
        }
        let Some(normal) = self.region_normal(&selected) else {
            return ElementWalk { ids: boundary, note: Some(WalkNote::FoldedRegion) };
        };
        let mut areas: Vec<(usize, f64)> = loops
            .iter()
            .enumerate()
            .map(|(index, loop_)| (index, self.loop_abs_area(loop_, normal)))
            .collect();
        areas.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(std::cmp::Ordering::Equal));
        let largest = areas.first().map(|(_, area)| *area).unwrap_or(0.0);
        let second = areas.get(1).map(|(_, area)| *area).unwrap_or(0.0);
        if largest < 1.0e-12 || second >= largest * 0.95 {
            return ElementWalk { ids: boundary, note: Some(WalkNote::AmbiguousOuter) };
        }
        let outer = areas[0].0;
        let holes = (loops.len() - 1) as u32;
        ElementWalk { ids: loops.into_iter().nth(outer).unwrap_or_default(), note: Some(WalkNote::HolesLeft(holes)) }
    }

    /// One layer of faces that share an edge with the selection.
    pub fn grow_faces(&self, seeds: &[u32]) -> ElementWalk {
        self.grow(seeds, |body, id| body.face_loop(id).is_some(), |body, id| body.face_neighbors(id))
    }

    /// One layer of edges that share a vertex with the selection.
    pub fn grow_edges(&self, seeds: &[u32]) -> ElementWalk {
        self.grow(seeds, |body, id| body.edge_record(id).is_some(), |body, id| body.edge_neighbors(id))
    }

    /// One layer of vertices joined to the selection by an edge.
    pub fn grow_vertices(&self, seeds: &[u32]) -> ElementWalk {
        self.grow(seeds, |body, id| body.vertex_position(id).is_some(), |body, id| body.vertex_neighbors(id))
    }

    /// Faces whose every edge-neighbor is already selected. A lone face shrinks away.
    pub fn shrink_faces(&self, seeds: &[u32]) -> ElementWalk {
        self.shrink(seeds, |body, id| body.face_loop(id).is_some(), |body, id| body.face_neighbors(id))
    }

    /// Edges whose every vertex-neighbor is already selected. One edge, or a loop one edge wide, shrinks away.
    pub fn shrink_edges(&self, seeds: &[u32]) -> ElementWalk {
        self.shrink(seeds, |body, id| body.edge_record(id).is_some(), |body, id| body.edge_neighbors(id))
    }

    /// Vertices whose every neighbor is already selected.
    pub fn shrink_vertices(&self, seeds: &[u32]) -> ElementWalk {
        self.shrink(seeds, |body, id| body.vertex_position(id).is_some(), |body, id| body.vertex_neighbors(id))
    }

    fn one_loop(&self, seed: u32) -> (Vec<u32>, Stop) {
        let Some(edge) = self.edge_record(seed) else {
            return (Vec::new(), Stop::Ambiguous);
        };
        let (forward, forward_stop) = self.walk_loop(seed, edge.b, &[seed]);
        let (backward, backward_stop) = if forward_stop == Stop::Closed {
            (Vec::new(), Stop::Closed)
        } else {
            let mut blocked = vec![seed];
            blocked.extend(forward.iter().copied());
            self.walk_loop(seed, edge.a, &blocked)
        };
        let mut ids = backward;
        ids.reverse();
        ids.push(seed);
        ids.extend(forward);
        (ids, stronger_stop(forward_stop, backward_stop))
    }

    fn walk_loop(&self, seed: u32, start_vertex: u32, blocked: &[u32]) -> (Vec<u32>, Stop) {
        let mut chain = Vec::new();
        let mut current = seed;
        let mut at = start_vertex;
        let limit = self.edges.len().saturating_add(1);
        for _ in 0..limit {
            let next = match self.loop_next(current, at) {
                Ok(next) => next,
                Err(stop) => return (chain, stop),
            };
            if next == seed || chain.contains(&next) || blocked.contains(&next) {
                return (chain, Stop::Closed);
            }
            let Some(next_at) = self.other_vertex(next, at) else {
                return (chain, Stop::Ambiguous);
            };
            chain.push(next);
            current = next;
            at = next_at;
        }
        (chain, Stop::Ambiguous)
    }

    /// The edge opposite `current` at `at`. Valence other than 4 is a pole. Anything else that is not one remaining edge is ambiguous.
    fn loop_next(&self, current: u32, at: u32) -> Result<u32, Stop> {
        match self.faces_of_edge(current).len() {
            0 | 1 => return Err(Stop::OpenEdge),
            2 => {}
            _ => return Err(Stop::Ambiguous),
        }
        let incident = self.incident_edges(at);
        if !incident.contains(&current) {
            return Err(Stop::Ambiguous);
        }
        if incident.len() != 4 {
            return Err(Stop::Pole);
        }
        let others: Vec<u32> = incident.into_iter().filter(|edge| *edge != current).collect();
        let sharing: Vec<u32> = others.iter().copied().filter(|edge| self.edges_share_face(current, *edge)).collect();
        if sharing.len() != 2 {
            return Err(Stop::Ambiguous);
        }
        let rest: Vec<u32> = others.into_iter().filter(|edge| !sharing.contains(edge)).collect();
        match rest.as_slice() {
            [next] => Ok(*next),
            _ => Err(Stop::Ambiguous),
        }
    }

    fn one_ring(&self, seed: u32) -> (Vec<u32>, Stop) {
        let faces = self.faces_of_edge(seed);
        if faces.is_empty() {
            return (vec![seed], Stop::OpenEdge);
        }
        if faces.len() > 2 {
            return (vec![seed], Stop::Ambiguous);
        }
        let mut directions = Vec::new();
        for face in faces {
            directions.push(self.opposite_on_face(face, seed).map(|edge| (edge, face)));
        }
        let (forward, forward_stop) = match directions.first() {
            Some(Ok((next, face))) => self.walk_ring(seed, *next, *face, &[seed]),
            Some(Err(stop)) => (Vec::new(), *stop),
            None => (Vec::new(), Stop::OpenEdge),
        };
        let (backward, backward_stop) = if forward_stop == Stop::Closed {
            (Vec::new(), Stop::Closed)
        } else {
            match directions.get(1) {
                Some(Ok((next, face))) => {
                    let mut blocked = vec![seed];
                    blocked.extend(forward.iter().copied());
                    self.walk_ring(seed, *next, *face, &blocked)
                }
                Some(Err(stop)) => (Vec::new(), *stop),
                None => (Vec::new(), Stop::OpenEdge),
            }
        };
        let mut ids = backward;
        ids.reverse();
        ids.push(seed);
        ids.extend(forward);
        (ids, stronger_stop(forward_stop, backward_stop))
    }

    fn walk_ring(&self, seed: u32, first: u32, came_from: u32, blocked: &[u32]) -> (Vec<u32>, Stop) {
        if first == seed || blocked.contains(&first) {
            return (Vec::new(), Stop::Closed);
        }
        let mut chain = Vec::new();
        let mut current = first;
        let mut came = came_from;
        let limit = self.edges.len().saturating_add(1);
        for _ in 0..limit {
            chain.push(current);
            match self.ring_next(current, came) {
                Ok((next, face)) => {
                    if next == seed || chain.contains(&next) || blocked.contains(&next) {
                        return (chain, Stop::Closed);
                    }
                    current = next;
                    came = face;
                }
                Err(stop) => return (chain, stop),
            }
        }
        (chain, Stop::Ambiguous)
    }

    fn ring_next(&self, current: u32, came_from: u32) -> Result<(u32, u32), Stop> {
        let choices: Vec<u32> = self.faces_of_edge(current).into_iter().filter(|face| *face != came_from).collect();
        match choices.as_slice() {
            [face] => {
                let next = self.opposite_on_face(*face, current)?;
                Ok((next, *face))
            }
            [] => Err(Stop::OpenEdge),
            _ => Err(Stop::Ambiguous),
        }
    }

    /// The one edge of a four-edge face that shares neither endpoint with `edge`.
    fn opposite_on_face(&self, face: u32, edge: u32) -> Result<u32, Stop> {
        let loop_ = self.face_loop(face).ok_or(Stop::NotQuad)?;
        if loop_.len() != 4 {
            return Err(Stop::NotQuad);
        }
        let (start, end) = self.edge_vertex_ids(edge).ok_or(Stop::Ambiguous)?;
        let mut opposite = Vec::new();
        for index in 0..4 {
            let Some(other) = self.edge_between(loop_[index], loop_[(index + 1) % 4]) else {
                return Err(Stop::Ambiguous);
            };
            if other == edge {
                continue;
            }
            let Some((left, right)) = self.edge_vertex_ids(other) else {
                return Err(Stop::Ambiguous);
            };
            let shares = left == start || left == end || right == start || right == end;
            if !shares {
                opposite.push(other);
            }
        }
        match opposite.as_slice() {
            [one] => Ok(*one),
            _ => Err(Stop::Ambiguous),
        }
    }

    fn connected(
        &self,
        seeds: &[u32],
        exists: impl Fn(&SolidBody, u32) -> bool,
        expand: impl Fn(&SolidBody, u32, &mut Vec<u32>),
    ) -> ElementWalk {
        let mut ids = Vec::new();
        let mut saw_present = false;
        let mut saw_missing = false;
        for seed in seeds {
            if *seed == 0 || !exists(self, *seed) {
                saw_missing = true;
                continue;
            }
            saw_present = true;
            if ids.contains(seed) {
                continue;
            }
            ids.push(*seed);
            let mut cursor = ids.len() - 1;
            while cursor < ids.len() {
                let id = ids[cursor];
                cursor += 1;
                expand(self, id, &mut ids);
            }
        }
        if !saw_present {
            return empty_walk(saw_missing || !seeds.is_empty());
        }
        ElementWalk { ids, note: None }
    }

    fn grow(
        &self,
        seeds: &[u32],
        exists: impl Fn(&SolidBody, u32) -> bool,
        neighbors: impl Fn(&SolidBody, u32) -> Vec<u32>,
    ) -> ElementWalk {
        let mut ids = Vec::new();
        let mut saw_present = false;
        let mut saw_missing = false;
        for seed in seeds {
            if *seed == 0 || !exists(self, *seed) {
                saw_missing = true;
                continue;
            }
            saw_present = true;
            if !ids.contains(seed) {
                ids.push(*seed);
            }
        }
        if !saw_present {
            return empty_walk(saw_missing || !seeds.is_empty());
        }
        let originals = ids.clone();
        for seed in originals {
            for neighbor in neighbors(self, seed) {
                if !ids.contains(&neighbor) {
                    ids.push(neighbor);
                }
            }
        }
        ElementWalk { ids, note: None }
    }

    fn shrink(
        &self,
        seeds: &[u32],
        exists: impl Fn(&SolidBody, u32) -> bool,
        neighbors: impl Fn(&SolidBody, u32) -> Vec<u32>,
    ) -> ElementWalk {
        let mut selected = Vec::new();
        let mut saw_present = false;
        let mut saw_missing = false;
        for seed in seeds {
            if *seed == 0 || !exists(self, *seed) {
                saw_missing = true;
                continue;
            }
            saw_present = true;
            if !selected.contains(seed) {
                selected.push(*seed);
            }
        }
        if !saw_present {
            return empty_walk(saw_missing || !seeds.is_empty());
        }
        let ids = selected
            .iter()
            .copied()
            .filter(|id| neighbors(self, *id).iter().all(|neighbor| selected.contains(neighbor)))
            .collect();
        ElementWalk { ids, note: None }
    }

    fn present_edges(&self, seeds: &[u32]) -> (Vec<u32>, bool) {
        let mut present = Vec::new();
        let mut missing = false;
        for seed in seeds {
            if self.edge_record(*seed).is_some() {
                if !present.contains(seed) {
                    present.push(*seed);
                }
            } else {
                missing = true;
            }
        }
        (present, missing)
    }

    fn present_faces(&self, seeds: &[u32]) -> (Vec<u32>, bool) {
        let mut present = Vec::new();
        let mut missing = false;
        for seed in seeds {
            if self.face_loop(*seed).is_some() {
                if !present.contains(seed) {
                    present.push(*seed);
                }
            } else {
                missing = true;
            }
        }
        (present, missing)
    }

    fn edge_record(&self, id: u32) -> Option<&crate::topology::SolidEdge> {
        self.edges.iter().find(|edge| edge.id == id)
    }

    fn edge_vertex_ids(&self, id: u32) -> Option<(u32, u32)> {
        self.edge_record(id).map(|edge| (edge.a, edge.b))
    }

    fn other_vertex(&self, edge: u32, vertex: u32) -> Option<u32> {
        let (start, end) = self.edge_vertex_ids(edge)?;
        if start == vertex {
            Some(end)
        } else if end == vertex {
            Some(start)
        } else {
            None
        }
    }

    fn incident_edges(&self, vertex: u32) -> Vec<u32> {
        self.edges.iter().filter(|edge| edge.a == vertex || edge.b == vertex).map(|edge| edge.id).collect()
    }

    fn edges_share_face(&self, left: u32, right: u32) -> bool {
        let faces = self.faces_of_edge(left);
        self.faces_of_edge(right).iter().any(|face| faces.contains(face))
    }

    fn face_neighbors(&self, face: u32) -> Vec<u32> {
        let Some(loop_) = self.face_loop(face) else { return Vec::new() };
        let mut neighbors = Vec::new();
        let count = loop_.len();
        for index in 0..count {
            let Some(edge) = self.edge_between(loop_[index], loop_[(index + 1) % count]) else { continue };
            for other in self.faces_of_edge(edge) {
                if other != face && !neighbors.contains(&other) {
                    neighbors.push(other);
                }
            }
        }
        neighbors
    }

    fn edge_neighbors(&self, edge: u32) -> Vec<u32> {
        let Some((start, end)) = self.edge_vertex_ids(edge) else { return Vec::new() };
        let mut neighbors = Vec::new();
        for vertex in [start, end] {
            for other in self.incident_edges(vertex) {
                if other != edge && !neighbors.contains(&other) {
                    neighbors.push(other);
                }
            }
        }
        neighbors
    }

    fn vertex_neighbors(&self, vertex: u32) -> Vec<u32> {
        let mut neighbors = Vec::new();
        for edge in self.incident_edges(vertex) {
            let Some((start, end)) = self.edge_vertex_ids(edge) else { continue };
            let other = if start == vertex { end } else { start };
            if other != vertex && !neighbors.contains(&other) {
                neighbors.push(other);
            }
        }
        neighbors
    }

    fn boundary_edges(&self, faces: &[u32]) -> Vec<u32> {
        self.edges
            .iter()
            .filter(|edge| {
                let count = self.faces_of_edge(edge.id).iter().filter(|face| faces.contains(face)).count();
                count == 1
            })
            .map(|edge| edge.id)
            .collect()
    }

    fn boundary_is_cycles(&self, edges: &[u32]) -> bool {
        let mut degrees: Vec<(u32, u32)> = Vec::new();
        for id in edges {
            let Some((start, end)) = self.edge_vertex_ids(*id) else { return false };
            for vertex in [start, end] {
                if let Some(slot) = degrees.iter_mut().find(|(stored, _)| *stored == vertex) {
                    slot.1 += 1;
                } else {
                    degrees.push((vertex, 1));
                }
            }
        }
        !degrees.is_empty() && degrees.iter().all(|(_, degree)| *degree == 2)
    }

    fn chain_loops(&self, edges: &[u32]) -> Option<Vec<Vec<u32>>> {
        let mut remaining = edges.to_vec();
        let mut loops = Vec::new();
        while !remaining.is_empty() {
            let start_index = remaining.iter().enumerate().min_by_key(|(_, id)| *id)?.0;
            let start = remaining.remove(start_index);
            let (origin, mut at) = self.edge_vertex_ids(start)?;
            let mut loop_edges = vec![start];
            let mut closed = false;
            for _ in 0..=edges.len() {
                if at == origin {
                    closed = true;
                    break;
                }
                let next_index = remaining.iter().position(|id| {
                    self.edge_vertex_ids(*id).is_some_and(|(start, end)| start == at || end == at)
                })?;
                let next = remaining.remove(next_index);
                let (start_vertex, end_vertex) = self.edge_vertex_ids(next)?;
                at = if start_vertex == at { end_vertex } else { start_vertex };
                loop_edges.push(next);
            }
            if !closed {
                return None;
            }
            loops.push(loop_edges);
        }
        Some(loops)
    }

    fn region_normal(&self, faces: &[u32]) -> Option<[f64; 3]> {
        let mut accum = [0.0; 3];
        for face in faces {
            let normal = self.unit_normal(*face)?;
            let area = self.face_area(*face)?;
            accum[0] += normal[0] * area;
            accum[1] += normal[1] * area;
            accum[2] += normal[2] * area;
        }
        let span = length(accum);
        if !span.is_finite() || span < 1.0e-8 {
            return None;
        }
        Some(scale(accum, 1.0 / span))
    }

    fn loop_points(&self, edges: &[u32]) -> Option<Vec<[f64; 3]>> {
        let (origin, mut at) = self.edge_vertex_ids(*edges.first()?)?;
        let mut points = vec![self.vertex_position(origin)?, self.vertex_position(at)?];
        for edge in edges.iter().skip(1) {
            let (start, end) = self.edge_vertex_ids(*edge)?;
            let next = if start == at { end } else if end == at { start } else { return None };
            if next != origin {
                points.push(self.vertex_position(next)?);
            }
            at = next;
        }
        if points.len() >= 3 { Some(points) } else { None }
    }

    fn loop_abs_area(&self, edges: &[u32], normal: [f64; 3]) -> f64 {
        let Some(points) = self.loop_points(edges) else { return 0.0 };
        let projected: Vec<[f64; 3]> = points
            .into_iter()
            .map(|point| {
                let along = dot(point, normal);
                sub(point, scale(normal, along))
            })
            .collect();
        dot(newell(&projected), normal).abs() * 0.5
    }
}

fn empty_walk(missing: bool) -> ElementWalk {
    ElementWalk { ids: Vec::new(), note: if missing { Some(WalkNote::Missing) } else { None } }
}

fn stronger_stop(forward: Stop, backward: Stop) -> Stop {
    let forward_note = stop_note(forward);
    let backward_note = stop_note(backward);
    match (forward_note, backward_note) {
        (None, None) => Stop::Closed,
        (Some(_), None) => forward,
        (None, Some(_)) => backward,
        (Some(left), Some(right)) => {
            if note_rank(right) > note_rank(left) {
                backward
            } else {
                forward
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::{SolidBody, SolidEdge, SolidFace, SolidVertex, TopologyError};

    fn box_body() -> SolidBody {
        SolidBody::from_box([2.0, 2.0, 2.0]).unwrap()
    }

    fn solid_volume(body: &SolidBody) -> f64 {
        let mut sum = 0.0;
        for face in &body.faces {
            let Some(positions) = body.face_positions(face.id) else { continue };
            if positions.len() < 3 {
                continue;
            }
            for index in 1..positions.len() - 1 {
                let (a, b, c) = (positions[0], positions[index], positions[index + 1]);
                let triple = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
                sum += triple;
            }
        }
        sum.abs() / 6.0
    }

    fn valence(body: &SolidBody, vertex: u32) -> usize {
        body.edges.iter().filter(|edge| edge.a == vertex || edge.b == vertex).count()
    }

    fn is_cycle(body: &SolidBody, edges: &[u32]) -> bool {
        if edges.len() < 3 {
            return false;
        }
        for index in 0..edges.len() {
            let Some((left_a, left_b)) = body.edge_endpoints_ids(edges[index]) else { return false };
            let Some((right_a, right_b)) = body.edge_endpoints_ids(edges[(index + 1) % edges.len()]) else { return false };
            let shares = left_a == right_a || left_a == right_b || left_b == right_a || left_b == right_b;
            if !shares {
                return false;
            }
        }
        true
    }

    fn top_faces(body: &SolidBody) -> Vec<u32> {
        body.faces
            .iter()
            .filter(|face| body.unit_normal(face.id).is_some_and(|normal| normal[1] > 0.9))
            .map(|face| face.id)
            .collect()
    }

    fn shares_edge(body: &SolidBody, left: u32, right: u32) -> bool {
        body.edges.iter().any(|edge| {
            let faces = body.faces_of_edge(edge.id);
            faces.contains(&left) && faces.contains(&right)
        })
    }

    fn shares_vertex(body: &SolidBody, left: u32, right: u32) -> bool {
        let Some(left_loop) = body.face_loop(left) else { return false };
        let Some(right_loop) = body.face_loop(right) else { return false };
        left_loop.iter().any(|vertex| right_loop.contains(vertex))
    }

    fn interior_top(body: &SolidBody) -> Option<u32> {
        top_faces(body).into_iter().find(|face| {
            let Some(loop_) = body.face_loop(*face) else { return false };
            let count = loop_.len();
            (0..count).all(|index| {
                let Some(edge) = body.edge_between(loop_[index], loop_[(index + 1) % count]) else { return false };
                let faces = body.faces_of_edge(edge);
                faces.len() == 2 && faces.iter().all(|other| body.unit_normal(*other).is_some_and(|normal| normal[1] > 0.9))
            })
        })
    }

    trait EdgeIds {
        fn edge_endpoints_ids(&self, id: u32) -> Option<(u32, u32)>;
    }

    impl EdgeIds for SolidBody {
        fn edge_endpoints_ids(&self, id: u32) -> Option<(u32, u32)> {
            self.edges.iter().find(|edge| edge.id == id).map(|edge| (edge.a, edge.b))
        }
    }

    #[test]
    fn a_cube_edge_loop_stops_at_the_valence_three_corner() {
        let body = box_body();
        let before = body.clone();
        let walk = body.edge_loop(&[16]);
        assert_eq!(walk.ids, vec![16]);
        assert_eq!(walk.note, Some(WalkNote::Pole));
        assert_eq!(walk.note.unwrap().sentence(), "The loop stopped at a pole.");
        let pair = body.edge_loop(&[16, 13]);
        assert_eq!(pair.ids, vec![16, 13]);
        assert_eq!(pair.note, Some(WalkNote::Pole));
        let missing = body.edge_loop(&[999]);
        assert!(missing.ids.is_empty());
        assert_eq!(missing.note, Some(WalkNote::Missing));
        let mixed = body.edge_loop(&[16, 999]);
        assert_eq!(mixed.ids, vec![16]);
        assert_eq!(mixed.note, Some(WalkNote::Pole));
        assert_eq!(body, before);
    }

    #[test]
    fn a_cube_edge_ring_closes_across_the_four_side_quads_and_bevels() {
        let body = box_body();
        let walk = body.edge_ring(&[16]);
        assert!(walk.note.is_none(), "{:?}", walk.note);
        assert_eq!(walk.ids.len(), 4);
        assert_eq!(walk.ids[0], 16);
        for index in 0..walk.ids.len() {
            let (left_a, left_b) = body.edge_endpoints_ids(walk.ids[index]).unwrap();
            let (right_a, right_b) = body.edge_endpoints_ids(walk.ids[(index + 1) % walk.ids.len()]).unwrap();
            assert_ne!(left_a, right_a);
            assert_ne!(left_a, right_b);
            assert_ne!(left_b, right_a);
            assert_ne!(left_b, right_b);
        }
        let before = solid_volume(&body);
        let cut = body.bevel_edges(&walk.ids, 0.2).unwrap();
        cut.edit.body.validate().unwrap();
        assert!(solid_volume(&cut.edit.body) < before - 1.0e-6);
        assert_eq!(cut.edges.len(), 4);
    }

    #[test]
    fn a_flat_grid_loop_stops_at_the_poles_and_its_ring_stops_on_the_split_side() {
        let body = box_body().subdivide_face(3, 4, 4).unwrap().body;
        let edge = body
            .edges
            .iter()
            .find(|edge| {
                valence(&body, edge.a) == 4
                    && valence(&body, edge.b) == 4
                    && body.faces_of_edge(edge.id).iter().all(|face| body.unit_normal(*face).is_some_and(|normal| normal[1] > 0.9))
            })
            .unwrap()
            .id;
        let loop_ = body.edge_loop(&[edge]);
        assert!(loop_.ids.len() >= 3, "{}", loop_.ids.len());
        assert_eq!(loop_.note, Some(WalkNote::Pole));
        assert_eq!(loop_.ids[0] == edge || loop_.ids.contains(&edge), true);
        for pair in loop_.ids.windows(2) {
            let (left_a, left_b) = body.edge_endpoints_ids(pair[0]).unwrap();
            let (right_a, right_b) = body.edge_endpoints_ids(pair[1]).unwrap();
            assert!(left_a == right_a || left_a == right_b || left_b == right_a || left_b == right_b);
        }
        let (first_a, first_b) = body.edge_endpoints_ids(*loop_.ids.first().unwrap()).unwrap();
        let (last_a, last_b) = body.edge_endpoints_ids(*loop_.ids.last().unwrap()).unwrap();
        assert!(valence(&body, first_a) != 4 || valence(&body, first_b) != 4);
        assert!(valence(&body, last_a) != 4 || valence(&body, last_b) != 4);
        let ring = body.edge_ring(&[edge]);
        assert!(ring.ids.len() > 1, "{:?}", ring.ids);
        assert_eq!(ring.note, Some(WalkNote::NotQuad));
        let unchanged = body.clone();
        assert!(matches!(body.bevel_edges(&loop_.ids, 0.5), Err(TopologyError::Degenerate)));
        assert_eq!(body, unchanged);
    }

    #[test]
    fn a_beveled_cube_edge_leaves_a_ring_that_will_not_cross_the_n_gon() {
        let body = box_body().bevel_edges(&[16], 0.2).unwrap().edit.body;
        body.validate().unwrap();
        let ngon = body.faces.iter().find(|face| face.vertices.len() != 4).expect("one beveled edge opens an n-gon");
        let mut refused = false;
        let count = ngon.vertices.len();
        for index in 0..count {
            let edge = body.edge_between(ngon.vertices[index], ngon.vertices[(index + 1) % count]).unwrap();
            let walk = body.edge_ring(&[edge]);
            if walk.note == Some(WalkNote::NotQuad) {
                let ngon_edges: Vec<u32> = (0..count)
                    .filter_map(|slot| body.edge_between(ngon.vertices[slot], ngon.vertices[(slot + 1) % count]))
                    .collect();
                assert!(ngon_edges.iter().any(|other| !walk.ids.contains(other)));
                refused = true;
                break;
            }
        }
        assert!(refused);
    }

    #[test]
    fn an_edge_with_three_faces_does_not_pick_a_continuation() {
        let body = SolidBody {
            next_id: 20,
            vertices: vec![
                SolidVertex { id: 1, position: [0.0, 0.0, 0.0] },
                SolidVertex { id: 2, position: [1.0, 0.0, 0.0] },
                SolidVertex { id: 3, position: [0.0, 1.0, 0.0] },
                SolidVertex { id: 4, position: [0.0, 0.0, 1.0] },
                SolidVertex { id: 5, position: [0.0, -1.0, 0.0] },
            ],
            edges: vec![
                SolidEdge { id: 10, a: 1, b: 2 },
                SolidEdge { id: 11, a: 1, b: 3 },
                SolidEdge { id: 12, a: 1, b: 4 },
                SolidEdge { id: 13, a: 1, b: 5 },
                SolidEdge { id: 14, a: 2, b: 3 },
                SolidEdge { id: 15, a: 2, b: 4 },
                SolidEdge { id: 16, a: 2, b: 5 },
            ],
            faces: vec![
                SolidFace { id: 1, vertices: vec![1, 2, 3] },
                SolidFace { id: 2, vertices: vec![1, 2, 4] },
                SolidFace { id: 3, vertices: vec![1, 2, 5] },
            ],
        };
        let walk = body.edge_loop(&[10]);
        assert_eq!(walk.ids, vec![10]);
        assert_eq!(walk.note, Some(WalkNote::Ambiguous));
        assert_eq!(walk.note.unwrap().sentence(), "The walk stopped where the next edge was not unique.");
        let open = SolidBody {
            next_id: 8,
            vertices: vec![
                SolidVertex { id: 1, position: [0.0, 0.0, 0.0] },
                SolidVertex { id: 2, position: [1.0, 0.0, 0.0] },
                SolidVertex { id: 3, position: [0.0, 1.0, 0.0] },
            ],
            edges: vec![
                SolidEdge { id: 4, a: 1, b: 2 },
                SolidEdge { id: 5, a: 2, b: 3 },
                SolidEdge { id: 6, a: 3, b: 1 },
            ],
            faces: vec![SolidFace { id: 1, vertices: vec![1, 2, 3] }],
        };
        let boundary = open.edge_loop(&[4]);
        assert_eq!(boundary.ids, vec![4]);
        assert_eq!(boundary.note, Some(WalkNote::OpenEdge));
    }

    #[test]
    fn connected_selection_is_the_whole_closed_solid() {
        let body = box_body();
        let faces = body.connected_faces(&[1]);
        assert_eq!(faces.ids[0], 1);
        assert_eq!(faces.ids.len(), body.faces.len());
        assert!(faces.note.is_none());
        let edges = body.connected_edges(&[16]);
        assert_eq!(edges.ids[0], 16);
        assert_eq!(edges.ids.len(), body.edges.len());
        let vertices = body.connected_vertices(&[1]);
        assert_eq!(vertices.ids[0], 1);
        assert_eq!(vertices.ids.len(), body.vertices.len());
        let grown = body.grow_faces(&faces.ids);
        let shrunk = body.shrink_faces(&grown.ids);
        assert_eq!(shrunk.ids.len(), body.faces.len());
    }

    #[test]
    fn boundary_keeps_one_loop_reports_a_hole_and_refuses_a_fold_or_a_branch() {
        let body = box_body();
        let one = body.region_boundary(&[1]);
        assert_eq!(one.note, None);
        assert_eq!(one.ids.len(), 4);
        assert!(is_cycle(&body, &one.ids));
        let face_edges: Vec<u32> = {
            let loop_ = body.face_loop(1).unwrap();
            (0..loop_.len()).map(|index| body.edge_between(loop_[index], loop_[(index + 1) % loop_.len()]).unwrap()).collect()
        };
        assert!(face_edges.iter().all(|edge| one.ids.contains(edge)));
        let closed = body.region_boundary(&[1, 2, 3, 4, 5, 6]);
        assert!(closed.ids.is_empty());
        assert_eq!(closed.note, Some(WalkNote::NoBoundary));
        let folded = body.region_boundary(&[1, 2]);
        assert_eq!(folded.note, Some(WalkNote::FoldedRegion));
        assert_eq!(folded.ids.len(), 8);

        let grid = body.subdivide_face(3, 2, 2).unwrap().body;
        let tops = top_faces(&grid);
        let adjacent = tops.iter().copied().find_map(|face| {
            tops.iter().copied().find(|other| *other != face && shares_edge(&grid, face, *other)).map(|other| (face, other))
        });
        let (left, right) = adjacent.unwrap();
        let seam = grid
            .edges
            .iter()
            .find(|edge| {
                let faces = grid.faces_of_edge(edge.id);
                faces.contains(&left) && faces.contains(&right)
            })
            .unwrap()
            .id;
        let region = grid.region_boundary(&[left, right]);
        assert_eq!(region.note, None);
        assert_eq!(region.ids.len(), 6);
        assert!(!region.ids.contains(&seam));
        assert!(is_cycle(&grid, &region.ids));
        let diagonal = tops.iter().copied().find_map(|face| {
            tops.iter().copied().find(|other| *other != face && !shares_edge(&grid, face, *other)).map(|other| (face, other))
        });
        let (front, back) = diagonal.unwrap();
        let branched = grid.region_boundary(&[front, back]);
        assert_eq!(branched.note, Some(WalkNote::BranchedBoundary));
        assert_eq!(branched.ids.len(), 8);

        let fine = body.subdivide_face(3, 4, 4).unwrap().body;
        let hole = interior_top(&fine).unwrap();
        let region_faces: Vec<u32> = top_faces(&fine).into_iter().filter(|face| *face != hole).collect();
        let holed = fine.region_boundary(&region_faces);
        assert_eq!(holed.note, Some(WalkNote::HolesLeft(1)));
        assert_eq!(holed.note.unwrap().sentence(), "The outer boundary is selected. 1 hole stayed out.");
        let hole_loop = fine.face_loop(hole).unwrap();
        for index in 0..hole_loop.len() {
            let edge = fine.edge_between(hole_loop[index], hole_loop[(index + 1) % hole_loop.len()]).unwrap();
            assert!(!holed.ids.contains(&edge));
        }
        assert!(holed.ids.len() > 4);
    }

    #[test]
    fn two_equal_disjoint_caps_do_not_invent_an_outer_loop() {
        let divided = box_body().subdivide_face(3, 4, 4).unwrap().body;
        let tops = top_faces(&divided);
        let first = tops[0];
        let second = tops
            .iter()
            .copied()
            .find(|face| *face != first && !shares_vertex(&divided, first, *face))
            .unwrap();
        let raised = divided.extrude_faces(&[first], [0.0, 0.3, 0.0]).unwrap().body;
        let raised = raised.extrude_faces(&[second], [0.0, 0.3, 0.0]).unwrap().body;
        raised.validate().unwrap();
        assert!(!shares_vertex(&raised, first, second));
        let walk = raised.region_boundary(&[first, second]);
        assert_eq!(walk.note, Some(WalkNote::AmbiguousOuter));
        assert_eq!(walk.ids.len(), 8);
    }

    #[test]
    fn grow_then_shrink_returns_one_element_and_a_full_component_stays() {
        let body = box_body();
        let grown = body.grow_faces(&[1]);
        assert_eq!(grown.ids[0], 1);
        assert_eq!(grown.ids.len(), 5);
        assert!(!grown.ids.contains(&2));
        assert_eq!(body.shrink_faces(&grown.ids).ids, vec![1]);
        assert!(body.shrink_faces(&[1]).ids.is_empty());

        let edge = body.grow_edges(&[16]);
        assert_eq!(edge.ids[0], 16);
        assert!(edge.ids.len() > 1);
        assert_eq!(body.shrink_edges(&edge.ids).ids, vec![16]);
        assert!(body.shrink_edges(&[16]).ids.is_empty());

        let vertex = body.grow_vertices(&[1]);
        assert_eq!(vertex.ids[0], 1);
        assert!(vertex.ids.contains(&2));
        assert_eq!(body.shrink_vertices(&vertex.ids).ids, vec![1]);
        assert!(body.shrink_vertices(&[1]).ids.is_empty());

        let ring = body.edge_ring(&[16]);
        assert!(body.shrink_edges(&ring.ids).ids.is_empty());
        let all = body.connected_edges(&[16]);
        assert_eq!(body.shrink_edges(&all.ids).ids.len(), body.edges.len());
    }
}
