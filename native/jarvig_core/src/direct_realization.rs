//! Experiment 3C. One analytic object, two observations, direct construction.
//!
//! An omitted surface allocates nothing. An admitted surface evaluates `n` as
//! scalars, then emits that one grid. A rejected `n` is not a mesh. This module
//! does not call `classify_face`, `pack_observation_gpu`, or `build_realization`.

use crate::mesh::{
    create_mesh, Mesh, MeshDesc, MeshError, MeshIndexFormat, MeshTopology, MeshVertexAttribute, MeshVertexFormat, SubmeshDesc, VertexStreamDesc,
};
use crate::microgeometry::projected_detail_px;

/// Bump only if this experiment's direct constructor changes. The 3A and 3B versions stay where they are.
pub const DIRECT_REALIZATION_ALGORITHM_VERSION: u32 = 1;
pub const SURFACE_A: u32 = 1;
pub const SURFACE_B: u32 = 2;
const DIRECT_N_CAP: u32 = 64;
const DIRECT_ERROR_SLACK_PX: f32 = 0.05;
const DIRECT_VERTEX_STRIDE: u32 = 60;
const SPECIMEN_TEXT: &[u8] = b"direct-realization-v1|p=(u,v,1)/sqrt(u*u+v*v+1)|u,v=[-0.5,0.5]|A=1+(3,0,0)|B=2+(0,0,0)|normal=+Z";

/// In-memory identity of the test specimen. Construction does not change it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalyticObject {
    pub content_hash: u64,
    pub revision: u64,
}

impl AnalyticObject {
    pub fn specimen() -> Self {
        Self { content_hash: fnv64(SPECIMEN_TEXT), revision: 1 }
    }
}

/// Object-local camera. The eyes are the frozen 3C pair, not the 3A eyes.
#[derive(Clone, Copy, Debug)]
pub struct DirectCamera {
    pub eye: [f64; 3],
    pub forward: [f64; 3],
    pub up: [f64; 3],
    pub vertical_fov_radians: f64,
    pub near_m: f64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub requested_error_px: f32,
}

/// Why the patch box was kept or left unbuilt. Only `Omit` skips construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchAdmission {
    Omit,
    AdmitInView,
    AdmitBehindCamera,
    AdmitCrossesNear,
    AdmitUncertain,
}

impl PatchAdmission {
    pub fn name(self) -> &'static str {
        match self {
            Self::Omit => "Omit",
            Self::AdmitInView => "AdmitInView",
            Self::AdmitBehindCamera => "AdmitBehindCamera",
            Self::AdmitCrossesNear => "AdmitCrossesNear",
            Self::AdmitUncertain => "AdmitUncertain",
        }
    }

    pub fn uncertain(self) -> bool {
        matches!(self, Self::AdmitBehindCamera | Self::AdmitCrossesNear | Self::AdmitUncertain)
    }
}

/// Scalar result of one `n`. It owns no positions and no mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalarPredicate {
    pub max_px: f32,
    pub samples: u32,
}

/// One emitted triangle. The count of these records is the constructed triangle count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectTriangleRecord {
    pub surface_id: u32,
    pub cell_i: u32,
    pub cell_j: u32,
    pub triangle_in_cell: u8,
}

/// Per-surface account. An omitted surface is the zero account.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceAccount {
    pub surface_id: u32,
    pub admission: PatchAdmission,
    pub n: u32,
    pub triangles_constructed: u32,
    pub candidate_grids_emitted: u32,
    pub candidate_grids_discarded: u32,
    pub resolution_predicate_evaluations: u32,
    pub coarser_failed: bool,
    pub coarser_px: f32,
    pub measured_error_px: f32,
    pub packed_triangles: u32,
    pub packed_vertices: u32,
    pub index_count: u32,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub gpu_bytes: u64,
}

