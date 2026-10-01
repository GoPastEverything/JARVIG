//! Logical mesh. Local geometry only. Not an asset, not an entity, not a GPU buffer.
//!
//! [`MeshId`] is an in-process id. It is not a stable C handle and it is not
//! `JarvigMeshHandle`. ADR-0022. The renderer uploads a mesh through the RHI
//! when it draws. This module does not know frames, cameras, or wgpu.

use std::fmt;

/// Object-local axis-aligned box. Not a world position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Object-local sphere derived from the box. Not a world position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalBounds {
    pub aabb: Aabb,
    pub sphere: BoundingSphere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshVertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
}

impl MeshVertexFormat {
    pub fn byte_size(self) -> u32 {
        match self {
            Self::Float32x2 => 8,
            Self::Float32x3 => 12,
            Self::Float32x4 => 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshVertexAttribute {
    pub shader_location: u32,
    pub offset: u32,
    pub format: MeshVertexFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexStreamDesc {
    pub stride: u32,
    pub attributes: Vec<MeshVertexAttribute>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshIndexFormat {
    Uint16,
    Uint32,
}

impl MeshIndexFormat {
    pub fn byte_size(self) -> u32 {
        match self {
            Self::Uint16 => 2,
            Self::Uint32 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshTopology {
    TriangleList,
}

/// A draw range inside a mesh. `material_slot` is a slot index. The mesh does not own a material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Submesh {
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    pub topology: MeshTopology,
    pub material_slot: u32,
    pub bounds: LocalBounds,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    streams: Vec<VertexStreamDesc>,
    index_format: MeshIndexFormat,
    index_bytes: Vec<u8>,
    submeshes: Vec<Submesh>,
    bounds: LocalBounds,
    /// Built once with the mesh. Selection queries it and does not rebuild it.
    bvh: crate::bvh::TriangleBvh,
}

impl Mesh {
    pub fn streams(&self) -> &[VertexStreamDesc] {
        &self.streams
    }

    pub fn index_format(&self) -> MeshIndexFormat {
        self.index_format
    }

    pub fn index_bytes(&self) -> &[u8] {
        &self.index_bytes
    }

    pub fn submeshes(&self) -> &[Submesh] {
        &self.submeshes
    }

    pub fn bounds(&self) -> LocalBounds {
        self.bounds
    }

    pub fn vertex_count(&self) -> u32 {
        let stream = &self.streams[0];
        (stream.bytes.len() / stream.stride as usize) as u32
    }

    pub fn index_count(&self) -> u32 {
        (self.index_bytes.len() / self.index_format.byte_size() as usize) as u32
    }

    /// Local-space ray. `t` is the parameter along `direction`, which is not renormalized.
    pub fn intersect_local_ray(&self, origin: [f64; 3], direction: [f64; 3]) -> Option<f64> {
        self.bvh.intersect(origin, direction, |index| self.position(index))
    }

    pub(crate) fn pick_bvh(&self) -> &crate::bvh::TriangleBvh {
        &self.bvh
    }

    /// Object-local position of one vertex. The first stream's first attribute is the position.
    pub fn position(&self, index: u32) -> Option<[f32; 3]> {
        let stream = self.streams.first()?;
        let stride = stream.stride as usize;
        let start = index as usize * stride;
        if start + 12 > stream.bytes.len() {
            return None;
        }
        Some([
            f32::from_le_bytes(stream.bytes[start..start + 4].try_into().ok()?),
            f32::from_le_bytes(stream.bytes[start + 4..start + 8].try_into().ok()?),
            f32::from_le_bytes(stream.bytes[start + 8..start + 12].try_into().ok()?),
        ])
    }

    /// Indexed triangles. Each index is a vertex of [`Self::position`].
    pub fn triangle_indices(&self) -> Vec<[u32; 3]> {
        let mut out = Vec::new();
        for submesh in &self.submeshes {
            let mut index = 0u32;
            while index + 2 < submesh.index_count {
                let first = submesh.first_index + index;
                let read = |at| read_index_optional(&self.index_bytes, self.index_format, at);
                if let (Some(a), Some(b), Some(c)) = (read(first), read(first + 1), read(first + 2)) {
                    let base = submesh.base_vertex;
                    out.push([
                        a.saturating_add_signed(base),
                        b.saturating_add_signed(base),
                        c.saturating_add_signed(base),
                    ]);
                }
                index += 3;
            }
        }
        out
    }
}

fn read_index_optional(bytes: &[u8], format: MeshIndexFormat, index: u32) -> Option<u32> {
    let size = format.byte_size() as usize;
    let start = index as usize * size;
    if start + size > bytes.len() {
        return None;
    }
    Some(match format {
        MeshIndexFormat::Uint16 => u16::from_le_bytes(bytes[start..start + 2].try_into().ok()?) as u32,
        MeshIndexFormat::Uint32 => u32::from_le_bytes(bytes[start..start + 4].try_into().ok()?),
    })
}

#[derive(Debug, Clone)]
pub struct MeshDesc {
    pub streams: Vec<VertexStreamDesc>,
    pub index_format: MeshIndexFormat,
    pub index_bytes: Vec<u8>,
    pub submeshes: Vec<SubmeshDesc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmeshDesc {
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    pub topology: MeshTopology,
    pub material_slot: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshError {
    Empty,
    BadStride,
    BadAttribute,
    StreamLength,
    StreamCountMismatch,
    MissingPosition,
    BadIndices,
    BadSubmesh,
    BadBounds,
}

impl fmt::Display for MeshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "mesh has no vertices or indices",
            Self::BadStride => "vertex stride is zero or not a multiple of 4",
            Self::BadAttribute => "a vertex attribute does not fit the stride",
            Self::StreamLength => "vertex bytes are not a whole number of vertices",
            Self::StreamCountMismatch => "vertex streams disagree on the vertex count",
            Self::MissingPosition => "mesh has no float3 position at shader location 0",
            Self::BadIndices => "index bytes do not match the index format",
            Self::BadSubmesh => "submesh range is outside the index or vertex buffer",
            Self::BadBounds => "a position is non-finite",
        })
    }
}

/// In-process id. Not a C ABI handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshId(pub u64);

#[derive(Default)]
pub struct MeshLibrary {
    next: u64,
    meshes: Vec<(MeshId, Mesh)>,
}

impl MeshLibrary {
    pub fn insert(&mut self, mesh: Mesh) -> MeshId {
        self.next += 1;
        let id = MeshId(self.next);
        self.meshes.push((id, mesh));
        id
    }

    /// Stores `mesh` under an existing id. Used when an execution copy draws a mesh the renderer already uploaded.
    pub fn insert_exact(&mut self, id: MeshId, mesh: Mesh) {
        if let Some((_, stored)) = self.meshes.iter_mut().find(|(stored_id, _)| *stored_id == id) {
            *stored = mesh;
        } else {
            self.meshes.push((id, mesh));
        }
        if id.0 > self.next {
            self.next = id.0;
        }
    }

    pub fn remove(&mut self, id: MeshId) -> bool {
        let before = self.meshes.len();
        self.meshes.retain(|(stored, _)| *stored != id);
        self.meshes.len() != before
    }

    pub fn get(&self, id: MeshId) -> Option<&Mesh> {
        self.meshes.iter().find(|(stored, _)| *stored == id).map(|(_, mesh)| mesh)
    }

    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
}

pub fn create_mesh(desc: MeshDesc) -> Result<Mesh, MeshError> {
    create_mesh_with(desc, None)
}

fn create_mesh_with(desc: MeshDesc, bvh: Option<crate::bvh::TriangleBvh>) -> Result<Mesh, MeshError> {
    if desc.streams.is_empty() || desc.submeshes.is_empty() || desc.index_bytes.is_empty() {
        return Err(MeshError::Empty);
    }
    let vertex_count = stream_vertex_count(&desc.streams[0])?;
    if vertex_count == 0 {
        return Err(MeshError::Empty);
    }
    for stream in &desc.streams {
        validate_stream(stream)?;
        if stream_vertex_count(stream)? != vertex_count {
            return Err(MeshError::StreamCountMismatch);
        }
    }
    let position = position_attribute(&desc.streams[0]).ok_or(MeshError::MissingPosition)?;
    let index_size = desc.index_format.byte_size() as usize;
    if desc.index_bytes.len() % index_size != 0 {
        return Err(MeshError::BadIndices);
    }
    let index_count = (desc.index_bytes.len() / index_size) as u32;
    if index_count == 0 {
        return Err(MeshError::Empty);
    }
    let mut submeshes = Vec::with_capacity(desc.submeshes.len());
    for submesh in &desc.submeshes {
        if submesh.index_count == 0 || submesh.first_index.saturating_add(submesh.index_count) > index_count {
            return Err(MeshError::BadSubmesh);
        }
        if submesh.topology == MeshTopology::TriangleList && submesh.index_count % 3 != 0 {
            return Err(MeshError::BadSubmesh);
        }
        let mut bounds: Option<LocalBounds> = None;
        for index in 0..submesh.index_count {
            let stored = read_index(&desc.index_bytes, desc.index_format, submesh.first_index + index)?;
            let vertex = stored as i64 + submesh.base_vertex as i64;
            if vertex < 0 || vertex >= vertex_count as i64 {
                return Err(MeshError::BadSubmesh);
            }
            let point = read_position(&desc.streams[0], position, vertex as u32)?;
            bounds = Some(match bounds {
                Some(existing) => expand(existing, point),
                None => bounds_from_point(point),
            });
        }
        submeshes.push(Submesh {
            first_index: submesh.first_index,
            index_count: submesh.index_count,
            base_vertex: submesh.base_vertex,
            topology: submesh.topology,
            material_slot: submesh.material_slot,
            bounds: bounds.ok_or(MeshError::BadSubmesh)?,
        });
    }
    let mut bounds = submeshes[0].bounds;
    for submesh in submeshes.iter().skip(1) {
        bounds = union(bounds, submesh.bounds);
    }
    let mut mesh = Mesh {
        streams: desc.streams,
        index_format: desc.index_format,
        index_bytes: desc.index_bytes,
        submeshes,
        bounds,
        bvh: crate::bvh::TriangleBvh::empty(),
    };
    mesh.bvh = bvh.unwrap_or_else(|| crate::bvh::TriangleBvh::build(&mesh));
    Ok(mesh)
}

fn stream_vertex_count(stream: &VertexStreamDesc) -> Result<u32, MeshError> {
    validate_stream(stream)?;
    if stream.bytes.len() % stream.stride as usize != 0 {
        return Err(MeshError::StreamLength);
    }
    Ok((stream.bytes.len() / stream.stride as usize) as u32)
}

fn validate_stream(stream: &VertexStreamDesc) -> Result<(), MeshError> {
    if stream.stride == 0 || stream.stride % 4 != 0 || stream.attributes.is_empty() {
        return Err(MeshError::BadStride);
    }
    let mut seen = Vec::new();
    for attribute in &stream.attributes {
        if attribute.offset % 4 != 0 {
            return Err(MeshError::BadAttribute);
        }
        if attribute.offset.saturating_add(attribute.format.byte_size()) > stream.stride {
            return Err(MeshError::BadAttribute);
        }
        if seen.contains(&attribute.shader_location) {
            return Err(MeshError::BadAttribute);
        }
        seen.push(attribute.shader_location);
    }
    Ok(())
}

fn position_attribute(stream: &VertexStreamDesc) -> Option<MeshVertexAttribute> {
    stream.attributes.iter().copied().find(|attribute| {
        attribute.shader_location == 0 && attribute.format == MeshVertexFormat::Float32x3
    })
}

fn read_index(bytes: &[u8], format: MeshIndexFormat, index: u32) -> Result<u32, MeshError> {
    let size = format.byte_size() as usize;
    let start = index as usize * size;
    let end = start + size;
    let slice = bytes.get(start..end).ok_or(MeshError::BadIndices)?;
    match format {
        MeshIndexFormat::Uint16 => Ok(u16::from_le_bytes(slice.try_into().unwrap()) as u32),
        MeshIndexFormat::Uint32 => Ok(u32::from_le_bytes(slice.try_into().unwrap())),
    }
}

fn read_position(stream: &VertexStreamDesc, position: MeshVertexAttribute, vertex: u32) -> Result<[f32; 3], MeshError> {
    let start = vertex as usize * stream.stride as usize + position.offset as usize;
    let end = start + 12;
    let slice = stream.bytes.get(start..end).ok_or(MeshError::BadBounds)?;
    let point = [
        f32::from_le_bytes(slice[0..4].try_into().unwrap()),
        f32::from_le_bytes(slice[4..8].try_into().unwrap()),
        f32::from_le_bytes(slice[8..12].try_into().unwrap()),
    ];
    if !point.iter().all(|value| value.is_finite()) {
        return Err(MeshError::BadBounds);
    }
    Ok(point)
}

fn bounds_from_point(point: [f32; 3]) -> LocalBounds {
    expand_aabb(Aabb { min: point, max: point })
}

fn expand(bounds: LocalBounds, point: [f32; 3]) -> LocalBounds {
    let aabb = Aabb {
        min: [
            bounds.aabb.min[0].min(point[0]),
            bounds.aabb.min[1].min(point[1]),
            bounds.aabb.min[2].min(point[2]),
        ],
        max: [
            bounds.aabb.max[0].max(point[0]),
            bounds.aabb.max[1].max(point[1]),
            bounds.aabb.max[2].max(point[2]),
        ],
    };
    expand_aabb(aabb)
}

fn union(left: LocalBounds, right: LocalBounds) -> LocalBounds {
    expand(expand(expand(left, right.aabb.min), right.aabb.max), left.aabb.max)
}

fn expand_aabb(aabb: Aabb) -> LocalBounds {
    let center = [
        (aabb.min[0] + aabb.max[0]) * 0.5,
        (aabb.min[1] + aabb.max[1]) * 0.5,
        (aabb.min[2] + aabb.max[2]) * 0.5,
    ];
    let radius = ((aabb.max[0] - center[0]).powi(2)
        + (aabb.max[1] - center[1]).powi(2)
        + (aabb.max[2] - center[2]).powi(2))
    .sqrt();
    LocalBounds { aabb, sphere: BoundingSphere { center, radius } }
}

fn triangle_stream(vertices: [[f32; 6]; 3], uvs: [[f32; 2]; 3]) -> VertexStreamDesc {
    let mut bytes = Vec::with_capacity(180);
    let normal = [0.0f32, 0.0, 1.0];
    let tangent = [1.0f32, 0.0, 0.0, 1.0];
    for (vertex, uv) in vertices.iter().zip(uvs) {
        for value in vertex.iter().chain(uv.iter()).chain(normal.iter()).chain(tangent.iter()) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    VertexStreamDesc {
        stride: 60,
        attributes: vec![
            MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
            MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
            MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
            MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
            MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
        ],
        bytes,
    }
}

/// UVs past 0..1 so a repeating sampler shows more than one checker cell.
fn repeating_uvs() -> [[f32; 2]; 3] {
    [[0.0, 0.0], [2.0, 0.0], [0.0, 2.0]]
}

fn triangle_indices() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(6);
    for index in [0u16, 1, 2] {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes
}

fn one_submesh() -> SubmeshDesc {
    SubmeshDesc {
        first_index: 0,
        index_count: 3,
        base_vertex: 0,
        topology: MeshTopology::TriangleList,
        material_slot: 0,
    }
}

/// The near RGB triangle. Vertices are object-local meters.
pub fn near_triangle_mesh() -> Mesh {
    create_mesh(MeshDesc {
        streams: vec![triangle_stream(
            [
                [-0.6, -0.5, 0.0, 0.90, 0.20, 0.15],
                [0.6, -0.5, 0.0, 0.15, 0.85, 0.30],
                [0.0, 0.6, 0.0, 0.20, 0.40, 0.95],
            ],
            repeating_uvs(),
        )],
        index_format: MeshIndexFormat::Uint16,
        index_bytes: triangle_indices(),
        submeshes: vec![one_submesh()],
    })
    .expect("near triangle")
}

fn push_vertex(bytes: &mut Vec<u8>, position: [f32; 3], color: [f32; 3], uv: [f32; 2], normal: [f32; 3], tangent: [f32; 4]) {
    for value in position.iter().chain(color.iter()).chain(uv.iter()).chain(normal.iter()).chain(tangent.iter()) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn mesh_from_triangles(triangles: &[[[f32; 3]; 3]], normal_of: impl Fn([f32; 3], [f32; 3], [f32; 3]) -> [f32; 3]) -> Mesh {
    let mut bytes = Vec::new();
    let mut indices = Vec::new();
    let color = [1.0_f32, 1.0, 1.0];
    for (triangle_index, triangle) in triangles.iter().enumerate() {
        let normal = normal_of(triangle[0], triangle[1], triangle[2]);
        let edge = [
            triangle[1][0] - triangle[0][0],
            triangle[1][1] - triangle[0][1],
            triangle[1][2] - triangle[0][2],
        ];
        let tangent = normalize_tangent(edge, normal);
        for (corner, position) in triangle.iter().enumerate() {
            let uv = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]][corner];
            push_vertex(&mut bytes, *position, color, uv, normal, tangent);
        }
        let base = (triangle_index * 3) as u16;
        for index in [base, base + 1, base + 2] {
            indices.extend_from_slice(&index.to_le_bytes());
        }
    }
    create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: 60,
            attributes: vec![
                MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
            ],
            bytes,
        }],
        index_format: MeshIndexFormat::Uint16,
        index_bytes: indices,
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: (triangles.len() * 3) as u32,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .expect("validation mesh")
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length < 1.0e-8 { [0.0, 1.0, 0.0] } else { [n[0] / length, n[1] / length, n[2] / length] }
}

fn normalize_tangent(edge: [f32; 3], normal: [f32; 3]) -> [f32; 4] {
    let length = (edge[0] * edge[0] + edge[1] * edge[1] + edge[2] * edge[2]).sqrt();
    let t = if length < 1.0e-8 { [1.0, 0.0, 0.0] } else { [edge[0] / length, edge[1] / length, edge[2] / length] };
    let _ = normal;
    [t[0], t[1], t[2], 1.0]
}

/// Horizontal floor. Normal is +Y. UVs are 0..1 across the quad. Tangent is +X, handedness +1.
/// `cross(+Y, +X)` points −Z, opposite texture +V, so a DirectX normal matches without a green flip.
pub fn floor_mesh(width_m: f32, depth_m: f32) -> Mesh {
    let x = width_m * 0.5;
    let z = depth_m * 0.5;
    let normal = [0.0_f32, 1.0, 0.0];
    let tangent = [1.0_f32, 0.0, 0.0, 1.0];
    let corners = [
        ([-x, 0.0, -z], [0.0_f32, 0.0]),
        ([x, 0.0, -z], [1.0, 0.0]),
        ([x, 0.0, z], [1.0, 1.0]),
        ([-x, 0.0, z], [0.0, 1.0]),
    ];
    let mut bytes = Vec::new();
    let color = [1.0_f32, 1.0, 1.0];
    for (position, uv) in corners {
        push_vertex(&mut bytes, position, color, uv, normal, tangent);
    }
    let mut indices = Vec::new();
    for index in [0u16, 2, 1, 0, 3, 2] {
        indices.extend_from_slice(&index.to_le_bytes());
    }
    create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: 60,
            attributes: vec![
                MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
            ],
            bytes,
        }],
        index_format: MeshIndexFormat::Uint16,
        index_bytes: indices,
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: 6,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .expect("floor")
}

/// Closed cube centered on the origin. Each face is one 0..1 quad with an outward normal.
/// Tangent follows +U and handedness is +1. Texture +V is opposite `cross(normal, tangent)`,
/// the same DirectX frame as [`floor_mesh`].
pub fn cube_mesh(size_m: f32) -> Mesh {
    let h = size_m * 0.5;
    // Outward normal, tangent along +U.
    let faces = [
        ([1.0_f32, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0]),
    ];
    let mut bytes = Vec::new();
    let mut indices = Vec::new();
    let color = [1.0_f32, 1.0, 1.0];
    for (normal, tangent) in faces {
        let bitangent = cross3(normal, tangent);
        let v_axis = [-bitangent[0], -bitangent[1], -bitangent[2]];
        let center = [normal[0] * h, normal[1] * h, normal[2] * h];
        let corners = [(0.0_f32, 0.0_f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let base = (bytes.len() / 60) as u16;
        for (u, v) in corners {
            let position = [
                center[0] + tangent[0] * (u * 2.0 - 1.0) * h + v_axis[0] * (v * 2.0 - 1.0) * h,
                center[1] + tangent[1] * (u * 2.0 - 1.0) * h + v_axis[1] * (v * 2.0 - 1.0) * h,
                center[2] + tangent[2] * (u * 2.0 - 1.0) * h + v_axis[2] * (v * 2.0 - 1.0) * h,
            ];
            push_vertex(&mut bytes, position, color, [u, v], normal, [tangent[0], tangent[1], tangent[2], 1.0]);
        }
        for index in [base, base + 2, base + 1, base, base + 3, base + 2] {
            indices.extend_from_slice(&index.to_le_bytes());
        }
    }
    create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: 60,
            attributes: vec![
                MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
            ],
            bytes,
        }],
        index_format: MeshIndexFormat::Uint16,
        index_bytes: indices,
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: 36,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .expect("cube")
}

/// Capsule along Y, centered on the origin. `cylinder_height_m` is the straight section.
/// The hemispheres add `radius_m` at each end. JARVIG builds this. It is not an imported asset.
pub fn capsule_mesh(radius_m: f32, cylinder_height_m: f32) -> Mesh {
    let radius = if radius_m.is_finite() && radius_m > 1.0e-4 { radius_m } else { 0.05 };
    let half = if cylinder_height_m.is_finite() && cylinder_height_m >= 0.0 { cylinder_height_m * 0.5 } else { 0.1 };
    let segments = 8u32;
    let cap_rings = 3u32;
    let mut rings: Vec<Vec<[f32; 3]>> = Vec::new();
    for ring in 0..=cap_rings {
        let phi = ring as f32 / cap_rings as f32 * std::f32::consts::FRAC_PI_2;
        rings.push(ring_points(segments, half + radius * phi.cos(), radius * phi.sin()));
    }
    for ring in 0..=cap_rings {
        let phi = std::f32::consts::FRAC_PI_2 + ring as f32 / cap_rings as f32 * std::f32::consts::FRAC_PI_2;
        rings.push(ring_points(segments, -half + radius * phi.cos(), radius * phi.sin()));
    }
    let mut triangles = Vec::new();
    for (upper, lower) in rings.iter().zip(rings.iter().skip(1)) {
        for segment in 0..segments as usize {
            let next = (segment + 1) % segments as usize;
            let a = upper[segment];
            let b = upper[next];
            let c = lower[segment];
            let d = lower[next];
            if a != b {
                triangles.push([a, b, c]);
            }
            if c != d {
                triangles.push([b, d, c]);
            }
        }
    }
    mesh_from_triangles(&triangles, face_normal)
}

fn ring_points(segments: u32, y: f32, radial: f32) -> Vec<[f32; 3]> {
    (0..segments)
        .map(|segment| {
            let theta = segment as f32 / segments as f32 * std::f32::consts::TAU;
            [radial * theta.cos(), y, radial * theta.sin()]
        })
        .collect()
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Smooth UV sphere. Vertices are shared. The stored normal is the radius, so the GPU interpolates it.
pub fn sphere_mesh(radius_m: f32, segments: u32, rings: u32) -> Mesh {
    uv_sphere(radius_m, segments, rings, true)
}

/// Same tessellation as [`sphere_mesh`], but each triangle keeps a constant face normal.
/// Use it to tell geometric facets from a blocky reflection.
pub fn flat_sphere_mesh(radius_m: f32, segments: u32, rings: u32) -> Mesh {
    uv_sphere(radius_m, segments, rings, false)
}

fn uv_sphere(radius_m: f32, segments: u32, rings: u32, smooth: bool) -> Mesh {
    let segments = segments.max(3);
    let rings = rings.max(2);
    let mut positions = Vec::new();
    for ring in 0..=rings {
        let phi = ring as f32 / rings as f32 * std::f32::consts::PI;
        for segment in 0..segments {
            let theta = segment as f32 / segments as f32 * std::f32::consts::TAU;
            positions.push([
                radius_m * phi.sin() * theta.cos(),
                radius_m * phi.cos(),
                radius_m * phi.sin() * theta.sin(),
            ]);
        }
    }
    let mut triangles = Vec::new();
    let mut corners: Vec<[usize; 3]> = Vec::new();
    for ring in 0..rings {
        for segment in 0..segments {
            let next = (segment + 1) % segments;
            let a = (ring * segments + segment) as usize;
            let b = (ring * segments + next) as usize;
            let c = ((ring + 1) * segments + segment) as usize;
            let d = ((ring + 1) * segments + next) as usize;
            if ring != 0 {
                triangles.push([positions[a], positions[b], positions[c]]);
                corners.push([a, b, c]);
            }
            if ring + 1 != rings {
                triangles.push([positions[b], positions[d], positions[c]]);
                corners.push([b, d, c]);
            }
        }
    }
    if !smooth {
        return mesh_from_triangles(&triangles, face_normal);
    }
    let mut bytes = Vec::new();
    let color = [1.0_f32, 1.0, 1.0];
    for (index, position) in positions.iter().enumerate() {
        let normal = unit3(*position);
        let theta = (index % segments as usize) as f32 / segments as f32 * std::f32::consts::TAU;
        // +U follows theta. +V follows phi, which increases toward -Y.
        // cross(normal, tangent) points along +V, so handedness -1 makes the shader bitangent
        // point opposite +V. That is the same DirectX frame as the floor. It is not an API choice.
        let tangent = [-theta.sin(), 0.0, theta.cos(), -1.0];
        let u = (index % segments as usize) as f32 / segments as f32;
        let v = (index / segments as usize) as f32 / rings as f32;
        push_vertex(&mut bytes, *position, color, [u, v], normal, tangent);
    }
    let index_count = (corners.len() * 3) as u32;
    let mut indices = Vec::new();
    for corner in &corners {
        for index in corner {
            indices.extend_from_slice(&(*index as u16).to_le_bytes());
        }
    }
    create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: 60,
            attributes: vec![
                MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
            ],
            bytes,
        }],
        index_format: MeshIndexFormat::Uint16,
        index_bytes: indices,
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .expect("smooth sphere")
}

