//! Procedural detail module.
//!
//! `linked_detail(false)` is disabled. `linked_detail(true)` is the flat
//! reference marker from `detail_provider`.

use crate::detail_provider::{public_detail, DisabledDetail, ProceduralMicrogeometry, ReferenceDetail};

pub const PROCEDURAL_MICROGEOMETRY_DEFAULT: bool = false;
pub const DETAIL_ERROR_THRESHOLD_PX: f32 = 1.0;
pub const DETAIL_FEATURE_SIZE_M: f32 = 0.02;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailQuery {
    pub enabled: bool,
    pub seed: u64,
    pub uv: [f32; 2],
    pub projected_error_px: f32,
}

impl DetailQuery {
    pub fn new(seed: u64, uv: [f32; 2], projected_error_px: f32) -> Self {
        Self { enabled: PROCEDURAL_MICROGEOMETRY_DEFAULT, seed, uv, projected_error_px }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailRule {
    Flat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailSample {
    pub rule: DetailRule,
    pub feature: u32,
    pub local: [f32; 2],
    pub offset: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DetailDecision {
    OrdinaryMaps,
    Fine(DetailSample),
}

pub fn evaluate_detail(query: &DetailQuery, sample: impl Fn(u64, [f32; 2]) -> DetailSample) -> DetailDecision {
    if !query.enabled || !query.projected_error_px.is_finite() || query.projected_error_px <= DETAIL_ERROR_THRESHOLD_PX || !query.uv[0].is_finite() || !query.uv[1].is_finite() {
        return DetailDecision::OrdinaryMaps;
    }
    DetailDecision::Fine(sample(query.seed, query.uv))
}

pub fn evaluate_procedural_microgeometry(query: &DetailQuery) -> DetailDecision {
    evaluate_detail(query, zero_sample)
}

pub fn projected_detail_px(feature_size_m: f32, depth_m: f32, viewport_height: f32, tan_half_fov: f32) -> f32 {
    if !feature_size_m.is_finite() || !depth_m.is_finite() || !viewport_height.is_finite() || !tan_half_fov.is_finite() {
        return 0.0;
    }
    let depth = depth_m.max(0.05);
    let tan_half = tan_half_fov.max(0.0001);
    feature_size_m * (viewport_height.max(1.0) * 0.5) / (depth * tan_half)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetailProbe {
    pub samples: u32,
    pub ordinary: u32,
    pub fine: u32,
    pub generation_us: u32,
    pub fingerprint: u64,
}

pub fn probe_detail(uvs: &[[f32; 2]], seed: u64, projected_error_px: f32, enabled: bool) -> DetailProbe {
    let mut ordinary = 0u32;
    let mut fine = 0u32;
    for uv in uvs {
        match evaluate_procedural_microgeometry(&DetailQuery { enabled, seed, uv: *uv, projected_error_px }) {
            DetailDecision::OrdinaryMaps => ordinary = ordinary.saturating_add(1),
            DetailDecision::Fine(_) => fine = fine.saturating_add(1),
        }
    }
    DetailProbe { samples: uvs.len() as u32, ordinary, fine, generation_us: 0, fingerprint: seed ^ u64::from(ordinary) ^ u64::from(fine) }
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceAnchor {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    pub depth_m: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MicroBudget {
    pub max_patches: u32,
    pub max_vertices: u32,
    pub max_triangles: u32,
    pub amplitude_m: f32,
    pub patch_size_m: f32,
}

impl MicroBudget {
    pub const fn standard() -> Self {
        Self { max_patches: 1, max_vertices: 3, max_triangles: 1, amplitude_m: 0.0, patch_size_m: 0.05 }
    }

    pub const fn surface() -> Self {
        Self::standard()
    }
}

pub struct LocalPatch {
    pub positions: [[f32; 3]; 9],
    pub triangles: [[u8; 3]; 8],
    pub triangle_count: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MicroSpan {
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
}

#[derive(Clone, Debug)]
pub struct MicroMesh {
    pub vertices: Vec<u8>,
    pub indices: Vec<u32>,
    pub spans: Vec<MicroSpan>,
    pub patches: u32,
    pub samples: u32,
    pub ordinary: u32,
    pub triangle_count: u32,
    pub vertex_count: u32,
    pub fallbacks: u32,
    pub max_displacement_m: f32,
    pub fingerprint: u64,
    pub generation_us: u32,
    pub invalid: u32,
}

pub fn group_micro_spans(mesh: &MicroMesh, _max_vertex_bytes: usize) -> Vec<MicroSpan> {
    if mesh.vertex_count == 0 || mesh.indices.is_empty() {
        return Vec::new();
    }
    vec![MicroSpan { vertex_start: 0, vertex_count: mesh.vertex_count, index_start: 0, index_count: mesh.indices.len() as u32 }]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailProvider {
    Disabled,
    Reference,
}

pub fn surface_level(projected_px: f32) -> u32 {
    u32::from(projected_px > DETAIL_ERROR_THRESHOLD_PX)
}

pub fn build_procedural_microtriangles(
    anchors: &[SurfaceAnchor],
    seed: u64,
    enabled: bool,
    viewport_height: f32,
    tan_half_fov: f32,
    _budget: &MicroBudget,
    provider: DetailProvider,
) -> MicroMesh {
    build_procedural_microtriangles_cancellable(anchors, seed, enabled, viewport_height, tan_half_fov, _budget, provider, &|| false).unwrap_or_else(|_| blank(seed))
}

pub fn build_procedural_microtriangles_cancellable(
    anchors: &[SurfaceAnchor],
    seed: u64,
    enabled: bool,
    viewport_height: f32,
    tan_half_fov: f32,
    _budget: &MicroBudget,
    provider: DetailProvider,
    cancel: &dyn Fn() -> bool,
) -> Result<MicroMesh, String> {
    let detail: Box<dyn ProceduralMicrogeometry> = match provider {
        DetailProvider::Disabled => Box::new(DisabledDetail),
        DetailProvider::Reference => Box::new(ReferenceDetail),
    };
    detail.build(anchors, seed, enabled, viewport_height, tan_half_fov, cancel)
}

pub fn linked_detail(surface: bool) -> Box<dyn ProceduralMicrogeometry> {
    public_detail(surface)
}

fn zero_sample(_seed: u64, _uv: [f32; 2]) -> DetailSample {
    DetailSample { rule: DetailRule::Flat, feature: 0, local: [0.0, 0.0], offset: [0.0, 0.0] }
}

fn blank(seed: u64) -> MicroMesh {
    MicroMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        spans: Vec::new(),
        patches: 0,
        samples: 0,
        ordinary: 0,
        triangle_count: 0,
        vertex_count: 0,
        fallbacks: 0,
        max_displacement_m: 0.0,
        fingerprint: seed,
        generation_us: 0,
        invalid: 0,
    }
}