/// One observation product. The mesh is the admitted grid only.
#[derive(Clone, Debug)]
pub struct DirectProduct {
    pub analytic: AnalyticObject,
    pub cache_key: String,
    pub surfaces: [SurfaceAccount; 2],
    pub trace: Vec<DirectTriangleRecord>,
    pub mesh: Mesh,
    pub constructed_triangles: u32,
    pub packed_triangles: u32,
    pub uploaded_triangles: u32,
    pub discarded_after_construction: u32,
    pub discarded_during_pack: u32,
    pub discarded_after_upload: u32,
    pub packed_vertices: u32,
    pub packed_indices: u32,
    pub packed_vertex_bytes: u64,
    pub packed_index_bytes: u64,
    pub expected_gpu_bytes: u64,
    pub cpu_bytes: u64,
    pub index_format: MeshIndexFormat,
    pub position_hash: u64,
    pub index_hash: u64,
    pub meshlets_constructed: u32,
    pub degenerate_normals: u32,
    pub vertices_welded: bool,
    pub object_mesh_consulted: bool,
    pub shared_render_mesh_consulted: bool,
    pub experiment2_mesh_consulted: bool,
    pub einstein_consulted: bool,
    pub candidate_grids_emitted: u32,
    pub candidate_grids_discarded: u32,
    pub resolution_predicate_evaluations: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectUploadProof {
    pub gpu_mesh_bytes: u64,
    pub vertex_create_bytes: u64,
    pub index_create_bytes: u64,
}

struct Chosen {
    n: u32,
    evaluations: u32,
    coarser_failed: bool,
    coarser_px: f32,
}

pub fn far_observation() -> DirectCamera {
    observation_camera([0.0, 0.0, 8.0], 160.0)
}

pub fn close_observation() -> DirectCamera {
    observation_camera([0.0, 0.0, 2.2], 960.0)
}

/// Project the eight AABB corners. This does not call `classify_face`.
pub fn classify_patch(surface_id: u32, camera: &DirectCamera) -> PatchAdmission {
    let Ok(corners) = patch_corners(surface_id) else {
        return PatchAdmission::AdmitUncertain;
    };
    let Some((right, up, forward)) = camera_basis(camera.forward, camera.up) else {
        return PatchAdmission::AdmitUncertain;
    };
    let width = f64::from(camera.viewport_width);
    let height = f64::from(camera.viewport_height);
    let error = f64::from(camera.requested_error_px);
    if !camera.near_m.is_finite() || camera.near_m <= 0.0 || !error.is_finite() || error < 0.0 || width < 1.0 || height < 1.0 {
        return PatchAdmission::AdmitUncertain;
    }
    let half = camera.vertical_fov_radians * 0.5;
    if !half.is_finite() || half <= 0.0 || half >= std::f64::consts::FRAC_PI_2 {
        return PatchAdmission::AdmitUncertain;
    }
    let tan_half_v = half.tan();
    let tan_half_h = tan_half_v * (width / height);
    if !tan_half_v.is_finite() || !tan_half_h.is_finite() || tan_half_v <= 0.0 || tan_half_h <= 0.0 {
        return PatchAdmission::AdmitUncertain;
    }
    let mut depths = [0.0; 8];
    for (index, corner) in corners.iter().enumerate() {
        depths[index] = dot(sub(*corner, camera.eye), forward);
    }
    if depths.iter().any(|depth| !depth.is_finite()) {
        return PatchAdmission::AdmitUncertain;
    }
    if depths.iter().any(|depth| *depth <= 0.0) {
        return PatchAdmission::AdmitBehindCamera;
    }
    if depths.iter().any(|depth| *depth <= camera.near_m) {
        return PatchAdmission::AdmitCrossesNear;
    }
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for (corner, depth) in corners.iter().zip(depths.iter()) {
        let relative = sub(*corner, camera.eye);
        let px = (dot(relative, right) / depth) / tan_half_h;
        let py = (dot(relative, up) / depth) / tan_half_v;
        if !px.is_finite() || !py.is_finite() {
            return PatchAdmission::AdmitUncertain;
        }
        let screen_x = (px * 0.5 + 0.5) * width;
        let screen_y = (py * 0.5 + 0.5) * height;
        min_x = min_x.min(screen_x);
        max_x = max_x.max(screen_x);
        min_y = min_y.min(screen_y);
        max_y = max_y.max(screen_y);
    }
    let misses = max_x < -error || min_x > width + error || max_y < -error || min_y > height + error;
    if misses { PatchAdmission::Omit } else { PatchAdmission::AdmitInView }
}

/// Error of candidate `n` in pixels. The return value is two scalars.
pub fn predicate_max_px(surface_id: u32, n: u32, camera: &DirectCamera) -> Result<ScalarPredicate, String> {
    if n == 0 || n > DIRECT_N_CAP {
        return Err(format!("n {n} is outside 1..={DIRECT_N_CAP}"));
    }
    let Some((_, _, forward)) = camera_basis(camera.forward, camera.up) else {
        return Err("specimen: the direct camera basis is degenerate".into());
    };
    let tan_half = ((camera.vertical_fov_radians * 0.5) as f32).tan();
    if !tan_half.is_finite() || tan_half <= 0.0 || !camera.requested_error_px.is_finite() {
        return Err("specimen: the direct camera field of view is not usable".into());
    }
    let mut max_px = 0.0f32;
    let mut samples = 0u32;
    for j in 0..n {
        for i in 0..n {
            for which in [0u8, 1] {
                let corners = triangle_corners(surface_id, n, i, j, which)?;
                let centroid = parameter_centroid(n, i, j, which)?;
                let point = place(surface_id, centroid.0, centroid.1)?;
                let depth = dot(sub(point, camera.eye), forward);
                if !depth.is_finite() {
                    return Err(format!("specimen: surface {surface_id} sample depth was not finite"));
                }
                if depth < 0.05 {
                    return Err(format!("specimen: surface {surface_id} sample depth {depth} m is under 0.05 m"));
                }
                let Some(deviation) = plane_distance(point, corners[0], corners[1], corners[2]) else {
                    max_px = f32::MAX;
                    samples = samples.saturating_add(1);
                    continue;
                };
                if !deviation.is_finite() {
                    max_px = f32::MAX;
                    samples = samples.saturating_add(1);
                    continue;
                }
                let px = projected_detail_px(deviation as f32, depth as f32, camera.viewport_height, tan_half);
                if !px.is_finite() {
                    max_px = f32::MAX;
                } else {
                    max_px = max_px.max(px);
                }
                samples = samples.saturating_add(1);
            }
        }
    }
    let expected = n.checked_mul(n).and_then(|value| value.checked_mul(2)).ok_or("predicate sample count overflowed")?;
    if samples != expected {
        return Err(format!("predicate sampled {samples} centroids, not {expected}"));
    }
    Ok(ScalarPredicate { max_px, samples })
}

/// Admit or omit, then construct the first passing grid once.
pub fn realize_direct(camera: &DirectCamera) -> Result<DirectProduct, String> {
    let analytic = AnalyticObject::specimen();
    let mut accounts = [zero_account(SURFACE_A), zero_account(SURFACE_B)];
    let mut trace = Vec::new();
    let mut corners = Vec::new();
    for (index, surface_id) in [SURFACE_A, SURFACE_B].into_iter().enumerate() {
        let admission = classify_patch(surface_id, camera);
        if admission.uncertain() {
            return Err(format!("surface {surface_id} is {}; this specimen does not construct a grid", admission.name()));
        }
        if admission == PatchAdmission::Omit {
            accounts[index].admission = PatchAdmission::Omit;
            continue;
        }
        if admission != PatchAdmission::AdmitInView {
            return Err(format!("surface {surface_id} admission {} is not a direct decision", admission.name()));
        }
        let chosen = choose_n(surface_id, camera)?;
        let measured = predicate_max_px(surface_id, chosen.n, camera)?;
        let limit = camera.requested_error_px + DIRECT_ERROR_SLACK_PX;
        if !measured.max_px.is_finite() || measured.max_px > limit {
            return Err(format!("surface {surface_id} measured {:.6} px after emit, above {:.6}", measured.max_px, limit));
        }
        if chosen.n > 1 && (!chosen.coarser_failed || chosen.coarser_px <= camera.requested_error_px) {
            return Err(format!("surface {surface_id} emitted n {} without a failing coarser grid", chosen.n));
        }
        let (mut records, mut grid) = emit_grid(surface_id, chosen.n)?;
        let triangles = records.len() as u32;
        if triangles != chosen.n.saturating_mul(chosen.n).saturating_mul(2) {
            return Err(format!("surface {surface_id} emitted {triangles} triangles for n {}", chosen.n));
        }
        accounts[index] = SurfaceAccount {
            surface_id,
            admission,
            n: chosen.n,
            triangles_constructed: triangles,
            candidate_grids_emitted: 1,
            candidate_grids_discarded: 0,
            resolution_predicate_evaluations: chosen.evaluations,
            coarser_failed: chosen.coarser_failed,
            coarser_px: chosen.coarser_px,
            measured_error_px: measured.max_px,
            packed_triangles: 0,
            packed_vertices: 0,
            index_count: 0,
            vertex_bytes: 0,
            index_bytes: 0,
            gpu_bytes: 0,
        };
        trace.append(&mut records);
        corners.append(&mut grid);
    }
    if trace.iter().any(|record| accounts.iter().any(|account| account.surface_id == record.surface_id && account.admission == PatchAdmission::Omit)) {
        return Err("an omitted surface entered the construction trace".into());
    }
    let packed = pack_direct(&trace, &corners)?;
    for account in &mut accounts {
        if let Some(bytes) = packed.surfaces.iter().find(|item| item.surface_id == account.surface_id) {
            account.packed_triangles = bytes.packed_triangles;
            account.packed_vertices = bytes.packed_vertices;
            account.index_count = bytes.index_count;
            account.vertex_bytes = bytes.vertex_bytes;
            account.index_bytes = bytes.index_bytes;
            account.gpu_bytes = bytes.gpu_bytes;
        }
        if account.admission == PatchAdmission::Omit && (account.packed_triangles != 0 || account.gpu_bytes != 0 || account.triangles_constructed != 0) {
            return Err(format!("omitted surface {} contributed primitives", account.surface_id));
        }
        if account.admission == PatchAdmission::AdmitInView && account.packed_triangles != account.triangles_constructed {
            return Err(format!("surface {} dropped triangles during pack", account.surface_id));
        }
    }
    let summed_vertex = accounts.iter().map(|account| account.vertex_bytes).sum::<u64>();
    let summed_index = accounts.iter().map(|account| account.index_bytes).sum::<u64>();
    if summed_vertex != packed.packed_vertex_bytes || summed_index != packed.packed_index_bytes {
        return Err("per-surface byte accounts do not add up to the observation mesh".into());
    }
    if packed.discarded_during_pack != 0 || packed.constructed_triangles != packed.packed_triangles {
        return Err("the direct pack discarded or split triangles".into());
    }
    Ok(DirectProduct {
        cache_key: cache_key(camera, analytic),
        analytic,
        surfaces: accounts,
        constructed_triangles: packed.constructed_triangles,
        packed_triangles: packed.packed_triangles,
        uploaded_triangles: packed.packed_triangles,
        discarded_after_construction: 0,
        discarded_during_pack: packed.discarded_during_pack,
        discarded_after_upload: 0,
        packed_vertices: packed.packed_vertices,
        packed_indices: packed.packed_indices,
        packed_vertex_bytes: packed.packed_vertex_bytes,
        packed_index_bytes: packed.packed_index_bytes,
        expected_gpu_bytes: packed.expected_gpu_bytes,
        cpu_bytes: packed.packed_vertex_bytes + packed.packed_index_bytes,
        index_format: packed.index_format,
        position_hash: packed.position_hash,
        index_hash: packed.index_hash,
        meshlets_constructed: 0,
        degenerate_normals: packed.degenerate_normals,
        vertices_welded: false,
        object_mesh_consulted: false,
        shared_render_mesh_consulted: false,
        experiment2_mesh_consulted: false,
        einstein_consulted: false,
        candidate_grids_emitted: accounts.iter().map(|account| account.candidate_grids_emitted).sum(),
        candidate_grids_discarded: accounts.iter().map(|account| account.candidate_grids_discarded).sum(),
        resolution_predicate_evaluations: accounts.iter().map(|account| account.resolution_predicate_evaluations).sum(),
        mesh: packed.mesh,
        trace,
    })
}

pub fn format_direct_product(label: &str, product: &DirectProduct, upload: Option<DirectUploadProof>) -> String {
    let mut out = String::new();
    out.push_str(&format!("{label}\n"));
    out.push_str(&format!("analytic_hash: {:016x}\n", product.analytic.content_hash));
    out.push_str(&format!("analytic_revision: {}\n", product.analytic.revision));
    out.push_str(&format!("cache_key: {}\n", product.cache_key));
    out.push_str(&format!("DIRECT_REALIZATION_ALGORITHM_VERSION: {DIRECT_REALIZATION_ALGORITHM_VERSION}\n"));
    for account in &product.surfaces {
        out.push_str(&format!("surface {} admission: {}\n", account.surface_id, account.admission.name()));
        out.push_str(&format!("surface {} n: {}\n", account.surface_id, account.n));
        out.push_str(&format!("surface {} triangles_constructed: {}\n", account.surface_id, account.triangles_constructed));
        out.push_str(&format!("surface {} measured_projected_error_px: {:.6}\n", account.surface_id, account.measured_error_px));
        out.push_str(&format!("surface {} coarser_predicate_failed: {}\n", account.surface_id, yes(account.coarser_failed)));
        out.push_str(&format!("surface {} coarser_px: {:.6}\n", account.surface_id, account.coarser_px));
        out.push_str(&format!("surface {} candidate_grids_emitted: {}\n", account.surface_id, account.candidate_grids_emitted));
        out.push_str(&format!("surface {} candidate_grids_discarded: {}\n", account.surface_id, account.candidate_grids_discarded));
        out.push_str(&format!("surface {} resolution_predicate_evaluations: {}\n", account.surface_id, account.resolution_predicate_evaluations));
        out.push_str(&format!("surface {} packed_triangles: {}\n", account.surface_id, account.packed_triangles));
        out.push_str(&format!("surface {} packed_vertices: {}\n", account.surface_id, account.packed_vertices));
        out.push_str(&format!("surface {} index_count: {}\n", account.surface_id, account.index_count));
        out.push_str(&format!("surface {} vertex_bytes: {}\n", account.surface_id, account.vertex_bytes));
        out.push_str(&format!("surface {} index_bytes: {}\n", account.surface_id, account.index_bytes));
        out.push_str(&format!("surface {} gpu_bytes: {}\n", account.surface_id, account.gpu_bytes));
    }
    out.push_str(&format!("triangles_directly_constructed: {}\n", product.constructed_triangles));
    out.push_str(&format!("packed_triangles: {}\n", product.packed_triangles));
    out.push_str(&format!("uploaded_triangles: {}\n", product.uploaded_triangles));
    out.push_str(&format!("packed_vertices: {}\n", product.packed_vertices));
    out.push_str(&format!("packed_indices: {}\n", product.packed_indices));
    out.push_str(&format!("CPU_bytes: {}\n", product.cpu_bytes));
    out.push_str(&format!("packed_vertex_bytes: {}\n", product.packed_vertex_bytes));
    out.push_str(&format!("packed_index_bytes: {}\n", product.packed_index_bytes));
    out.push_str(&format!("expected_gpu_bytes: {}\n", product.expected_gpu_bytes));
    out.push_str(&format!("index_format: {}\n", index_format_name(product.index_format)));
    out.push_str(&format!("index_byte_size: {}\n", product.index_format.byte_size()));
    out.push_str(&format!("position_hash: {:016x}\n", product.position_hash));
    out.push_str(&format!("index_hash: {:016x}\n", product.index_hash));
    out.push_str(&format!("triangles_discarded_after_construction: {}\n", product.discarded_after_construction));
    out.push_str(&format!("triangles_dropped_during_pack: {}\n", product.discarded_during_pack));
    out.push_str(&format!("triangles_discarded_after_upload: {}\n", product.discarded_after_upload));
    out.push_str(&format!("candidate_grids_emitted: {}\n", product.candidate_grids_emitted));
    out.push_str(&format!("candidate_grids_discarded: {}\n", product.candidate_grids_discarded));
    out.push_str(&format!("resolution_predicate_evaluations: {}\n", product.resolution_predicate_evaluations));
    out.push_str("resolution_predicate_evaluations are a discard count: NO\n");
    out.push_str("rejected n allocated a mesh: NO\n");
    out.push_str(&format!("vertices_welded: {}\n", yes(product.vertices_welded)));
    out.push_str(&format!("meshlets_constructed: {}\n", product.meshlets_constructed));
    out.push_str(&format!("degenerate_normals_kept: {}\n", product.degenerate_normals));
    out.push_str(&format!("object_mesh_consulted: {}\n", yes(product.object_mesh_consulted)));
    out.push_str(&format!("shared_render_mesh_consulted: {}\n", yes(product.shared_render_mesh_consulted)));
    out.push_str(&format!("experiment2_mesh_consulted: {}\n", yes(product.experiment2_mesh_consulted)));
    out.push_str(&format!("einstein_consulted: {}\n", yes(product.einstein_consulted)));
    out.push_str(&format!("construction_trace_records: {}\n", product.trace.len()));
    out.push_str(&format!("construction_trace_contains_surface_1: {}\n", yes(product.trace.iter().any(|record| record.surface_id == SURFACE_A))));
    for record in &product.trace {
        out.push_str(&format!(
            "trace surface {} cell {} {} triangle {}\n",
            record.surface_id, record.cell_i, record.cell_j, record.triangle_in_cell
        ));
    }
    match upload {
        Some(upload) => {
            out.push_str(&format!("GpuMesh.bytes: {}\n", upload.gpu_mesh_bytes));
            out.push_str(&format!("create_buffer_bytes: {}\n", upload.vertex_create_bytes + upload.index_create_bytes));
            out.push_str(&format!("vertex_create_bytes: {}\n", upload.vertex_create_bytes));
            out.push_str(&format!("index_create_bytes: {}\n", upload.index_create_bytes));
        }
        None => {
            out.push_str("GpuMesh.bytes: NOT UPLOADED\n");
            out.push_str("create_buffer_bytes: NOT UPLOADED\n");
        }
    }
    out
}

fn observation_camera(eye: [f64; 3], width: f32) -> DirectCamera {
    DirectCamera {
        eye,
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_radians: 60.0_f64.to_radians(),
        near_m: 0.1,
        viewport_width: width,
        viewport_height: 567.0,
        requested_error_px: 0.5,
    }
}

fn choose_n(surface_id: u32, camera: &DirectCamera) -> Result<Chosen, String> {
    let mut n = 1u32;
    let mut evaluations = 0u32;
    let mut coarser_failed = false;
    let mut coarser_px = 0.0f32;
    loop {
        if n > DIRECT_N_CAP {
            return Err(format!("surface {surface_id} stayed above {:.3} px through n = {DIRECT_N_CAP}", camera.requested_error_px));
        }
        let scalar = predicate_max_px(surface_id, n, camera)?;
        evaluations = evaluations.saturating_add(1);
        if scalar.max_px <= camera.requested_error_px {
            return Ok(Chosen { n, evaluations, coarser_failed, coarser_px });
        }
        coarser_failed = true;
        coarser_px = scalar.max_px;
        n = n.saturating_add(1);
    }
}

fn emit_grid(surface_id: u32, n: u32) -> Result<(Vec<DirectTriangleRecord>, Vec<[[f64; 3]; 3]>), String> {
    let mut records = Vec::new();
    let mut corners = Vec::new();
    for j in 0..n {
        for i in 0..n {
            for which in [0u8, 1] {
                let triangle = triangle_corners(surface_id, n, i, j, which)?;
                records.push(DirectTriangleRecord { surface_id, cell_i: i, cell_j: j, triangle_in_cell: which });
                corners.push(triangle);
            }
        }
    }
    Ok((records, corners))
}

struct PackedSurface {
    surface_id: u32,
    packed_triangles: u32,
    packed_vertices: u32,
    index_count: u32,
    vertex_bytes: u64,
    index_bytes: u64,
    gpu_bytes: u64,
}

struct PackedDirect {
    mesh: Mesh,
    surfaces: [PackedSurface; 2],
    constructed_triangles: u32,
    packed_triangles: u32,
    packed_vertices: u32,
    packed_indices: u32,
    packed_vertex_bytes: u64,
    packed_index_bytes: u64,
    expected_gpu_bytes: u64,
    index_format: MeshIndexFormat,
    position_hash: u64,
    index_hash: u64,
    degenerate_normals: u32,
    discarded_during_pack: u32,
}

fn pack_direct(records: &[DirectTriangleRecord], corners: &[[[f64; 3]; 3]]) -> Result<PackedDirect, String> {
    if records.is_empty() || records.len() != corners.len() {
        return Err("the direct product constructed no triangles to pack".into());
    }
    let packed_triangles = u32::try_from(records.len()).map_err(|_| "too many triangles")?;
    let packed_vertices = packed_triangles.checked_mul(3).ok_or("vertex count overflowed")?;
    let packed_indices = packed_vertices;
    let index_format = if packed_vertices > u32::from(u16::MAX) { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 };
    let index_stride = u64::from(index_format.byte_size());
    let packed_vertex_bytes = u64::from(packed_vertices) * u64::from(DIRECT_VERTEX_STRIDE);
    let packed_index_bytes = u64::from(packed_indices) * index_stride;
    let mut bytes = Vec::with_capacity(packed_vertex_bytes as usize);
    let mut index_bytes = Vec::with_capacity(packed_index_bytes as usize);
    let mut position_bytes = Vec::with_capacity((packed_vertices as usize) * 12);
    let mut degenerate_normals = 0u32;
    for (triangle_index, (record, triangle)) in records.iter().zip(corners.iter()).enumerate() {
        if record.surface_id != SURFACE_A && record.surface_id != SURFACE_B {
            return Err(format!("trace names unknown surface {}", record.surface_id));
        }
        if record.triangle_in_cell > 1 {
            return Err("trace names a triangle outside the cell".into());
        }
        let packed = [
            f32_corner(triangle[0])?,
            f32_corner(triangle[1])?,
            f32_corner(triangle[2])?,
        ];
        let (normal, degenerate) = packed_normal(packed[0], packed[1], packed[2]);
        if degenerate {
            degenerate_normals = degenerate_normals.saturating_add(1);
        }
        let base = (triangle_index as u32).saturating_mul(3);
        for corner in packed {
            for value in corner {
                position_bytes.extend_from_slice(&value.to_le_bytes());
            }
            push_vertex(&mut bytes, corner, normal);
        }
        for offset in 0..3u32 {
            push_index(&mut index_bytes, index_format, base + offset);
        }
    }
    if bytes.len() as u64 != packed_vertex_bytes || index_bytes.len() as u64 != packed_index_bytes {
        return Err("packed byte lengths drifted from the triangle count".into());
    }
    let mesh = create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: DIRECT_VERTEX_STRIDE,
            attributes: vec![
                MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
            ],
            bytes,
        }],
        index_format,
        index_bytes: index_bytes.clone(),
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: packed_indices,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .map_err(|error: MeshError| error.to_string())?;
    let mut surfaces = [packed_surface(SURFACE_A, 0, index_stride), packed_surface(SURFACE_B, 0, index_stride)];
    for record in records {
        let slot = if record.surface_id == SURFACE_A { 0 } else { 1 };
        surfaces[slot].packed_triangles = surfaces[slot].packed_triangles.saturating_add(1);
    }
    for surface in &mut surfaces {
        surface.packed_vertices = surface.packed_triangles.saturating_mul(3);
        surface.index_count = surface.packed_vertices;
        surface.vertex_bytes = u64::from(surface.packed_vertices) * u64::from(DIRECT_VERTEX_STRIDE);
        surface.index_bytes = u64::from(surface.index_count) * index_stride;
        surface.gpu_bytes = surface.vertex_bytes + surface.index_bytes;
    }
    Ok(PackedDirect {
        mesh,
        surfaces,
        constructed_triangles: packed_triangles,
        packed_triangles,
        packed_vertices,
        packed_indices,
        packed_vertex_bytes,
        packed_index_bytes,
        expected_gpu_bytes: packed_vertex_bytes + packed_index_bytes,
        index_format,
        position_hash: fnv64(&position_bytes),
        index_hash: fnv64(&index_bytes),
        degenerate_normals,
        discarded_during_pack: 0,
    })
}