fn unit3(value: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length < 1.0e-8 { [0.0, 1.0, 0.0] } else { [value[0] / length, value[1] / length, value[2] / length] }
}

/// Vertical panel in the YZ plane. Normal is -X, so it faces the origin from +X.
pub fn emissive_panel_mesh(height_m: f32, depth_m: f32) -> Mesh {
    let y = height_m * 0.5;
    let z = depth_m * 0.5;
    let a = [0.0, -y, -z];
    let b = [0.0, y, -z];
    let c = [0.0, y, z];
    let d = [0.0, -y, z];
    mesh_from_triangles(&[[a, b, c], [a, c, d]], face_normal)
}

/// The farther gold triangle, still object-local, drawn behind the near one.
pub fn far_triangle_mesh() -> Mesh {
    create_mesh(MeshDesc {
        streams: vec![triangle_stream(
            [
                [-2.4, -1.8, 0.0, 0.95, 0.72, 0.15],
                [2.4, -1.8, 0.0, 0.95, 0.72, 0.15],
                [0.0, 2.2, 0.0, 0.95, 0.72, 0.15],
            ],
            repeating_uvs(),
        )],
        index_format: MeshIndexFormat::Uint16,
        index_bytes: triangle_indices(),
        submeshes: vec![one_submesh()],
    })
    .expect("far triangle")
}

