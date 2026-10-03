//! Offline clusters of a derived JARVIG mesh. Not a glTF read and not a frame-time build.
//!
//! The ordinary index buffer stays the rendering reference. These clusters are a
//! partition of those triangles: same vertices, same winding, grouped inside a
//! spatial cell so a later pass can draw or cull a cluster without walking the
//! whole mesh. This is not virtual geometry and not a GPU-driven draw.

use std::time::Instant;

use crate::mesh::Mesh;

/// Measured pack limits for this experiment. Not a permanent architecture constant.
pub const MESHLET_MAX_VERTICES: u32 = 128;
pub const MESHLET_MAX_TRIANGLES: u32 = 128;
/// Bump when the cluster algorithm or its limits change. The sidecar stores this.
pub const MESHLET_BUILDER_VERSION: u32 = 1;
const MESHLET_MAGIC: &[u8; 8] = b"JARVMLET";
const MESHLET_VERSION: u32 = 2;
const MESHLET_HEADER: usize = 72;

#[derive(Clone, Debug, PartialEq)]
pub struct Meshlet {
    pub vertex_offset: u32,
    pub vertex_count: u32,
    pub index_offset: u32,
    pub index_count: u32,
    pub material_slot: u32,
    pub submesh: u32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub sphere_center: [f32; 3],
    pub sphere_radius: f32,
    /// Average face normal. Unit length when the cluster has area.
    pub cone_axis: [f32; 3],
    /// Smallest dot of a face normal with `cone_axis`.
    pub cone_cutoff: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshletStats {
    pub source_triangles: u32,
    pub meshlet_count: u32,
    pub average_triangles: f32,
    pub average_vertices: f32,
    pub min_triangles: u32,
    pub max_triangles: u32,
    pub min_vertices: u32,
    pub max_vertices: u32,
    pub derived_bytes: u64,
    pub build_ms: f32,
    /// Time to read the sidecar in this process. Zero when this process built the clusters.
    pub load_ms: f32,
    /// Time to write the sidecar in this process. Zero when the file was only read.
    pub write_ms: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshletSet {
    pub max_vertices: u32,
    pub max_triangles: u32,
    /// Cells along each axis of the mesh bounds. A cluster does not cross a cell.
    pub grid_resolution: u32,
    pub builder_version: u32,
    pub importer_version: u32,
    pub source_vertices: u32,
    /// Catalog fingerprint of the source bytes. Empty until the asset loader stamps it.
    pub source_fingerprint: String,
    pub meshlets: Vec<Meshlet>,
    /// Parent-mesh vertex ids, packed per cluster.
    pub vertex_indices: Vec<u32>,
    /// Triangle corners, local to that cluster's vertex range. One byte because the vertex limit is below 256.
    pub local_indices: Vec<u8>,
    pub stats: MeshletStats,
}

/// Indices back into the parent mesh, in cluster order, plus one packed color per triangle.
/// The ranges line up with the source submeshes. Drawing these indices is the clustered mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshletDraw {
    pub indices: Vec<u32>,
    pub colors: Vec<u32>,
    /// Draw-order meshlet id for each triangle. Parallel to `colors`.
    pub owners: Vec<u32>,
    pub ranges: Vec<MeshletDrawRange>,
    /// One span per meshlet in the same order as `owners`.
    pub spans: Vec<MeshletDrawRange>,
    /// `spans[i]` refers to this index in `MeshletSet::meshlets`.
    pub span_source: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshletDrawRange {
    pub first_index: u32,
    pub index_count: u32,
}

struct OpenCluster {
    verts: Vec<u32>,
    locals: Vec<u8>,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    faces: Vec<[f32; 3]>,
    slot: u32,
    submesh: u32,
    cell: u32,
}

struct Seed {
    a: u32,
    b: u32,
    c: u32,
    submesh: u32,
    slot: u32,
    cell: u32,
    order: u32,
}

impl OpenCluster {
    fn new(slot: u32, submesh: u32, cell: u32) -> Self {
        Self {
            verts: Vec::new(),
            locals: Vec::new(),
            bounds_min: [f32::MAX; 3],
            bounds_max: [f32::MIN; 3],
            faces: Vec::new(),
            slot,
            submesh,
            cell,
        }
    }
}

/// One offline pass over the derived triangles. A cluster stays inside one submesh and one spatial cell.
pub fn build_meshlets(mesh: &Mesh) -> MeshletSet {
    let started = Instant::now();
    let grid = grid_resolution(mesh.index_count() / 3);
    let bounds = mesh.bounds();
    let min = bounds.aabb.min;
    let extent = [
        bounds.aabb.max[0] - min[0],
        bounds.aabb.max[1] - min[1],
        bounds.aabb.max[2] - min[2],
    ];
    let mut seeds = Vec::new();
    let mut order = 0u32;
    for (submesh_index, submesh) in mesh.submeshes().iter().enumerate() {
        let mut cursor = 0u32;
        while cursor + 2 < submesh.index_count {
            let first = submesh.first_index + cursor;
            let corners = [read_vertex(mesh, first), read_vertex(mesh, first + 1), read_vertex(mesh, first + 2)];
            cursor += 3;
            let [Some(a), Some(b), Some(c)] = corners else { continue };
            let cell = cell_code(centroid(mesh, a, b, c), min, extent, grid);
            seeds.push(Seed { a, b, c, submesh: submesh_index as u32, slot: submesh.material_slot, cell, order });
            order = order.saturating_add(1);
        }
    }
    seeds.sort_by(|left, right| left.submesh.cmp(&right.submesh).then(left.cell.cmp(&right.cell)).then(left.order.cmp(&right.order)));
    let mut stamp = vec![0u32; mesh.vertex_count() as usize];
    let mut local_of = vec![0u8; mesh.vertex_count() as usize];
    let mut generation = 1u32;
    let mut vertex_indices = Vec::new();
    let mut local_indices = Vec::new();
    let mut meshlets = Vec::new();
    let mut open = OpenCluster::new(0, 0, 0);
    let mut opened = false;
    for seed in seeds {
        if !opened || open.submesh != seed.submesh || open.cell != seed.cell {
            if opened {
                flush(&mut open, &mut meshlets, &mut vertex_indices, &mut local_indices);
            }
            generation = bump_generation(&mut stamp, generation);
            open = OpenCluster::new(seed.slot, seed.submesh, seed.cell);
            opened = true;
        }
        let fresh = [seed.a, seed.b, seed.c].into_iter().filter(|vertex| stamp.get(*vertex as usize).copied() != Some(generation)).count() as u32;
        let next_tris = open.locals.len() as u32 / 3 + 1;
        if !open.verts.is_empty() && (open.verts.len() as u32 + fresh > MESHLET_MAX_VERTICES || next_tris > MESHLET_MAX_TRIANGLES) {
            flush(&mut open, &mut meshlets, &mut vertex_indices, &mut local_indices);
            generation = bump_generation(&mut stamp, generation);
            open = OpenCluster::new(seed.slot, seed.submesh, seed.cell);
        }
        for vertex in [seed.a, seed.b, seed.c] {
            let local = if stamp.get(vertex as usize).copied() == Some(generation) {
                local_of[vertex as usize]
            } else {
                let local = open.verts.len() as u8;
                stamp[vertex as usize] = generation;
                local_of[vertex as usize] = local;
                open.verts.push(vertex);
                local
            };
            open.locals.push(local);
            if let Some(position) = mesh.position(vertex) {
                for axis in 0..3 {
                    open.bounds_min[axis] = open.bounds_min[axis].min(position[axis]);
                    open.bounds_max[axis] = open.bounds_max[axis].max(position[axis]);
                }
            }
        }
        if let (Some(pa), Some(pb), Some(pc)) = (mesh.position(seed.a), mesh.position(seed.b), mesh.position(seed.c)) {
            open.faces.push(face_normal(pa, pb, pc));
        }
    }
    if opened && !open.verts.is_empty() {
        flush(&mut open, &mut meshlets, &mut vertex_indices, &mut local_indices);
    }
    let stats = summarize(mesh.index_count() / 3, &meshlets, started.elapsed().as_secs_f32() * 1000.0, 0);
    let mut set = MeshletSet {
        max_vertices: MESHLET_MAX_VERTICES,
        max_triangles: MESHLET_MAX_TRIANGLES,
        grid_resolution: grid,
        builder_version: MESHLET_BUILDER_VERSION,
        importer_version: 0,
        source_vertices: mesh.vertex_count(),
        source_fingerprint: String::new(),
        meshlets,
        vertex_indices,
        local_indices,
        stats,
    };
    set.stats.derived_bytes = encode_meshlets(&set).len() as u64;
    set.stats.build_ms = started.elapsed().as_secs_f32() * 1000.0;
    set
}

fn bump_generation(stamp: &mut [u32], generation: u32) -> u32 {
    let next = generation.wrapping_add(1);
    if next == 0 {
        stamp.fill(0);
        1
    } else {
        next
    }
}

fn flush(open: &mut OpenCluster, meshlets: &mut Vec<Meshlet>, vertex_indices: &mut Vec<u32>, local_indices: &mut Vec<u8>) {
    if open.verts.is_empty() || open.locals.len() < 3 {
        return;
    }
    let vertex_offset = vertex_indices.len() as u32;
    let index_offset = local_indices.len() as u32;
    vertex_indices.extend_from_slice(&open.verts);
    local_indices.extend_from_slice(&open.locals);
    let center = [
        (open.bounds_min[0] + open.bounds_max[0]) * 0.5,
        (open.bounds_min[1] + open.bounds_max[1]) * 0.5,
        (open.bounds_min[2] + open.bounds_max[2]) * 0.5,
    ];
    let extent = [
        open.bounds_max[0] - center[0],
        open.bounds_max[1] - center[1],
        open.bounds_max[2] - center[2],
    ];
    let radius = (extent[0] * extent[0] + extent[1] * extent[1] + extent[2] * extent[2]).sqrt();
    let mut cone = [0.0f32; 3];
    for face in &open.faces {
        cone = [cone[0] + face[0], cone[1] + face[1], cone[2] + face[2]];
    }
    let axis = unit(cone);
    let cutoff = open.faces.iter().fold(1.0f32, |best, face| best.min(dot(axis, *face)));
    meshlets.push(Meshlet {
        vertex_offset,
        vertex_count: open.verts.len() as u32,
        index_offset,
        index_count: open.locals.len() as u32,
        material_slot: open.slot,
        submesh: open.submesh,
        bounds_min: open.bounds_min,
        bounds_max: open.bounds_max,
        sphere_center: center,
        sphere_radius: radius,
        cone_axis: axis,
        cone_cutoff: cutoff,
    });
}

fn summarize(source_triangles: u32, meshlets: &[Meshlet], build_ms: f32, derived_bytes: u64) -> MeshletStats {
    if meshlets.is_empty() {
        return MeshletStats {
            source_triangles,
            meshlet_count: 0,
            average_triangles: 0.0,
            average_vertices: 0.0,
            min_triangles: 0,
            max_triangles: 0,
            min_vertices: 0,
            max_vertices: 0,
            derived_bytes,
            build_ms,
            load_ms: 0.0,
            write_ms: 0.0,
        };
    }
    let mut min_triangles = u32::MAX;
    let mut max_triangles = 0u32;
    let mut min_vertices = u32::MAX;
    let mut max_vertices = 0u32;
    let mut triangles = 0u64;
    let mut vertices = 0u64;
    for meshlet in meshlets {
        let tri = meshlet.index_count / 3;
        min_triangles = min_triangles.min(tri);
        max_triangles = max_triangles.max(tri);
        min_vertices = min_vertices.min(meshlet.vertex_count);
        max_vertices = max_vertices.max(meshlet.vertex_count);
        triangles += u64::from(tri);
        vertices += u64::from(meshlet.vertex_count);
    }
    let count = meshlets.len() as f32;
    MeshletStats {
        source_triangles,
        meshlet_count: meshlets.len() as u32,
        average_triangles: triangles as f32 / count,
        average_vertices: vertices as f32 / count,
        min_triangles,
        max_triangles,
        min_vertices,
        max_vertices,
        derived_bytes,
        build_ms,
        load_ms: 0.0,
        write_ms: 0.0,
    }
}

/// How the leaf clusters relate to the canonical index buffer.
/// `missing_triangles == 0` and `duplicate_triangles == 0` means each source triangle appears once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshletCoverage {
    pub canonical_triangles: u32,
    pub leaf_triangles: u32,
    pub leaf_meshlets: u32,
    pub missing_triangles: u32,
    pub duplicate_triangles: u32,
}

/// The clusters are the same oriented triangles as the source mesh. Order may differ.
pub fn meshlets_cover_source(mesh: &Mesh, set: &MeshletSet) -> bool {
    if set.stats.source_triangles != mesh.index_count() / 3 {
        return false;
    }
    let coverage = meshlet_coverage(mesh, set);
    coverage.missing_triangles == 0 && coverage.duplicate_triangles == 0
}

/// Count leaf coverage of an imported mesh. This does not draw and does not open a project file.
pub fn meshlet_coverage(mesh: &Mesh, set: &MeshletSet) -> MeshletCoverage {
    let mut source = Vec::new();
    for submesh in mesh.submeshes() {
        let mut cursor = 0u32;
        while cursor + 2 < submesh.index_count {
            let first = submesh.first_index + cursor;
            cursor += 3;
            let [Some(a), Some(b), Some(c)] = [read_vertex(mesh, first), read_vertex(mesh, first + 1), read_vertex(mesh, first + 2)] else { continue };
            source.push(oriented(a, b, c));
        }
    }
    let canonical = source.len() as u32;
    let mut covered = Vec::new();
    let mut valid = true;
    for meshlet in &set.meshlets {
        if meshlet.vertex_count > set.max_vertices || meshlet.index_count / 3 > set.max_triangles || meshlet.submesh as usize >= mesh.submeshes().len() {
            valid = false;
            break;
        }
        let verts = set.vertex_indices.get(meshlet.vertex_offset as usize..meshlet.vertex_offset as usize + meshlet.vertex_count as usize);
        let locals = set.local_indices.get(meshlet.index_offset as usize..meshlet.index_offset as usize + meshlet.index_count as usize);
        let (Some(verts), Some(locals)) = (verts, locals) else {
            valid = false;
            break;
        };
        for triangle in locals.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (
                verts.get(triangle[0] as usize).copied(),
                verts.get(triangle[1] as usize).copied(),
                verts.get(triangle[2] as usize).copied(),
            ) else {
                valid = false;
                break;
            };
            covered.push(oriented(a, b, c));
        }
        if !valid {
            break;
        }
    }
    if !valid {
        return MeshletCoverage {
            canonical_triangles: canonical,
            leaf_triangles: 0,
            leaf_meshlets: set.meshlets.len() as u32,
            missing_triangles: canonical.max(1),
            duplicate_triangles: 0,
        };
    }
    let leaf_triangles = covered.len() as u32;
    source.sort_unstable();
    covered.sort_unstable();
    let mut missing = 0u32;
    let mut duplicate = 0u32;
    let mut left = 0usize;
    let mut right = 0usize;
    while left < source.len() && right < covered.len() {
        if source[left] < covered[right] {
            missing = missing.saturating_add(1);
            left += 1;
        } else if covered[right] < source[left] {
            duplicate = duplicate.saturating_add(1);
            right += 1;
        } else {
            left += 1;
            right += 1;
        }
    }
    missing = missing.saturating_add((source.len() - left) as u32);
    duplicate = duplicate.saturating_add((covered.len() - right) as u32);
    MeshletCoverage {
        canonical_triangles: canonical,
        leaf_triangles,
        leaf_meshlets: set.meshlets.len() as u32,
        missing_triangles: missing,
        duplicate_triangles: duplicate,
    }
}