fn packed_surface(surface_id: u32, triangles: u32, index_stride: u64) -> PackedSurface {
    let vertices = triangles.saturating_mul(3);
    let vertex_bytes = u64::from(vertices) * u64::from(DIRECT_VERTEX_STRIDE);
    let index_bytes = u64::from(vertices) * index_stride;
    PackedSurface {
        surface_id,
        packed_triangles: triangles,
        packed_vertices: vertices,
        index_count: vertices,
        vertex_bytes,
        index_bytes,
        gpu_bytes: vertex_bytes + index_bytes,
    }
}

fn zero_account(surface_id: u32) -> SurfaceAccount {
    SurfaceAccount {
        surface_id,
        admission: PatchAdmission::AdmitUncertain,
        n: 0,
        triangles_constructed: 0,
        candidate_grids_emitted: 0,
        candidate_grids_discarded: 0,
        resolution_predicate_evaluations: 0,
        coarser_failed: false,
        coarser_px: 0.0,
        measured_error_px: 0.0,
        packed_triangles: 0,
        packed_vertices: 0,
        index_count: 0,
        vertex_bytes: 0,
        index_bytes: 0,
        gpu_bytes: 0,
    }
}

fn cache_key(camera: &DirectCamera, analytic: AnalyticObject) -> String {
    let milli = (f64::from(camera.requested_error_px) * 1000.0).round() as i32;
    let near = (camera.near_m / 0.01).round() as i32;
    format!(
        "direct-h{:016x}-r{}-e{:.4}-{:.4}-{:.4}-f{:.0}-{:.0}-{:.0}-px{milli}-w{:.0}-h{:.0}-n{near}-a{}",
        analytic.content_hash,
        analytic.revision,
        camera.eye[0],
        camera.eye[1],
        camera.eye[2],
        camera.forward[0],
        camera.forward[1],
        camera.forward[2],
        camera.viewport_width,
        camera.viewport_height,
        DIRECT_REALIZATION_ALGORITHM_VERSION
    )
}