/// One material range of the canonical mesh. Empty normals or tangents are generated.
///
/// This is the geometry a later cluster build reads. It is not a glTF document.
#[derive(Clone, Debug)]
pub struct CanonicalSurface {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub texcoords: Vec<[f32; 2]>,
    pub tangents: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub material_slot: u32,
}

pub fn canonical_vertex_attributes() -> Vec<MeshVertexAttribute> {
    vec![
        MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
        MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
        MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
        MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
        MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
    ]
}

/// Interleaved position, white color, UV0, normal, tangent. Stride 60. One submesh per surface.
pub fn mesh_from_surfaces(surfaces: &[CanonicalSurface]) -> Result<Mesh, MeshError> {
    assemble_surfaces(surfaces, None)
}

pub(crate) fn mesh_from_surfaces_with_bvh(surfaces: &[CanonicalSurface], bvh: crate::bvh::TriangleBvh) -> Result<Mesh, MeshError> {
    assemble_surfaces(surfaces, Some(bvh))
}

fn assemble_surfaces(surfaces: &[CanonicalSurface], bvh: Option<crate::bvh::TriangleBvh>) -> Result<Mesh, MeshError> {
    if surfaces.is_empty() {
        return Err(MeshError::Empty);
    }
    let mut bytes = Vec::new();
    let mut indices = Vec::new();
    let mut submeshes = Vec::new();
    let color = [1.0_f32, 1.0, 1.0];
    for surface in surfaces {
        if surface.positions.is_empty() || surface.indices.is_empty() || surface.indices.len() % 3 != 0 {
            return Err(MeshError::Empty);
        }
        if surface.texcoords.len() != surface.positions.len() {
            return Err(MeshError::BadAttribute);
        }
        let normals = if surface.normals.len() == surface.positions.len() {
            surface.normals.clone()
        } else if surface.normals.is_empty() {
            face_normals(&surface.positions, &surface.indices)
        } else {
            return Err(MeshError::BadAttribute);
        };
        let tangents = if surface.tangents.len() == surface.positions.len() {
            surface.tangents.clone()
        } else if surface.tangents.is_empty() {
            generated_tangents(&surface.positions, &normals, &surface.texcoords, &surface.indices)
        } else {
            return Err(MeshError::BadAttribute);
        };
        let base = (bytes.len() / 60) as u32;
        if base as usize != bytes.len() / 60 {
            return Err(MeshError::BadIndices);
        }
        for index in 0..surface.positions.len() {
            push_vertex(&mut bytes, surface.positions[index], color, surface.texcoords[index], normals[index], tangents[index]);
        }
        let first_index = (indices.len() / 4) as u32;
        for index in &surface.indices {
            let absolute = base.checked_add(*index).ok_or(MeshError::BadIndices)?;
            indices.extend_from_slice(&absolute.to_le_bytes());
        }
        submeshes.push(SubmeshDesc {
            first_index,
            index_count: surface.indices.len() as u32,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: surface.material_slot,
        });
    }
    create_mesh_with(
        MeshDesc {
            streams: vec![VertexStreamDesc { stride: 60, attributes: canonical_vertex_attributes(), bytes }],
            index_format: MeshIndexFormat::Uint32,
            index_bytes: indices,
            submeshes,
        },
        bvh,
    )
}