/// Expand clusters into parent-mesh indices, grouped by source submesh.
pub fn meshlet_draw(set: &MeshletSet) -> MeshletDraw {
    let submesh_count = set.meshlets.iter().map(|meshlet| meshlet.submesh).max().map(|last| last as usize + 1).unwrap_or(0);
    let mut indices = Vec::with_capacity(set.local_indices.len());
    let mut colors = Vec::with_capacity(set.local_indices.len() / 3);
    let mut owners = Vec::with_capacity(set.local_indices.len() / 3);
    let mut spans = Vec::with_capacity(set.meshlets.len());
    let mut span_source = Vec::with_capacity(set.meshlets.len());
    let mut ranges = vec![MeshletDrawRange { first_index: 0, index_count: 0 }; submesh_count];
    for submesh in 0..submesh_count as u32 {
        let first_index = indices.len() as u32;
        for (index, meshlet) in set.meshlets.iter().enumerate() {
            if meshlet.submesh != submesh {
                continue;
            }
            let color = pack_color(meshlet_color(index as u32));
            let owner = spans.len() as u32;
            let span_first = indices.len() as u32;
            let Some(verts) = set.vertex_indices.get(meshlet.vertex_offset as usize..meshlet.vertex_offset as usize + meshlet.vertex_count as usize) else {
                continue;
            };
            let Some(locals) = set.local_indices.get(meshlet.index_offset as usize..meshlet.index_offset as usize + meshlet.index_count as usize) else {
                continue;
            };
            for triangle in locals.chunks_exact(3) {
                let Some(a) = verts.get(triangle[0] as usize).copied() else { continue };
                let Some(b) = verts.get(triangle[1] as usize).copied() else { continue };
                let Some(c) = verts.get(triangle[2] as usize).copied() else { continue };
                indices.extend_from_slice(&[a, b, c]);
                colors.push(color);
                owners.push(owner);
            }
            spans.push(MeshletDrawRange { first_index: span_first, index_count: indices.len() as u32 - span_first });
            span_source.push(index as u32);
        }
        ranges[submesh as usize] = MeshletDrawRange { first_index, index_count: indices.len() as u32 - first_index };
    }
    MeshletDraw { indices, colors, owners, ranges, spans, span_source }
}