fn patch_corners(surface_id: u32) -> Result<[[f64; 3]; 8], String> {
    let x_pos = place(surface_id, 0.5, 0.0)?;
    let x_neg = place(surface_id, -0.5, 0.0)?;
    let y_pos = place(surface_id, 0.0, 0.5)?;
    let y_neg = place(surface_id, 0.0, -0.5)?;
    let z_max_point = place(surface_id, 0.0, 0.0)?;
    let z00 = place(surface_id, -0.5, -0.5)?;
    let z01 = place(surface_id, -0.5, 0.5)?;
    let z10 = place(surface_id, 0.5, -0.5)?;
    let z11 = place(surface_id, 0.5, 0.5)?;
    let xmin = x_neg[0].min(x_pos[0]);
    let xmax = x_neg[0].max(x_pos[0]);
    let ymin = y_neg[1].min(y_pos[1]);
    let ymax = y_neg[1].max(y_pos[1]);
    let zmin = z00[2].min(z01[2]).min(z10[2]).min(z11[2]);
    let zmax = z_max_point[2];
    let mut corners = [[0.0; 3]; 8];
    let mut index = 0;
    for z in [zmin, zmax] {
        for y in [ymin, ymax] {
            for x in [xmin, xmax] {
                corners[index] = [x, y, z];
                index += 1;
            }
        }
    }
    Ok(corners)
}