fn face_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0_f32; 3]; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let (a, b, c) = (triangle[0] as usize, triangle[1] as usize, triangle[2] as usize);
        if a >= positions.len() || b >= positions.len() || c >= positions.len() {
            continue;
        }
        let normal = face_normal(positions[a], positions[b], positions[c]);
        for index in [a, b, c] {
            normals[index] = [normals[index][0] + normal[0], normals[index][1] + normal[1], normals[index][2] + normal[2]];
        }
    }
    for normal in &mut normals {
        *normal = unit3_or(*normal, [0.0, 1.0, 0.0]);
    }
    normals
}

fn generated_tangents(positions: &[[f32; 3]], normals: &[[f32; 3]], uvs: &[[f32; 2]], indices: &[u32]) -> Vec<[f32; 4]> {
    let mut tangent = vec![[0.0_f32; 3]; positions.len()];
    let mut bitangent = vec![[0.0_f32; 3]; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let ids = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
        if ids.iter().any(|index| *index >= positions.len()) {
            continue;
        }
        let (p0, p1, p2) = (positions[ids[0]], positions[ids[1]], positions[ids[2]]);
        let (w0, w1, w2) = (uvs[ids[0]], uvs[ids[1]], uvs[ids[2]]);
        let edge1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let edge2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let duv1 = [w1[0] - w0[0], w1[1] - w0[1]];
        let duv2 = [w2[0] - w0[0], w2[1] - w0[1]];
        let denom = duv1[0] * duv2[1] - duv1[1] * duv2[0];
        let (t, b) = if denom.abs() < 1.0e-12 {
            (perpendicular(normals[ids[0]]), [0.0, 0.0, 0.0])
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
            tangent[index] = [tangent[index][0] + t[0], tangent[index][1] + t[1], tangent[index][2] + t[2]];
            bitangent[index] = [bitangent[index][0] + b[0], bitangent[index][1] + b[1], bitangent[index][2] + b[2]];
        }
    }
    normals
        .iter()
        .zip(tangent.iter())
        .zip(bitangent.iter())
        .map(|((normal, tangent), bitangent)| {
            let n = unit3_or(*normal, [0.0, 1.0, 0.0]);
            let dot = tangent[0] * n[0] + tangent[1] * n[1] + tangent[2] * n[2];
            let mut t = [tangent[0] - n[0] * dot, tangent[1] - n[1] * dot, tangent[2] - n[2] * dot];
            t = unit3_or(t, perpendicular(n));
            let cross = [n[1] * t[2] - n[2] * t[1], n[2] * t[0] - n[0] * t[2], n[0] * t[1] - n[1] * t[0]];
            let handed = cross[0] * bitangent[0] + cross[1] * bitangent[1] + cross[2] * bitangent[2];
            [t[0], t[1], t[2], if handed < 0.0 { -1.0 } else { 1.0 }]
        })
        .collect()
}