pub fn encode_meshlets(set: &MeshletSet) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MESHLET_MAGIC);
    bytes.extend_from_slice(&MESHLET_VERSION.to_le_bytes());
    bytes.extend_from_slice(&set.max_vertices.to_le_bytes());
    bytes.extend_from_slice(&set.max_triangles.to_le_bytes());
    bytes.extend_from_slice(&(set.meshlets.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&set.stats.source_triangles.to_le_bytes());
    bytes.extend_from_slice(&(set.vertex_indices.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(set.local_indices.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&set.stats.build_ms.to_le_bytes());
    bytes.extend_from_slice(&set.grid_resolution.to_le_bytes());
    bytes.extend_from_slice(&set.builder_version.to_le_bytes());
    bytes.extend_from_slice(&set.importer_version.to_le_bytes());
    bytes.extend_from_slice(&set.source_vertices.to_le_bytes());
    bytes.extend_from_slice(&fingerprint_bytes(&set.source_fingerprint));
    for meshlet in &set.meshlets {
        for value in [
            meshlet.vertex_offset,
            meshlet.vertex_count,
            meshlet.index_offset,
            meshlet.index_count,
            meshlet.material_slot,
            meshlet.submesh,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for lane in meshlet.bounds_min.iter().chain(meshlet.bounds_max.iter()).chain(meshlet.sphere_center.iter()).chain(meshlet.cone_axis.iter()) {
            bytes.extend_from_slice(&lane.to_le_bytes());
        }
        bytes.extend_from_slice(&meshlet.sphere_radius.to_le_bytes());
        bytes.extend_from_slice(&meshlet.cone_cutoff.to_le_bytes());
    }
    for index in &set.vertex_indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes.extend_from_slice(&set.local_indices);
    bytes
}

pub fn decode_meshlets(bytes: &[u8]) -> Result<MeshletSet, String> {
    if bytes.len() < MESHLET_HEADER || &bytes[0..8] != MESHLET_MAGIC {
        return Err("meshlet header is not JARVIG cluster data".into());
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if version != MESHLET_VERSION {
        return Err(format!("meshlet version {version} is not supported"));
    }
    let max_vertices = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let max_triangles = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let meshlet_count = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let source_triangles = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let vertex_count = u32::from_le_bytes(bytes[28..32].try_into().unwrap()) as usize;
    let local_count = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
    let build_ms = f32::from_le_bytes(bytes[36..40].try_into().unwrap());
    let grid_resolution = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
    let builder_version = u32::from_le_bytes(bytes[44..48].try_into().unwrap());
    let importer_version = u32::from_le_bytes(bytes[48..52].try_into().unwrap());
    let source_vertices = u32::from_le_bytes(bytes[52..56].try_into().unwrap());
    let source_fingerprint = fingerprint_text(&bytes[56..72]);
    let record = 24 + 12 * 4 + 8;
    let header = MESHLET_HEADER + meshlet_count * record;
    if bytes.len() < header + vertex_count * 4 + local_count {
        return Err("meshlet file is truncated".into());
    }
    let mut meshlets = Vec::with_capacity(meshlet_count);
    let mut cursor = MESHLET_HEADER;
    for _ in 0..meshlet_count {
        let mut ints = [0u32; 6];
        for slot in &mut ints {
            *slot = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
        }
        let mut lanes = [0f32; 12];
        for lane in &mut lanes {
            *lane = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
        }
        let sphere_radius = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        cursor += 4;
        let cone_cutoff = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        cursor += 4;
        meshlets.push(Meshlet {
            vertex_offset: ints[0],
            vertex_count: ints[1],
            index_offset: ints[2],
            index_count: ints[3],
            material_slot: ints[4],
            submesh: ints[5],
            bounds_min: [lanes[0], lanes[1], lanes[2]],
            bounds_max: [lanes[3], lanes[4], lanes[5]],
            sphere_center: [lanes[6], lanes[7], lanes[8]],
            sphere_radius,
            cone_axis: [lanes[9], lanes[10], lanes[11]],
            cone_cutoff,
        });
    }
    let mut vertex_indices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertex_indices.push(u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()));
        cursor += 4;
    }
    let local_indices = bytes[cursor..cursor + local_count].to_vec();
    let mut stats = summarize(source_triangles, &meshlets, build_ms, bytes.len() as u64);
    stats.build_ms = build_ms;
    Ok(MeshletSet {
        max_vertices,
        max_triangles,
        grid_resolution,
        builder_version,
        importer_version,
        source_vertices,
        source_fingerprint,
        meshlets,
        vertex_indices,
        local_indices,
        stats,
    })
}

fn fingerprint_bytes(text: &str) -> [u8; 16] {
    let mut raw = [b' '; 16];
    let bytes = text.as_bytes();
    let count = bytes.len().min(16);
    raw[..count].copy_from_slice(&bytes[..count]);
    raw
}

fn fingerprint_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

pub fn meshlet_color(index: u32) -> [f32; 4] {
    let red = ((index.wrapping_mul(47).wrapping_add(29)) % 200 + 40) as f32 / 255.0;
    let green = ((index.wrapping_mul(91).wrapping_add(13)) % 200 + 40) as f32 / 255.0;
    let blue = ((index.wrapping_mul(17).wrapping_add(71)) % 200 + 40) as f32 / 255.0;
    [red, green, blue, 1.0]
}

fn pack_color(color: [f32; 4]) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    channel(color[0]) | (channel(color[1]) << 8) | (channel(color[2]) << 16) | (channel(color[3]) << 24)
}

fn grid_resolution(triangle_count: u32) -> u32 {
    let cells = (triangle_count / MESHLET_MAX_TRIANGLES).max(1);
    let axis = (cells as f64).cbrt().ceil() as u32;
    axis.clamp(8, 48)
}

fn cell_code(point: [f32; 3], min: [f32; 3], extent: [f32; 3], grid: u32) -> u32 {
    let grid = grid.max(1);
    let mut packed = 0u32;
    for axis in 0..3 {
        let t = if extent[axis] > 1.0e-8 { ((point[axis] - min[axis]) / extent[axis]).clamp(0.0, 1.0) } else { 0.0 };
        let cell = ((t * grid as f32) as u32).min(grid - 1);
        packed |= cell << (axis * 6);
    }
    packed
}

fn centroid(mesh: &Mesh, a: u32, b: u32, c: u32) -> [f32; 3] {
    match (mesh.position(a), mesh.position(b), mesh.position(c)) {
        (Some(pa), Some(pb), Some(pc)) => [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ],
        _ => [0.0, 0.0, 0.0],
    }
}

fn read_vertex(mesh: &Mesh, index_at: u32) -> Option<u32> {
    let format = mesh.index_format();
    let size = format.byte_size() as usize;
    let start = index_at as usize * size;
    let bytes = mesh.index_bytes();
    let stored = match format {
        crate::mesh::MeshIndexFormat::Uint16 => {
            let raw = bytes.get(start..start + 2)?;
            u16::from_le_bytes(raw.try_into().ok()?) as u32
        }
        crate::mesh::MeshIndexFormat::Uint32 => {
            let raw = bytes.get(start..start + 4)?;
            u32::from_le_bytes(raw.try_into().ok()?)
        }
    };
    let submesh = mesh.submeshes().iter().find(|submesh| index_at >= submesh.first_index && index_at < submesh.first_index + submesh.index_count)?;
    Some(stored.saturating_add_signed(submesh.base_vertex))
}

fn oriented(a: u32, b: u32, c: u32) -> [u32; 3] {
    if a <= b && a <= c {
        [a, b, c]
    } else if b <= c {
        [b, c, a]
    } else {
        [c, a, b]
    }
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    unit([
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ])
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn unit(value: [f32; 3]) -> [f32; 3] {
    let length = dot(value, value).sqrt();
    if length < 1.0e-8 { [0.0, 1.0, 0.0] } else { [value[0] / length, value[1] / length, value[2] / length] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{mesh_from_surfaces, CanonicalSurface, MeshError};

    fn surface(positions: Vec<[f32; 3]>, indices: Vec<u32>, material_slot: u32) -> CanonicalSurface {
        let count = positions.len();
        CanonicalSurface {
            positions,
            normals: Vec::new(),
            texcoords: vec![[0.0, 0.0]; count],
            tangents: Vec::new(),
            indices,
            material_slot,
        }
    }

    fn strip(triangles: u32) -> Mesh {
        let vertices = triangles + 2;
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for index in 0..vertices {
            positions.push([index as f32, if index % 2 == 0 { 0.0 } else { 1.0 }, 0.0]);
        }
        for triangle in 0..triangles {
            indices.extend_from_slice(&[triangle, triangle + 1, triangle + 2]);
        }
        mesh_from_surfaces(&[surface(positions, indices, 0)]).expect("strip")
    }

    #[test]
    fn clusters_partition_a_strip_and_round_trip() {
        let mesh = strip(300);
        let set = build_meshlets(&mesh);
        assert!(set.stats.meshlet_count > 1);
        assert!(set.stats.max_vertices <= MESHLET_MAX_VERTICES);
        assert!(set.stats.max_triangles <= MESHLET_MAX_TRIANGLES);
        assert!(set.stats.min_triangles >= 1);
        assert!(meshlets_cover_source(&mesh, &set));
        // Library strip. An imported project mesh is not loaded here.
        let coverage = meshlet_coverage(&mesh, &set);
        assert_eq!(coverage.canonical_triangles, 300);
        assert_eq!(coverage.leaf_triangles, coverage.canonical_triangles);
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert!(coverage.leaf_meshlets > 1);
        assert!(set.meshlets.iter().all(|meshlet| meshlet.sphere_radius.is_finite() && meshlet.cone_axis.iter().all(|lane| lane.is_finite())));
        let decoded = decode_meshlets(&encode_meshlets(&set)).unwrap();
        assert_eq!(decoded.stats.meshlet_count, set.stats.meshlet_count);
        assert_eq!(decoded.grid_resolution, set.grid_resolution);
        assert!(meshlets_cover_source(&mesh, &decoded));
        let draw = meshlet_draw(&set);
        assert_eq!(draw.indices.len(), 300 * 3);
        assert_eq!(draw.colors.len(), 300);
        assert_eq!(draw.ranges.iter().map(|range| range.index_count).sum::<u32>(), 900);
        let mut reversed = set.clone();
        reversed.local_indices.swap(0, 1);
        assert!(!meshlets_cover_source(&mesh, &reversed));
        let _ = MeshError::Empty;
    }

    #[test]
    fn distant_islands_stay_in_different_clusters() {
        let near = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        let far = [[100.0, 0.0, 0.0], [101.0, 0.0, 0.0], [100.0, 1.0, 0.0], [101.0, 1.0, 0.0]];
        let mut positions = near.to_vec();
        positions.extend_from_slice(&far);
        let indices = vec![0, 1, 2, 4, 5, 6, 1, 3, 2, 5, 7, 6];
        let mesh = mesh_from_surfaces(&[surface(positions, indices, 0)]).expect("islands");
        let set = build_meshlets(&mesh);
        assert!(meshlets_cover_source(&mesh, &set));
        assert!(set.stats.meshlet_count >= 2);
        assert!(set.meshlets.iter().all(|meshlet| meshlet.bounds_max[0] - meshlet.bounds_min[0] < 50.0));
    }

    #[test]
    fn one_triangle_is_one_cluster_and_a_limit_cross_splits() {
        let one = strip(1);
        let single = build_meshlets(&one);
        assert_eq!(single.stats.meshlet_count, 1);
        assert_eq!(single.stats.max_triangles, 1);
        assert!(meshlets_cover_source(&one, &single));
        let over = strip(129);
        let split = build_meshlets(&over);
        assert!(split.stats.meshlet_count >= 2);
        assert!(split.stats.max_triangles <= MESHLET_MAX_TRIANGLES);
        assert!(split.stats.max_vertices <= MESHLET_MAX_VERTICES);
        assert!(meshlets_cover_source(&over, &split));
        assert!(mesh_from_surfaces(&[surface(vec![[0.0, 0.0, 0.0]], Vec::new(), 0)]).is_err());
    }

    #[test]
    fn material_slots_and_seams_stay_attached_to_their_vertices() {
        let first = surface(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![0, 1, 2], 0);
        let second = surface(vec![[2.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 1.0, 0.0]], vec![0, 1, 2], 1);
        let mesh = mesh_from_surfaces(&[first, second]).expect("slots");
        let set = build_meshlets(&mesh);
        assert!(meshlets_cover_source(&mesh, &set));
        assert!(set.meshlets.iter().any(|meshlet| meshlet.material_slot == 0 && meshlet.submesh == 0));
        assert!(set.meshlets.iter().any(|meshlet| meshlet.material_slot == 1 && meshlet.submesh == 1));
        assert!(set.meshlets.iter().all(|meshlet| meshlet.submesh == meshlet.material_slot));
        let seam = surface(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![0, 1, 2, 3, 4, 5],
            0,
        );
        let seamed = mesh_from_surfaces(&[seam]).expect("seam");
        let clusters = build_meshlets(&seamed);
        assert!(meshlets_cover_source(&seamed, &clusters));
        let draw = meshlet_draw(&clusters);
        assert!(draw.indices.contains(&0) && draw.indices.contains(&3));
        let mut again = build_meshlets(&seamed);
        again.stats.build_ms = clusters.stats.build_ms;
        assert_eq!(again.vertex_indices, clusters.vertex_indices);
        assert_eq!(again.local_indices, clusters.local_indices);
        assert_eq!(again.meshlets.len(), clusters.meshlets.len());
    }

    #[test]
    fn a_box_surface_is_covered_by_the_existing_builder() {
        let mesh = crate::box_mesh([2.0, 2.0, 2.0]);
        let set = build_meshlets(&mesh);
        let coverage = meshlet_coverage(&mesh, &set);
        assert_eq!(coverage.canonical_triangles, 12);
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert_eq!(coverage.leaf_triangles, 12);
        // The builder's minimum 8-cell grid splits each quad. One triangle per cluster. The grid is not retuned for the cube.
        assert_eq!(coverage.leaf_meshlets, 12);
        assert_eq!(set.stats.source_triangles, 12);
        assert!(set.meshlets.iter().all(|meshlet| meshlet.index_count / 3 >= 1 && meshlet.cone_axis.iter().all(|lane| lane.is_finite())));
        let records: Vec<crate::GpuMeshletRecord> = set.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        let triangles: u32 = records.iter().map(|record| record.triangles).sum();
        assert_eq!(triangles, 12);
        let hierarchy = crate::build_cluster_hierarchy(&records);
        assert_eq!(hierarchy.leaf_count as usize, records.len());
        assert!(!hierarchy.roots.is_empty());
    }
}