fn parameter(n: u32, index: u32) -> f64 {
    -0.5 + f64::from(index) / f64::from(n)
}

fn parameter_centroid(n: u32, i: u32, j: u32, which: u8) -> Result<(f64, f64), String> {
    let uv = triangle_parameters(n, i, j, which)?;
    Ok(((uv[0].0 + uv[1].0 + uv[2].0) / 3.0, (uv[0].1 + uv[1].1 + uv[2].1) / 3.0))
}

fn triangle_parameters(n: u32, i: u32, j: u32, which: u8) -> Result<[(f64, f64); 3], String> {
    if i >= n || j >= n {
        return Err("cell is outside the grid".into());
    }
    let (a, b, c) = match which {
        0 => ((i, j), (i + 1, j), (i + 1, j + 1)),
        1 => ((i, j), (i + 1, j + 1), (i, j + 1)),
        _ => return Err("a cell has only two triangles".into()),
    };
    Ok([
        (parameter(n, a.0), parameter(n, a.1)),
        (parameter(n, b.0), parameter(n, b.1)),
        (parameter(n, c.0), parameter(n, c.1)),
    ])
}

fn triangle_corners(surface_id: u32, n: u32, i: u32, j: u32, which: u8) -> Result<[[f64; 3]; 3], String> {
    let uv = triangle_parameters(n, i, j, which)?;
    Ok([place(surface_id, uv[0].0, uv[0].1)?, place(surface_id, uv[1].0, uv[1].1)?, place(surface_id, uv[2].0, uv[2].1)?])
}