fn unit3_or(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length < 1.0e-8 { fallback } else { [value[0] / length, value[1] / length, value[2] / length] }
}

fn perpendicular(normal: [f32; 3]) -> [f32; 3] {
    let axis = if normal[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    unit3_or(
        [normal[1] * axis[2] - normal[2] * axis[1], normal[2] * axis[0] - normal[0] * axis[2], normal[0] * axis[1] - normal[1] * axis[0]],
        [1.0, 0.0, 0.0],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex_f32(mesh: &Mesh, vertex: u32, offset: usize) -> f32 {
        let vertex = vertex as usize;
        let bytes = &mesh.streams()[0].bytes;
        let start = vertex * 60 + offset;
        f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
    }

    #[test]
    fn cube_top_and_sphere_equator_share_the_floor_directx_frame() {
        let cube = cube_mesh(2.0);
        assert_eq!(cube.index_count(), 36);
        let mut top = None;
        for vertex in 0..cube.vertex_count() {
            let y = vertex_f32(&cube, vertex, 4);
            let x = vertex_f32(&cube, vertex, 0);
            let z = vertex_f32(&cube, vertex, 8);
            let normal_y = vertex_f32(&cube, vertex, 36);
            if (y - 1.0).abs() < 1.0e-5 && (x + 1.0).abs() < 1.0e-5 && (z + 1.0).abs() < 1.0e-5 && normal_y > 0.9 {
                top = Some(vertex);
            }
        }
        let top = top.expect("cube corner");
        assert!(vertex_f32(&cube, top, 36) > 0.9, "top face normal points out");
        assert!((vertex_f32(&cube, top, 24) - 0.0).abs() < 1.0e-5);
        assert!((vertex_f32(&cube, top, 28) - 0.0).abs() < 1.0e-5);
        assert!((vertex_f32(&cube, top, 44) - 1.0).abs() < 1.0e-5);
        assert!((vertex_f32(&cube, top, 56) - 1.0).abs() < 1.0e-5);
        let sphere = sphere_mesh(1.0, 8, 4);
        let equator = 2 * 8;
        assert!((vertex_f32(&sphere, equator, 0) - 1.0).abs() < 1.0e-4);
        assert!(vertex_f32(&sphere, equator, 4).abs() < 1.0e-4);
        assert!((vertex_f32(&sphere, equator, 56) + 1.0).abs() < 1.0e-5, "sphere handedness is -1");
    }

    #[test]
    fn a_dense_sphere_is_hit_through_its_bvh_and_a_miss_stays_a_miss() {
        let mesh = sphere_mesh(1.0, 64, 32);
        assert!(mesh.index_count() > 1_000);
        let hit = mesh.intersect_local_ray([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
        assert!(hit.is_some_and(|distance| distance > 3.0 && distance < 5.0));
        assert!(mesh.intersect_local_ray([0.0, 5.0, 5.0], [0.0, 0.0, -1.0]).is_none());
    }

    #[test]
    fn near_mesh_is_one_local_submesh() {
        let mesh = near_triangle_mesh();
        assert_eq!(mesh.submeshes().len(), 1);
        assert_eq!(mesh.index_format(), MeshIndexFormat::Uint16);
        assert_eq!(mesh.index_count(), 3);
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.submeshes()[0].material_slot, 0);
        let bounds = mesh.bounds();
        assert!((bounds.aabb.min[0] + 0.6).abs() < 1.0e-5);
        assert!((bounds.aabb.max[0] - 0.6).abs() < 1.0e-5);
        assert!((bounds.aabb.min[1] + 0.5).abs() < 1.0e-5);
        assert!((bounds.aabb.max[1] - 0.6).abs() < 1.0e-5);
        assert_eq!(bounds.aabb.min[2], 0.0);
        assert!(bounds.sphere.radius < 2.0);
        assert!(bounds.aabb.max[0].abs() < 10.0);
    }

    #[test]
    fn far_mesh_is_larger_and_still_local() {
        let near = near_triangle_mesh().bounds();
        let far = far_triangle_mesh().bounds();
        assert!(far.sphere.radius > near.sphere.radius);
        assert!(far.aabb.max[0].abs() < 10.0);
    }

    #[test]
    fn uint32_indices_are_accepted() {
        let stream = triangle_stream(
            [
                [0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                [0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        );
        let mut index_bytes = Vec::new();
        for index in [0u32, 1, 2] {
            index_bytes.extend_from_slice(&index.to_le_bytes());
        }
        let mesh = create_mesh(MeshDesc {
            streams: vec![stream],
            index_format: MeshIndexFormat::Uint32,
            index_bytes,
            submeshes: vec![one_submesh()],
        })
        .unwrap();
        assert_eq!(mesh.index_format(), MeshIndexFormat::Uint32);
    }

    #[test]
    fn bad_ranges_and_layouts_are_rejected() {
        let stream = triangle_stream(
            [
                [0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                [0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        );
        let indices = triangle_indices();
        let mut over = one_submesh();
        over.index_count = 6;
        assert_eq!(
            create_mesh(MeshDesc {
                streams: vec![stream.clone()],
                index_format: MeshIndexFormat::Uint16,
                index_bytes: indices.clone(),
                submeshes: vec![over],
            }),
            Err(MeshError::BadSubmesh)
        );
        let mut bad_index = indices.clone();
        bad_index[0] = 9;
        assert_eq!(
            create_mesh(MeshDesc {
                streams: vec![stream.clone()],
                index_format: MeshIndexFormat::Uint16,
                index_bytes: bad_index,
                submeshes: vec![one_submesh()],
            }),
            Err(MeshError::BadSubmesh)
        );
        let mut broken = stream.clone();
        broken.stride = 2;
        assert!(create_mesh(MeshDesc {
            streams: vec![broken],
            index_format: MeshIndexFormat::Uint16,
            index_bytes: indices.clone(),
            submeshes: vec![one_submesh()],
        })
        .is_err());
        let mut odd = indices;
        odd.pop();
        assert_eq!(
            create_mesh(MeshDesc {
                streams: vec![stream],
                index_format: MeshIndexFormat::Uint16,
                index_bytes: odd,
                submeshes: vec![one_submesh()],
            }),
            Err(MeshError::BadIndices)
        );
    }

    #[test]
    fn smooth_sphere_normals_follow_the_radius_and_flat_sphere_normals_do_not() {
        let smooth = sphere_mesh(1.0, 8, 4);
        assert_eq!(smooth.vertex_count(), 5 * 8);
        let stream = &smooth.streams()[0];
        for vertex in 0..smooth.vertex_count() {
            let position = read3(stream, vertex, 0);
            let normal = read3(stream, vertex, 32);
            let radial = unit_test(position);
            let dot = normal[0] * radial[0] + normal[1] * radial[1] + normal[2] * radial[2];
            assert!(dot > 0.999, "{vertex} {normal:?} {radial:?}");
        }
        let indices = smooth.index_bytes();
        let i0 = u16::from_le_bytes(indices[0..2].try_into().unwrap()) as u32;
        let i1 = u16::from_le_bytes(indices[2..4].try_into().unwrap()) as u32;
        let i2 = u16::from_le_bytes(indices[4..6].try_into().unwrap()) as u32;
        let a = read3(stream, i0, 0);
        let b = read3(stream, i1, 0);
        let c = read3(stream, i2, 0);
        let face = face_normal(a, b, c);
        let mid = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
        let outward = unit_test(mid);
        let facing = face[0] * outward[0] + face[1] * outward[1] + face[2] * outward[2];
        assert!(facing > 0.5, "sphere winding points inward {face:?} {outward:?}");
        let flat = flat_sphere_mesh(1.0, 8, 4);
        assert!(flat.vertex_count() > smooth.vertex_count());
        let flat_stream = &flat.streams()[0];
        let mismatched = (0..flat.vertex_count())
            .filter(|vertex| {
                let position = read3(flat_stream, *vertex, 0);
                let normal = read3(flat_stream, *vertex, 32);
                let radial = unit_test(position);
                normal[0] * radial[0] + normal[1] * radial[1] + normal[2] * radial[2] < 0.98
            })
            .count();
        assert!(mismatched > 0);
    }

    fn read3(stream: &VertexStreamDesc, vertex: u32, offset: usize) -> [f32; 3] {
        let start = vertex as usize * stream.stride as usize + offset;
        let bytes = &stream.bytes[start..start + 12];
        [
            f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        ]
    }

    fn unit_test(value: [f32; 3]) -> [f32; 3] {
        let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
        [value[0] / length, value[1] / length, value[2] / length]
    }

    #[test]
    fn library_ids_are_stable_and_not_reused() {
        let mut library = MeshLibrary::default();
        let first = library.insert(near_triangle_mesh());
        let second = library.insert(far_triangle_mesh());
        assert_ne!(first, second);
        assert_eq!(library.len(), 2);
        assert_eq!(library.get(first).unwrap().vertex_count(), 3);
        assert!(library.get(MeshId(0)).is_none());
    }
}