fn place(surface_id: u32, u: f64, v: f64) -> Result<[f64; 3], String> {
    let shift = match surface_id {
        SURFACE_A => [3.0, 0.0, 0.0],
        SURFACE_B => [0.0, 0.0, 0.0],
        _ => return Err(format!("unknown surface {surface_id}")),
    };
    let radius = (u * u + v * v + 1.0).sqrt();
    if !radius.is_finite() || radius <= 0.0 {
        return Err("spherical square radius was not finite".into());
    }
    let point = [u / radius + shift[0], v / radius + shift[1], 1.0 / radius + shift[2]];
    if point.iter().any(|axis| !axis.is_finite()) {
        return Err("spherical square point was not finite".into());
    }
    Ok(point)
}

fn plane_distance(point: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    let normal = cross(sub(b, a), sub(c, a));
    let length = (dot(normal, normal)).sqrt();
    if !length.is_finite() || length < 1.0e-18 {
        return None;
    }
    Some(dot(sub(point, a), normal).abs() / length)
}

fn f32_corner(corner: [f64; 3]) -> Result<[f32; 3], String> {
    let packed = [corner[0] as f32, corner[1] as f32, corner[2] as f32];
    if packed.iter().any(|axis| !axis.is_finite()) {
        return Err("a packed corner was not finite".into());
    }
    Ok(packed)
}

fn packed_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> ([f32; 3], bool) {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
    let length_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    if !length_sq.is_finite() || length_sq < 1.0e-16 {
        return ([0.0, 1.0, 0.0], true);
    }
    let inverse = 1.0 / length_sq.sqrt();
    ([normal[0] * inverse, normal[1] * inverse, normal[2] * inverse], false)
}

fn push_vertex(bytes: &mut Vec<u8>, position: [f32; 3], normal: [f32; 3]) {
    for value in position.into_iter().chain([1.0, 1.0, 1.0]).chain([0.0, 0.0]).chain(normal).chain([1.0, 0.0, 0.0, 1.0]) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_index(bytes: &mut Vec<u8>, format: MeshIndexFormat, index: u32) {
    match format {
        MeshIndexFormat::Uint16 => bytes.extend_from_slice(&(index as u16).to_le_bytes()),
        MeshIndexFormat::Uint32 => bytes.extend_from_slice(&index.to_le_bytes()),
    }
}

fn index_format_name(format: MeshIndexFormat) -> &'static str {
    match format {
        MeshIndexFormat::Uint16 => "Uint16",
        MeshIndexFormat::Uint32 => "Uint32",
    }
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn yes(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

fn camera_basis(forward: [f64; 3], up_hint: [f64; 3]) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    let forward = normalize(forward)?;
    let hint = match normalize(up_hint) {
        Some(hint) if dot(hint, forward).abs() <= 0.999 => hint,
        _ => {
            if forward[1].abs() < 0.9 {
                [0.0, 1.0, 0.0]
            } else {
                [0.0, 0.0, 1.0]
            }
        }
    };
    let right = normalize(cross(forward, hint))?;
    let up = cross(right, forward);
    Some((right, up, forward))
}

fn normalize(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = (dot(value, value)).sqrt();
    if !length.is_finite() || length < 1.0e-12 {
        None
    } else {
        Some(scale(value, 1.0 / length))
    }
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
    [left[1] * right[2] - left[2] * right[1], left[2] * right[0] - left[0] * right[2], left[0] * right[1] - left[1] * right[0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_realization_constructs_only_the_passing_grid() {
        println!("Frame: NOT PRESENTED");
        assert_eq!(std::mem::size_of::<ScalarPredicate>(), 8);
        assert!(!std::mem::needs_drop::<ScalarPredicate>());
        assert_eq!(DIRECT_REALIZATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::observation::OBSERVATION_ALGORITHM_VERSION, 1);
        assert_eq!(crate::view_realization::REALIZATION_ALGORITHM_VERSION, 1);
        assert_eq!(DIRECT_VERTEX_STRIDE, crate::observation::OBSERVATION_VERTEX_STRIDE);
        let far_camera = far_observation();
        let close_camera = close_observation();
        assert_eq!(classify_patch(SURFACE_A, &far_camera), PatchAdmission::Omit);
        assert_eq!(classify_patch(SURFACE_A, &close_camera), PatchAdmission::Omit);
        assert_eq!(classify_patch(SURFACE_B, &far_camera), PatchAdmission::AdmitInView);
        assert_eq!(classify_patch(SURFACE_B, &close_camera), PatchAdmission::AdmitInView);
        let far = realize_direct(&far_camera).expect("far realization");
        let close = realize_direct(&close_camera).expect("close realization");
        println!("{}", format_direct_product("Observation Far", &far, None));
        println!("{}", format_direct_product("Observation Close", &close, None));
        assert_eq!(far.analytic, close.analytic);
        assert_eq!(far.analytic, AnalyticObject::specimen());
        assert_eq!(far.analytic.revision, 1);
        let n_far = far.surfaces[1].n;
        let n_close = close.surfaces[1].n;
        println!("n_far: {n_far}");
        println!("n_close: {n_close}");
        assert!(n_close > n_far && n_far >= 1, "n_close {n_close} n_far {n_far}");
        for (label, product, camera) in [("far", &far, &far_camera), ("close", &close, &close_camera)] {
            let omitted = &product.surfaces[0];
            let admitted = &product.surfaces[1];
            assert_eq!(omitted.surface_id, SURFACE_A, "{label}");
            assert_eq!(admitted.surface_id, SURFACE_B, "{label}");
            assert_eq!(omitted.admission, PatchAdmission::Omit, "{label}");
            assert_eq!(admitted.admission, PatchAdmission::AdmitInView, "{label}");
            assert_eq!(omitted.triangles_constructed, 0, "{label}");
            assert_eq!(omitted.packed_triangles, 0, "{label}");
            assert_eq!(omitted.index_count, 0, "{label}");
            assert_eq!(omitted.vertex_bytes, 0, "{label}");
            assert_eq!(omitted.index_bytes, 0, "{label}");
            assert_eq!(omitted.gpu_bytes, 0, "{label}");
            assert_eq!(omitted.candidate_grids_emitted, 0, "{label}");
            assert_eq!(omitted.candidate_grids_discarded, 0, "{label}");
            assert_eq!(omitted.resolution_predicate_evaluations, 0, "{label}");
            assert!(!product.trace.iter().any(|record| record.surface_id == SURFACE_A), "{label}");
            assert_eq!(admitted.candidate_grids_emitted, 1, "{label}");
            assert_eq!(admitted.candidate_grids_discarded, 0, "{label}");
            assert_eq!(admitted.resolution_predicate_evaluations, admitted.n, "{label}");
            assert_eq!(admitted.triangles_constructed, admitted.n.saturating_mul(admitted.n).saturating_mul(2), "{label}");
            assert!(admitted.measured_error_px <= camera.requested_error_px + DIRECT_ERROR_SLACK_PX, "{label}");
            if admitted.n > 1 {
                assert!(admitted.coarser_failed, "{label}");
                let coarser = predicate_max_px(SURFACE_B, admitted.n - 1, camera).expect("coarser scalar");
                assert!(coarser.max_px > camera.requested_error_px, "{label} coarser {}", coarser.max_px);
                assert_eq!(std::mem::size_of_val(&coarser), 8);
            }
            assert_eq!(product.discarded_after_construction, 0, "{label}");
            assert_eq!(product.discarded_during_pack, 0, "{label}");
            assert_eq!(product.discarded_after_upload, 0, "{label}");
            assert_eq!(product.constructed_triangles, product.packed_triangles, "{label}");
            assert_eq!(product.packed_triangles, product.uploaded_triangles, "{label}");
            assert_eq!(product.trace.len() as u32, product.constructed_triangles, "{label}");
            assert_eq!(product.packed_vertices, product.constructed_triangles.saturating_mul(3), "{label}");
            assert_eq!(product.packed_indices, product.packed_vertices, "{label}");
            assert!(!product.vertices_welded, "{label}");
            assert_eq!(product.cpu_bytes, product.packed_vertex_bytes + product.packed_index_bytes, "{label}");
            assert_eq!(product.expected_gpu_bytes, product.cpu_bytes, "{label}");
            assert_eq!(product.meshlets_constructed, 0, "{label}");
            assert!(!product.object_mesh_consulted && !product.shared_render_mesh_consulted && !product.experiment2_mesh_consulted && !product.einstein_consulted);
            for record in &product.trace {
                assert_eq!(record.surface_id, SURFACE_B);
                assert!(record.cell_i < admitted.n && record.cell_j < admitted.n);
                assert!(record.triangle_in_cell <= 1);
            }
        }
        assert_ne!(far.position_hash, close.position_hash);
        assert_ne!(far.index_hash, close.index_hash);
        assert_ne!(far.expected_gpu_bytes, close.expected_gpu_bytes);
        assert_ne!(far.packed_vertices, 144);
        assert_ne!(close.packed_vertices, 144);
        assert_ne!(far.constructed_triangles, 48);
        assert_ne!(close.constructed_triangles, 37);
        println!("Frame: NOT PRESENTED");
    }
}
