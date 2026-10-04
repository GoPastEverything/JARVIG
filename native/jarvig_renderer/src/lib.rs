//! Engine-owned frame path.
//!
//! A host calls [`Renderer::render_target`]. It does not encode passes.
//! One target frame is acquired, every view of that target is drawn, and the
//! target is presented once. Views share the meshes and the compiled material.
//! Each view has its own camera, origin, viewport, and depth. A material
//! instance is parameter data, not a second pipeline.
//!
//! A logical mesh has no wgpu type and no RHI type. [`GpuResidency`] is the only
//! GPU-side record. This crate is single-threaded: the device owner calls `render_target`.

mod output;
mod probe_capture;
mod shadow_pass;
mod detail_private;

pub use shadow_pass::ShadowDiagnostics;

pub use output::{exposure_multiplier, PresentationMode, EXPOSURE_EV_MAX, EXPOSURE_EV_MIN};

use std::collections::HashSet;

use jarvig_core::{
    instance_gpu_transforms, reflection_probe_packet, render_light_record, select_reflection_probe, world_grid_phase, Camera, EntityId, EnvironmentLight,
    GpuEnvironmentPacket, GpuLightRecord, GpuReflectionProbePacket, GpuTransforms, Mat4, Mesh, MeshError, MeshId, MeshIndexFormat, MeshLibrary,
    MeshVertexFormat, ProbeId, RenderInstanceId, RenderLight, RenderSceneSnapshot, ResolvedPose, SpaceError, TextureLibrary, DEPTH_CLEAR,
    REFLECTION_PROBE_MIP_COUNT,
};
use jarvig_material::{
    ColorSpace, CompiledMaterial, MaterialInstanceId, MaterialLibrary, MasterMaterialId, ResourceClass, SamplerId, SamplerState,
    ShadingModel, TextureId as LogicalTextureId, TextureSemantic, VertexSemantic,
};

use jarvig_rhi::{
    AcquireFrame, BindGroupDesc, BindGroupEntry, BindGroupId, BindGroupLayoutDesc, BindGroupLayoutEntry, BindGroupLayoutId,
    BindingType, BufferDesc, BufferId, BufferUsage, ClearColor, ColorAttachment, CompareFunction, CullMode, DepthAttachment, DepthLoadOp,
    DepthState, Device, IndexFormat, LoadOp, PipelineId, PrimitiveTopology, RenderPassDesc, RenderPipelineDesc, ResourceKind,
    ResourceStats, RhiError, ScissorRect, ShaderModuleDesc, ShaderModuleId, ShaderSource, ShaderStage, StoreOp, SwapchainId,
    TextureDesc, TextureFormat, TextureId, TextureUsage, TextureViewId, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexStepMode, Viewport, JARVIG_CLEAR,
};

pub use jarvig_rhi::JARVIG_CLEAR as CLEAR;

/// Renderer-level identity. Not an RHI resource id and not a C ABI handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderViewId {
    pub index: u32,
    pub generation: u32,
}

impl RenderViewId {
    pub const INVALID: Self = Self { index: 0, generation: 0 };
}

/// Where pixels go. Surface is implemented. A texture target is a later ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderTargetKind {
    Surface,
}

/// Target identity. Not a window, and not a [`RenderViewId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderTargetId {
    pub index: u32,
    pub generation: u32,
}

/// Fraction of the target. (0, 0) is the top-left. Width and height are in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl NormalizedRect {
    pub const FULL: Self = Self { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
    pub const LEFT: Self = Self { x: 0.0, y: 0.0, width: 0.5, height: 1.0 };
    pub const RIGHT: Self = Self { x: 0.5, y: 0.0, width: 0.5, height: 1.0 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// View settings. Not a show-flag system and not scene state.
/// `exposure_ev` is stops. Zero is the baseline. It is not a material parameter.
/// `lighting` isolates contributors for inspection. It does not edit the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderViewSettings {
    pub exposure_ev: f32,
    pub lighting: LightingDebug,
    /// Debug tint of cascade regions. View state. It does not change the light sum when off, and it is not saved.
    pub cascade_debug: bool,
    /// Short screen-space contact shadows. A quality toggle, not a level property.
    pub contact_shadows: bool,
    /// Display dither. View state. It does not change the scene color target.
    pub output_dither: bool,
    /// Which presentation the output pass shows. Not saved.
    pub presentation: output::PresentationMode,
    /// 0 full shading. 1 base color, 2 normal, 3 roughness, 4 AO, 5 metallic. Not saved.
    pub material_channel: u32,
}

impl Default for RenderViewSettings {
    fn default() -> Self {
        Self {
            exposure_ev: 0.0,
            lighting: LightingDebug::default(),
            cascade_debug: false,
            contact_shadows: true,
            output_dither: true,
            presentation: output::PresentationMode::Tonemap,
            material_channel: 0,
        }
    }
}

/// Which shading terms a view adds. Default is every term.
/// A zero GPU mask means unrestricted full lighting, so untouched views stay on the old path.
/// This does not change a BRDF. It only keeps a term out of the sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightingDebug {
    pub directional: bool,
    pub point: bool,
    pub spot: bool,
    pub env_diffuse: bool,
    pub env_specular: bool,
    pub probe: bool,
    pub emissive: bool,
    /// Direct-light visibility. Does not darken emissive, environment, or probe radiance.
    pub shadows: bool,
    /// Local bounced diffuse from the probe irradiance. Not the sky hemisphere and not a constant.
    pub indirect: bool,
}

pub const LIGHTING_DEBUG_DIRECT: u32 = 1;
pub const LIGHTING_DEBUG_ENV_DIFFUSE: u32 = 2;
pub const LIGHTING_DEBUG_ENV_SPECULAR: u32 = 4;
pub const LIGHTING_DEBUG_PROBE: u32 = 8;
pub const LIGHTING_DEBUG_EMISSIVE: u32 = 16;
/// Direct-light visibility. A zero mask still means shadows are on.
pub const LIGHTING_DEBUG_SHADOWS: u32 = 32;
/// Local indirect diffuse. A zero mask still means this term is on.
pub const LIGHTING_DEBUG_INDIRECT: u32 = 64;
/// Set whenever the mask is not "everything". Term bits are then authoritative.
pub const LIGHTING_DEBUG_ACTIVE: u32 = 0x8000_0000;

impl Default for LightingDebug {
    fn default() -> Self {
        Self {
            directional: true,
            point: true,
            spot: true,
            env_diffuse: true,
            env_specular: true,
            probe: true,
            emissive: true,
            shadows: true,
            indirect: true,
        }
    }
}

impl LightingDebug {
    pub fn direct_only() -> Self {
        Self { env_diffuse: false, env_specular: false, probe: false, emissive: false, indirect: false, ..Self::default() }
    }

    pub fn environment_diffuse_only() -> Self {
        Self {
            directional: false,
            point: false,
            spot: false,
            env_diffuse: true,
            env_specular: false,
            probe: false,
            emissive: false,
            shadows: true,
            indirect: false,
        }
    }

    pub fn environment_specular_only() -> Self {
        Self {
            directional: false,
            point: false,
            spot: false,
            env_diffuse: false,
            env_specular: true,
            probe: false,
            emissive: false,
            shadows: true,
            indirect: false,
        }
    }

    pub fn probe_specular_only() -> Self {
        Self {
            directional: false,
            point: false,
            spot: false,
            env_diffuse: false,
            env_specular: false,
            probe: true,
            emissive: false,
            shadows: true,
            indirect: false,
        }
    }

    pub fn emissive_only() -> Self {
        Self {
            directional: false,
            point: false,
            spot: false,
            env_diffuse: false,
            env_specular: false,
            probe: false,
            emissive: true,
            shadows: true,
            indirect: false,
        }
    }

    pub fn directional_only() -> Self {
        Self {
            directional: true,
            point: false,
            spot: false,
            env_diffuse: false,
            env_specular: false,
            probe: false,
            emissive: false,
            shadows: true,
            indirect: false,
        }
    }

    pub fn point_only() -> Self {
        Self {
            directional: false,
            point: true,
            spot: false,
            env_diffuse: false,
            env_specular: false,
            probe: false,
            emissive: false,
            shadows: true,
            indirect: false,
        }
    }

    pub fn spot_only() -> Self {
        Self {
            directional: false,
            point: false,
            spot: true,
            env_diffuse: false,
            env_specular: false,
            probe: false,
            emissive: false,
            shadows: true,
            indirect: false,
        }
    }

    /// Direct lights with visibility forced off. Environment, probes, bounce, and emissive stay out.
    pub fn direct_unshadowed() -> Self {
        Self {
            env_diffuse: false,
            env_specular: false,
            probe: false,
            emissive: false,
            shadows: false,
            indirect: false,
            ..Self::default()
        }
    }

    /// Every shading term, with direct-light visibility forced off. Not an ambient term.
    pub fn shadows_disabled() -> Self {
        Self { shadows: false, ..Self::default() }
    }

    /// Captured cosine irradiance only. Not the analytical sky and not a direct light.
    pub fn indirect_diffuse_only() -> Self {
        Self {
            directional: false,
            point: false,
            spot: false,
            env_diffuse: false,
            env_specular: false,
            probe: false,
            emissive: false,
            shadows: false,
            indirect: true,
        }
    }

    pub fn allows_kind(self, kind: jarvig_core::LightKind) -> bool {
        match kind {
            jarvig_core::LightKind::Directional => self.directional,
            jarvig_core::LightKind::Point => self.point,
            jarvig_core::LightKind::Spot => self.spot,
        }
    }

    /// `0` is full lighting. Any other value has [`LIGHTING_DEBUG_ACTIVE`] set.
    pub fn gpu_mask(self) -> u32 {
        if self == Self::default() {
            return 0;
        }
        let mut mask = LIGHTING_DEBUG_ACTIVE;
        if self.directional || self.point || self.spot {
            mask |= LIGHTING_DEBUG_DIRECT;
        }
        if self.env_diffuse {
            mask |= LIGHTING_DEBUG_ENV_DIFFUSE;
        }
        if self.env_specular {
            mask |= LIGHTING_DEBUG_ENV_SPECULAR;
        }
        if self.probe {
            mask |= LIGHTING_DEBUG_PROBE;
        }
        if self.emissive {
            mask |= LIGHTING_DEBUG_EMISSIVE;
        }
        if self.shadows {
            mask |= LIGHTING_DEBUG_SHADOWS;
        }
        if self.indirect {
            mask |= LIGHTING_DEBUG_INDIRECT;
        }
        mask
    }

    pub fn label(self) -> &'static str {
        if self == Self::default() {
            "Full"
        } else if self == Self::direct_only() {
            "Direct"
        } else if self == Self::environment_diffuse_only() {
            "Env Diffuse"
        } else if self == Self::environment_specular_only() {
            "Env Specular"
        } else if self == Self::probe_specular_only() {
            "Probe"
        } else if self == Self::emissive_only() {
            "Emissive"
        } else if self == Self::directional_only() {
            "Directional"
        } else if self == Self::point_only() {
            "Point"
        } else if self == Self::spot_only() {
            "Spot"
        } else if self == Self::shadows_disabled() {
            "No Shadows"
        } else if self == Self::indirect_diffuse_only() {
            "Indirect"
        } else if self == Self::direct_unshadowed() {
            "Direct Unshadowed"
        } else {
            "Custom"
        }
    }
}

/// Linear HDR scene color. The swapchain is the display target, not this.
pub const HDR_SCENE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

pub struct RenderViewDesc {
    pub label: String,
    pub target: RenderTargetId,
    pub camera: Camera,
    pub layout: NormalizedRect,
    pub settings: RenderViewSettings,
}

pub struct RenderViewUpdate {
    pub camera: Option<Camera>,
    pub layout: Option<NormalizedRect>,
    pub settings: Option<RenderViewSettings>,
    /// Resolved view pose. Not a scene camera and not an authoring command.
    /// `None` leaves the current override alone.
    pub pose: Option<ResolvedPose>,
}

struct RenderView {
    label: String,
    target: RenderTargetId,
    camera: Camera,
    /// When set, this pose is the view origin. The snapshot camera is not written.
    pose_override: Option<ResolvedPose>,
    layout: NormalizedRect,
    settings: RenderViewSettings,
    depth: Option<TextureId>,
    depth_view: Option<TextureViewId>,
    /// Full target size. Not the viewport size. See `docs/rendering/views.md`.
    depth_size: Option<(u32, u32)>,
    /// Leaf and parent draw classification for this camera. Not stored on the mesh.
    cluster_flags: Vec<u32>,
    parent_indices: Vec<u32>,
    parent_key: u64,
}

struct InstanceBinding {
    view: RenderViewId,
    instance: RenderInstanceId,
    uniform: BufferId,
    group: BindGroupId,
    last_bytes: Option<[u8; GpuTransforms::BYTES]>,
}

struct ViewSlot {
    generation: u32,
    view: Option<RenderView>,
}

struct SurfaceTarget {
    generation: u32,
    kind: RenderTargetKind,
    swapchain: SwapchainId,
    clear: ClearColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOutcome {
    Presented { frame: u64 },
    Minimized,
    TimedOut,
}

#[derive(Debug)]
pub enum RenderError {
    Rhi(RhiError),
    UnknownView(RenderViewId),
    UnknownTarget(RenderTargetId),
    Space(SpaceError),
    Mesh(MeshError),
    UnknownMaterial,
    UnknownTexture,
    UnknownSampler,
    IncompatibleMaterial,
    TooManyLights,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rhi(error) => write!(formatter, "{error}"),
            Self::UnknownView(id) => write!(formatter, "unknown render view {}:{}", id.index, id.generation),
            Self::UnknownTarget(id) => write!(formatter, "unknown render target {}:{}", id.index, id.generation),
            Self::Space(error) => write!(formatter, "{error}"),
            Self::Mesh(error) => write!(formatter, "{error}"),
            Self::UnknownMaterial => write!(formatter, "unknown material instance"),
            Self::UnknownTexture => write!(formatter, "unknown texture"),
            Self::UnknownSampler => write!(formatter, "unknown sampler"),
            Self::IncompatibleMaterial => write!(formatter, "mesh does not provide the material vertex inputs"),
            Self::TooManyLights => write!(formatter, "too many lights for the renderer light list"),
        }
    }
}

pub struct Renderer {
    device: Box<dyn Device>,
    color_format: TextureFormat,
    width: u32,
    height: u32,
    target: SurfaceTarget,
    /// Slot 0 is unused so a zero index is never a live view.
    views: Vec<ViewSlot>,
    free_views: Vec<u32>,
    bindings: Vec<InstanceBinding>,
    light_packets: Vec<ViewLightPacket>,
    environment_gpu: Option<EnvironmentGpu>,
    probe_residents: Vec<ProbeResident>,
    /// Shader binding used only when no captured probe cube exists. Not probe residency.
    probe_unbound: Option<ProbeGpu>,
    probe_count: u32,
    probe_active_count: u32,
    probe_capture_count: u32,
    probe_recapture_count: u32,
    irradiance_build_count: u32,
    last_capture_ms: u32,
    last_prefilter_ms: u32,
    probe_queue_len: u32,
    probe_job: Option<probe_capture::ProbeCaptureJob>,
    probe_selected_far: u32,
    probe_fallback_count: u32,
    camera_layout: Option<BindGroupLayoutId>,
    light_layout: Option<BindGroupLayoutId>,
    gpu_masters: Vec<GpuMasterMaterial>,
    gpu_materials: Vec<GpuMaterialInstance>,
    gpu_textures: Vec<ResidentTexture>,
    gpu_samplers: Vec<(SamplerState, jarvig_rhi::SamplerId)>,
    gpu_meshes: Vec<(MeshId, GpuResidency)>,
    mesh_uploads: u32,
    material_pipelines: u32,
    material_parameter_uploads: u32,
    unbound_material_skips: u32,
    texture_uploads: u32,
    texture_binding_updates: u32,
    missing_texture_uses: u32,
    light_uploads: u32,
    environment_uploads: u32,
    instance_transform_uploads: u32,
    tone_map_parameter_uploads: u32,
    tone_map_pipelines: u32,
    output_passes_last: u32,
    submitted_last: u32,
    depth_creates: u64,
    acquires_last: u32,
    presents_last: u32,
    resize_applied: u32,
    resize_failures: u32,
    shut_down: bool,
    overlay: Option<EditorOverlay>,
    overlay_gpu: Option<OverlayGpu>,
    /// Depth-tested editor grid and axis triad. Not a scene object. `None` draws nothing.
    reference: Option<EditorOverlay>,
    reference_gpu: Option<OverlayGpu>,
    /// Editor surface grid. Not a scene object. `None` draws nothing.
    terrain_grid: Option<TerrainGridDesc>,
    terrain_grid_meshes: HashSet<MeshId>,
    /// Authoring ids skipped while drawing, shadowing, and the contact prepass. Not saved visibility.
    entity_hidden: HashSet<EntityId>,
    /// Forces the view light mask off for this frame. Does not write the view's lighting debug.
    land_unlit: bool,
    terrain_grid_gpu: Option<TerrainGridGpu>,
    hdr: Option<HdrScene>,
    output: Option<OutputGpu>,
    output_views: Vec<OutputView>,
    shadow: Option<shadow_pass::ShadowGpu>,
    shadow_world_revision: u64,
    shadow_update_count: u32,
    shadow_maps_updated: u32,
    shadow_draws_frame: u32,
    shadow_casters: u32,
    shadow_pass_ms: f32,
    contact: Option<ContactDepth>,
    contact_gpu: Option<ContactGpu>,
    contact_requested: bool,
    meshlet_gpu: Option<MeshletGpu>,
    parent_gpu: Option<ParentGpu>,
    einstein_debug: bool,
    einstein_seed: u64,
    einstein_shader: Option<ShaderModuleId>,
    einstein_layout: Option<BindGroupLayoutId>,
    einstein_pipeline: Option<PipelineId>,
    einstein_uniform: Option<BufferId>,
    einstein_group: Option<BindGroupId>,
    micro_enabled: bool,
    micro_surface: bool,
    /// Exact parametric solid. Classification runs. No relief is built.
    detail_exact: bool,
    micro_color: bool,
    micro_seed: u64,
    micro_live: Vec<MicroGpuChunk>,
    micro_pending: Option<MicroPending>,
    micro_request: Option<MicroBuildRequest>,
    micro_next_epoch: u64,
    micro_wanted_epoch: u64,
    micro_published_epoch: u64,
    micro_wanted_key: u64,
    micro_inflight_epoch: u64,
    micro_inflight_key: u64,
    micro_stage: &'static str,
    micro_queue_wait_us: u32,
    micro_classify_us: u32,
    micro_host_wait_us: u32,
    micro_publish_us: u32,
    micro_cancelled: u32,
    micro_stale_discarded: u32,
    micro_partial: u32,
    micro_swapped: bool,
    micro_index_count: u32,
    micro_fingerprint: u64,
    micro_input_key: u64,
    micro_debug: jarvig_core::MicroDebugMode,
    micro_compatible: bool,
    micro_reasons: Vec<jarvig_core::DetailReject>,
    micro_projected: Vec<f32>,
    micro_desired_patches: u32,
    micro_missing: u32,
    micro_eligible_visible: u32,
    micro_detail_requested: u32,
    micro_unsupported: u32,
    micro_unclassified: u32,
    micro_states: Vec<jarvig_core::DetailLifecycle>,
    micro_still_frames: u32,
    micro_coverage_prev_key: u64,
    micro_class_shader: Option<ShaderModuleId>,
    micro_class_pipeline: Option<PipelineId>,
    micro_parent_pipeline: Option<PipelineId>,
    micro_parent_solid: Option<PipelineId>,
    diagnostic_draw_shader: Option<ShaderModuleId>,
    diagnostic_draw_layout: Option<BindGroupLayoutId>,
    diagnostic_draw_uniform: Option<BufferId>,
    diagnostic_draw_group: Option<BindGroupId>,
    diagnostic_draw_pipelines: Vec<(u64, bool, PipelineId)>,
    diagnostic_base_capacity: usize,
    diagnostic_run_cursor: u32,
    /// One uniform per debug color. Queue writes are not ordered inside a pass, so colors cannot share a buffer.
    micro_class_colors: Vec<(u64, BufferId, BindGroupId)>,
    micro_color_pipeline: Option<PipelineId>,
    micro_color_shader: Option<ShaderModuleId>,
    diagnostic_freeze: bool,
    diagnostic_reused: bool,
    diagnostic_frozen: Option<FrozenDiagnostic>,
    gpu_scene: GpuSceneState,
}

#[derive(Clone)]
struct FrozenDiagnostic {
    pipeline: jarvig_core::GeometryDiagnostic,
    flags: Vec<u32>,
    parent_indices: Vec<u32>,
    parent_key: u64,
    coarseness: Vec<u32>,
    submitted: u32,
    frustum_rejected: u32,
    occlusion_rejected: u32,
    conservative: u32,
    triangles: u32,
    parents: u32,
    leaves: u32,
    hierarchy: bool,
}

struct GpuSceneState {
    remembered: Vec<(jarvig_core::MeshId, Vec<jarvig_core::GpuMeshletRecord>)>,
    instances: Option<BufferId>,
    instance_capacity: usize,
    geometries: Option<BufferId>,
    meshlets: Option<BufferId>,
    meshlet_key: u64,
    stats: GpuSceneFrameStats,
    frustum: bool,
    occlusion: bool,
    freeze: bool,
    frozen_flags: Vec<u32>,
    frozen_stats: Option<GpuSceneFrameStats>,
    highlight: Option<u32>,
    hierarchy: bool,
    hierarchy_key: u64,
    hierarchy_nodes: Option<jarvig_core::ClusterHierarchy>,
    frozen_cut: bool,
    hierarchy_error_px: f32,
    debug_flags: Vec<u32>,
    /// Parent-mesh indices for the view most recently culled. Each view keeps its own copy.
    parent_indices: Vec<u32>,
    parent_key: u64,
    /// Leaf Truth is the leaf-only diagnostic. The other diagnostics live beside it.
    leaf_truth: bool,
    diagnostic: jarvig_core::GeometryDiagnostic,
    visualization: jarvig_core::DiagnosticVisualization,
}

struct ParentChunk {
    vertices: BufferId,
    debug_vertices: BufferId,
    first_vertex: u32,
    vertex_count: u32,
    frame: Option<BufferId>,
    frame_count: u32,
}

struct ParentGpu {
    mesh: MeshId,
    chunks: Vec<ParentChunk>,
    hierarchy: jarvig_core::ClusterHierarchy,
    ranges: Vec<jarvig_core::ParentRange>,
    leaf_counts: Vec<u32>,
    leaf_triangles: u32,
    root_triangles: u32,
    empty_parents: u32,
    indices: Vec<u32>,
    cpu_bytes: u64,
    gpu_static_bytes: u64,
    build_ms: f32,
    frame_count: u32,
    frame_key: u64,
    shader: ShaderModuleId,
    pipeline: Option<PipelineId>,
    /// Cull used when `pipeline` was created. `true` keeps both sides.
    color_two_sided: bool,
    /// Parallel to `hierarchy.nodes`. False expands that parent back to leaves.
    draw_ok: Vec<bool>,
    /// Object-local positions for the coverage compare. Empty when the mesh is over the keep cap.
    positions: Vec<[f32; 3]>,
    positions_retained: bool,
}

/// CPU parent triangles for Compare Against Leaf Truth. Not a GPU readback.
pub struct ParentCoverageSource<'a> {
    pub mesh: MeshId,
    pub positions: &'a [[f32; 3]],
    pub indices: &'a [u32],
    pub ranges: &'a [jarvig_core::ParentRange],
    pub hierarchy: &'a jarvig_core::ClusterHierarchy,
    pub draw_ok: &'a [bool],
    pub leaf_counts: &'a [u32],
    pub positions_retained: bool,
}

/// What the last view uploaded. Meshlet bytes stay resident until the mesh set changes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HierarchyResidency {
    pub leaf_meshlet_bytes: u64,
    pub hierarchy_cpu_bytes: u64,
    pub parent_gpu_static_bytes: u64,
    pub parent_gpu_frame_bytes: u64,
    pub recorded_build_ms: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuSceneFrameStats {
    pub instances: u32,
    pub geometries: u32,
    pub meshlets: u32,
    pub meshlet_bytes: u64,
    pub instance_upload_us: u32,
    pub meshlet_uploads: u32,
    pub meshlets_submitted: u32,
    pub meshlets_rejected: u32,
    pub triangles_submitted: u32,
    pub instances_visible: u32,
    pub cull_us: u32,
    pub occlusion_rejected: u32,
    pub conservative_visible: u32,
    pub occlusion_us: u32,
    pub frustum: bool,
    pub occlusion: bool,
    pub frozen: bool,
    pub hierarchy: bool,
    pub hierarchy_cut: u32,
    pub hierarchy_nodes: u32,
    pub hierarchy_lod_triangles: u32,
    pub hierarchy_leaf_triangles: u32,
    pub hierarchy_leaves: u32,
    pub hierarchy_parents: u32,
    pub hierarchy_depth: u32,
    pub hierarchy_select_us: u32,
    pub hierarchy_cut_us: u32,
    pub hierarchy_upload_us: u32,
    pub hierarchy_upload_bytes: u64,
    pub hierarchy_draw_prepare_us: u32,
    pub hierarchy_covered_leaves: u32,
    pub renderer_cpu_us: u32,
    /// GPU timestamp queries are not implemented on this RHI. Stay false rather than invent a time.
    pub gpu_frame_measured: bool,
    pub hierarchy_root_triangles: u32,
    pub hierarchy_empty_parents: u32,
    pub hierarchy_candidates: u32,
    pub hz_width: u32,
    pub hz_height: u32,
    pub einstein_debug_draws: u32,
    pub micro_patches: u32,
    pub micro_samples: u32,
    pub micro_vertices: u32,
    pub micro_triangles: u32,
    pub micro_generation_us: u32,
    pub micro_upload_us: u32,
    pub micro_fallbacks: u32,
    pub micro_ordinary: u32,
    pub micro_invalid: u32,
    pub micro_reused: u32,
    pub micro_fingerprint: u64,
    /// Largest normal displacement, in micrometers, so the stat stays an integer.
    pub micro_max_displacement_um: u32,
    pub micro_queue_wait_us: u32,
    /// Cluster classification on the frame thread, before a build is queued.
    pub micro_classify_us: u32,
    /// Time a finished build sat before the frame thread accepted it.
    pub micro_host_wait_us: u32,
    pub micro_cancelled: u32,
    pub micro_stale_discarded: u32,
    pub micro_publish_us: u32,
    /// Increments only if a draw would have shown an incomplete patch set. Stays 0 when publication is atomic.
    pub micro_partial: u32,
    /// Eligible clusters the current view wants built.
    pub micro_desired_patches: u32,
    /// Desired clusters that are not in the published set. Zero once the stopped view has converged.
    pub micro_missing: u32,
    /// Drawn clusters (flag 1 or 4). This is the Einstein denominator.
    pub micro_eligible_visible: u32,
    /// Selected for generation. Not the published patch count.
    pub micro_detail_requested: u32,
    /// Requested clusters whose patch is in the published set.
    pub micro_detail_ready: u32,
    /// Patches currently published.
    pub micro_detail_published: u32,
    /// Drawn clusters that cannot take detail.
    pub micro_unsupported: u32,
    /// Drawn clusters outside the four detail buckets. Acceptance is zero.
    pub micro_unclassified: u32,
    /// Visible clusters under the 1 px feature gate.
    pub micro_below: u32,
    /// Exact parametric clusters. They are classified and build no patch.
    pub micro_exact: u32,
    pub micro_occluded: u32,
    pub micro_offscreen: u32,
    /// Leaves replaced by a parent in this view.
    pub micro_replaced: u32,
    pub micro_budget_rejected: u32,
    pub micro_no_anchor: u32,
    pub micro_incompatible: u32,
    /// Largest 2 cm feature among clusters this view is drawing, in thousandths of a pixel.
    pub micro_near_px_milli: u32,
    /// Visible clusters that clear the 1 px gate. Below-threshold and parent shells are outside this.
    pub micro_visible_eligible: u32,
    pub micro_coverage_requested: u32,
    pub micro_coverage_generating: u32,
    pub micro_coverage_ready: u32,
    pub micro_coverage_published: u32,
    pub micro_coverage_drawn: u32,
    pub micro_coverage_rejected: u32,
    pub micro_coverage_pending: u32,
    pub micro_coverage_unclassified: u32,
    pub micro_requested_hash: u64,
    pub micro_ready_hash: u64,
    pub micro_published_hash: u64,
    pub micro_drawn_hash: u64,
    pub micro_camera_stable: bool,
    pub micro_pipeline_settled: bool,
    /// Triangles submitted by the ordinary index buffer this frame.
    pub legacy_draw_triangles: u32,
    /// Triangles submitted by the leaf meshlet cut this frame.
    pub meshlet_draw_triangles: u32,
    /// Triangles submitted by selected parent meshes this frame.
    pub parent_draw_triangles: u32,
    /// Leaves under the cut that no drawn parent and no submitted leaf represents.
    pub hierarchy_uncovered_leaves: u32,
    pub leaf_truth: bool,
}

const MICRO_CHUNK_VERTEX_BYTES: usize = 256 * 1024;

struct MicroGpuChunk {
    vertices: BufferId,
    indices: BufferId,
    index_count: u32,
}

struct MicroPending {
    epoch: u64,
    key: u64,
    mesh: jarvig_core::MicroMesh,
    groups: Vec<jarvig_core::MicroSpan>,
    next: usize,
    gpu: Vec<MicroGpuChunk>,
    upload_us: u32,
    started: std::time::Instant,
}

/// One CPU build the editor submits to the job queue. The renderer does not build it.
pub struct MicroBuildRequest {
    pub epoch: u64,
    pub key: u64,
    pub anchors: Vec<jarvig_core::SurfaceAnchor>,
    pub seed: u64,
    pub height: f32,
    pub tan_half: f32,
    pub budget: jarvig_core::MicroBudget,
    pub provider: jarvig_core::DetailProvider,
    pub skipped: u32,
}

struct MeshletGpu {
    mesh: MeshId,
    indices: BufferId,
    index_count: u32,
    colors: BufferId,
    color_group: BindGroupId,
    color_layout: BindGroupLayoutId,
    shader: ShaderModuleId,
    pipelines: Vec<(u64, bool, PipelineId)>,
    occluded_pipelines: Vec<(u64, bool, PipelineId)>,
    ranges: Vec<(u32, u32)>,
    spans: Vec<(u32, u32)>,
    span_source: Vec<u32>,
    anchor_vertex: Vec<u32>,
    owners: BufferId,
    visible: BufferId,
    visible_capacity: usize,
    show_ids: bool,
    shade_clustered: bool,
}

/// One imported mesh, drawn from its stored clusters. The ordinary index buffer stays available.
pub struct MeshletDebugBatch {
    pub mesh: MeshId,
    pub indices: Vec<u32>,
    pub colors: Vec<u32>,
    pub ranges: Vec<(u32, u32)>,
    pub spans: Vec<(u32, u32)>,
    pub span_source: Vec<u32>,
    pub owners: Vec<u32>,
    pub show_ids: bool,
    pub shade_clustered: bool,
}

struct ContactGpu {
    shader: ShaderModuleId,
    pipelines: Vec<(u64, PipelineId)>,
}

struct ContactDepth {
    texture: TextureId,
    view: TextureViewId,
    width: u32,
    height: u32,
}

struct HdrScene {
    texture: TextureId,
    view: TextureViewId,
    width: u32,
    height: u32,
}

struct OutputGpu {
    shader: ShaderModuleId,
    pipeline: PipelineId,
    layout: BindGroupLayoutId,
    sampler: jarvig_rhi::SamplerId,
}

struct OutputView {
    view: RenderViewId,
    uniform: BufferId,
    group: BindGroupId,
    last_bytes: [u8; 16],
}

/// Whether the renderer currently holds GPU buffers for one logical mesh.
enum GpuResidency {
    Resident(GpuMesh),
    Evicted,
}

/// GPU upload of one logical mesh. RHI buffer handles only. Shared by every view.
struct GpuMesh {
    vertices: BufferId,
    indices: BufferId,
    index_format: IndexFormat,
    submeshes: Vec<GpuSubmesh>,
}

struct GpuSubmesh {
    index_count: u32,
    first_index: u32,
    base_vertex: i32,
}

struct PreparedDraw {
    mesh: MeshId,
    submesh: usize,
    transform: BindGroupId,
    material: BindGroupId,
    lights: Option<BindGroupId>,
    pipeline: PipelineId,
    vertex_stride: u32,
}

enum PreparedCut {
    Material,
    HiddenSubmesh,
    Clustered { indices: BufferId, runs: Vec<(u32, u32)>, parent_draw: bool },
}

struct ViewLightPacket {
    view: RenderViewId,
    header: BufferId,
    storage: BufferId,
    probe: BufferId,
    shadow: BufferId,
    group: BindGroupId,
    unshadowed: BindGroupId,
    last_header: [u8; 16],
    last_storage: Vec<u8>,
    last_shadow: Vec<u8>,
    last_probe: [u8; GpuReflectionProbePacket::BYTES],
    probe_view: TextureViewId,
    irradiance_view: TextureViewId,
    probe_sampler: jarvig_rhi::SamplerId,
}

/// One probe's HDR cubemap. Not a material texture and not shared with another probe.
struct ProbeResident {
    id: ProbeId,
    gpu: ProbeGpu,
    captured_world: u64,
    captured_lighting: u64,
    captured_serial: u64,
}

/// One HDR cubemap. Not a material texture.
struct ProbeGpu {
    texture: TextureId,
    sample: TextureViewId,
    sampler: jarvig_rhi::SamplerId,
    resolution: u32,
    mip_count: u32,
    /// Cosine irradiance of `texture`. Not the specular cube and not a second capture.
    irradiance: TextureId,
    irradiance_sample: TextureViewId,
}

/// One environment uniform for every view. Not a direct-light record.
struct EnvironmentGpu {
    buffer: BufferId,
    last_bytes: [u8; GpuEnvironmentPacket::BYTES],
}

/// Renderer safety cap. Not part of the material program. The shader loops `min(count, arrayLength)`.
const LIGHT_CAPACITY: usize = 64;

const CONTACT_SHADER: &str = r#"
struct RenderUniforms {
    projection: mat4x4<f32>,
    view: mat4x4<f32>,
    model: mat4x4<f32>,
}
struct ContactOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) view_z: f32,
}
@group(0) @binding(0) var<uniform> render: RenderUniforms;
@vertex
fn vs(@location(0) position: vec3<f32>) -> ContactOut {
    var out: ContactOut;
    let eye = render.view * render.model * vec4<f32>(position, 1.0);
    out.clip = render.projection * eye;
    out.view_z = max(-eye.z, 0.0);
    return out;
}
@fragment
fn fs(in: ContactOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.view_z, 0.0, 0.0, 1.0);
}
"#;

const OVERLAY_SHADER: &str = r#"
struct Camera {
  projection: mat4x4<f32>,
  view: mat4x4<f32>,
}
struct OverlayIn {
  @location(0) position: vec3<f32>,
  @location(1) color: vec4<f32>,
}
struct OverlayOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) color: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;
@vertex
fn vs(input: OverlayIn) -> OverlayOut {
  var output: OverlayOut;
  output.clip = camera.projection * camera.view * vec4<f32>(input.position, 1.0);
  output.color = input.color;
  return output;
}
@fragment
fn fs(input: OverlayOut) -> @location(0) vec4<f32> {
  return input.color;
}
"#;

/// Lines on the terrain triangles already drawn this pass. Non-line fragments are discarded.
/// Minor lines thin until they are gone at about nine pixels apart. Major lines stay farther out.
/// There is no blend: a faded line is a thinner line. The clip bias is toward the camera in reversed-Z.
/// Depth write stays off. Derivatives stay outside the per-pixel returns.
const TERRAIN_GRID_SHADER: &str = r#"
struct RenderUniforms {
    projection: mat4x4<f32>,
    view: mat4x4<f32>,
    model: mat4x4<f32>,
}
struct GridUniforms {
    phase: vec4<f32>,
    spacing: vec4<f32>,
    flags: vec4<f32>,
    metric: vec4<f32>,
    brush: vec4<f32>,
    color: vec4<f32>,
}
struct GridOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local_xz: vec2<f32>,
    @location(1) world_xz: vec2<f32>,
    @location(2) world_pos: vec3<f32>,
}
@group(0) @binding(0) var<uniform> render: RenderUniforms;
@group(1) @binding(0) var<uniform> grid: GridUniforms;
@vertex
fn vs(@location(0) position: vec3<f32>) -> GridOut {
    var out: GridOut;
    let world = render.model * vec4<f32>(position, 1.0);
    out.clip = render.projection * render.view * world;
    out.clip.z = out.clip.z + out.clip.w * 0.00008;
    out.local_xz = position.xz;
    out.world_xz = world.xz;
    out.world_pos = world.xyz;
    return out;
}
fn dist_to_int(value: f32) -> f32 {
    let wrapped = fract(value);
    return min(wrapped, 1.0 - wrapped);
}
// Pixel half-width of one axis, or 0 once the line would fill the view.
fn axis_px(dist: f32, fw: f32, full_px: f32, fade_end: f32) -> f32 {
    let safe = max(fw, 1e-6);
    let keep = clamp((fade_end - safe) / fade_end, 0.0, 1.0);
    let px = full_px * keep;
    var kept = px;
    if (safe >= fade_end || px < 0.12 || dist / safe > px) {
        kept = 0.0;
    }
    return kept;
}
// 1 near the camera, 0 once the line should be gone. Grazing pulls that distance in.
fn span_keep(dist: f32, graze: f32, near_m: f32, far_m: f32, pull: f32) -> f32 {
    let amount = clamp(graze, 0.0, 1.0) * pull;
    let far = max(far_m * (1.0 - amount), near_m + 1.0);
    let near = min(near_m * (1.0 - amount * 0.35), far - 1.0);
    return clamp((far - dist) / (far - near), 0.0, 1.0);
}
fn grid_line(coord: vec2<f32>, fw: vec2<f32>, full_px: f32, fade_end: f32, keep: f32) -> bool {
    if (keep <= 0.02) {
        return false;
    }
    let width = full_px * keep;
    let px_x = axis_px(dist_to_int(coord.x), fw.x, width, fade_end);
    let px_z = axis_px(dist_to_int(coord.y), fw.y, width, fade_end);
    return px_x > 0.0 || px_z > 0.0;
}
fn on_dot(coord: vec2<f32>, fw: vec2<f32>) -> bool {
    let density = max(max(fw.x, fw.y), 1e-6);
    let keep = clamp((0.18 - density) / 0.18, 0.0, 1.0);
    let rad = 1.7 * keep;
    var hit = false;
    if (rad >= 0.30) {
        let dx = dist_to_int(coord.x) / max(fw.x, 1e-6);
        let dz = dist_to_int(coord.y) / max(fw.y, 1e-6);
        hit = dx * dx + dz * dz <= rad * rad;
    }
    return hit;
}
fn on_cross(coord: vec2<f32>, fw: vec2<f32>) -> bool {
    let density = max(max(fw.x, fw.y), 1e-6);
    let keep = clamp((0.30 - density) / 0.30, 0.0, 1.0);
    let px = 1.25 * keep;
    var hit = false;
    if (px >= 0.20) {
        let dx = abs(fract(coord.x) - 0.5);
        let dz = abs(fract(coord.y) - 0.5);
        let thin_x = dx / max(fw.x, 1e-6) <= px;
        let thin_z = dz / max(fw.y, 1e-6) <= px;
        let arm = 0.10 * max(keep, 0.40);
        hit = (thin_z && dx < arm) || (thin_x && dz < arm);
    }
    return hit;
}
@fragment
fn fs(in: GridOut) -> @location(0) vec4<f32> {
    // Derivatives stay outside the per-pixel returns. Distance and grazing only scale width.
    let dist = length(in.world_pos);
    let axis_x = dpdx(in.world_pos);
    let axis_y = dpdy(in.world_pos);
    let n = cross(axis_x, axis_y);
    let nlen = length(n);
    let facing = select(1.0, abs(dot(n / max(nlen, 1e-6), in.world_pos / max(dist, 1e-4))), nlen > 1e-5 && dist > 0.05);
    let graze = clamp(1.0 - facing * 1.35, 0.0, 1.0);
    // Near: minor and major. Middle distance: major. The horizon drops both.
    let minor_keep = span_keep(dist, graze, 12.0, 40.0, 0.55);
    let major_keep = span_keep(dist, graze, 28.0, 120.0, 0.40);
    // Brush, then major, minor, chunk, vertex dots, LOD. Flags are uniform, so each
    // derivative below runs for every fragment or for none.
    var brush_hit = false;
    var brush_rgb = vec3<f32>(0.0);
    if (grid.brush.w > 0.5 && grid.brush.z > 0.0) {
        let dist = length(in.local_xz - grid.brush.xy);
        let fw = max(fwidth(dist), 1e-5);
        let radius = grid.brush.z;
        let strength = max(grid.spacing.w, 0.0);
        var t75 = 0.25;
        var t25 = 0.75;
        if (grid.brush.w > 1.5) {
            t75 = 0.33333334;
            t25 = 0.6666667;
        }
        let contour = clamp(0.45 + strength * 1.8, 0.45, 2.6) * fw;
        let dot_r = (2.4 + clamp(strength, 0.0, 4.0) * 0.9) * fw;
        let outer = abs(dist - radius) <= 1.5 * fw;
        let falloff = strength > 0.02 && (abs(dist - radius * t75) <= contour || abs(dist - radius * 0.5) <= contour || abs(dist - radius * t25) <= contour);
        let center = dist <= dot_r;
        if (outer) {
            brush_hit = true;
            brush_rgb = grid.color.rgb;
        } else if (falloff) {
            brush_hit = true;
            brush_rgb = grid.color.rgb * 0.78;
        } else if (center) {
            brush_hit = true;
            brush_rgb = grid.color.rgb;
        }
    }

    var major_hit = false;
    var minor_hit = false;
    if (grid.flags.x > 0.5 && grid.spacing.x > 0.0 && grid.spacing.y > 0.0) {
        let major = in.world_xz / grid.spacing.y + grid.phase.zw;
        let minor = in.world_xz / grid.spacing.x + grid.phase.xy;
        let major_fw = fwidth(major);
        let minor_fw = fwidth(minor);
        // Screen spacing still drops a line that would fill the view. Distance does the rest.
        major_hit = grid_line(major, major_fw, 1.55, 0.34, major_keep);
        minor_hit = grid_line(minor, minor_fw, 0.85, 0.11, minor_keep);
    }

    var chunk_hit = false;
    if (grid.flags.z > 0.5 && grid.metric.w > 0.0) {
        let coord = (in.local_xz + grid.metric.xy) / grid.metric.w;
        chunk_hit = grid_line(coord, fwidth(coord), 1.25, 0.25, major_keep);
    }

    var vertex_hit = false;
    if (grid.flags.y > 0.5 && grid.metric.z > 0.0) {
        let coord = (in.local_xz + grid.metric.xy) / grid.metric.z;
        vertex_hit = on_dot(coord, fwidth(coord)) && minor_keep > 0.35;
    }

    var lod_hit = false;
    if (grid.flags.w > 0.5 && grid.metric.w > 0.0) {
        let coord = (in.local_xz + grid.metric.xy) / grid.metric.w;
        lod_hit = on_cross(coord, fwidth(coord));
    }

    if (brush_hit) {
        return vec4<f32>(brush_rgb, 1.0);
    }
    if (major_hit) {
        return vec4<f32>(0.94, 0.96, 0.90, 1.0);
    }
    if (minor_hit) {
        return vec4<f32>(0.55, 0.62, 0.50, 1.0);
    }
    if (chunk_hit) {
        return vec4<f32>(0.95, 0.62, 0.22, 1.0);
    }
    if (vertex_hit) {
        return vec4<f32>(0.45, 0.78, 0.95, 1.0);
    }
    if (lod_hit) {
        return vec4<f32>(0.62, 0.48, 0.95, 1.0) * grid.spacing.z;
    }
    // Discard sits in its own block. FXC still requires a return on that path,
    // and a return in the same block as discard is rejected by the shader front end.
    if (grid.phase.x == grid.phase.x) {
        discard;
    }
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
"#;

fn terrain_grid_bytes(pose: &ResolvedPose, desc: &TerrainGridDesc) -> [u8; 96] {
    let mut words = [0f32; 24];
    words[0] = world_grid_phase(pose.translation.x, desc.minor_m);
    words[1] = world_grid_phase(pose.translation.z, desc.minor_m);
    words[2] = world_grid_phase(pose.translation.x, desc.major_m);
    words[3] = world_grid_phase(pose.translation.z, desc.major_m);
    words[4] = desc.minor_m;
    words[5] = desc.major_m;
    words[6] = if desc.lod_enabled { 1.0 } else { 0.45 };
    // spacing.w is brush strength in meters. color.a stays 1. The uniform stays 96 bytes.
    words[7] = desc.brush_strength;
    words[8] = if desc.world { 1.0 } else { 0.0 };
    words[9] = if desc.vertices { 1.0 } else { 0.0 };
    words[10] = if desc.chunks { 1.0 } else { 0.0 };
    words[11] = if desc.lod { 1.0 } else { 0.0 };
    words[12] = desc.half_x;
    words[13] = desc.half_z;
    words[14] = desc.vertex_spacing;
    words[15] = desc.chunk_m;
    words[16] = desc.brush_x;
    words[17] = desc.brush_z;
    words[18] = desc.brush_radius;
    // brush.w: 0 off, 1 linear falloff, 2 smooth falloff.
    words[19] = if !desc.brush_enabled {
        0.0
    } else if desc.brush_smooth {
        2.0
    } else {
        1.0
    };
    words[20] = desc.brush_color[0];
    words[21] = desc.brush_color[1];
    words[22] = desc.brush_color[2];
    words[23] = desc.brush_color[3];
    let mut bytes = [0u8; 96];
    for (index, word) in words.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

const MESHLET_SHADER: &str = r#"
struct RenderUniforms {
    projection: mat4x4<f32>,
    view: mat4x4<f32>,
    model: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> render: RenderUniforms;
@group(1) @binding(0) var<storage, read> colors: array<u32>;
@group(1) @binding(1) var<storage, read> owners: array<u32>;
@group(1) @binding(2) var<storage, read> visible: array<u32>;
struct MeshletOut {
    @builtin(position) clip: vec4<f32>,
}
@vertex
fn vs(@location(0) position: vec3<f32>) -> MeshletOut {
    var out: MeshletOut;
    out.clip = render.projection * render.view * render.model * vec4<f32>(position, 1.0);
    return out;
}
@fragment
fn fs(@builtin(primitive_index) triangle: u32) -> @location(0) vec4<f32> {
    let flag = visible[owners[triangle]];
    if (flag == 6u) { discard; }
    if (flag == 7u) {
        let packed = colors[triangle];
        let red = f32(packed & 0xFFu) / 255.0;
        let green = f32((packed >> 8u) & 0xFFu) / 255.0;
        let blue = f32((packed >> 16u) & 0xFFu) / 255.0;
        return vec4<f32>(red, green, blue, 1.0);
    }
    if (flag >= 16u) {
        let level = flag - 16u;
        let warm = f32(min(level, 8u)) / 8.0;
        return vec4<f32>(warm, 0.85 - warm * 0.45, 1.0 - warm, 1.0);
    }
    if (flag == 5u) { return vec4<f32>(1.0, 0.15, 0.85, 1.0); }
    if (flag == 1u) { return vec4<f32>(0.15, 0.85, 0.25, 1.0); }
    if (flag == 4u) { return vec4<f32>(0.95, 0.78, 0.20, 1.0); }
    if (flag == 3u) { return vec4<f32>(0.20, 0.45, 0.95, 1.0); }
    if (flag == 0u) { return vec4<f32>(0.90, 0.16, 0.14, 1.0); }
    if (flag == 2u) { return vec4<f32>(1.0, 0.15, 0.85, 1.0); }
    let packed = colors[triangle];
    let red = f32(packed & 0xFFu) / 255.0;
    let green = f32((packed >> 8u) & 0xFFu) / 255.0;
    let blue = f32((packed >> 16u) & 0xFFu) / 255.0;
    return vec4<f32>(red, green, blue, 1.0);
}
@fragment
fn fs_occluded(@builtin(position) pixel: vec4<f32>, @builtin(primitive_index) triangle: u32) -> @location(0) vec4<f32> {
    let flag = visible[owners[triangle]];
    let x = u32(pixel.x);
    let y = u32(pixel.y);
    if (flag != 3u || ((x + y) & 7u) != 0u) {
        discard;
    }
    return vec4<f32>(0.20, 0.45, 0.95, 1.0);
}
"#;

const DIAGNOSTIC_COLOR_SHADER: &str = r#"
struct RenderUniforms {
    projection: mat4x4<f32>,
    view: mat4x4<f32>,
    model: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> render: RenderUniforms;
@group(1) @binding(0) var<storage, read> colors: array<u32>;
@group(1) @binding(1) var<storage, read> owners: array<u32>;
@group(1) @binding(2) var<storage, read> visible: array<u32>;
@group(2) @binding(0) var<storage, read> triangle_bases: array<u32>;
struct MeshletOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) run: u32,
}
@vertex
fn vs(@location(0) position: vec3<f32>, @builtin(instance_index) run: u32) -> MeshletOut {
    var out: MeshletOut;
    out.clip = render.projection * render.view * render.model * vec4<f32>(position, 1.0);
    out.run = run;
    return out;
}
fn unpack(flag: u32) -> vec3<f32> {
    let red = f32(flag & 0xFFu) / 255.0;
    let green = f32((flag >> 8u) & 0xFFu) / 255.0;
    let blue = f32((flag >> 16u) & 0xFFu) / 255.0;
    return vec3<f32>(red, green, blue);
}
@fragment
fn fs(in: MeshletOut, @builtin(primitive_index) local_triangle: u32) -> @location(0) vec4<f32> {
    let triangle = triangle_bases[in.run] + local_triangle;
    let flag = visible[owners[triangle]];
    if (flag == 6u) { discard; }
    if (flag >= 256u) {
        return vec4<f32>(unpack(flag - 256u), 1.0);
    }
    if (flag == 7u) {
        return vec4<f32>(unpack(colors[triangle]), 1.0);
    }
    if (flag >= 16u) {
        let level = flag - 16u;
        let warm = f32(min(level, 8u)) / 8.0;
        return vec4<f32>(warm, 0.85 - warm * 0.45, 1.0 - warm, 1.0);
    }
    if (flag == 5u) { return vec4<f32>(1.0, 0.15, 0.85, 1.0); }
    if (flag == 1u) { return vec4<f32>(0.15, 0.85, 0.25, 1.0); }
    if (flag == 4u) { return vec4<f32>(0.95, 0.78, 0.20, 1.0); }
    if (flag == 3u) { return vec4<f32>(0.20, 0.45, 0.95, 1.0); }
    if (flag == 0u) { return vec4<f32>(0.90, 0.16, 0.14, 1.0); }
    if (flag == 2u) { return vec4<f32>(1.0, 0.15, 0.85, 1.0); }
    return vec4<f32>(unpack(colors[triangle]), 1.0);
}
"#;

const PARENT_COLOR_SHADER: &str = r#"
struct RenderUniforms {
    projection: mat4x4<f32>,
    view: mat4x4<f32>,
    model: mat4x4<f32>,
}
struct ParentOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
}
@group(0) @binding(0) var<uniform> render: RenderUniforms;
@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> ParentOut {
    var out: ParentOut;
    out.clip = render.projection * render.view * render.model * vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}
@fragment
fn fs(in: ParentOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;


/// Compiled program residency. Shared by every instance of that master.
struct GpuMasterMaterial {
    master: MasterMaterialId,
    program_key: u64,
    vertex_key: u64,
    shader: ShaderModuleId,
    pipeline: PipelineId,
    layout: BindGroupLayoutId,
    cull: jarvig_material::CullMode,
    two_sided: bool,
}

/// Parameter residency for one logical instance. Not a shader and not a pipeline.
struct GpuMaterialInstance {
    instance: MaterialInstanceId,
    revision: u64,
    buffer: BufferId,
    group: BindGroupId,
    textures: Vec<LogicalTextureId>,
    samplers: Vec<SamplerId>,
}

struct ResidentTexture {
    id: LogicalTextureId,
    revision: u64,
    texture: TextureId,
    view: TextureViewId,
}

struct Prepared {
    id: RenderViewId,
    label: String,
    rect: PixelRect,
    depth: TextureViewId,
    draws: Vec<PreparedDraw>,
    terrain_grid: Vec<TerrainGridDraw>,
}

struct TerrainGridDraw {
    mesh: MeshId,
    transform: BindGroupId,
}

/// One editing grid painted on existing terrain meshes. Not a level object and not a second mesh.
#[derive(Clone, Debug)]
pub struct TerrainGridDesc {
    pub meshes: Vec<MeshId>,
    pub minor_m: f32,
    pub major_m: f32,
    pub world: bool,
    pub vertices: bool,
    pub chunks: bool,
    pub lod: bool,
    pub lod_enabled: bool,
    pub half_x: f32,
    pub half_z: f32,
    pub vertex_spacing: f32,
    pub chunk_m: f32,
    pub brush_x: f32,
    pub brush_z: f32,
    pub brush_radius: f32,
    pub brush_enabled: bool,
    /// Meters at the brush center. Thickens the falloff rings. Does not move a vertex.
    pub brush_strength: f32,
    /// Smooth cosine falloff when true. Linear when false. Ignored while the brush is off.
    pub brush_smooth: bool,
    pub brush_color: [f32; 4],
}

struct TerrainGridGpu {
    shader: ShaderModuleId,
    layout: BindGroupLayoutId,
    pipelines: Vec<(u64, PipelineId)>,
    views: Vec<TerrainGridView>,
}

struct TerrainGridView {
    view: RenderViewId,
    buffer: BufferId,
    group: BindGroupId,
}

/// One editor overlay for one view. Not a scene object and not an entity.
/// Positions are camera-relative meters, already small float32 values.
#[derive(Clone, Debug)]
pub struct OverlayVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct EditorOverlay {
    pub view: RenderViewId,
    pub vertices: Vec<OverlayVertex>,
}

struct OverlayGpu {
    shader: ShaderModuleId,
    pipeline: PipelineId,
    uniform: BufferId,
    vertices: Option<BufferId>,
    group: BindGroupId,
    last_vertices: Vec<u8>,
}

impl Renderer {
    pub fn new(
        device: Box<dyn Device>,
        swapchain: SwapchainId,
        color_format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            device,
            color_format,
            width,
            height,
            target: SurfaceTarget {
                generation: 1,
                kind: RenderTargetKind::Surface,
                swapchain,
                clear: JARVIG_CLEAR,
            },
            views: vec![ViewSlot { generation: 0, view: None }],
            free_views: Vec::new(),
            bindings: Vec::new(),
            light_packets: Vec::new(),
            environment_gpu: None,
            probe_residents: Vec::new(),
            probe_unbound: None,
            probe_count: 0,
            probe_active_count: 0,
            probe_capture_count: 0,
            probe_recapture_count: 0,
            irradiance_build_count: 0,
            last_capture_ms: 0,
            last_prefilter_ms: 0,
            probe_queue_len: 0,
            probe_job: None,
            probe_selected_far: 0,
            probe_fallback_count: 0,
            camera_layout: None,
            light_layout: None,
            gpu_masters: Vec::new(),
            gpu_materials: Vec::new(),
            gpu_textures: Vec::new(),
            gpu_samplers: Vec::new(),
            gpu_meshes: Vec::new(),
            mesh_uploads: 0,
            material_pipelines: 0,
            material_parameter_uploads: 0,
            unbound_material_skips: 0,
            texture_uploads: 0,
            texture_binding_updates: 0,
            missing_texture_uses: 0,
            light_uploads: 0,
            environment_uploads: 0,
            instance_transform_uploads: 0,
            tone_map_parameter_uploads: 0,
            tone_map_pipelines: 0,
            output_passes_last: 0,
            submitted_last: 0,
            depth_creates: 0,
            acquires_last: 0,
            presents_last: 0,
            resize_applied: 0,
            resize_failures: 0,
            shut_down: false,
            overlay: None,
            overlay_gpu: None,
            reference: None,
            reference_gpu: None,
            terrain_grid: None,
            terrain_grid_meshes: HashSet::new(),
            entity_hidden: HashSet::new(),
            land_unlit: false,
            terrain_grid_gpu: None,
            hdr: None,
            output: None,
            output_views: Vec::new(),
            shadow: None,
            shadow_world_revision: 0,
            shadow_update_count: 0,
            shadow_maps_updated: 0,
            shadow_draws_frame: 0,
            shadow_casters: 0,
            shadow_pass_ms: 0.0,
            contact: None,
            contact_gpu: None,
            contact_requested: true,
            meshlet_gpu: None,
            parent_gpu: None,
            einstein_debug: false,
            einstein_seed: 1,
            einstein_shader: None,
            einstein_layout: None,
            einstein_pipeline: None,
            einstein_uniform: None,
            einstein_group: None,
            micro_enabled: false,
            micro_surface: true,
            detail_exact: false,
            micro_color: false,
            micro_seed: 1,
            micro_live: Vec::new(),
            micro_pending: None,
            micro_request: None,
            micro_next_epoch: 0,
            micro_wanted_epoch: 0,
            micro_published_epoch: 0,
            micro_wanted_key: 0,
            micro_inflight_epoch: 0,
            micro_inflight_key: 0,
            micro_stage: "Ready",
            micro_queue_wait_us: 0,
            micro_classify_us: 0,
            micro_host_wait_us: 0,
            micro_publish_us: 0,
            micro_cancelled: 0,
            micro_stale_discarded: 0,
            micro_partial: 0,
            micro_swapped: false,
            micro_index_count: 0,
            micro_fingerprint: 0,
            micro_input_key: 0,
            micro_debug: jarvig_core::MicroDebugMode::Off,
            micro_compatible: true,
            micro_reasons: Vec::new(),
            micro_projected: Vec::new(),
            micro_desired_patches: 0,
            micro_missing: 0,
            micro_eligible_visible: 0,
            micro_detail_requested: 0,
            micro_unsupported: 0,
            micro_unclassified: 0,
            micro_states: Vec::new(),
            micro_still_frames: 0,
            micro_coverage_prev_key: 0,
            micro_class_shader: None,
            micro_class_pipeline: None,
            micro_parent_pipeline: None,
            micro_parent_solid: None,
            diagnostic_draw_shader: None,
            diagnostic_draw_layout: None,
            diagnostic_draw_uniform: None,
            diagnostic_draw_group: None,
            diagnostic_draw_pipelines: Vec::new(),
            diagnostic_base_capacity: 0,
            diagnostic_run_cursor: 0,
            micro_class_colors: Vec::new(),
            micro_color_pipeline: None,
            micro_color_shader: None,
            diagnostic_freeze: false,
            diagnostic_reused: false,
            diagnostic_frozen: None,
            gpu_scene: GpuSceneState {
                remembered: Vec::new(),
                instances: None,
                instance_capacity: 0,
                geometries: None,
                meshlets: None,
                meshlet_key: 0,
                stats: GpuSceneFrameStats::default(),
                frustum: true,
                occlusion: true,
                freeze: false,
                frozen_flags: Vec::new(),
                frozen_stats: None,
                highlight: None,
                hierarchy: true,
                hierarchy_key: 0,
                hierarchy_nodes: None,
                frozen_cut: false,
                hierarchy_error_px: 1.0,
                debug_flags: Vec::new(),
                parent_indices: Vec::new(),
                parent_key: 0,
                leaf_truth: false,
                diagnostic: jarvig_core::GeometryDiagnostic::Normal,
                visualization: jarvig_core::DiagnosticVisualization::None,
            },
        }
    }

    /// Keep one mesh's cluster bounds. A second instance of the same mesh does not copy them.
    pub fn remember_meshlets(&mut self, mesh: jarvig_core::MeshId, records: &[jarvig_core::GpuMeshletRecord]) {
        if let Some(slot) = self.gpu_scene.remembered.iter_mut().find(|(id, _)| *id == mesh) {
            if slot.1.len() == records.len() {
                return;
            }
            slot.1 = records.to_vec();
            return;
        }
        self.gpu_scene.remembered.push((mesh, records.to_vec()));
    }

    pub fn gpu_scene_stats(&self) -> GpuSceneFrameStats {
        let mut stats = self.gpu_scene.stats;
        stats.frustum = self.visibility_frustum();
        stats.occlusion = self.gpu_scene.diagnostic.occlusion(self.gpu_scene.occlusion);
        stats.frozen = self.gpu_scene.freeze;
        stats.hierarchy = self.visibility_parents();
        stats.leaf_truth = self.gpu_scene.diagnostic == jarvig_core::GeometryDiagnostic::LeafTruth;
        stats
    }

    pub fn set_geometry_diagnostic(&mut self, mode: jarvig_core::GeometryDiagnostic) {
        let changed = self.gpu_scene.diagnostic != mode;
        self.gpu_scene.diagnostic = mode;
        self.gpu_scene.leaf_truth = mode == jarvig_core::GeometryDiagnostic::LeafTruth;
        if changed {
            self.diagnostic_frozen = None;
            self.diagnostic_reused = false;
            if mode != jarvig_core::GeometryDiagnostic::LeafTruth {
                self.gpu_scene.frozen_cut = false;
            }
        }
    }

    pub fn set_diagnostic_visualization(&mut self, mode: jarvig_core::DiagnosticVisualization) {
        self.gpu_scene.visualization = mode;
    }

    pub fn set_diagnostic_freeze(&mut self, enabled: bool) {
        self.diagnostic_freeze = enabled;
        if !enabled {
            self.diagnostic_frozen = None;
            self.diagnostic_reused = false;
        }
    }

    pub fn set_leaf_truth(&mut self, enabled: bool) {
        if enabled {
            self.set_geometry_diagnostic(jarvig_core::GeometryDiagnostic::LeafTruth);
        } else if self.gpu_scene.diagnostic == jarvig_core::GeometryDiagnostic::LeafTruth {
            self.set_geometry_diagnostic(jarvig_core::GeometryDiagnostic::Normal);
        }
    }

    fn visibility_frustum(&self) -> bool {
        self.gpu_scene.diagnostic.frustum(self.gpu_scene.frustum)
    }

    fn visibility_occlusion(&self) -> bool {
        self.visibility_frustum() && self.gpu_scene.diagnostic.occlusion(self.gpu_scene.occlusion)
    }

    fn visibility_parents(&self) -> bool {
        self.gpu_scene.diagnostic.parents(self.gpu_scene.hierarchy)
    }

    pub fn set_meshlet_frustum(&mut self, enabled: bool) {
        self.gpu_scene.frustum = enabled;
    }

    pub fn set_meshlet_occlusion(&mut self, enabled: bool) {
        self.gpu_scene.occlusion = enabled;
    }

    pub fn set_meshlet_freeze(&mut self, enabled: bool) {
        self.gpu_scene.freeze = enabled;
        if !enabled {
            self.gpu_scene.frozen_flags.clear();
            self.gpu_scene.frozen_stats = None;
        }
    }

    pub fn set_meshlet_highlight(&mut self, cluster: Option<u32>) {
        self.gpu_scene.highlight = cluster;
    }

    pub fn set_micro_surface(&mut self, surface: bool) {
        self.micro_surface = surface;
    }

    pub fn set_detail_exact(&mut self, exact: bool) {
        self.detail_exact = exact;
    }

    pub fn set_micro_debug(&mut self, mode: jarvig_core::MicroDebugMode) {
        self.micro_debug = mode;
    }

    pub fn set_micro_compatible(&mut self, compatible: bool) {
        self.micro_compatible = compatible;
    }

    pub fn set_microgeometry(&mut self, enabled: bool, seed: u64, color: bool) {
        if self.micro_enabled && !enabled {
            self.publish_micro_absence();
            self.clear_drawn_micro_stats();
        }
        self.micro_enabled = enabled;
        self.micro_seed = seed;
        self.micro_color = color;
    }

    pub fn take_micro_request(&mut self) -> Option<MicroBuildRequest> {
        self.micro_request.take()
    }

    pub fn micro_wanted_epoch(&self) -> u64 {
        self.micro_wanted_epoch
    }

    pub fn micro_stage(&self) -> &'static str {
        self.micro_stage
    }

    pub fn micro_publish_settled(&self) -> bool {
        !self.micro_enabled
            || (self.micro_request.is_none()
                && self.micro_pending.is_none()
                && self.micro_wanted_epoch == self.micro_published_epoch
                && self.micro_wanted_key == self.micro_input_key)
    }

    pub fn note_micro_cancelled(&mut self) {
        self.micro_cancelled = self.micro_cancelled.saturating_add(1);
        self.gpu_scene.stats.micro_cancelled = self.micro_cancelled;
    }

    /// Host delay between the worker finishing and this thread taking the mesh. Not the GPU upload.
    pub fn note_micro_host_wait(&mut self, microseconds: u32) {
        self.micro_host_wait_us = microseconds;
        self.gpu_scene.stats.micro_host_wait_us = microseconds;
    }

    /// The worker finished without a mesh the renderer kept. The next frame may request that key again.
    pub fn abandon_micro_build(&mut self, epoch: u64) {
        if self.micro_inflight_epoch == epoch && self.micro_pending.as_ref().is_none_or(|pending| pending.epoch != epoch) {
            self.micro_inflight_epoch = 0;
            self.micro_inflight_key = 0;
        }
    }

    pub fn set_einstein_debug(&mut self, enabled: bool, seed: u64) {
        self.einstein_debug = enabled;
        self.einstein_seed = seed;
    }

    pub fn set_cluster_hierarchy(&mut self, enabled: bool) {
        self.gpu_scene.hierarchy = enabled;
        if !enabled {
            self.gpu_scene.hierarchy_nodes = None;
            self.gpu_scene.hierarchy_key = 0;
            self.gpu_scene.stats.hierarchy = false;
            self.gpu_scene.stats.hierarchy_cut = 0;
            self.gpu_scene.stats.hierarchy_nodes = 0;
            self.gpu_scene.stats.hierarchy_lod_triangles = 0;
            self.gpu_scene.stats.hierarchy_leaf_triangles = 0;
            self.gpu_scene.stats.hierarchy_leaves = 0;
            self.gpu_scene.stats.hierarchy_parents = 0;
            self.gpu_scene.stats.hierarchy_depth = 0;
            self.gpu_scene.stats.hierarchy_select_us = 0;
            self.gpu_scene.stats.hierarchy_cut_us = 0;
            self.gpu_scene.stats.hierarchy_upload_us = 0;
            self.gpu_scene.stats.hierarchy_upload_bytes = 0;
            self.gpu_scene.stats.hierarchy_covered_leaves = 0;
            self.gpu_scene.stats.hierarchy_uncovered_leaves = 0;
        }
    }

    pub fn set_hierarchy_error_px(&mut self, pixels: f32) {
        if pixels.is_finite() && pixels > 0.0 {
            self.gpu_scene.hierarchy_error_px = pixels.clamp(0.25, 8.0);
        }
    }

    pub fn parent_geometry_mesh(&self) -> Option<MeshId> {
        self.parent_gpu.as_ref().map(|gpu| gpu.mesh)
    }

    /// Resident hierarchy bytes after the sidecar is live. Peak build memory is not retained on a cache hit.
    pub fn hierarchy_residency(&self) -> HierarchyResidency {
        let parent = self.parent_gpu.as_ref();
        HierarchyResidency {
            leaf_meshlet_bytes: self.gpu_scene.stats.meshlet_bytes,
            hierarchy_cpu_bytes: parent.map(|gpu| gpu.cpu_bytes).unwrap_or(0),
            parent_gpu_static_bytes: parent.map(|gpu| gpu.gpu_static_bytes).unwrap_or(0),
            parent_gpu_frame_bytes: parent.map(|gpu| u64::from(gpu.frame_count) * 4).unwrap_or(0),
            recorded_build_ms: parent.map(|gpu| gpu.build_ms).unwrap_or(0.0),
        }
    }

    /// Upload simplified parent meshes for one hierarchy. The leaf meshlets stay.
    pub fn set_parent_geometry(&mut self, mesh: MeshId, geometry: jarvig_core::ParentGeometry) -> Result<(), RenderError> {
        self.clear_parent_geometry()?;
        if geometry.vertices.is_empty() || geometry.vertices.len() % 60 != 0 || geometry.debug_colors.len() != geometry.vertices.len() / 60 {
            return Ok(());
        }
        self.ensure_camera_layout()?;
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(PARENT_COLOR_SHADER.into()),
                label: Some("JARVIG.ParentColor".into()),
            })
            .map_err(RenderError::Rhi)?;
        let groups = parent_vertex_groups(&geometry.indices, &geometry.ranges, geometry.vertices.len() / 60);
        let mut chunks = Vec::new();
        for (start, end) in groups {
            let byte_start = start as usize * 60;
            let byte_end = end as usize * 60;
            let vert_bytes = geometry.vertices.get(byte_start..byte_end).unwrap_or_default().to_vec();
            let mut debug_bytes = Vec::with_capacity((end - start) as usize * 24);
            for index in start..end {
                let color = geometry.debug_colors.get(index as usize).copied().unwrap_or([1.0, 1.0, 1.0]);
                let origin = index as usize * 60;
                if let Some(position) = geometry.vertices.get(origin..origin + 12) {
                    debug_bytes.extend_from_slice(position);
                }
                for lane in color {
                    debug_bytes.extend_from_slice(&lane.to_le_bytes());
                }
            }
            let vertices = match self.try_parent_buffer(BufferDesc {
                size: vert_bytes.len() as u64,
                usage: BufferUsage::Vertex,
                label: Some("JARVIG.ParentGeometry.Vertices".into()),
                contents: Some(vert_bytes),
            }) {
                Ok(vertices) => vertices,
                Err(error) => {
                    self.destroy_parent_chunks(&chunks)?;
                    let _ = self.device.destroy(ResourceKind::ShaderModule, shader.raw());
                    return Err(error);
                }
            };
            let debug_vertices = match self.try_parent_buffer(BufferDesc {
                size: debug_bytes.len() as u64,
                usage: BufferUsage::Vertex,
                label: Some("JARVIG.ParentGeometry.Colors".into()),
                contents: Some(debug_bytes),
            }) {
                Ok(debug_vertices) => debug_vertices,
                Err(error) => {
                    let _ = self.device.destroy(ResourceKind::Buffer, vertices.raw());
                    self.destroy_parent_chunks(&chunks)?;
                    let _ = self.device.destroy(ResourceKind::ShaderModule, shader.raw());
                    return Err(error);
                }
            };
            chunks.push(ParentChunk {
                vertices,
                debug_vertices,
                first_vertex: start,
                vertex_count: end.saturating_sub(start),
                frame: None,
                frame_count: 0,
            });
        }
        let draw_ok = jarvig_core::drawable_parent_mask(&geometry);
        let vertex_count = geometry.vertices.len() / 60;
        let positions_retained = vertex_count <= jarvig_core::PARENT_POSITION_KEEP;
        let positions = if positions_retained {
            geometry
                .vertices
                .chunks_exact(60)
                .map(|chunk| {
                    [
                        f32::from_le_bytes(chunk[0..4].try_into().unwrap_or([0; 4])),
                        f32::from_le_bytes(chunk[4..8].try_into().unwrap_or([0; 4])),
                        f32::from_le_bytes(chunk[8..12].try_into().unwrap_or([0; 4])),
                    ]
                })
                .collect()
        } else {
            Vec::new()
        };
        let cpu_bytes = (geometry.hierarchy.nodes.len() * std::mem::size_of::<jarvig_core::ClusterNode>()
            + positions.len() * std::mem::size_of::<[f32; 3]>()
            + geometry.hierarchy.roots.len() * std::mem::size_of::<u32>()
            + geometry.ranges.len() * std::mem::size_of::<jarvig_core::ParentRange>()
            + geometry.leaf_counts.len() * std::mem::size_of::<u32>()
            + geometry.indices.len() * std::mem::size_of::<u32>()) as u64;
        let gpu_static_bytes = chunks.iter().fold(0u64, |sum, chunk| sum + u64::from(chunk.vertex_count) * (60 + 24));
        let build_ms = geometry.build_ms;
        self.parent_gpu = Some(ParentGpu {
            mesh,
            chunks,
            hierarchy: geometry.hierarchy,
            ranges: geometry.ranges,
            leaf_counts: geometry.leaf_counts,
            leaf_triangles: geometry.leaf_triangles,
            root_triangles: geometry.root_triangles,
            empty_parents: geometry.empty_parents,
            indices: geometry.indices,
            cpu_bytes,
            gpu_static_bytes,
            build_ms,
            frame_count: 0,
            frame_key: 0,
            shader,
            pipeline: None,
            color_two_sided: false,
            draw_ok,
            positions,
            positions_retained,
        });
        Ok(())
    }

    /// Positions kept beside the upload so a coverage compare can raster parent triangles.
    pub fn parent_coverage_source(&self) -> Option<ParentCoverageSource<'_>> {
        let gpu = self.parent_gpu.as_ref()?;
        Some(ParentCoverageSource {
            mesh: gpu.mesh,
            positions: &gpu.positions,
            indices: &gpu.indices,
            ranges: &gpu.ranges,
            hierarchy: &gpu.hierarchy,
            draw_ok: &gpu.draw_ok,
            leaf_counts: &gpu.leaf_counts,
            positions_retained: gpu.positions_retained,
        })
    }

    fn destroy_parent_chunks(&mut self, chunks: &[ParentChunk]) -> Result<(), RenderError> {
        for chunk in chunks {
            self.device.destroy(ResourceKind::Buffer, chunk.vertices.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, chunk.debug_vertices.raw()).map_err(RenderError::Rhi)?;
            if let Some(frame) = chunk.frame {
                self.device.destroy(ResourceKind::Buffer, frame.raw()).map_err(RenderError::Rhi)?;
            }
        }
        Ok(())
    }

    /// Intel UHD rejects a buffer over 256 MB by panicking inside wgpu. Stay under that, and do not let the panic escape the frame.
    fn try_parent_buffer(&mut self, desc: BufferDesc) -> Result<BufferId, RenderError> {
        const MAX_BUFFER: u64 = 200 * 1024 * 1024;
        if desc.size > MAX_BUFFER {
            return Err(RenderError::Rhi(RhiError::Validation(format!(
                "{} is {} bytes. This device accepts at most 256 MB, and the upload cap is {} bytes. Leaf meshlets stay.",
                desc.label.as_deref().unwrap_or("buffer"),
                desc.size,
                MAX_BUFFER
            ))));
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.device.create_buffer(&desc)));
        match outcome {
            Ok(result) => result.map_err(RenderError::Rhi),
            Err(_) => Err(RenderError::Rhi(RhiError::Validation(format!(
                "{} creation failed. Leaf meshlets stay.",
                desc.label.as_deref().unwrap_or("buffer")
            )))),
        }
    }

    fn clear_parent_geometry(&mut self) -> Result<(), RenderError> {
        let Some(gpu) = self.parent_gpu.take() else { return Ok(()) };
        if let Some(pipeline) = gpu.pipeline {
            self.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        self.destroy_parent_chunks(&gpu.chunks)?;
        self.device.destroy(ResourceKind::ShaderModule, gpu.shader.raw()).map_err(RenderError::Rhi)?;
        self.gpu_scene.frozen_cut = false;
        Ok(())
    }

    fn sync_gpu_scene(
        &mut self,
        snapshot: &jarvig_core::RenderSceneSnapshot,
        camera: &jarvig_core::ResolvedPose,
        fov: f64,
        near: f32,
        aspect: f32,
    ) -> Result<(), RenderError> {
        let pack = {
            let lookups: Vec<jarvig_core::GpuSceneGeometry<'_>> = snapshot
                .instances()
                .iter()
                .map(|instance| jarvig_core::GpuSceneGeometry {
                    mesh: instance.mesh,
                    meshlets: self
                        .gpu_scene
                        .remembered
                        .iter()
                        .find(|(id, _)| *id == instance.mesh)
                        .map(|(_, records)| records.as_slice())
                        .unwrap_or(&[]),
                })
                .collect();
            jarvig_core::pack_gpu_scene(snapshot.instances(), camera, fov, near, aspect, &lookups).map_err(RenderError::Space)?
        };
        let key = self.gpu_scene.remembered.iter().fold(0x84222325cbf29ce4u64, |key, (mesh, records)| {
            key.wrapping_mul(0x100000001b3).wrapping_add(mesh.0).wrapping_add(records.len() as u64)
        });
        let started = std::time::Instant::now();
        if key != self.gpu_scene.meshlet_key {
            self.replace_scene_buffer(&mut |state| &mut state.geometries, &pack.geometries, "JARVIG.GpuScene.Geometries")?;
            self.replace_scene_buffer(&mut |state| &mut state.meshlets, &pack.meshlets, "JARVIG.GpuScene.Meshlets")?;
            self.gpu_scene.meshlet_key = key;
            self.gpu_scene.stats.meshlet_uploads = self.gpu_scene.stats.meshlet_uploads.saturating_add(1);
        }
        self.write_scene_instances(&pack.instances)?;
        self.gpu_scene.stats.instances = pack.stats.instance_count;
        self.gpu_scene.stats.geometries = pack.stats.geometry_count;
        self.gpu_scene.stats.meshlets = pack.stats.meshlet_count;
        self.gpu_scene.stats.meshlet_bytes = pack.stats.meshlet_bytes;
        self.gpu_scene.stats.instance_upload_us = u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX);
        self.cull_frame(snapshot, camera, fov, near, aspect)?;
        Ok(())
    }

    fn store_view_cut(&mut self, id: RenderViewId) {
        let flags = self.gpu_scene.debug_flags.clone();
        let indices = self.gpu_scene.parent_indices.clone();
        let key = self.gpu_scene.parent_key;
        if let Ok(view) = self.slot_mut(id) {
            view.cluster_flags = flags;
            view.parent_indices = indices;
            view.parent_key = key;
        }
    }

    /// Put this camera's cluster cut back before its draw. Another view's cut is not reused.
    fn apply_view_cut(&mut self, id: RenderViewId) -> Result<(), RenderError> {
        let (flags, indices, key) = {
            let view = self.slot(id)?;
            (view.cluster_flags.clone(), view.parent_indices.clone(), view.parent_key)
        };
        if !flags.is_empty() {
            self.gpu_scene.debug_flags = flags;
        }
        if self.visibility_parents() && self.parent_gpu.is_some() {
            self.replace_parent_frame(key, &indices)?;
        }
        Ok(())
    }

    fn cull_frame(
        &mut self,
        snapshot: &jarvig_core::RenderSceneSnapshot,
        camera: &jarvig_core::ResolvedPose,
        fov: f64,
        near: f32,
        aspect: f32,
    ) -> Result<(), RenderError> {
        if self.restore_frozen_diagnostic()? {
            return Ok(());
        }
        self.diagnostic_reused = false;
        self.gpu_scene.parent_indices.clear();
        self.gpu_scene.parent_key = 0;
        let cull = {
            let lookups: Vec<jarvig_core::GpuSceneGeometry<'_>> = snapshot
                .instances()
                .iter()
                .map(|instance| jarvig_core::GpuSceneGeometry {
                    mesh: instance.mesh,
                    meshlets: self.gpu_scene.remembered.iter().find(|(id, _)| *id == instance.mesh).map(|(_, records)| records.as_slice()).unwrap_or(&[]),
                })
                .collect();
            let frustum_on = self.visibility_frustum();
            let occlusion_on = self.visibility_occlusion();
            let frustum = jarvig_core::frustum_cull_meshlets(snapshot.instances(), camera, fov, near, aspect, &lookups, frustum_on).map_err(RenderError::Space)?;
            let (hz_width, hz_height) = jarvig_core::hierarchical_depth_extent(self.width, self.height);
            let occlusion = jarvig_core::hierarchical_occlusion_cull(
                snapshot.instances(),
                camera,
                fov,
                near,
                aspect,
                &lookups,
                &frustum,
                occlusion_on,
                self.width,
                self.height,
            )
            .map_err(RenderError::Space)?;
            (frustum, occlusion, hz_width, hz_height)
        };
        let (frustum, occlusion, hz_width, hz_height) = cull;
        self.gpu_scene.stats.hz_width = hz_width;
        self.gpu_scene.stats.hz_height = hz_height;
        self.gpu_scene.stats.meshlets_submitted = occlusion.submitted;
        self.gpu_scene.stats.meshlets_rejected = occlusion.frustum_rejected;
        self.gpu_scene.stats.occlusion_rejected = occlusion.occlusion_rejected;
        self.gpu_scene.stats.conservative_visible = occlusion.conservative_visible;
        self.gpu_scene.stats.triangles_submitted = occlusion.triangles_submitted;
        self.gpu_scene.stats.instances_visible = frustum.instances_visible;
        self.gpu_scene.stats.cull_us = frustum.cpu_us.saturating_add(occlusion.cpu_us);
        self.gpu_scene.stats.occlusion_us = occlusion.cpu_us;
        self.gpu_scene.stats.frustum = self.visibility_frustum();
        self.gpu_scene.stats.occlusion = self.gpu_scene.diagnostic.occlusion(self.gpu_scene.occlusion);
        let debug_mesh = self.meshlet_gpu.as_ref().map(|gpu| (gpu.mesh, gpu.span_source.clone(), gpu.visible));
        let Some((mesh, span_source, visible)) = debug_mesh else {
            self.gpu_scene.debug_flags.clear();
            return Ok(());
        };
        let records_len = self.gpu_scene.remembered.iter().find(|(id, _)| *id == mesh).map(|(_, records)| records.len()).unwrap_or(0);
        let mut cursor = 0usize;
        let mut source_flags: &[u32] = &[];
        for instance in snapshot.instances() {
            if instance.mesh != mesh {
                let skip = self.gpu_scene.remembered.iter().find(|(id, _)| *id == instance.mesh).map(|(_, records)| records.len()).unwrap_or(0);
                cursor += skip;
                continue;
            }
            let end = (cursor + records_len).min(occlusion.flags.len());
            source_flags = &occlusion.flags[cursor..end];
            break;
        }
        let mut draw_flags = vec![2u32; span_source.len().max(1)];
        if self.visibility_frustum() {
            for (draw_index, source) in span_source.iter().enumerate() {
                draw_flags[draw_index] = source_flags.get(*source as usize).copied().unwrap_or(0);
            }
        }
        let parent_ready = self.visibility_parents() && self.parent_gpu.as_ref().map(|gpu| gpu.mesh == mesh).unwrap_or(false);
        let reuse_frozen_cut = self.gpu_scene.diagnostic == jarvig_core::GeometryDiagnostic::Normal
            && parent_ready
            && self.gpu_scene.freeze
            && self.gpu_scene.frozen_cut
            && self.gpu_scene.frozen_flags.len() == draw_flags.len();
        if reuse_frozen_cut {
            draw_flags = self.gpu_scene.frozen_flags.clone();
            if let Some(stats) = self.gpu_scene.frozen_stats {
                self.gpu_scene.stats = stats;
            }
        } else if self.gpu_scene.freeze && !parent_ready {
            if self.gpu_scene.frozen_flags.len() == draw_flags.len() && !self.gpu_scene.frozen_flags.is_empty() {
                draw_flags = self.gpu_scene.frozen_flags.clone();
                if let Some(stats) = self.gpu_scene.frozen_stats {
                    self.gpu_scene.stats.meshlets_submitted = stats.meshlets_submitted;
                    self.gpu_scene.stats.meshlets_rejected = stats.meshlets_rejected;
                    self.gpu_scene.stats.occlusion_rejected = stats.occlusion_rejected;
                    self.gpu_scene.stats.conservative_visible = stats.conservative_visible;
                    self.gpu_scene.stats.triangles_submitted = stats.triangles_submitted;
                }
            } else {
                self.gpu_scene.frozen_flags = draw_flags.clone();
                self.gpu_scene.frozen_stats = Some(self.gpu_scene.stats);
            }
        } else if !self.gpu_scene.freeze {
            self.gpu_scene.frozen_flags.clear();
            self.gpu_scene.frozen_stats = None;
            self.gpu_scene.frozen_cut = false;
        }
        if !reuse_frozen_cut {
            self.gpu_scene.stats.hierarchy = self.gpu_scene.hierarchy;
            self.gpu_scene.stats.hierarchy_cut = 0;
            self.gpu_scene.stats.hierarchy_nodes = 0;
            self.gpu_scene.stats.hierarchy_lod_triangles = 0;
            self.gpu_scene.stats.hierarchy_leaf_triangles = 0;
            self.gpu_scene.stats.hierarchy_leaves = 0;
            self.gpu_scene.stats.hierarchy_parents = 0;
            self.gpu_scene.stats.hierarchy_depth = 0;
            self.gpu_scene.stats.hierarchy_select_us = 0;
            self.gpu_scene.stats.hierarchy_cut_us = 0;
            self.gpu_scene.stats.hierarchy_upload_us = 0;
            self.gpu_scene.stats.hierarchy_upload_bytes = 0;
            self.gpu_scene.stats.hierarchy_draw_prepare_us = 0;
            self.gpu_scene.stats.hierarchy_covered_leaves = 0;
            self.gpu_scene.stats.hierarchy_uncovered_leaves = 0;
        }
        let coarseness = if !self.visibility_parents() {
            Vec::new()
        } else if reuse_frozen_cut {
            Vec::new()
        } else if parent_ready {
            self.submit_parent_cut(snapshot, camera, fov, near, aspect, &span_source, &mut draw_flags)?;
            if self.gpu_scene.freeze {
                self.gpu_scene.frozen_flags = draw_flags.clone();
                self.gpu_scene.frozen_stats = Some(self.gpu_scene.stats);
                self.gpu_scene.frozen_cut = true;
            }
            Vec::new()
        } else if self.visibility_parents() {
            let records = self.gpu_scene.remembered.iter().find(|(id, _)| *id == mesh).map(|(_, records)| records.clone()).unwrap_or_default();
            let key = mesh.0 ^ ((records.len() as u64) << 32);
            if self.gpu_scene.hierarchy_key != key || self.gpu_scene.hierarchy_nodes.is_none() {
                self.gpu_scene.hierarchy_nodes = Some(jarvig_core::build_cluster_hierarchy(&records));
                self.gpu_scene.hierarchy_key = key;
            }
            let node_count = self.gpu_scene.hierarchy_nodes.as_ref().map(|hierarchy| hierarchy.nodes.len() as u32).unwrap_or(0);
            let leaf_count = records.len();
            let mut leaf_flags = vec![1u32; leaf_count];
            if self.visibility_frustum() {
                leaf_flags.fill(0);
                for (draw_index, source) in span_source.iter().enumerate() {
                    if let Some(slot) = leaf_flags.get_mut(*source as usize) {
                        *slot = draw_flags.get(draw_index).copied().unwrap_or(0);
                    }
                }
            }
            let triangles: Vec<u32> = records.iter().map(|record| record.triangles).collect();
            let cut = self.gpu_scene.hierarchy_nodes.as_ref().and_then(|hierarchy| {
                snapshot.instances().iter().find(|instance| instance.mesh == mesh).and_then(|instance| {
                    jarvig_core::cut_visible_hierarchy_for_pose(
                        hierarchy,
                        instance,
                        camera,
                        fov,
                        near,
                        aspect,
                        self.height as f32,
                        self.gpu_scene.hierarchy_error_px,
                        &leaf_flags,
                        &triangles,
                    )
                    .ok()
                })
            });
            if let Some(cut) = cut {
                self.gpu_scene.stats.hierarchy = true;
                self.gpu_scene.stats.hierarchy_cut = cut.selected.len() as u32;
                self.gpu_scene.stats.hierarchy_nodes = node_count;
                self.gpu_scene.stats.hierarchy_leaves = cut.leaf_clusters;
                self.gpu_scene.stats.hierarchy_parents = cut.parent_clusters;
                self.gpu_scene.stats.hierarchy_candidates = cut.candidate_nodes;
                self.gpu_scene.stats.hierarchy_select_us = cut.selection_us;
                self.gpu_scene.stats.hierarchy_covered_leaves = cut.covered_leaves;
                cut.coarseness
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let prepare_started = std::time::Instant::now();
        if self.gpu_scene.diagnostic.identity_colors() {
            draw_flags = jarvig_core::stage_leaf_flags(self.gpu_scene.diagnostic, &draw_flags);
            self.gpu_scene.parent_indices.clear();
            self.gpu_scene.parent_key = 0;
            self.gpu_scene.stats.hierarchy_parents = 0;
            self.gpu_scene.stats.hierarchy_lod_triangles = 0;
            self.gpu_scene.stats.hierarchy_uncovered_leaves = 0;
            self.replace_parent_frame(0, &[])?;
        }
        self.gpu_scene.stats.leaf_truth = self.gpu_scene.diagnostic == jarvig_core::GeometryDiagnostic::LeafTruth;
        if self.diagnostic_freeze {
            self.diagnostic_frozen = Some(FrozenDiagnostic {
                pipeline: self.gpu_scene.diagnostic,
                flags: draw_flags.clone(),
                parent_indices: self.gpu_scene.parent_indices.clone(),
                parent_key: self.gpu_scene.parent_key,
                coarseness: coarseness.clone(),
                submitted: self.gpu_scene.stats.meshlets_submitted,
                frustum_rejected: self.gpu_scene.stats.meshlets_rejected,
                occlusion_rejected: self.gpu_scene.stats.occlusion_rejected,
                conservative: self.gpu_scene.stats.conservative_visible,
                triangles: self.gpu_scene.stats.triangles_submitted,
                parents: self.gpu_scene.stats.hierarchy_parents,
                leaves: self.gpu_scene.stats.hierarchy_leaves,
                hierarchy: self.gpu_scene.stats.hierarchy,
            });
        }
        self.paint_diagnostic_colors(&span_source, visible, draw_flags, &coarseness, parent_ready)?;
        self.gpu_scene.stats.hierarchy_draw_prepare_us = prepare_started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        Ok(())
    }

    /// Reuse the captured submission. Visualization may recolor it and does not run the cull again.
    fn restore_frozen_diagnostic(&mut self) -> Result<bool, RenderError> {
        if !self.diagnostic_freeze {
            return Ok(false);
        }
        let Some(frozen) = self.diagnostic_frozen.clone() else { return Ok(false) };
        let Some(gpu) = self.meshlet_gpu.as_ref() else { return Ok(false) };
        let span_len = gpu.span_source.len().max(1);
        if !jarvig_core::diagnostic_frame_is_reusable(true, Some(frozen.pipeline), frozen.flags.len(), self.gpu_scene.diagnostic, span_len) {
            return Ok(false);
        }
        let span_source = gpu.span_source.clone();
        let visible = gpu.visible;
        let parents = frozen.parents > 0;
        self.gpu_scene.stats.meshlets_submitted = frozen.submitted;
        self.gpu_scene.stats.meshlets_rejected = frozen.frustum_rejected;
        self.gpu_scene.stats.occlusion_rejected = frozen.occlusion_rejected;
        self.gpu_scene.stats.conservative_visible = frozen.conservative;
        self.gpu_scene.stats.triangles_submitted = frozen.triangles;
        self.gpu_scene.stats.hierarchy_parents = frozen.parents;
        self.gpu_scene.stats.hierarchy_leaves = frozen.leaves;
        self.gpu_scene.stats.hierarchy = frozen.hierarchy;
        self.gpu_scene.stats.frustum = self.visibility_frustum();
        self.gpu_scene.stats.occlusion = self.gpu_scene.diagnostic.occlusion(self.gpu_scene.occlusion);
        self.gpu_scene.stats.leaf_truth = self.gpu_scene.diagnostic == jarvig_core::GeometryDiagnostic::LeafTruth;
        self.gpu_scene.parent_indices = frozen.parent_indices.clone();
        self.gpu_scene.parent_key = frozen.parent_key;
        self.replace_parent_frame(frozen.parent_key, &frozen.parent_indices)?;
        self.diagnostic_reused = true;
        self.paint_diagnostic_colors(&span_source, visible, frozen.flags, &frozen.coarseness, parents)?;
        Ok(true)
    }

    /// Colors follow the visualization. `draw_flags` stay the pipeline submission.
    fn paint_diagnostic_colors(
        &mut self,
        span_source: &[u32],
        visible: BufferId,
        draw_flags: Vec<u32>,
        coarseness: &[u32],
        parent_ready: bool,
    ) -> Result<(), RenderError> {
        let mut color_flags = draw_flags.clone();
        let visualization = self.gpu_scene.visualization;
        if visualization.shows_level_colors() && !coarseness.is_empty() {
            for (draw_index, source) in span_source.iter().enumerate() {
                let level = coarseness.get(*source as usize).copied().unwrap_or(0).min(15);
                if let Some(slot) = color_flags.get_mut(draw_index) {
                    *slot = 16 + level;
                }
            }
        } else if visualization.shows_level_colors() && parent_ready {
            for flag in &mut color_flags {
                if *flag == 1 || *flag == 4 {
                    *flag = 16;
                }
            }
        }
        if let Some(cluster) = self.gpu_scene.highlight {
            if !self.gpu_scene.diagnostic.identity_colors() && !visualization.paints_einstein() {
                for (draw_index, source) in span_source.iter().enumerate() {
                    if *source == cluster {
                        if let Some(slot) = color_flags.get_mut(draw_index) {
                            *slot = 5;
                        }
                    }
                }
            }
        }
        let capacity = self.meshlet_gpu.as_ref().map(|gpu| gpu.visible_capacity).unwrap_or(0);
        self.gpu_scene.debug_flags = draw_flags;
        self.gpu_scene.stats.frozen = self.gpu_scene.freeze || self.diagnostic_freeze;
        if !color_flags.is_empty() && color_flags.len() <= capacity {
            let bytes = u32_bytes(&color_flags);
            self.device.write_buffer(visible, 0, &bytes).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    /// Einstein colors replace the flag words in the color buffer. `debug_flags` stay the submission.
    fn paint_einstein_colors(&mut self) -> Result<(), RenderError> {
        let Some(mode) = self.gpu_scene.visualization.detail_mode() else { return Ok(()) };
        let Some(gpu) = self.meshlet_gpu.as_ref() else { return Ok(()) };
        let count = gpu.span_source.len();
        let capacity = gpu.visible_capacity;
        let visible = gpu.visible;
        if count == 0 || count > capacity {
            return Ok(());
        }
        let stage = self.micro_debug_stage();
        let neutral = [0.22_f32, 0.24, 0.28];
        let active = [0.95_f32, 0.40, 0.72];
        let mut colors = Vec::with_capacity(count);
        for index in 0..count {
            let reason = self.micro_reasons.get(index).copied().unwrap_or(jarvig_core::DetailReject::Base);
            let projected = self.micro_projected.get(index).copied().unwrap_or(0.0);
            let color = if mode == jarvig_core::MicroDebugMode::Active {
                if reason == jarvig_core::DetailReject::Selected { active } else { neutral }
            } else if mode == jarvig_core::MicroDebugMode::State {
                let state = self.micro_states.get(index).copied().unwrap_or(jarvig_core::DetailLifecycle::NotEligible);
                jarvig_core::detail_state_color(state)
            } else {
                jarvig_core::micro_debug_color(mode, reason, projected, stage).unwrap_or(neutral)
            };
            colors.push(jarvig_core::pack_diagnostic_color(color));
        }
        self.device.write_buffer(visible, 0, &u32_bytes(&colors)).map_err(RenderError::Rhi)
    }

    fn submit_parent_cut(
        &mut self,
        snapshot: &jarvig_core::RenderSceneSnapshot,
        camera: &jarvig_core::ResolvedPose,
        fov: f64,
        near: f32,
        aspect: f32,
        span_source: &[u32],
        draw_flags: &mut [u32],
    ) -> Result<(), RenderError> {
        let frustum_on = self.visibility_frustum();
        let threshold = self.gpu_scene.hierarchy_error_px;
        let Some(parent) = self.parent_gpu.as_ref() else { return Ok(()) };
        let mesh = parent.mesh;
        let instance = snapshot.instances().iter().find(|instance| instance.mesh == mesh);
        let Some(instance) = instance else { return Ok(()) };
        let leaf_count = parent.leaf_counts.len();
        let mut leaf_flags = vec![1u32; leaf_count];
        if frustum_on {
            leaf_flags.fill(0);
            for (draw_index, source) in span_source.iter().enumerate() {
                if let Some(slot) = leaf_flags.get_mut(*source as usize) {
                    *slot = draw_flags.get(draw_index).copied().unwrap_or(0);
                }
            }
        }
        let view_cut = jarvig_core::cut_visible_hierarchy_for_pose(
            &parent.hierarchy,
            instance,
            camera,
            fov,
            near,
            aspect,
            self.height as f32,
            threshold,
            &leaf_flags,
            &parent.leaf_counts,
        )
        .map_err(RenderError::Space)?;
        let select_us = view_cut.selection_us;
        let cut = jarvig_core::ClusterCut {
            selected: view_cut.selected,
            coarseness: view_cut.coarseness,
            covered_leaves: view_cut.covered_leaves,
        };
        let cut_started = std::time::Instant::now();
        // The cut already omitted hidden and off-screen branches. A parent in
        // `selected` covers only leaves this view can draw.
        let drawable = parent.draw_ok.clone();
        let submission = jarvig_core::submission_keeping_coverage(
            &parent.hierarchy,
            &parent.ranges,
            &parent.leaf_counts,
            parent.leaf_triangles,
            &cut,
            None,
            &drawable,
        );
        let mut replaced = vec![false; leaf_count];
        for node in &submission.parent_nodes {
            jarvig_core::visit_leaves(&parent.hierarchy, *node, &mut |leaf| {
                if let Some(slot) = replaced.get_mut(leaf as usize) {
                    *slot = true;
                }
            });
        }
        let mut draw_leaf = vec![false; leaf_count];
        for leaf in &submission.leaf_meshlets {
            if let Some(slot) = draw_leaf.get_mut(*leaf as usize) {
                *slot = true;
            }
        }
        let frame_indices = jarvig_core::parent_draw_indices(&parent.indices, &parent.ranges, &submission.parent_nodes);
        self.gpu_scene.parent_indices = frame_indices.clone();
        let mut key = 0xcbf29ce484222325u64;
        for node in &submission.parent_nodes {
            key ^= u64::from(*node);
            key = key.wrapping_mul(0x100000001b3);
        }
        key ^= u64::from(frame_indices.len() as u32);
        self.gpu_scene.parent_key = key;
        let cut_len = cut.selected.len() as u32;
        let node_count = parent.hierarchy.nodes.len() as u32;
        let leaf_triangles = parent.leaf_triangles;
        let parent_root = parent.root_triangles;
        let parent_empty = parent.empty_parents;
        let lod_triangles = submission.triangles;
        let hierarchy_leaves = submission.leaf_meshlets.len() as u32;
        let hierarchy_parents = submission.parent_nodes.len() as u32;
        let hierarchy_depth = cut.coarseness.iter().copied().max().unwrap_or(0);
        let covered_leaves = cut.covered_leaves;
        let uncovered_leaves = submission.uncovered_leaf_descendants;
        let cut_us = cut_started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        let upload_bytes = (frame_indices.len() * std::mem::size_of::<u32>()) as u64;
        for (draw_index, source) in span_source.iter().enumerate() {
            let source = *source as usize;
            if replaced.get(source).copied().unwrap_or(false) {
                if let Some(flag) = draw_flags.get_mut(draw_index) {
                    *flag = 6;
                }
            } else if draw_leaf.get(source).copied().unwrap_or(false) && !frustum_on {
                if let Some(flag) = draw_flags.get_mut(draw_index) {
                    *flag = 1;
                }
            }
        }
        self.gpu_scene.stats.hierarchy = true;
        self.gpu_scene.stats.hierarchy_cut = cut_len;
        self.gpu_scene.stats.hierarchy_nodes = node_count;
        self.gpu_scene.stats.hierarchy_lod_triangles = lod_triangles;
        self.gpu_scene.stats.hierarchy_leaf_triangles = leaf_triangles;
        self.gpu_scene.stats.hierarchy_leaves = hierarchy_leaves;
        self.gpu_scene.stats.hierarchy_parents = hierarchy_parents;
        self.gpu_scene.stats.hierarchy_candidates = view_cut.candidate_nodes;
        self.gpu_scene.stats.hierarchy_depth = hierarchy_depth;
        self.gpu_scene.stats.hierarchy_select_us = select_us;
        self.gpu_scene.stats.hierarchy_cut_us = cut_us;
        self.gpu_scene.stats.hierarchy_covered_leaves = covered_leaves;
        self.gpu_scene.stats.hierarchy_uncovered_leaves = uncovered_leaves;
        self.gpu_scene.stats.hierarchy_root_triangles = parent_root;
        self.gpu_scene.stats.hierarchy_empty_parents = parent_empty;
        let reused = self.parent_gpu.as_ref().is_some_and(|parent| parent.frame_key == key && parent.frame_count as usize == frame_indices.len());
        let upload_started = std::time::Instant::now();
        let uploaded = self.replace_parent_frame(key, &frame_indices);
        let upload_us = upload_started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        self.gpu_scene.stats.hierarchy_upload_us = upload_us;
        self.gpu_scene.stats.hierarchy_upload_bytes = if reused { 0 } else { upload_bytes };
        uploaded
    }

    fn replace_parent_frame(&mut self, key: u64, indices: &[u32]) -> Result<(), RenderError> {
        let Some(parent) = self.parent_gpu.as_ref() else { return Ok(()) };
        if parent.frame_key == key && parent.frame_count as usize == indices.len() {
            return Ok(());
        }
        let ranges: Vec<(u32, u32)> = parent.chunks.iter().map(|chunk| (chunk.first_vertex, chunk.vertex_count)).collect();
        let old: Vec<BufferId> = {
            let parent = self.parent_gpu.as_mut().expect("parent geometry");
            parent.chunks.iter_mut().filter_map(|chunk| chunk.frame.take()).collect()
        };
        for frame in old {
            self.device.destroy(ResourceKind::Buffer, frame.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(parent) = self.parent_gpu.as_mut() {
            for chunk in &mut parent.chunks {
                chunk.frame_count = 0;
            }
            parent.frame_count = 0;
            parent.frame_key = key;
        }
        if indices.is_empty() {
            return Ok(());
        }
        let mut buckets = vec![Vec::<u32>::new(); ranges.len()];
        for triangle in indices.chunks_exact(3) {
            let vertex = triangle[0];
            let index = ranges.partition_point(|(first, _)| *first <= vertex).saturating_sub(1);
            let Some((first, count)) = ranges.get(index) else { continue };
            if vertex < first.saturating_add(*count) {
                let base = *first;
                buckets[index].extend([triangle[0] - base, triangle[1] - base, triangle[2] - base]);
            }
        }
        let mut total = 0u32;
        for (index, bucket) in buckets.into_iter().enumerate() {
            if bucket.is_empty() {
                continue;
            }
            let count = bucket.len() as u32;
            let frame = self.try_parent_buffer(BufferDesc {
                size: (bucket.len() * 4) as u64,
                usage: BufferUsage::Index,
                label: Some("JARVIG.ParentGeometry.Frame".into()),
                contents: Some(u32_bytes(&bucket)),
            })?;
            let Some(chunk) = self.parent_gpu.as_mut().and_then(|parent| parent.chunks.get_mut(index)) else {
                let _ = self.device.destroy(ResourceKind::Buffer, frame.raw());
                continue;
            };
            chunk.frame = Some(frame);
            chunk.frame_count = count;
            total = total.saturating_add(count);
        }
        if let Some(parent) = self.parent_gpu.as_mut() {
            parent.frame_count = total;
            parent.frame_key = key;
        }
        Ok(())
    }

    fn ensure_parent_color_pipeline(&mut self) -> Result<PipelineId, RenderError> {
        let two_sided = self.material_two_sided() == Some(true);
        if let Some(pipeline) = self.parent_gpu.as_ref().and_then(|gpu| {
            if gpu.color_two_sided == two_sided {
                gpu.pipeline
            } else {
                None
            }
        }) {
            return Ok(pipeline);
        }
        if let Some(old) = self.parent_gpu.as_mut().and_then(|gpu| gpu.pipeline.take()) {
            self.device.destroy(ResourceKind::Pipeline, old.raw()).map_err(RenderError::Rhi)?;
        }
        let shader = self.parent_gpu.as_ref().ok_or(RenderError::Rhi(RhiError::InvalidResource("parent geometry")))?.shader;
        let camera = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("parent camera layout")))?;
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: HDR_SCENE_FORMAT,
                layouts: vec![camera],
                vertex_buffers: vec![VertexBufferLayout {
                    stride: 24,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![
                        VertexAttribute { shader_location: 0, offset: 0, format: VertexFormat::Float32x3 },
                        VertexAttribute { shader_location: 1, offset: 12, format: VertexFormat::Float32x3 },
                    ],
                }],
                depth: Some(DepthState { format: TextureFormat::Depth32Float, write_enabled: false, compare: CompareFunction::GreaterEqual }),
                cull: if two_sided { CullMode::None } else { CullMode::Back },
                label: Some("JARVIG.ParentColor".into()),
            })
            .map_err(RenderError::Rhi)?;
        let parent = self.parent_gpu.as_mut().expect("parent geometry");
        parent.pipeline = Some(pipeline);
        parent.color_two_sided = two_sided;
        Ok(pipeline)
    }

    fn replace_scene_buffer(
        &mut self,
        slot: &mut dyn FnMut(&mut GpuSceneState) -> &mut Option<BufferId>,
        bytes: &[u8],
        label: &str,
    ) -> Result<(), RenderError> {
        let previous = slot(&mut self.gpu_scene).take();
        if let Some(buffer) = previous {
            self.device.destroy(ResourceKind::Buffer, buffer.raw()).map_err(RenderError::Rhi)?;
        }
        if bytes.is_empty() {
            return Ok(());
        }
        let buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: bytes.len() as u64,
                usage: BufferUsage::Storage,
                label: Some(label.into()),
                contents: Some(bytes.to_vec()),
            })
            .map_err(RenderError::Rhi)?;
        *slot(&mut self.gpu_scene) = Some(buffer);
        Ok(())
    }

    fn write_scene_instances(&mut self, bytes: &[u8]) -> Result<(), RenderError> {
        if bytes.is_empty() {
            if let Some(buffer) = self.gpu_scene.instances.take() {
                self.device.destroy(ResourceKind::Buffer, buffer.raw()).map_err(RenderError::Rhi)?;
            }
            self.gpu_scene.instance_capacity = 0;
            return Ok(());
        }
        let needed = bytes.len();
        if self.gpu_scene.instance_capacity < needed {
            if let Some(buffer) = self.gpu_scene.instances.take() {
                self.device.destroy(ResourceKind::Buffer, buffer.raw()).map_err(RenderError::Rhi)?;
            }
            let buffer = self
                .device
                .create_buffer(&BufferDesc {
                    size: needed as u64,
                    usage: BufferUsage::Storage,
                    label: Some("JARVIG.GpuScene.Instances".into()),
                    contents: Some(bytes.to_vec()),
                })
                .map_err(RenderError::Rhi)?;
            self.gpu_scene.instances = Some(buffer);
            self.gpu_scene.instance_capacity = needed;
            return Ok(());
        }
        let buffer = self.gpu_scene.instances.expect("instance buffer");
        self.device.write_buffer(buffer, 0, bytes).map_err(RenderError::Rhi)
    }

    fn drop_gpu_scene(&mut self) -> Result<(), RenderError> {
        for buffer in [self.gpu_scene.instances.take(), self.gpu_scene.geometries.take(), self.gpu_scene.meshlets.take()].into_iter().flatten() {
            self.device.destroy(ResourceKind::Buffer, buffer.raw()).map_err(RenderError::Rhi)?;
        }
        self.gpu_scene.instance_capacity = 0;
        self.gpu_scene.meshlet_key = 0;
        Ok(())
    }

    pub fn surface_target(&self) -> RenderTargetId {
        RenderTargetId { index: 1, generation: self.target.generation }
    }

    pub fn submitted_last(&self) -> u32 {
        self.submitted_last
    }

    pub fn flush(&mut self) -> Result<(), RenderError> {
        self.device.flush().map_err(RenderError::Rhi)
    }

    pub fn resource_stats(&self) -> ResourceStats {
        self.device.resource_stats()
    }

    /// Drop GPU objects that belonged to the world just replaced.
    ///
    /// `MeshId` starts again at 1 in the next world. A resident buffer kept under
    /// the old id is drawn as the new mesh. The device, swapchain, material
    /// pipelines, and editor overlays stay. ADR-0063.
    pub fn release_world_scene(&mut self) -> Result<(), RenderError> {
        let gpu_meshes = std::mem::take(&mut self.gpu_meshes);
        for (_, residency) in gpu_meshes {
            if let GpuResidency::Resident(gpu) = residency {
                self.device.destroy(ResourceKind::Buffer, gpu.vertices.raw()).map_err(RenderError::Rhi)?;
                self.device.destroy(ResourceKind::Buffer, gpu.indices.raw()).map_err(RenderError::Rhi)?;
            }
        }
        self.gpu_scene.remembered.clear();
        self.gpu_scene.hierarchy_nodes = None;
        self.gpu_scene.hierarchy_key = 0;
        self.gpu_scene.debug_flags.clear();
        self.gpu_scene.parent_indices.clear();
        self.gpu_scene.parent_key = 0;
        self.gpu_scene.frozen_flags.clear();
        self.gpu_scene.frozen_stats = None;
        self.gpu_scene.frozen_cut = false;
        self.gpu_scene.highlight = None;
        self.gpu_scene.meshlet_key = 0;
        self.diagnostic_frozen = None;
        self.diagnostic_reused = false;
        self.entity_hidden.clear();
        self.terrain_grid = None;
        self.terrain_grid_meshes.clear();
        self.land_unlit = false;
        for slot in &mut self.views {
            if let Some(view) = slot.view.as_mut() {
                view.cluster_flags.clear();
                view.parent_indices.clear();
                view.parent_key = 0;
            }
        }
        self.publish_micro_absence();
        self.clear_meshlet_debug()?;
        self.clear_parent_geometry()?;
        self.drop_gpu_scene()?;
        self.gpu_scene.stats = GpuSceneFrameStats::default();
        self.device.flush().map_err(RenderError::Rhi)
    }

    pub fn evict_gpu_mesh(&mut self, mesh: MeshId) -> Result<(), RenderError> {
        let taken = {
            let Some(slot) = self.gpu_meshes.iter_mut().find(|(id, _)| *id == mesh) else {
                return Ok(());
            };
            std::mem::replace(&mut slot.1, GpuResidency::Evicted)
        };
        if let GpuResidency::Resident(gpu) = taken {
            self.device.destroy(ResourceKind::Buffer, gpu.vertices.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, gpu.indices.raw()).map_err(RenderError::Rhi)?;
            self.device.flush().map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), RenderError> {
        if self.shut_down {
            return Ok(());
        }
        self.shut_down = true;
        self.device.flush().map_err(RenderError::Rhi)?;
        let mut owned = Vec::new();
        for slot in &mut self.views {
            if let Some(view) = slot.view.take() {
                owned.push(view);
            }
        }
        let bindings = std::mem::take(&mut self.bindings);
        for binding in &bindings {
            self.device.destroy(ResourceKind::BindGroup, binding.group.raw()).map_err(RenderError::Rhi)?;
        }
        let packets = std::mem::take(&mut self.light_packets);
        for packet in &packets {
            self.device.destroy(ResourceKind::BindGroup, packet.group.raw()).map_err(RenderError::Rhi)?;
            if packet.unshadowed.is_valid() {
                self.device.destroy(ResourceKind::BindGroup, packet.unshadowed.raw()).map_err(RenderError::Rhi)?;
            }
        }
        let materials = std::mem::take(&mut self.gpu_materials);
        for material in &materials {
            self.device.destroy(ResourceKind::BindGroup, material.group.raw()).map_err(RenderError::Rhi)?;
        }
        for binding in bindings {
            self.device.destroy(ResourceKind::Buffer, binding.uniform.raw()).map_err(RenderError::Rhi)?;
        }
        for packet in packets {
            self.device.destroy(ResourceKind::Buffer, packet.header.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, packet.storage.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, packet.shadow.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, packet.probe.raw()).map_err(RenderError::Rhi)?;
        }
        self.destroy_shadow_resources()?;
        self.cancel_probe_job()?;
        for resident in std::mem::take(&mut self.probe_residents) {
            self.destroy_probe_gpu(resident.gpu)?;
        }
        if let Some(probe) = self.probe_unbound.take() {
            self.destroy_probe_gpu(probe)?;
        }
        if let Some(environment) = self.environment_gpu.take() {
            self.device.destroy(ResourceKind::Buffer, environment.buffer.raw()).map_err(RenderError::Rhi)?;
        }
        for material in materials {
            self.device.destroy(ResourceKind::Buffer, material.buffer.raw()).map_err(RenderError::Rhi)?;
        }
        let textures = std::mem::take(&mut self.gpu_textures);
        for texture in &textures {
            self.device.destroy(ResourceKind::TextureView, texture.view.raw()).map_err(RenderError::Rhi)?;
        }
        for texture in textures {
            self.device.destroy(ResourceKind::Texture, texture.texture.raw()).map_err(RenderError::Rhi)?;
        }
        let samplers = std::mem::take(&mut self.gpu_samplers);
        for (_, sampler) in samplers {
            self.device.destroy(ResourceKind::Sampler, sampler.raw()).map_err(RenderError::Rhi)?;
        }
        for view in owned {
            self.release_view_gpu(view)?;
        }
        let gpu_meshes = std::mem::take(&mut self.gpu_meshes);
        for (_, residency) in gpu_meshes {
            if let GpuResidency::Resident(gpu) = residency {
                self.device.destroy(ResourceKind::Buffer, gpu.vertices.raw()).map_err(RenderError::Rhi)?;
                self.device.destroy(ResourceKind::Buffer, gpu.indices.raw()).map_err(RenderError::Rhi)?;
            }
        }
        let masters = std::mem::take(&mut self.gpu_masters);
        for master in &masters {
            self.device.destroy(ResourceKind::Pipeline, master.pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        for master in &masters {
            self.device.destroy(ResourceKind::ShaderModule, master.shader.raw()).map_err(RenderError::Rhi)?;
        }
        for master in masters {
            self.device.destroy(ResourceKind::BindGroupLayout, master.layout.raw()).map_err(RenderError::Rhi)?;
        }
        self.drop_output_views()?;
        if let Some(output) = self.output.take() {
            self.device.destroy(ResourceKind::Pipeline, output.pipeline.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::ShaderModule, output.shader.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::BindGroupLayout, output.layout.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Sampler, output.sampler.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(hdr) = self.hdr.take() {
            self.device.destroy(ResourceKind::TextureView, hdr.view.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Texture, hdr.texture.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(contact) = self.contact.take() {
            self.device.destroy(ResourceKind::TextureView, contact.view.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Texture, contact.texture.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(contact) = self.contact_gpu.take() {
            for (_, pipeline) in &contact.pipelines {
                self.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
            }
            self.device.destroy(ResourceKind::ShaderModule, contact.shader.raw()).map_err(RenderError::Rhi)?;
        }
        let _ = self.discard_pending(false);
        self.destroy_live()?;
        if let Some(pipeline) = self.micro_color_pipeline.take() {
            self.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(shader) = self.micro_color_shader.take() {
            self.device.destroy(ResourceKind::ShaderModule, shader.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(pipeline) = self.micro_class_pipeline.take() {
            self.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(pipeline) = self.micro_parent_pipeline.take() {
            self.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(pipeline) = self.micro_parent_solid.take() {
            self.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        self.drop_diagnostic_draw()?;
        if let Some(shader) = self.micro_class_shader.take() {
            self.device.destroy(ResourceKind::ShaderModule, shader.raw()).map_err(RenderError::Rhi)?;
        }
        for (_, buffer, group) in self.micro_class_colors.drain(..) {
            self.device.destroy(ResourceKind::BindGroup, group.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, buffer.raw()).map_err(RenderError::Rhi)?;
        }
        self.clear_meshlet_debug()?;
        self.clear_parent_geometry()?;
        self.drop_gpu_scene()?;
        if let Some(overlay) = self.overlay_gpu.take() {
            self.device.destroy(ResourceKind::BindGroup, overlay.group.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, overlay.uniform.raw()).map_err(RenderError::Rhi)?;
            if let Some(vertices) = overlay.vertices {
                self.device.destroy(ResourceKind::Buffer, vertices.raw()).map_err(RenderError::Rhi)?;
            }
            self.device.destroy(ResourceKind::Pipeline, overlay.pipeline.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::ShaderModule, overlay.shader.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(reference) = self.reference_gpu.take() {
            self.device.destroy(ResourceKind::BindGroup, reference.group.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, reference.uniform.raw()).map_err(RenderError::Rhi)?;
            if let Some(vertices) = reference.vertices {
                self.device.destroy(ResourceKind::Buffer, vertices.raw()).map_err(RenderError::Rhi)?;
            }
            self.device.destroy(ResourceKind::Pipeline, reference.pipeline.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::ShaderModule, reference.shader.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(layout) = self.camera_layout.take() {
            self.device.destroy(ResourceKind::BindGroupLayout, layout.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(layout) = self.light_layout.take() {
            self.device.destroy(ResourceKind::BindGroupLayout, layout.raw()).map_err(RenderError::Rhi)?;
        }
        self.device.flush().map_err(RenderError::Rhi)
    }

    pub fn depth_target_count(&self) -> u64 {
        self.depth_creates
    }

    pub fn depth_size(&self, view: RenderViewId) -> Result<Option<(u32, u32)>, RenderError> {
        Ok(self.slot(view)?.depth_size)
    }

    pub fn viewport(&self, view: RenderViewId) -> Result<Option<PixelRect>, RenderError> {
        let layout = self.slot(view)?.layout;
        if self.width == 0 || self.height == 0 {
            return Ok(None);
        }
        Ok(Some(pixel_rect(layout, self.width, self.height)?))
    }

    pub fn view_count(&self) -> usize {
        self.views.iter().filter(|slot| slot.view.is_some()).count()
    }

    pub fn target_count(&self) -> usize {
        1
    }

    pub fn target_kind(&self, target: RenderTargetId) -> Result<RenderTargetKind, RenderError> {
        self.require_target(target)?;
        Ok(self.target.kind)
    }

    pub fn acquires_last_frame(&self) -> u32 {
        self.acquires_last
    }

    pub fn presents_last_frame(&self) -> u32 {
        self.presents_last
    }

    pub fn presented_frames(&self) -> u64 {
        self.device.presented_frames()
    }

    pub fn mesh_upload_count(&self) -> u32 {
        self.mesh_uploads
    }

    pub fn material_pipeline_count(&self) -> u32 {
        self.material_pipelines
    }

    pub fn material_parameter_upload_count(&self) -> u32 {
        self.material_parameter_uploads
    }

    pub fn unbound_material_skip_count(&self) -> u32 {
        self.unbound_material_skips
    }

    pub fn texture_upload_count(&self) -> u32 {
        self.texture_uploads
    }

    pub fn gpu_texture_count(&self) -> usize {
        self.gpu_textures.len()
    }

    pub fn gpu_sampler_count(&self) -> usize {
        self.gpu_samplers.len()
    }

    pub fn material_texture_binding_updates(&self) -> u32 {
        self.texture_binding_updates
    }

    pub fn missing_texture_uses(&self) -> u32 {
        self.missing_texture_uses
    }

    pub fn light_buffer_upload_count(&self) -> u32 {
        self.light_uploads
    }

    /// Rewrites of the shared environment uniform. Not a material compile and not a direct-light upload.
    pub fn environment_packet_upload_count(&self) -> u32 {
        self.environment_uploads
    }

    /// Times the per-instance transform uniform was rewritten because its bytes changed.
    /// A repeated frame does not increment it. A mesh upload is a different counter.
    pub fn instance_transform_upload_count(&self) -> u32 {
        self.instance_transform_uploads
    }

    /// Last uploaded transform packet for one view and instance. `None` if that draw has not uploaded.
    pub fn instance_transform_bytes(&self, view: RenderViewId, instance: RenderInstanceId) -> Option<[u8; GpuTransforms::BYTES]> {
        self.bindings.iter().find(|binding| binding.view == view && binding.instance == instance).and_then(|binding| binding.last_bytes)
    }

    pub fn gpu_light_packet_count(&self) -> usize {
        self.light_packets.len()
    }

    /// Ascending slot index. Not hash iteration. Creation order until a slot is reused.
    pub fn views_for(&self, target: RenderTargetId) -> Vec<RenderViewId> {
        self.view_ids(target)
    }

    pub fn draw_count(&self) -> u64 {
        self.device.draw_count()
    }

    pub fn indexed_draw_count(&self) -> u64 {
        self.device.indexed_draw_count()
    }

    pub fn pipeline_count(&self) -> u64 {
        self.device.pipeline_count()
    }

    pub fn buffer_count(&self) -> u64 {
        self.device.buffer_count()
    }

    pub fn surface_label() -> &'static str {
        "JARVIG.MainSurface"
    }

    pub fn create_view(&mut self, desc: RenderViewDesc) -> Result<RenderViewId, RenderError> {
        self.require_target(desc.target)?;
        check_layout(desc.layout)?;
        let view = RenderView {
            label: desc.label,
            target: desc.target,
            camera: desc.camera,
            pose_override: None,
            layout: desc.layout,
            settings: desc.settings,
            depth: None,
            depth_view: None,
            depth_size: None,
            cluster_flags: Vec::new(),
            parent_indices: Vec::new(),
            parent_key: 0,
        };
        if let Some(index) = self.free_views.pop() {
            let slot = &mut self.views[index as usize];
            let generation = slot.generation;
            slot.view = Some(view);
            return Ok(RenderViewId { index, generation });
        }
        let index = self.views.len() as u32;
        self.views.push(ViewSlot { generation: 1, view: Some(view) });
        Ok(RenderViewId { index, generation: 1 })
    }

    pub fn update_view(&mut self, id: RenderViewId, update: RenderViewUpdate) -> Result<(), RenderError> {
        if let Some(layout) = update.layout {
            check_layout(layout)?;
        }
        let view = self.slot_mut(id)?;
        let mut changed = false;
        if let Some(camera) = update.camera {
            if view.camera != camera {
                view.camera = camera;
                changed = true;
            }
        }
        if let Some(pose) = update.pose {
            if view.pose_override != Some(pose) {
                view.pose_override = Some(pose);
                changed = true;
            }
        }
        if let Some(layout) = update.layout {
            if view.layout != layout {
                view.layout = layout;
                changed = true;
            }
        }
        if let Some(settings) = update.settings {
            view.settings = settings;
        }
        if changed {
            for binding in &mut self.bindings {
                if binding.view == id {
                    binding.last_bytes = None;
                }
            }
        }
        Ok(())
    }

    /// The pose last supplied for this view. `None` means prepare still reads the snapshot camera.
    pub fn view_pose(&self, id: RenderViewId) -> Result<Option<ResolvedPose>, RenderError> {
        Ok(self.slot(id)?.pose_override)
    }

    pub fn destroy_view(&mut self, id: RenderViewId) -> Result<(), RenderError> {
        let slot = self.views.get_mut(id.index as usize).ok_or(RenderError::UnknownView(id))?;
        if slot.generation != id.generation || slot.view.is_none() {
            return Err(RenderError::UnknownView(id));
        }
        let view = slot.view.take().expect("view");
        if slot.generation < u32::MAX {
            slot.generation += 1;
            self.free_views.push(id.index);
        }
        self.release_view_bindings(id)?;
        self.release_view_lights(id)?;
        self.release_output_view(id)?;
        self.release_view_gpu(view)?;
        self.device.flush().map_err(RenderError::Rhi)
    }

    pub fn view_label(&self, view: RenderViewId) -> Result<&str, RenderError> {
        Ok(self.slot(view)?.label.as_str())
    }

    pub fn view_settings(&self, view: RenderViewId) -> Result<RenderViewSettings, RenderError> {
        Ok(self.slot(view)?.settings)
    }

    pub fn configured_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Aspect of the configured drawable. Not the dock rectangle.
    pub fn configured_aspect(&self) -> Option<f32> {
        if self.width == 0 || self.height == 0 {
            None
        } else {
            Some(self.width as f32 / self.height as f32)
        }
    }

    pub fn resize_applied(&self) -> u32 {
        self.resize_applied
    }

    pub fn resize_failures(&self) -> u32 {
        self.resize_failures
    }

    /// Replace the editor overlay drawn after the scene. `None` removes it.
    /// The packet is not a scene object. Positions are camera-relative.
    pub fn set_editor_overlay(&mut self, overlay: Option<EditorOverlay>) {
        self.overlay = overlay.filter(|overlay| !overlay.vertices.is_empty());
    }

    /// Replace the depth-tested editor grid. `None` removes it. Not a scene object.
    /// Positions are camera-relative meters. This does not change the gizmo overlay.
    pub fn set_editor_reference(&mut self, reference: Option<EditorOverlay>) {
        self.reference = reference.filter(|reference| !reference.vertices.is_empty());
    }

    /// Paint the editing grid on these terrain meshes. `None` draws no grid. Spacing is a uniform, not a mesh rebuild.
    pub fn set_terrain_grid(&mut self, grid: Option<TerrainGridDesc>) {
        self.terrain_grid_meshes.clear();
        if let Some(grid) = grid {
            self.terrain_grid_meshes.extend(grid.meshes.iter().copied());
            self.terrain_grid = Some(grid);
        } else {
            self.terrain_grid = None;
        }
    }

    /// Skip these authoring ids in the color pass, the shadow caster list, and the contact prepass.
    pub fn set_hidden_entities(&mut self, entities: &[EntityId]) {
        self.entity_hidden.clear();
        self.entity_hidden.extend(entities.iter().copied());
    }

    /// Turn direct light and the lighting terms off for the next frames. The stored view debug is left alone.
    pub fn set_land_unlit(&mut self, unlit: bool) {
        self.land_unlit = unlit;
    }

    /// Upload cluster indices for one mesh. `None` drops them. The ordinary mesh buffers stay.
    pub fn set_meshlet_debug(&mut self, batch: Option<MeshletDebugBatch>) -> Result<(), RenderError> {
        self.clear_meshlet_debug()?;
        let Some(batch) = batch else { return Ok(()) };
        if batch.indices.is_empty() || batch.colors.is_empty() || batch.indices.len() != batch.colors.len() * 3 || batch.owners.len() != batch.colors.len() {
            return Err(RenderError::Mesh(MeshError::BadSubmesh));
        }
        self.ensure_camera_layout()?;
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(MESHLET_SHADER.into()),
                label: Some("JARVIG.MeshletIds".into()),
            })
            .map_err(RenderError::Rhi)?;
        let index_bytes = u32_bytes(&batch.indices);
        let color_bytes = u32_bytes(&batch.colors);
        let indices = match self.device.create_buffer(&BufferDesc {
            size: index_bytes.len() as u64,
            usage: BufferUsage::Index,
            label: Some("JARVIG.MeshletIds.Indices".into()),
            contents: Some(index_bytes),
        }) {
            Ok(indices) => indices,
            Err(error) => {
                let _ = self.device.destroy(ResourceKind::ShaderModule, shader.raw());
                return Err(RenderError::Rhi(error));
            }
        };
        let colors = match self.device.create_buffer(&BufferDesc {
            size: color_bytes.len() as u64,
            usage: BufferUsage::Storage,
            label: Some("JARVIG.MeshletIds.Colors".into()),
            contents: Some(color_bytes),
        }) {
            Ok(colors) => colors,
            Err(error) => {
                let _ = self.device.destroy(ResourceKind::Buffer, indices.raw());
                let _ = self.device.destroy(ResourceKind::ShaderModule, shader.raw());
                return Err(RenderError::Rhi(error));
            }
        };
        let owner_bytes = u32_bytes(&batch.owners);
        let owners = match self.device.create_buffer(&BufferDesc {
            size: owner_bytes.len() as u64,
            usage: BufferUsage::Storage,
            label: Some("JARVIG.MeshletIds.Owners".into()),
            contents: Some(owner_bytes),
        }) {
            Ok(owners) => owners,
            Err(error) => {
                let _ = self.device.destroy(ResourceKind::Buffer, colors.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, indices.raw());
                let _ = self.device.destroy(ResourceKind::ShaderModule, shader.raw());
                return Err(RenderError::Rhi(error));
            }
        };
        let visible_flags = vec![2u32; batch.spans.len().max(1)];
        let visible_bytes = u32_bytes(&visible_flags);
        let visible = match self.device.create_buffer(&BufferDesc {
            size: visible_bytes.len() as u64,
            usage: BufferUsage::Storage,
            label: Some("JARVIG.MeshletIds.Visible".into()),
            contents: Some(visible_bytes),
        }) {
            Ok(visible) => visible,
            Err(error) => {
                let _ = self.device.destroy(ResourceKind::Buffer, owners.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, colors.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, indices.raw());
                let _ = self.device.destroy(ResourceKind::ShaderModule, shader.raw());
                return Err(RenderError::Rhi(error));
            }
        };
        let color_layout = match self.device.create_bind_group_layout(&BindGroupLayoutDesc {
            entries: vec![
                BindGroupLayoutEntry { binding: 0, kind: BindingType::StorageBuffer, stage: ShaderStage::Fragment },
                BindGroupLayoutEntry { binding: 1, kind: BindingType::StorageBuffer, stage: ShaderStage::Fragment },
                BindGroupLayoutEntry { binding: 2, kind: BindingType::StorageBuffer, stage: ShaderStage::Fragment },
            ],
            label: Some("JARVIG.MeshletIds.Colors".into()),
        }) {
            Ok(layout) => layout,
            Err(error) => {
                let _ = self.device.destroy(ResourceKind::Buffer, visible.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, owners.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, colors.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, indices.raw());
                let _ = self.device.destroy(ResourceKind::ShaderModule, shader.raw());
                return Err(RenderError::Rhi(error));
            }
        };
        let color_group = match self.device.create_bind_group(&BindGroupDesc {
            layout: color_layout,
            entries: vec![
                BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(colors) },
                BindGroupEntry { binding: 1, resource: jarvig_rhi::BindResource::Buffer(owners) },
                BindGroupEntry { binding: 2, resource: jarvig_rhi::BindResource::Buffer(visible) },
            ],
            label: Some("JARVIG.MeshletIds.Colors".into()),
        }) {
            Ok(group) => group,
            Err(error) => {
                let _ = self.device.destroy(ResourceKind::BindGroupLayout, color_layout.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, visible.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, owners.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, colors.raw());
                let _ = self.device.destroy(ResourceKind::Buffer, indices.raw());
                let _ = self.device.destroy(ResourceKind::ShaderModule, shader.raw());
                return Err(RenderError::Rhi(error));
            }
        };
        let anchor_vertex = batch
            .spans
            .iter()
            .map(|(first, count)| if *count == 0 { u32::MAX } else { batch.indices.get(*first as usize).copied().unwrap_or(u32::MAX) })
            .collect();
        self.meshlet_gpu = Some(MeshletGpu {
            mesh: batch.mesh,
            indices,
            index_count: batch.indices.len() as u32,
            colors,
            color_group,
            color_layout,
            shader,
            pipelines: Vec::new(),
            occluded_pipelines: Vec::new(),
            ranges: batch.ranges,
            spans: batch.spans,
            span_source: batch.span_source,
            anchor_vertex,
            owners,
            visible,
            visible_capacity: visible_flags.len(),
            show_ids: batch.show_ids,
            shade_clustered: batch.shade_clustered,
        });
        Ok(())
    }

    pub fn set_meshlet_mode(&mut self, show_ids: bool, shade_clustered: bool) {
        if let Some(gpu) = self.meshlet_gpu.as_mut() {
            gpu.show_ids = show_ids;
            gpu.shade_clustered = shade_clustered;
        }
    }

    pub fn clear_meshlet_debug(&mut self) -> Result<(), RenderError> {
        self.drop_diagnostic_pipelines()?;
        let Some(gpu) = self.meshlet_gpu.take() else { return Ok(()) };
        for (_, _, pipeline) in gpu.pipelines.iter().chain(gpu.occluded_pipelines.iter()) {
            self.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        self.device.destroy(ResourceKind::BindGroup, gpu.color_group.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::BindGroupLayout, gpu.color_layout.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Buffer, gpu.indices.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Buffer, gpu.colors.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Buffer, gpu.owners.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Buffer, gpu.visible.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::ShaderModule, gpu.shader.raw()).map_err(RenderError::Rhi)?;
        Ok(())
    }

    /// Reconfigure the surface and depth together. Dimensions commit only after
    /// both succeed. Zero suspends the drawable and does not configure a 0×0 surface.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if self.width == width && self.height == height {
            return Ok(());
        }
        match self.device.configure_swapchain(self.target.swapchain, width, height) {
            Ok(()) => {
                self.width = width;
                self.height = height;
                self.resize_applied = self.resize_applied.saturating_add(1);
                self.sync_depth()?;
                self.sync_hdr()
            }
            Err(error) => {
                self.resize_failures = self.resize_failures.saturating_add(1);
                Err(RenderError::Rhi(error))
            }
        }
    }

    /// Acquire the target once, draw every view bound to it, present once.
    pub fn render_target(
        &mut self,
        target: RenderTargetId,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
    ) -> Result<FrameOutcome, RenderError> {
        if self.shut_down {
            return Err(RenderError::Rhi(RhiError::Validation("renderer is shut down".into())));
        }
        self.require_target(target)?;
        self.acquires_last = 0;
        self.presents_last = 0;
        if self.width == 0 || self.height == 0 {
            return Ok(FrameOutcome::Minimized);
        }
        let color = match self.acquire_with_retry()? {
            AcquireFrame::Minimized => return Ok(FrameOutcome::Minimized),
            AcquireFrame::Ready { view } => view,
        };
        self.sync_depth()?;
        self.sync_hdr()?;
        let cpu_started = std::time::Instant::now();
        self.encode_target(target, color, snapshot, meshes, materials, textures)?;
        self.gpu_scene.stats.renderer_cpu_us = cpu_started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        match self.device.present(self.device.graphics_queue(), self.target.swapchain) {
            Ok(frame) => {
                self.presents_last = 1;
                Ok(FrameOutcome::Presented { frame })
            }
            Err(RhiError::Minimized) => Ok(FrameOutcome::Minimized),
            Err(RhiError::SurfaceTimeout) => Ok(FrameOutcome::TimedOut),
            Err(RhiError::SurfaceOutdated | RhiError::SurfaceLost) => {
                let _ = self.resize(self.width, self.height);
                Ok(FrameOutcome::TimedOut)
            }
            Err(error) => Err(RenderError::Rhi(error)),
        }
    }

    fn require_target(&self, target: RenderTargetId) -> Result<(), RenderError> {
        if target.index == 1 && target.generation == self.target.generation {
            Ok(())
        } else {
            Err(RenderError::UnknownTarget(target))
        }
    }

    fn acquire_with_retry(&mut self) -> Result<AcquireFrame, RenderError> {
        let swapchain = self.target.swapchain;
        let acquired = match self.device.acquire_frame(swapchain) {
            Ok(frame) => Ok(frame),
            Err(RhiError::SurfaceOutdated | RhiError::SurfaceLost) => {
                // The window changed after the last configure. Reconfigure once
                // at this frame boundary, then acquire again. Do not loop.
                self.resize(self.width, self.height)?;
                self.device.acquire_frame(swapchain).map_err(RenderError::Rhi)
            }
            Err(RhiError::SurfaceTimeout) => Ok(AcquireFrame::Minimized),
            Err(RhiError::Minimized) => Ok(AcquireFrame::Minimized),
            Err(error) => Err(RenderError::Rhi(error)),
        }?;
        if matches!(acquired, AcquireFrame::Ready { .. }) {
            self.acquires_last = self.acquires_last.saturating_add(1);
        }
        Ok(acquired)
    }

    fn view_ids(&self, target: RenderTargetId) -> Vec<RenderViewId> {
        self.views
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let view = slot.view.as_ref()?;
                if view.target != target {
                    return None;
                }
                Some(RenderViewId { index: index as u32, generation: slot.generation })
            })
            .collect()
    }

    fn alive_ids(&self) -> Vec<RenderViewId> {
        self.view_ids(self.surface_target())
    }

    fn slot(&self, id: RenderViewId) -> Result<&RenderView, RenderError> {
        let slot = self.views.get(id.index as usize).ok_or(RenderError::UnknownView(id))?;
        if slot.generation != id.generation {
            return Err(RenderError::UnknownView(id));
        }
        slot.view.as_ref().ok_or(RenderError::UnknownView(id))
    }

    fn slot_mut(&mut self, id: RenderViewId) -> Result<&mut RenderView, RenderError> {
        let slot = self.views.get_mut(id.index as usize).ok_or(RenderError::UnknownView(id))?;
        if slot.generation != id.generation {
            return Err(RenderError::UnknownView(id));
        }
        slot.view.as_mut().ok_or(RenderError::UnknownView(id))
    }

    fn encode_target(
        &mut self,
        target: RenderTargetId,
        color: TextureViewId,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
    ) -> Result<(), RenderError> {
        self.ensure_draw_resources(snapshot, meshes)?;
        self.ensure_camera_layout()?;
        self.ensure_light_layout()?;
        self.ensure_shadow_targets()?;
        self.sync_contact()?;
        self.update_shadow_maps(snapshot, meshes)?;
        let ids_for_shadows = self.view_ids(target);
        for id in &ids_for_shadows {
            let (frame, fov, near, aspect) = {
                let view = self.slot(*id)?;
                let rect = pixel_rect(view.layout, self.width, self.height)?;
                (view.camera.frame, view.camera.vertical_fov_radians, view.camera.near_m, rect.width as f32 / rect.height.max(1) as f32)
            };
            let pose = if let Some(pose) = self.slot(*id)?.pose_override {
                pose
            } else {
                snapshot.camera(frame).ok_or(RenderError::Space(SpaceError::MissingFrame))?.pose
            };
            self.sync_gpu_scene(snapshot, &pose, fov, near, aspect)?;
            self.store_view_cut(*id);
            self.update_directional_cascades(*id, &pose, fov, aspect, near, snapshot, meshes)?;
        }
        self.submitted_last = snapshot.visible_count() as u32;
        let clear = self.target.clear;
        let ids = self.view_ids(target);
        // Capture before view bind groups, so a deleted probe's cube is gone before
        // a group can keep a view of it, and a new probe's cube exists before sampling.
        self.note_reflection_probes(snapshot);
        self.capture_static_probe(snapshot, meshes, materials, textures)?;
        let mut prepared = Vec::with_capacity(ids.len());
        for id in ids {
            prepared.push(self.prepare(id, snapshot, meshes, materials, textures)?);
        }
        let hdr = self.hdr_view().ok_or(RenderError::Rhi(RhiError::InvalidResource("hdr scene")))?;
        for pass in &prepared {
            self.ensure_output_view(pass.id)?;
        }
        let contact_on = prepared.iter().any(|pass| self.slot(pass.id).map(|view| view.settings.contact_shadows).unwrap_or(false));
        if contact_on {
            let hidden = self.entity_hidden.clone();
            for instance in snapshot.instances().iter().filter(|instance| instance.visible && !hidden.contains(&instance.entity)) {
                let mesh = meshes.get(instance.mesh).ok_or(RenderError::Mesh(MeshError::Empty))?;
                if let Some(stream) = mesh.streams().first() {
                    self.ensure_contact_pipeline(u64::from(stream.stride))?;
                }
            }
        }
        if let Some(pass) = prepared.last() {
            self.sync_microtriangles(pass, snapshot, meshes)?;
        }
        let mut encoder = self
            .device
            .create_command_encoder(Some("JARVIG.MainOpaquePass"))
            .map_err(RenderError::Rhi)?;
        // Scene light accumulates in the float target. The swapchain is display only.
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("JARVIG.HdrClear".into()),
                colors: vec![ColorAttachment { target: hdr, load: LoadOp::Clear(clear), store: StoreOp::Store }],
                depth: None,
            })
            .map_err(RenderError::Rhi)?;
        encoder.end_render_pass().map_err(RenderError::Rhi)?;
        let mut contact_cleared = false;
        self.contact_requested = false;
        self.gpu_scene.stats.legacy_draw_triangles = 0;
        self.gpu_scene.stats.meshlet_draw_triangles = 0;
        self.gpu_scene.stats.parent_draw_triangles = 0;
        let mut staged_runs = 0u32;
        for pass in &prepared {
            self.apply_view_cut(pass.id)?;
            if self.slot(pass.id)?.settings.contact_shadows {
                self.contact_requested = true;
                self.encode_contact_prepass(&mut *encoder, snapshot, meshes, pass, &mut contact_cleared)?;
            }
            if self.gpu_scene.visualization.paints_einstein() {
                self.paint_einstein_colors()?;
            }
            let recolor = self.gpu_scene.visualization.recolors_submission();
            let pass_runs = staged_runs;
            if recolor {
                staged_runs = self.append_diagnostic_bases(pass, staged_runs)?;
            }
            encoder
                .begin_render_pass(&RenderPassDesc {
                    label: Some(pass.label.clone()),
                    colors: vec![ColorAttachment { target: hdr, load: LoadOp::Load, store: StoreOp::Store }],
                    depth: Some(DepthAttachment {
                        target: pass.depth,
                        load: DepthLoadOp::Clear(DEPTH_CLEAR),
                        store: StoreOp::Store,
                    }),
                })
                .map_err(RenderError::Rhi)?;
            encoder
                .set_viewport(Viewport {
                    x: pass.rect.x as f32,
                    y: pass.rect.y as f32,
                    width: pass.rect.width as f32,
                    height: pass.rect.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                })
                .map_err(RenderError::Rhi)?;
            encoder
                .set_scissor(ScissorRect {
                    x: pass.rect.x,
                    y: pass.rect.y,
                    width: pass.rect.width,
                    height: pass.rect.height,
                })
                .map_err(RenderError::Rhi)?;
            self.gpu_scene.stats.einstein_debug_draws = 0;
            if recolor {
                self.diagnostic_run_cursor = pass_runs;
            }
            for draw in &pass.draws {
                self.draw_prepared(&mut *encoder, draw, recolor)?;
            }
            if !self.gpu_scene.diagnostic.suspends_detail() {
                self.draw_microtriangles(&mut *encoder, pass)?;
            }
            if self.gpu_scene.visualization.overlays_detail() {
                self.draw_einstein_debug(&mut *encoder, pass)?;
            }
            self.draw_meshlet_ids(&mut *encoder, pass, meshes)?;
            self.draw_terrain_grid(&mut *encoder, pass, meshes, snapshot)?;
            self.draw_editor_reference(&mut *encoder, pass, snapshot)?;
            encoder.end_render_pass().map_err(RenderError::Rhi)?;
        }
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("JARVIG.DisplayClear".into()),
                colors: vec![ColorAttachment {
                    target: color,
                    load: LoadOp::Clear(ClearColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                    store: StoreOp::Store,
                }],
                depth: None,
            })
            .map_err(RenderError::Rhi)?;
        encoder.end_render_pass().map_err(RenderError::Rhi)?;
        self.output_passes_last = 0;
        for pass in &prepared {
            self.draw_output(&mut *encoder, color, pass)?;
        }
        // Gizmo colors are display colors. They are drawn after tone mapping.
        self.draw_editor_overlay(&mut *encoder, color, &prepared, snapshot)?;
        let buffer = encoder.finish().map_err(RenderError::Rhi)?;
        self.device
            .submit(self.device.graphics_queue(), &[buffer], None)
            .map_err(RenderError::Rhi)
    }

    fn draw_terrain_grid(
        &mut self,
        encoder: &mut dyn jarvig_rhi::CommandEncoder,
        pass: &Prepared,
        meshes: &MeshLibrary,
        snapshot: &RenderSceneSnapshot,
    ) -> Result<(), RenderError> {
        if pass.terrain_grid.is_empty() {
            return Ok(());
        }
        let Some(desc) = self.terrain_grid.clone() else { return Ok(()) };
        let pose = {
            let frame = self.slot(pass.id)?.camera.frame;
            if let Some(pose) = self.slot(pass.id)?.pose_override {
                pose
            } else {
                snapshot.camera(frame).ok_or(RenderError::Space(SpaceError::MissingFrame))?.pose
            }
        };
        let bytes = terrain_grid_bytes(&pose, &desc);
        let group = self.terrain_grid_group(pass.id)?;
        let buffer = self
            .terrain_grid_gpu
            .as_ref()
            .and_then(|gpu| gpu.views.iter().find(|slot| slot.view == pass.id))
            .map(|slot| slot.buffer)
            .ok_or(RenderError::Rhi(RhiError::InvalidResource("terrain grid")))?;
        self.device.write_buffer(buffer, 0, &bytes).map_err(RenderError::Rhi)?;
        for draw in &pass.terrain_grid {
            let Some(mesh) = meshes.get(draw.mesh) else { continue };
            let Some(stream) = mesh.streams().first() else { continue };
            let pipeline = self.terrain_grid_pipeline(u64::from(stream.stride))?;
            let Some((vertices, indices, index_format, ranges)) = self.gpu_meshes.iter().find(|(id, _)| *id == draw.mesh).and_then(|(_, slot)| match slot {
                GpuResidency::Resident(gpu) => Some((
                    gpu.vertices,
                    gpu.indices,
                    gpu.index_format,
                    gpu.submeshes.iter().map(|range| (range.index_count, range.first_index, range.base_vertex)).collect::<Vec<_>>(),
                )),
                GpuResidency::Evicted => None,
            }) else {
                continue;
            };
            encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
            encoder.set_bind_group(0, draw.transform).map_err(RenderError::Rhi)?;
            encoder.set_bind_group(1, group).map_err(RenderError::Rhi)?;
            encoder.set_vertex_buffer(0, vertices, 0).map_err(RenderError::Rhi)?;
            encoder.set_index_buffer(indices, index_format, 0).map_err(RenderError::Rhi)?;
            for (index_count, first_index, base_vertex) in ranges {
                if index_count > 0 {
                    encoder.draw_indexed(index_count, 1, first_index, base_vertex, 0).map_err(RenderError::Rhi)?;
                }
            }
        }
        Ok(())
    }

    fn terrain_grid_group(&mut self, view: RenderViewId) -> Result<BindGroupId, RenderError> {
        self.ensure_terrain_grid_gpu()?;
        if let Some(found) = self.terrain_grid_gpu.as_ref().and_then(|gpu| gpu.views.iter().find(|slot| slot.view == view)) {
            return Ok(found.group);
        }
        let layout = self.terrain_grid_gpu.as_ref().expect("terrain grid").layout;
        let buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: 96,
                usage: BufferUsage::Uniform,
                label: Some("JARVIG.TerrainGrid".into()),
                contents: None,
            })
            .map_err(RenderError::Rhi)?;
        let group = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout,
                entries: vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(buffer) }],
                label: Some("JARVIG.TerrainGrid".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.terrain_grid_gpu.as_mut().expect("terrain grid").views.push(TerrainGridView { view, buffer, group });
        Ok(group)
    }

    fn terrain_grid_pipeline(&mut self, stride: u64) -> Result<PipelineId, RenderError> {
        self.ensure_terrain_grid_gpu()?;
        if let Some(found) = self.terrain_grid_gpu.as_ref().and_then(|gpu| gpu.pipelines.iter().find(|(stored, _)| *stored == stride).map(|(_, pipeline)| *pipeline)) {
            return Ok(found);
        }
        let (shader, layout) = {
            let gpu = self.terrain_grid_gpu.as_ref().expect("terrain grid");
            (gpu.shader, gpu.layout)
        };
        let camera = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("terrain grid camera")))?;
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: HDR_SCENE_FORMAT,
                layouts: vec![camera, layout],
                vertex_buffers: vec![VertexBufferLayout {
                    stride,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![VertexAttribute { shader_location: 0, offset: 0, format: VertexFormat::Float32x3 }],
                }],
                depth: Some(DepthState { format: TextureFormat::Depth32Float, write_enabled: false, compare: CompareFunction::GreaterEqual }),
                cull: CullMode::Back,
                label: Some(format!("JARVIG.TerrainGrid.Stride{stride}")),
            })
            .map_err(RenderError::Rhi)?;
        self.terrain_grid_gpu.as_mut().expect("terrain grid").pipelines.push((stride, pipeline));
        Ok(pipeline)
    }

    fn ensure_terrain_grid_gpu(&mut self) -> Result<(), RenderError> {
        if self.terrain_grid_gpu.is_some() {
            return Ok(());
        }
        self.ensure_camera_layout()?;
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(TERRAIN_GRID_SHADER.into()),
                label: Some("JARVIG.TerrainGrid".into()),
            })
            .map_err(RenderError::Rhi)?;
        let layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries: vec![BindGroupLayoutEntry { binding: 0, kind: BindingType::UniformBuffer, stage: ShaderStage::VertexFragment }],
                label: Some("JARVIG.TerrainGrid".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.terrain_grid_gpu = Some(TerrainGridGpu { shader, layout, pipelines: Vec::new(), views: Vec::new() });
        Ok(())
    }

    fn draw_editor_reference(
        &mut self,
        encoder: &mut dyn jarvig_rhi::CommandEncoder,
        pass: &Prepared,
        snapshot: &RenderSceneSnapshot,
    ) -> Result<(), RenderError> {
        let Some(reference) = self.reference.clone() else { return Ok(()) };
        if reference.view != pass.id {
            return Ok(());
        }
        let (frame, fov, near) = {
            let camera = &self.slot(reference.view)?.camera;
            (camera.frame, camera.vertical_fov_radians, camera.near_m)
        };
        let pose = if let Some(pose) = self.slot(reference.view)?.pose_override {
            pose
        } else {
            snapshot.camera(frame).ok_or(RenderError::Space(SpaceError::MissingFrame))?.pose
        };
        let aspect = pass.rect.width as f32 / pass.rect.height.max(1) as f32;
        let projection = Mat4::perspective_infinite_reverse_z(fov as f32, aspect, near).map_err(RenderError::Space)?;
        let view = Mat4::from_rotation(pose.rotation.conjugate());
        let mut uniform = [0u8; 128];
        projection.write_column_major(&mut uniform[0..64]);
        view.write_column_major(&mut uniform[64..128]);
        self.ensure_reference_gpu()?;
        let mut bytes = Vec::with_capacity(reference.vertices.len() * 32);
        for vertex in &reference.vertices {
            bytes.extend_from_slice(&vertex.position[0].to_le_bytes());
            bytes.extend_from_slice(&vertex.position[1].to_le_bytes());
            bytes.extend_from_slice(&vertex.position[2].to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            for channel in vertex.color {
                bytes.extend_from_slice(&channel.to_le_bytes());
            }
        }
        self.replace_reference_vertices(bytes)?;
        let gpu = self.reference_gpu.as_ref().expect("reference");
        let uniform_id = gpu.uniform;
        let vertex_id = gpu.vertices.ok_or(RenderError::Rhi(RhiError::InvalidResource("reference vertices")))?;
        let group = gpu.group;
        let pipeline = gpu.pipeline;
        self.device.write_buffer(uniform_id, 0, &uniform).map_err(RenderError::Rhi)?;
        encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(0, group).map_err(RenderError::Rhi)?;
        encoder.set_vertex_buffer(0, vertex_id, 0).map_err(RenderError::Rhi)?;
        encoder.draw(reference.vertices.len() as u32, 1, 0, 0).map_err(RenderError::Rhi)
    }

    fn replace_reference_vertices(&mut self, bytes: Vec<u8>) -> Result<(), RenderError> {
        let gpu = self.reference_gpu.as_mut().ok_or(RenderError::Rhi(RhiError::InvalidResource("reference")))?;
        if gpu.last_vertices == bytes {
            return Ok(());
        }
        if let Some(previous) = gpu.vertices.take() {
            self.device.destroy(ResourceKind::Buffer, previous.raw()).map_err(RenderError::Rhi)?;
        }
        let buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: bytes.len() as u64,
                usage: BufferUsage::Vertex,
                label: Some("JARVIG.EditorReference.Vertices".into()),
                contents: Some(bytes.clone()),
            })
            .map_err(RenderError::Rhi)?;
        let gpu = self.reference_gpu.as_mut().expect("reference");
        gpu.vertices = Some(buffer);
        gpu.last_vertices = bytes;
        Ok(())
    }

    fn ensure_reference_gpu(&mut self) -> Result<(), RenderError> {
        if self.reference_gpu.is_some() {
            return Ok(());
        }
        self.ensure_camera_layout()?;
        let layout = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("reference layout")))?;
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(OVERLAY_SHADER.into()),
                label: Some("JARVIG.EditorReference".into()),
            })
            .map_err(RenderError::Rhi)?;
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: HDR_SCENE_FORMAT,
                layouts: vec![layout],
                vertex_buffers: vec![VertexBufferLayout {
                    stride: 32,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![
                        VertexAttribute { shader_location: 0, offset: 0, format: VertexFormat::Float32x3 },
                        VertexAttribute { shader_location: 1, offset: 16, format: VertexFormat::Float32x4 },
                    ],
                }],
                depth: Some(DepthState { format: TextureFormat::Depth32Float, write_enabled: false, compare: CompareFunction::GreaterEqual }),
                cull: CullMode::None,
                label: Some("JARVIG.EditorReference".into()),
            })
            .map_err(RenderError::Rhi)?;
        let uniform = self
            .device
            .create_buffer(&BufferDesc {
                size: 128,
                usage: BufferUsage::Uniform,
                label: Some("JARVIG.EditorReference.Camera".into()),
                contents: None,
            })
            .map_err(RenderError::Rhi)?;
        let group = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout,
                entries: vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(uniform) }],
                label: Some("JARVIG.EditorReference.Camera".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.reference_gpu = Some(OverlayGpu { shader, pipeline, uniform, vertices: None, group, last_vertices: Vec::new() });
        Ok(())
    }

    fn draw_editor_overlay(
        &mut self,
        encoder: &mut dyn jarvig_rhi::CommandEncoder,
        color: TextureViewId,
        prepared: &[Prepared],
        snapshot: &RenderSceneSnapshot,
    ) -> Result<(), RenderError> {
        let Some(overlay) = self.overlay.clone() else { return Ok(()) };
        let Some(pass) = prepared.iter().find(|pass| pass.id == overlay.view) else { return Ok(()) };
        let (frame, fov, near) = {
            let camera = &self.slot(overlay.view)?.camera;
            (camera.frame, camera.vertical_fov_radians, camera.near_m)
        };
        let pose = if let Some(pose) = self.slot(overlay.view)?.pose_override {
            pose
        } else {
            snapshot.camera(frame).ok_or(RenderError::Space(SpaceError::MissingFrame))?.pose
        };
        let aspect = pass.rect.width as f32 / pass.rect.height.max(1) as f32;
        let projection = Mat4::perspective_infinite_reverse_z(fov as f32, aspect, near).map_err(RenderError::Space)?;
        let view = Mat4::from_rotation(pose.rotation.conjugate());
        let mut uniform = [0u8; 128];
        projection.write_column_major(&mut uniform[0..64]);
        view.write_column_major(&mut uniform[64..128]);
        self.ensure_overlay_gpu()?;
        let mut bytes = Vec::with_capacity(overlay.vertices.len() * 32);
        for vertex in &overlay.vertices {
            bytes.extend_from_slice(&vertex.position[0].to_le_bytes());
            bytes.extend_from_slice(&vertex.position[1].to_le_bytes());
            bytes.extend_from_slice(&vertex.position[2].to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            for channel in vertex.color {
                bytes.extend_from_slice(&channel.to_le_bytes());
            }
        }
        self.replace_overlay_vertices(bytes)?;
        let gpu = self.overlay_gpu.as_ref().expect("overlay");
        let uniform_id = gpu.uniform;
        let vertex_id = gpu.vertices.ok_or(RenderError::Rhi(RhiError::InvalidResource("overlay vertices")))?;
        let group = gpu.group;
        let pipeline = gpu.pipeline;
        self.device.write_buffer(uniform_id, 0, &uniform).map_err(RenderError::Rhi)?;
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("JARVIG.EditorOverlay".into()),
                colors: vec![ColorAttachment { target: color, load: LoadOp::Load, store: StoreOp::Store }],
                depth: None,
            })
            .map_err(RenderError::Rhi)?;
        encoder
            .set_viewport(Viewport {
                x: pass.rect.x as f32,
                y: pass.rect.y as f32,
                width: pass.rect.width as f32,
                height: pass.rect.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            })
            .map_err(RenderError::Rhi)?;
        encoder
            .set_scissor(ScissorRect { x: pass.rect.x, y: pass.rect.y, width: pass.rect.width, height: pass.rect.height })
            .map_err(RenderError::Rhi)?;
        encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(0, group).map_err(RenderError::Rhi)?;
        encoder.set_vertex_buffer(0, vertex_id, 0).map_err(RenderError::Rhi)?;
        encoder.draw(overlay.vertices.len() as u32, 1, 0, 0).map_err(RenderError::Rhi)?;
        encoder.end_render_pass().map_err(RenderError::Rhi)
    }

    fn replace_overlay_vertices(&mut self, bytes: Vec<u8>) -> Result<(), RenderError> {
        let gpu = self.overlay_gpu.as_mut().ok_or(RenderError::Rhi(RhiError::InvalidResource("overlay")))?;
        if gpu.last_vertices == bytes {
            return Ok(());
        }
        if let Some(previous) = gpu.vertices.take() {
            self.device.destroy(ResourceKind::Buffer, previous.raw()).map_err(RenderError::Rhi)?;
        }
        let buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: bytes.len() as u64,
                usage: BufferUsage::Vertex,
                label: Some("JARVIG.EditorOverlay.Vertices".into()),
                contents: Some(bytes.clone()),
            })
            .map_err(RenderError::Rhi)?;
        let gpu = self.overlay_gpu.as_mut().expect("overlay");
        gpu.vertices = Some(buffer);
        gpu.last_vertices = bytes;
        Ok(())
    }

    fn ensure_overlay_gpu(&mut self) -> Result<(), RenderError> {
        if self.overlay_gpu.is_some() {
            return Ok(());
        }
        self.ensure_camera_layout()?;
        let layout = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("overlay layout")))?;
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(OVERLAY_SHADER.into()),
                label: Some("JARVIG.EditorOverlay".into()),
            })
            .map_err(RenderError::Rhi)?;
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: self.color_format,
                layouts: vec![layout],
                vertex_buffers: vec![VertexBufferLayout {
                    stride: 32,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![
                        VertexAttribute { shader_location: 0, offset: 0, format: VertexFormat::Float32x3 },
                        VertexAttribute { shader_location: 1, offset: 16, format: VertexFormat::Float32x4 },
                    ],
                }],
                depth: None,
                cull: CullMode::None,
                label: Some("JARVIG.EditorOverlay".into()),
            })
            .map_err(RenderError::Rhi)?;
        let uniform = self
            .device
            .create_buffer(&BufferDesc {
                size: 128,
                usage: BufferUsage::Uniform,
                label: Some("JARVIG.EditorOverlay.Camera".into()),
                contents: None,
            })
            .map_err(RenderError::Rhi)?;
        let group = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout,
                entries: vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(uniform) }],
                label: Some("JARVIG.EditorOverlay.Camera".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.overlay_gpu = Some(OverlayGpu { shader, pipeline, uniform, vertices: None, group, last_vertices: Vec::new() });
        Ok(())
    }

    fn prepare(
        &mut self,
        id: RenderViewId,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
    ) -> Result<Prepared, RenderError> {
        let rect = {
            let layout = self.slot(id)?.layout;
            pixel_rect(layout, self.width, self.height)?
        };
        let aspect = rect.width as f32 / rect.height as f32;
        let (frame, fov, near) = {
            let camera = &self.slot(id)?.camera;
            (camera.frame, camera.vertical_fov_radians, camera.near_m)
        };
        let pose = if let Some(pose) = self.slot(id)?.pose_override {
            pose
        } else {
            snapshot.camera(frame).ok_or(RenderError::Space(SpaceError::MissingFrame))?.pose
        };
        let mut draws = Vec::new();
        let mut terrain_grid = Vec::new();
        let mut light_group = None;
        let hidden = self.entity_hidden.clone();
        let grid_meshes = self.terrain_grid_meshes.clone();
        for instance in snapshot.instances().iter().filter(|instance| instance.visible && !hidden.contains(&instance.entity)) {
            let packet = instance_gpu_transforms(instance, &pose, fov, near, aspect).map_err(RenderError::Space)?;
            self.write_binding(id, instance.id, &packet.to_bytes())?;
            let transform = self
                .bindings
                .iter()
                .find(|binding| binding.view == id && binding.instance == instance.id)
                .expect("binding")
                .group;
            if grid_meshes.contains(&instance.mesh) {
                terrain_grid.push(TerrainGridDraw { mesh: instance.mesh, transform });
            }
            let mesh = meshes.get(instance.mesh).ok_or(RenderError::Mesh(MeshError::Empty))?;
            for (submesh_index, submesh) in mesh.submeshes().iter().enumerate() {
                let Some(material_id) = instance.material_for_slot(submesh.material_slot) else {
                    self.unbound_material_skips = self.unbound_material_skips.saturating_add(1);
                    continue;
                };
                let master = materials.master_of(material_id).map_err(|_| RenderError::UnknownMaterial)?;
                let compiled = materials.compiled(master).map_err(|_| RenderError::UnknownMaterial)?.clone();
                if !mesh_satisfies(mesh, &compiled) {
                    return Err(RenderError::IncompatibleMaterial);
                }
                let pipeline = self.ensure_gpu_master(master, &compiled, mesh)?;
                let material = self.ensure_gpu_material(material_id, master, &compiled, materials, textures)?;
                let lights = if compiled.shading == ShadingModel::StandardMetalRough {
                    if light_group.is_none() {
                        light_group = Some(self.ensure_view_lights(id, &pose, snapshot.lights(), snapshot.environment(), snapshot.reflection_probes())?);
                    }
                    if instance.receive_shadows {
                        light_group
                    } else {
                        Some(self.ensure_unshadowed_lights(id)?)
                    }
                } else {
                    None
                };
                draws.push(PreparedDraw {
                    mesh: instance.mesh,
                    submesh: submesh_index,
                    transform,
                    material,
                    lights,
                    pipeline,
                    vertex_stride: mesh.streams().first().map(|stream| stream.stride).unwrap_or(0),
                });
            }
        }
        let view = self.slot(id)?;
        Ok(Prepared {
            id,
            label: view.label.clone(),
            rect,
            depth: view.depth_view.ok_or(RenderError::Rhi(RhiError::InvalidResource("depth view")))?,
            draws,
            terrain_grid,
        })
    }

    fn ensure_draw_resources(&mut self, snapshot: &RenderSceneSnapshot, meshes: &MeshLibrary) -> Result<(), RenderError> {
        let hidden = self.entity_hidden.clone();
        for instance in snapshot.instances().iter().filter(|instance| instance.visible && !hidden.contains(&instance.entity)) {
            self.ensure_gpu_mesh(instance.mesh, meshes)?;
        }
        Ok(())
    }

    fn ensure_camera_layout(&mut self) -> Result<(), RenderError> {
        if self.camera_layout.is_some() {
            return Ok(());
        }
        self.camera_layout = Some(
            self.device
                .create_bind_group_layout(&BindGroupLayoutDesc {
                    entries: vec![BindGroupLayoutEntry {
                        binding: 0,
                        kind: BindingType::UniformBuffer,
                        stage: ShaderStage::VertexFragment,
                    }],
                    label: Some("JARVIG.Camera".into()),
                })
                .map_err(RenderError::Rhi)?,
        );
        Ok(())
    }

    fn ensure_gpu_master(
        &mut self,
        master: MasterMaterialId,
        compiled: &CompiledMaterial,
        mesh: &Mesh,
    ) -> Result<PipelineId, RenderError> {
        let vertex_key = vertex_key(mesh);
        if let Some(existing) = self
            .gpu_masters
            .iter()
            .find(|gpu| gpu.master == master && gpu.program_key == compiled.key && gpu.vertex_key == vertex_key)
        {
            return Ok(existing.pipeline);
        }
        let camera = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("pipeline layout")))?;
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(compiled.artifact.clone()),
                label: Some(format!("JARVIG.Material{}", master.0)),
            })
            .map_err(RenderError::Rhi)?;
        let mut entries = vec![BindGroupLayoutEntry {
            binding: 0,
            kind: BindingType::UniformBuffer,
            stage: ShaderStage::Fragment,
        }];
        for resource in &compiled.resources {
            let kind = match resource.class {
                ResourceClass::Texture2D => BindingType::Texture,
                ResourceClass::Sampler => BindingType::Sampler,
            };
            entries.push(BindGroupLayoutEntry { binding: resource.binding, kind, stage: ShaderStage::Fragment });
        }
        let layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries,
                label: Some(format!("JARVIG.Material{}.Layout", master.0)),
            })
            .map_err(RenderError::Rhi)?;
        let mut layouts = vec![camera, layout];
        if compiled.shading == ShadingModel::StandardMetalRough {
            layouts.push(self.ensure_light_layout()?);
        }
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: HDR_SCENE_FORMAT,
                layouts,
                vertex_buffers: vec![rhi_layout(mesh)?],
                depth: Some(DepthState {
                    format: TextureFormat::Depth32Float,
                    write_enabled: true,
                    compare: CompareFunction::GreaterEqual,
                }),
                cull: match compiled.cull {
                    jarvig_material::CullMode::Back => CullMode::Back,
                    jarvig_material::CullMode::Front => CullMode::Front,
                    jarvig_material::CullMode::None => CullMode::None,
                },
                label: Some(format!("JARVIG.Material{}", master.0)),
            })
            .map_err(RenderError::Rhi)?;
        self.material_pipelines = self.material_pipelines.saturating_add(1);
        self.gpu_masters.push(GpuMasterMaterial {
            master,
            program_key: compiled.key,
            vertex_key,
            shader,
            pipeline,
            layout,
            cull: compiled.cull,
            two_sided: compiled.two_sided,
        });
        Ok(pipeline)
    }

    /// Raster policy of the first resident standard master. `None` if nothing has compiled.
    pub fn material_cull(&self) -> Option<jarvig_material::CullMode> {
        self.gpu_masters.first().map(|master| master.cull)
    }

    pub fn material_two_sided(&self) -> Option<bool> {
        self.gpu_masters.first().map(|master| master.two_sided)
    }

    fn ensure_gpu_material(
        &mut self,
        instance: MaterialInstanceId,
        master: MasterMaterialId,
        compiled: &CompiledMaterial,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
    ) -> Result<BindGroupId, RenderError> {
        let revision = materials.instance_revision(instance).map_err(|_| RenderError::UnknownMaterial)?;
        let bytes = materials.parameter_bytes(instance).map_err(|_| RenderError::UnknownMaterial)?.to_vec();
        if bytes.is_empty() || bytes.len() % 4 != 0 {
            return Err(RenderError::UnknownMaterial);
        }
        let bound_textures = self.bound_textures(instance, compiled, materials)?;
        let bound_samplers = self.bound_samplers(instance, compiled, materials)?;
        if let Some(slot) = self.gpu_materials.iter().position(|gpu| gpu.instance == instance) {
            if self.gpu_materials[slot].revision == revision {
                return Ok(self.gpu_materials[slot].group);
            }
            let buffer = self.gpu_materials[slot].buffer;
            let resources_changed = self.gpu_materials[slot].textures != bound_textures || self.gpu_materials[slot].samplers != bound_samplers;
            self.device.write_buffer(buffer, 0, &bytes).map_err(RenderError::Rhi)?;
            self.material_parameter_uploads = self.material_parameter_uploads.saturating_add(1);
            if resources_changed {
                let old = self.gpu_materials[slot].group;
                let layout = self.gpu_masters.iter().find(|gpu| gpu.master == master).map(|gpu| gpu.layout).ok_or(RenderError::UnknownMaterial)?;
                self.device.destroy(ResourceKind::BindGroup, old.raw()).map_err(RenderError::Rhi)?;
                let group = self.material_group(instance, compiled, layout, buffer, &bound_textures, &bound_samplers, textures)?;
                self.gpu_materials[slot].group = group;
                self.gpu_materials[slot].textures = bound_textures;
                self.gpu_materials[slot].samplers = bound_samplers;
                self.texture_binding_updates = self.texture_binding_updates.saturating_add(1);
            }
            self.gpu_materials[slot].revision = revision;
            return Ok(self.gpu_materials[slot].group);
        }
        let layout = self.gpu_masters.iter().find(|gpu| gpu.master == master).map(|gpu| gpu.layout).ok_or(RenderError::UnknownMaterial)?;
        let buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: bytes.len() as u64,
                usage: BufferUsage::Uniform,
                label: Some(format!("JARVIG.MaterialInstance{}", instance.0)),
                contents: Some(bytes.clone()),
            })
            .map_err(RenderError::Rhi)?;
        let group = self.material_group(instance, compiled, layout, buffer, &bound_textures, &bound_samplers, textures)?;
        self.material_parameter_uploads = self.material_parameter_uploads.saturating_add(1);
        self.gpu_materials.push(GpuMaterialInstance {
            instance,
            revision,
            buffer,
            group,
            textures: bound_textures,
            samplers: bound_samplers,
        });
        Ok(group)
    }

    fn bound_textures(
        &self,
        instance: MaterialInstanceId,
        compiled: &CompiledMaterial,
        materials: &MaterialLibrary,
    ) -> Result<Vec<LogicalTextureId>, RenderError> {
        let mut ids = Vec::new();
        for resource in &compiled.resources {
            if resource.class == ResourceClass::Texture2D {
                ids.push(materials.texture_binding(instance, &resource.name).map_err(|_| RenderError::UnknownMaterial)?);
            }
        }
        Ok(ids)
    }

    fn bound_samplers(
        &self,
        instance: MaterialInstanceId,
        compiled: &CompiledMaterial,
        materials: &MaterialLibrary,
    ) -> Result<Vec<SamplerId>, RenderError> {
        let mut ids = Vec::new();
        for resource in &compiled.resources {
            if resource.class == ResourceClass::Sampler {
                ids.push(materials.sampler_binding(instance, &resource.name).map_err(|_| RenderError::UnknownMaterial)?);
            }
        }
        Ok(ids)
    }

    fn material_group(
        &mut self,
        instance: MaterialInstanceId,
        compiled: &CompiledMaterial,
        layout: BindGroupLayoutId,
        buffer: BufferId,
        textures: &[LogicalTextureId],
        samplers: &[SamplerId],
        library: &TextureLibrary,
    ) -> Result<BindGroupId, RenderError> {
        let mut entries = vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(buffer) }];
        let mut texture_index = 0usize;
        let mut sampler_index = 0usize;
        for resource in &compiled.resources {
            match resource.class {
                ResourceClass::Texture2D => {
                    let view = self.resolve_texture(textures[texture_index], resource.semantic, library)?;
                    texture_index += 1;
                    entries.push(BindGroupEntry { binding: resource.binding, resource: jarvig_rhi::BindResource::TextureView(view) });
                }
                ResourceClass::Sampler => {
                    let sampler = self.resolve_sampler(samplers[sampler_index], library)?;
                    sampler_index += 1;
                    entries.push(BindGroupEntry { binding: resource.binding, resource: jarvig_rhi::BindResource::Sampler(sampler) });
                }
            }
        }
        self.device
            .create_bind_group(&BindGroupDesc {
                layout,
                entries,
                label: Some(format!("JARVIG.MaterialInstance{}", instance.0)),
            })
            .map_err(RenderError::Rhi)
    }

    fn resolve_texture(
        &mut self,
        id: LogicalTextureId,
        semantic: Option<TextureSemantic>,
        library: &TextureLibrary,
    ) -> Result<TextureViewId, RenderError> {
        let id = if id.0 == 0 {
            self.default_texture(semantic, library)?
        } else if library.get(id).is_none() {
            if semantic.map(TextureSemantic::color_space) != Some(ColorSpace::Linear) {
                self.missing_texture_uses = self.missing_texture_uses.saturating_add(1);
                library.error_color().ok_or(RenderError::UnknownTexture)?
            } else {
                return Err(RenderError::UnknownTexture);
            }
        } else {
            id
        };
        self.ensure_gpu_texture(id, library)
    }

    fn default_texture(&self, semantic: Option<TextureSemantic>, library: &TextureLibrary) -> Result<LogicalTextureId, RenderError> {
        let id = match semantic {
            Some(TextureSemantic::Emissive) => library.black_srgb(),
            Some(TextureSemantic::Normal) => library.flat_normal(),
            Some(TextureSemantic::Roughness) => library.half_linear(),
            Some(TextureSemantic::AmbientOcclusion | TextureSemantic::Orm) => library.neutral_orm(),
            Some(TextureSemantic::Metallic | TextureSemantic::Height | TextureSemantic::Data) => library.zero_linear(),
            Some(TextureSemantic::BaseColor | TextureSemantic::UnlitColor) | None => library.white_srgb(),
        };
        id.ok_or(RenderError::UnknownTexture)
    }

    fn ensure_gpu_texture(&mut self, id: LogicalTextureId, library: &TextureLibrary) -> Result<TextureViewId, RenderError> {
        let texture = library.get(id).ok_or(RenderError::UnknownTexture)?;
        let mip_count = if texture.has_mip_chain() {
            texture.mip_count()
        } else if texture.mip_count() == 1 {
            1
        } else {
            return Err(RenderError::Rhi(RhiError::Unsupported("only one mip is uploaded")));
        };
        if let Some(resident) = self.gpu_textures.iter().find(|resident| resident.id == id && resident.revision == texture.revision()) {
            return Ok(resident.view);
        }
        let format = sampled_format(texture.color_space());
        let width = texture.width();
        let height = texture.height();
        let revision = texture.revision();
        let levels: Vec<Vec<u8>> = if texture.has_mip_chain() {
            (0..mip_count).map(|level| texture.mip_pixels(level).unwrap_or_default().to_vec()).collect()
        } else {
            vec![texture.pixels().to_vec()]
        };
        let gpu = self
            .device
            .create_texture(&TextureDesc {
                width,
                height,
                format,
                usage: TextureUsage::Sampled,
                mip_count,
                label: Some(format!("JARVIG.Texture{}", id.0)),
            })
            .map_err(RenderError::Rhi)?;
        for (level, pixels) in levels.iter().enumerate() {
            self.device.write_texture_mip(gpu, level as u32, pixels).map_err(RenderError::Rhi)?;
        }
        let view = self.device.create_texture_view(gpu).map_err(RenderError::Rhi)?;
        self.texture_uploads = self.texture_uploads.saturating_add(1);
        self.gpu_textures.push(ResidentTexture { id, revision, texture: gpu, view });
        Ok(view)
    }

    fn resolve_sampler(&mut self, id: SamplerId, library: &TextureLibrary) -> Result<jarvig_rhi::SamplerId, RenderError> {
        if id.0 == 0 {
            return Err(RenderError::UnknownSampler);
        }
        let state = library.sampler(id).ok_or(RenderError::UnknownSampler)?;
        self.ensure_gpu_sampler(state)
    }

    fn ensure_gpu_sampler(&mut self, state: SamplerState) -> Result<jarvig_rhi::SamplerId, RenderError> {
        if let Some((_, sampler)) = self.gpu_samplers.iter().find(|(stored, _)| *stored == state) {
            return Ok(*sampler);
        }
        let sampler = self
            .device
            .create_sampler(&jarvig_rhi::SamplerDesc {
                min_filter: map_filter(state.min_filter),
                mag_filter: map_filter(state.mag_filter),
                mip_filter: map_filter(state.mip_filter),
                address_u: map_address(state.address_u),
                address_v: map_address(state.address_v),
                address_w: map_address(state.address_w),
                anisotropy: u16::from(state.anisotropy.max(1)),
                label: Some("JARVIG.Sampler".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.gpu_samplers.push((state, sampler));
        Ok(sampler)
    }

    fn ensure_gpu_mesh(&mut self, id: MeshId, meshes: &MeshLibrary) -> Result<(), RenderError> {
        let resident = self
            .gpu_meshes
            .iter()
            .any(|(mesh, slot)| *mesh == id && matches!(slot, GpuResidency::Resident(_)));
        if resident {
            return Ok(());
        }
        let mesh = meshes.get(id).ok_or(RenderError::Mesh(MeshError::Empty))?.clone();
        let uploaded = self.upload_mesh(&mesh, &format!("JARVIG.Mesh{}", id.0))?;
        self.mesh_uploads += 1;
        if let Some(slot) = self.gpu_meshes.iter_mut().find(|(mesh, _)| *mesh == id) {
            slot.1 = GpuResidency::Resident(uploaded);
        } else {
            self.gpu_meshes.push((id, GpuResidency::Resident(uploaded)));
        }
        Ok(())
    }

    fn upload_mesh(&mut self, mesh: &Mesh, label: &str) -> Result<GpuMesh, RenderError> {
        let stream = mesh.streams().first().ok_or(RenderError::Mesh(MeshError::Empty))?;
        let vertices = self
            .device
            .create_buffer(&BufferDesc {
                size: stream.bytes.len() as u64,
                usage: BufferUsage::Vertex,
                label: Some(format!("{label}.Vertices")),
                contents: Some(stream.bytes.clone()),
            })
            .map_err(RenderError::Rhi)?;
        let indices = self
            .device
            .create_buffer(&BufferDesc {
                size: mesh.index_bytes().len() as u64,
                usage: BufferUsage::Index,
                label: Some(format!("{label}.Indices")),
                contents: Some(mesh.index_bytes().to_vec()),
            })
            .map_err(RenderError::Rhi)?;
        Ok(GpuMesh {
            vertices,
            indices,
            index_format: match mesh.index_format() {
                MeshIndexFormat::Uint16 => IndexFormat::Uint16,
                MeshIndexFormat::Uint32 => IndexFormat::Uint32,
            },
            submeshes: mesh
                .submeshes()
                .iter()
                .map(|submesh| GpuSubmesh {
                    index_count: submesh.index_count,
                    first_index: submesh.first_index,
                    base_vertex: submesh.base_vertex,
                })
                .collect(),
        })
    }

    fn encode_contact_prepass(
        &mut self,
        encoder: &mut dyn jarvig_rhi::CommandEncoder,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        pass: &Prepared,
        cleared: &mut bool,
    ) -> Result<(), RenderError> {
        let Some(target) = self.contact.as_ref().map(|contact| contact.view) else {
            return Ok(());
        };
        let mut prepared = Vec::new();
        let hidden = self.entity_hidden.clone();
        for instance in snapshot.instances().iter().filter(|instance| instance.visible && !hidden.contains(&instance.entity)) {
            let Some(group) = self.bindings.iter().find(|binding| binding.view == pass.id && binding.instance == instance.id).map(|binding| binding.group) else {
                continue;
            };
            let mesh = meshes.get(instance.mesh).ok_or(RenderError::Mesh(MeshError::Empty))?;
            let stream = mesh.streams().first().ok_or(RenderError::Mesh(MeshError::Empty))?;
            let pipeline = self.ensure_contact_pipeline(u64::from(stream.stride))?;
            prepared.push((instance.mesh, pipeline, group));
        }
        let clear = !*cleared;
        *cleared = true;
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("JARVIG.ContactDepth".into()),
                colors: vec![ColorAttachment {
                    target,
                    load: if clear { LoadOp::Clear(ClearColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }) } else { LoadOp::Load },
                    store: StoreOp::Store,
                }],
                depth: Some(DepthAttachment {
                    target: pass.depth,
                    load: DepthLoadOp::Clear(DEPTH_CLEAR),
                    store: StoreOp::Store,
                }),
            })
            .map_err(RenderError::Rhi)?;
        encoder
            .set_viewport(Viewport {
                x: pass.rect.x as f32,
                y: pass.rect.y as f32,
                width: pass.rect.width as f32,
                height: pass.rect.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            })
            .map_err(RenderError::Rhi)?;
        encoder
            .set_scissor(ScissorRect { x: pass.rect.x, y: pass.rect.y, width: pass.rect.width, height: pass.rect.height })
            .map_err(RenderError::Rhi)?;
        for (mesh, pipeline, group) in prepared {
            let gpu = self
                .gpu_meshes
                .iter()
                .find(|(id, _)| *id == mesh)
                .and_then(|(_, slot)| match slot {
                    GpuResidency::Resident(gpu) => Some(gpu),
                    GpuResidency::Evicted => None,
                })
                .ok_or(RenderError::Mesh(MeshError::Empty))?;
            encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
            encoder.set_bind_group(0, group).map_err(RenderError::Rhi)?;
            encoder.set_vertex_buffer(0, gpu.vertices, 0).map_err(RenderError::Rhi)?;
            encoder.set_index_buffer(gpu.indices, gpu.index_format, 0).map_err(RenderError::Rhi)?;
            for range in &gpu.submeshes {
                encoder.draw_indexed(range.index_count, 1, range.first_index, range.base_vertex, 0).map_err(RenderError::Rhi)?;
            }
        }
        encoder.end_render_pass().map_err(RenderError::Rhi)
    }

    fn ensure_contact_pipeline(&mut self, stride: u64) -> Result<PipelineId, RenderError> {
        if self.contact_gpu.is_none() {
            let shader = self
                .device
                .create_shader_module(&ShaderModuleDesc {
                    source: ShaderSource::Wgsl(CONTACT_SHADER.into()),
                    label: Some("JARVIG.ContactDepth".into()),
                })
                .map_err(RenderError::Rhi)?;
            self.contact_gpu = Some(ContactGpu { shader, pipelines: Vec::new() });
        }
        if let Some(found) = self.contact_gpu.as_ref().and_then(|gpu| gpu.pipelines.iter().find(|(stored, _)| *stored == stride)) {
            return Ok(found.1);
        }
        let shader = self.contact_gpu.as_ref().expect("contact").shader;
        let layout = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("camera layout")))?;
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: TextureFormat::Rgba16Float,
                layouts: vec![layout],
                vertex_buffers: vec![VertexBufferLayout {
                    stride,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![VertexAttribute { shader_location: 0, offset: 0, format: VertexFormat::Float32x3 }],
                }],
                depth: Some(DepthState { format: TextureFormat::Depth32Float, write_enabled: true, compare: CompareFunction::GreaterEqual }),
                cull: jarvig_rhi::CullMode::None,
                label: Some(format!("JARVIG.Contact.Stride{stride}")),
            })
            .map_err(RenderError::Rhi)?;
        self.contact_gpu.as_mut().expect("contact").pipelines.push((stride, pipeline));
        Ok(pipeline)
    }

    fn prepared_cut(&self, draw: &PreparedDraw) -> PreparedCut {
        let isolate = self.gpu_scene.diagnostic.identity_colors() && self.meshlet_gpu.as_ref().is_some_and(|debug| debug.mesh == draw.mesh);
        let parent_draw = self.visibility_parents() && self.parent_gpu.as_ref().map(|parent| parent.mesh == draw.mesh).unwrap_or(false);
        if !(isolate || self.visibility_frustum() || parent_draw) {
            return PreparedCut::Material;
        }
        let Some(debug) = self.meshlet_gpu.as_ref().filter(|debug| (isolate || debug.shade_clustered) && debug.mesh == draw.mesh) else {
            return PreparedCut::Material;
        };
        if draw.submesh != 0 {
            return PreparedCut::HiddenSubmesh;
        }
        PreparedCut::Clustered {
            indices: debug.indices,
            runs: coalesce_spans(&debug.spans, &self.gpu_scene.debug_flags),
            parent_draw,
        }
    }

    /// One record per submitted run, written before the color pass. Queue writes are not ordered inside that pass.
    fn append_diagnostic_bases(&mut self, pass: &Prepared, start: u32) -> Result<u32, RenderError> {
        let mut bases = Vec::new();
        for draw in &pass.draws {
            let PreparedCut::Clustered { runs, .. } = self.prepared_cut(draw) else { continue };
            for (first_index, index_count) in runs {
                if index_count > 0 {
                    bases.push(first_index / 3);
                }
            }
        }
        if bases.is_empty() {
            return Ok(start);
        }
        let end = start.saturating_add(bases.len() as u32);
        self.ensure_diagnostic_bases(end as usize)?;
        let buffer = self.diagnostic_draw_uniform.ok_or(RenderError::Rhi(RhiError::InvalidResource("diagnostic bases")))?;
        self.device.write_buffer(buffer, u64::from(start) * 4, &u32_bytes(&bases)).map_err(RenderError::Rhi)?;
        Ok(end)
    }

    fn draw_prepared(&mut self, encoder: &mut dyn jarvig_rhi::CommandEncoder, draw: &PreparedDraw, recolor: bool) -> Result<(), RenderError> {
        let (vertices, mesh_indices, index_format, first_index, index_count, base_vertex) = {
            let gpu = self
                .gpu_meshes
                .iter()
                .find(|(id, _)| *id == draw.mesh)
                .and_then(|(_, slot)| match slot {
                    GpuResidency::Resident(gpu) => Some(gpu),
                    GpuResidency::Evicted => None,
                })
                .ok_or(RenderError::Mesh(MeshError::Empty))?;
            let range = gpu.submeshes.get(draw.submesh).ok_or(RenderError::Mesh(MeshError::BadSubmesh))?;
            (gpu.vertices, gpu.indices, gpu.index_format, range.first_index, range.index_count, range.base_vertex)
        };
        encoder.set_vertex_buffer(0, vertices, 0).map_err(RenderError::Rhi)?;
        let cut = self.prepared_cut(draw);
        if matches!(cut, PreparedCut::HiddenSubmesh) {
            return Ok(());
        }
        if let PreparedCut::Clustered { indices, runs, parent_draw } = cut {
            if recolor {
                return self.draw_recolored_submission(encoder, draw, indices, &runs, parent_draw);
            }
            encoder.set_pipeline(draw.pipeline).map_err(RenderError::Rhi)?;
            encoder.set_bind_group(0, draw.transform).map_err(RenderError::Rhi)?;
            encoder.set_bind_group(1, draw.material).map_err(RenderError::Rhi)?;
            if let Some(lights) = draw.lights {
                encoder.set_bind_group(2, lights).map_err(RenderError::Rhi)?;
            }
            encoder.set_index_buffer(indices, IndexFormat::Uint32, 0).map_err(RenderError::Rhi)?;
            let mut meshlet_tris = 0u32;
            for (first_index, index_count) in runs {
                if index_count > 0 {
                    encoder.draw_indexed(index_count, 1, first_index, 0, 0).map_err(RenderError::Rhi)?;
                    meshlet_tris = meshlet_tris.saturating_add(index_count / 3);
                }
            }
            self.gpu_scene.stats.meshlet_draw_triangles = self.gpu_scene.stats.meshlet_draw_triangles.saturating_add(meshlet_tris);
            if parent_draw {
                let parent_tris = self.draw_parent_triangles(encoder)?;
                self.gpu_scene.stats.parent_draw_triangles = self.gpu_scene.stats.parent_draw_triangles.saturating_add(parent_tris);
            }
            return Ok(());
        }
        encoder.set_pipeline(draw.pipeline).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(0, draw.transform).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(1, draw.material).map_err(RenderError::Rhi)?;
        if let Some(lights) = draw.lights {
            encoder.set_bind_group(2, lights).map_err(RenderError::Rhi)?;
        }
        let clustered = self.meshlet_gpu.as_ref().and_then(|debug| {
            if debug.shade_clustered && debug.mesh == draw.mesh {
                debug.ranges.get(draw.submesh).copied().map(|range| (debug.indices, range))
            } else {
                None
            }
        });
        if let Some((indices, (first_index, index_count))) = clustered {
            if index_count == 0 {
                return Ok(());
            }
            encoder.set_index_buffer(indices, IndexFormat::Uint32, 0).map_err(RenderError::Rhi)?;
            self.gpu_scene.stats.meshlet_draw_triangles = self.gpu_scene.stats.meshlet_draw_triangles.saturating_add(index_count / 3);
            encoder.draw_indexed(index_count, 1, first_index, 0, 0).map_err(RenderError::Rhi)
        } else {
            encoder.set_index_buffer(mesh_indices, index_format, 0).map_err(RenderError::Rhi)?;
            self.gpu_scene.stats.legacy_draw_triangles = self.gpu_scene.stats.legacy_draw_triangles.saturating_add(index_count / 3);
            encoder.draw_indexed(index_count, 1, first_index, base_vertex, 0).map_err(RenderError::Rhi)
        }
    }

    /// The same index ranges and the same vertex buffer as the shaded cut. Only the fragment color changes.
    fn draw_recolored_submission(
        &mut self,
        encoder: &mut dyn jarvig_rhi::CommandEncoder,
        draw: &PreparedDraw,
        indices: BufferId,
        runs: &[(u32, u32)],
        parent_draw: bool,
    ) -> Result<(), RenderError> {
        let pipeline = self.ensure_diagnostic_color_pipeline(u64::from(draw.vertex_stride))?;
        let params = self.ensure_diagnostic_bases(1)?;
        let color_group = self
            .meshlet_gpu
            .as_ref()
            .map(|gpu| gpu.color_group)
            .ok_or(RenderError::Rhi(RhiError::InvalidResource("diagnostic colors")))?;
        encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(0, draw.transform).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(1, color_group).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(2, params).map_err(RenderError::Rhi)?;
        encoder.set_index_buffer(indices, IndexFormat::Uint32, 0).map_err(RenderError::Rhi)?;
        let mut meshlet_tris = 0u32;
        for (first_index, index_count) in runs {
            if *index_count == 0 {
                continue;
            }
            let run = self.diagnostic_run_cursor;
            self.diagnostic_run_cursor = self.diagnostic_run_cursor.saturating_add(1);
            encoder.draw_indexed(*index_count, 1, *first_index, 0, run).map_err(RenderError::Rhi)?;
            meshlet_tris = meshlet_tris.saturating_add(index_count / 3);
        }
        self.gpu_scene.stats.meshlet_draw_triangles = self.gpu_scene.stats.meshlet_draw_triangles.saturating_add(meshlet_tris);
        if parent_draw {
            let color = self.parent_recolor();
            let parent_tris = self.draw_recolored_parents(encoder, draw.transform, color)?;
            self.gpu_scene.stats.parent_draw_triangles = self.gpu_scene.stats.parent_draw_triangles.saturating_add(parent_tris);
        }
        Ok(())
    }

    fn parent_recolor(&self) -> [f32; 3] {
        match self.gpu_scene.visualization {
            jarvig_core::DiagnosticVisualization::HierarchyLevels => [0.95, 0.62, 0.18],
            jarvig_core::DiagnosticVisualization::CutReasons => [0.95, 0.55, 0.12],
            jarvig_core::DiagnosticVisualization::EinsteinState => jarvig_core::detail_state_color(jarvig_core::DetailLifecycle::NotEligible),
            _ => [0.22, 0.24, 0.28],
        }
    }

    fn ensure_diagnostic_bases(&mut self, needed: usize) -> Result<BindGroupId, RenderError> {
        let needed = needed.max(1);
        if let Some(group) = self.diagnostic_draw_group {
            if self.diagnostic_base_capacity >= needed {
                return Ok(group);
            }
        }
        if let Some(group) = self.diagnostic_draw_group.take() {
            self.device.destroy(ResourceKind::BindGroup, group.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(buffer) = self.diagnostic_draw_uniform.take() {
            self.device.destroy(ResourceKind::Buffer, buffer.raw()).map_err(RenderError::Rhi)?;
        }
        if self.diagnostic_draw_layout.is_none() {
            let layout = self
                .device
                .create_bind_group_layout(&BindGroupLayoutDesc {
                    entries: vec![BindGroupLayoutEntry { binding: 0, kind: BindingType::StorageBuffer, stage: ShaderStage::Fragment }],
                    label: Some("JARVIG.DiagnosticColor.Bases".into()),
                })
                .map_err(RenderError::Rhi)?;
            self.diagnostic_draw_layout = Some(layout);
        }
        let layout = self.diagnostic_draw_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("diagnostic bases")))?;
        let capacity = needed.max(16_384).next_power_of_two();
        let buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: (capacity * 4) as u64,
                usage: BufferUsage::Storage,
                label: Some("JARVIG.DiagnosticColor.Bases".into()),
                contents: Some(vec![0u8; capacity * 4]),
            })
            .map_err(RenderError::Rhi)?;
        let group = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout,
                entries: vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(buffer) }],
                label: Some("JARVIG.DiagnosticColor.Bases".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.diagnostic_draw_uniform = Some(buffer);
        self.diagnostic_draw_group = Some(group);
        self.diagnostic_base_capacity = capacity;
        Ok(group)
    }

    fn ensure_diagnostic_color_pipeline(&mut self, stride: u64) -> Result<PipelineId, RenderError> {
        let two_sided = self.material_two_sided() == Some(true);
        if let Some((_, _, pipeline)) = self.diagnostic_draw_pipelines.iter().find(|(stored, sided, _)| *stored == stride && *sided == two_sided) {
            return Ok(*pipeline);
        }
        let _ = self.ensure_diagnostic_bases(1)?;
        if self.diagnostic_draw_shader.is_none() {
            let shader = self
                .device
                .create_shader_module(&ShaderModuleDesc {
                    source: ShaderSource::Wgsl(DIAGNOSTIC_COLOR_SHADER.into()),
                    label: Some("JARVIG.DiagnosticColor".into()),
                })
                .map_err(RenderError::Rhi)?;
            self.diagnostic_draw_shader = Some(shader);
        }
        let shader = self.diagnostic_draw_shader.ok_or(RenderError::Rhi(RhiError::InvalidResource("diagnostic shader")))?;
        let draw_layout = self.diagnostic_draw_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("diagnostic draw layout")))?;
        let color_layout = self.meshlet_gpu.as_ref().map(|gpu| gpu.color_layout).ok_or(RenderError::Rhi(RhiError::InvalidResource("diagnostic colors")))?;
        let camera = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("diagnostic camera")))?;
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: HDR_SCENE_FORMAT,
                layouts: vec![camera, color_layout, draw_layout],
                vertex_buffers: vec![VertexBufferLayout {
                    stride,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![VertexAttribute { shader_location: 0, offset: 0, format: VertexFormat::Float32x3 }],
                }],
                depth: Some(DepthState { format: TextureFormat::Depth32Float, write_enabled: true, compare: CompareFunction::GreaterEqual }),
                cull: if two_sided { CullMode::None } else { CullMode::Back },
                label: Some(format!("JARVIG.DiagnosticColor.Stride{stride}")),
            })
            .map_err(RenderError::Rhi)?;
        self.diagnostic_draw_pipelines.push((stride, two_sided, pipeline));
        Ok(pipeline)
    }

    fn drop_diagnostic_pipelines(&mut self) -> Result<(), RenderError> {
        for (_, _, pipeline) in self.diagnostic_draw_pipelines.drain(..) {
            self.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn drop_diagnostic_draw(&mut self) -> Result<(), RenderError> {
        self.drop_diagnostic_pipelines()?;
        if let Some(group) = self.diagnostic_draw_group.take() {
            self.device.destroy(ResourceKind::BindGroup, group.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(uniform) = self.diagnostic_draw_uniform.take() {
            self.device.destroy(ResourceKind::Buffer, uniform.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(layout) = self.diagnostic_draw_layout.take() {
            self.device.destroy(ResourceKind::BindGroupLayout, layout.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(shader) = self.diagnostic_draw_shader.take() {
            self.device.destroy(ResourceKind::ShaderModule, shader.raw()).map_err(RenderError::Rhi)?;
        }
        self.diagnostic_base_capacity = 0;
        self.diagnostic_run_cursor = 0;
        Ok(())
    }

    fn draw_meshlet_ids(
        &mut self,
        encoder: &mut dyn jarvig_rhi::CommandEncoder,
        pass: &Prepared,
        meshes: &MeshLibrary,
    ) -> Result<(), RenderError> {
        if self.gpu_scene.visualization.recolors_submission() {
            return Ok(());
        }
        let (mesh_id, index_buffer, index_count, spans, color_group, submitted_only) = {
            let Some(debug) = self.meshlet_gpu.as_ref() else { return Ok(()) };
            let show_colors = match self.gpu_scene.visualization {
                jarvig_core::DiagnosticVisualization::CutReasons | jarvig_core::DiagnosticVisualization::HierarchyLevels => true,
                jarvig_core::DiagnosticVisualization::None => debug.show_ids || self.gpu_scene.diagnostic.identity_colors(),
                _ => false,
            };
            if !show_colors {
                return Ok(());
            }
            let submitted_only = matches!(
                self.gpu_scene.visualization,
                jarvig_core::DiagnosticVisualization::CutReasons | jarvig_core::DiagnosticVisualization::HierarchyLevels
            );
            (debug.mesh, debug.indices, debug.index_count, debug.spans.clone(), debug.color_group, submitted_only)
        };
        let Some(mesh) = meshes.get(mesh_id) else { return Ok(()) };
        let Some(stream) = mesh.streams().first() else { return Ok(()) };
        let stride = u64::from(stream.stride);
        let vertices = self
            .gpu_meshes
            .iter()
            .find(|(id, _)| *id == mesh_id)
            .and_then(|(_, slot)| match slot {
                GpuResidency::Resident(gpu) => Some(gpu.vertices),
                GpuResidency::Evicted => None,
            });
        let Some(vertices) = vertices else { return Ok(()) };
        let pipeline = self.ensure_meshlet_color_pipeline(stride)?;
        let mut seen = Vec::new();
        for draw in &pass.draws {
            if draw.mesh != mesh_id || seen.contains(&draw.transform) {
                continue;
            }
            seen.push(draw.transform);
            encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
            encoder.set_bind_group(0, draw.transform).map_err(RenderError::Rhi)?;
            encoder.set_bind_group(1, color_group).map_err(RenderError::Rhi)?;
            encoder.set_vertex_buffer(0, vertices, 0).map_err(RenderError::Rhi)?;
            encoder.set_index_buffer(index_buffer, IndexFormat::Uint32, 0).map_err(RenderError::Rhi)?;
            if submitted_only {
                for (first_index, run_count) in coalesce_spans(&spans, &self.gpu_scene.debug_flags) {
                    if run_count > 0 {
                        encoder.draw_indexed(run_count, 1, first_index, 0, 0).map_err(RenderError::Rhi)?;
                    }
                }
            } else {
                encoder.draw_indexed(index_count, 1, 0, 0, 0).map_err(RenderError::Rhi)?;
            }
        }
        if self.visibility_parents() && self.parent_gpu.as_ref().map(|parent| parent.mesh == mesh_id && parent.frame_count > 0).unwrap_or(false) {
            let pipeline = self.ensure_parent_color_pipeline()?;
            let draws: Vec<(BufferId, BufferId, u32)> = self
                .parent_gpu
                .as_ref()
                .map(|parent| {
                    parent
                        .chunks
                        .iter()
                        .filter_map(|chunk| chunk.frame.map(|frame| (chunk.debug_vertices, frame, chunk.frame_count)))
                        .filter(|(_, _, count)| *count > 0)
                        .collect()
                })
                .unwrap_or_default();
            let mut seen = Vec::new();
            for draw in &pass.draws {
                if draw.mesh != mesh_id || seen.contains(&draw.transform) {
                    continue;
                }
                seen.push(draw.transform);
                for (debug_vertices, frame, frame_count) in &draws {
                    encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
                    encoder.set_bind_group(0, draw.transform).map_err(RenderError::Rhi)?;
                    encoder.set_vertex_buffer(0, *debug_vertices, 0).map_err(RenderError::Rhi)?;
                    encoder.set_index_buffer(*frame, IndexFormat::Uint32, 0).map_err(RenderError::Rhi)?;
                    encoder.draw_indexed(*frame_count, 1, 0, 0, 0).map_err(RenderError::Rhi)?;
                }
            }
        }
        let show_occluded = self.gpu_scene.visualization == jarvig_core::DiagnosticVisualization::CutReasons
            || (self.gpu_scene.visualization == jarvig_core::DiagnosticVisualization::None
                && self.gpu_scene.diagnostic == jarvig_core::GeometryDiagnostic::Normal
                && self.meshlet_gpu.as_ref().is_some_and(|debug| debug.show_ids));
        if show_occluded && self.visibility_occlusion() && self.gpu_scene.stats.occlusion_rejected > 0 {
            let occluded = self.ensure_meshlet_occluded_pipeline(stride)?;
            let mut seen = Vec::new();
            for draw in &pass.draws {
                if draw.mesh != mesh_id || seen.contains(&draw.transform) {
                    continue;
                }
                seen.push(draw.transform);
                encoder.set_pipeline(occluded).map_err(RenderError::Rhi)?;
                encoder.set_bind_group(0, draw.transform).map_err(RenderError::Rhi)?;
                encoder.set_bind_group(1, color_group).map_err(RenderError::Rhi)?;
                encoder.set_vertex_buffer(0, vertices, 0).map_err(RenderError::Rhi)?;
                encoder.set_index_buffer(index_buffer, IndexFormat::Uint32, 0).map_err(RenderError::Rhi)?;
                encoder.draw_indexed(index_count, 1, 0, 0, 0).map_err(RenderError::Rhi)?;
            }
        }
        Ok(())
    }


    fn draw_parent_triangles(&self, encoder: &mut dyn jarvig_rhi::CommandEncoder) -> Result<u32, RenderError> {
        let Some(parent) = self.parent_gpu.as_ref() else { return Ok(0) };
        let mut triangles = 0u32;
        for chunk in &parent.chunks {
            let Some(frame) = chunk.frame else { continue };
            if chunk.frame_count == 0 {
                continue;
            }
            encoder.set_vertex_buffer(0, chunk.vertices, 0).map_err(RenderError::Rhi)?;
            encoder.set_index_buffer(frame, IndexFormat::Uint32, 0).map_err(RenderError::Rhi)?;
            encoder.draw_indexed(chunk.frame_count, 1, 0, 0, 0).map_err(RenderError::Rhi)?;
            triangles = triangles.saturating_add(chunk.frame_count / 3);
        }
        Ok(triangles)
    }

    fn ensure_meshlet_color_pipeline(&mut self, stride: u64) -> Result<PipelineId, RenderError> {
        self.ensure_meshlet_pipeline(stride, false)
    }

    fn ensure_meshlet_occluded_pipeline(&mut self, stride: u64) -> Result<PipelineId, RenderError> {
        self.ensure_meshlet_pipeline(stride, true)
    }

    fn ensure_meshlet_pipeline(&mut self, stride: u64, occluded: bool) -> Result<PipelineId, RenderError> {
        let two_sided = self.material_two_sided() == Some(true);
        if let Some(found) = self.meshlet_gpu.as_ref().and_then(|gpu| {
            let pipelines = if occluded { &gpu.occluded_pipelines } else { &gpu.pipelines };
            pipelines.iter().find(|(stored, sided, _)| *stored == stride && *sided == two_sided).map(|(_, _, pipeline)| *pipeline)
        }) {
            return Ok(found);
        }
        let (shader, layout) = {
            let gpu = self.meshlet_gpu.as_ref().ok_or(RenderError::Rhi(RhiError::InvalidResource("meshlet debug")))?;
            (gpu.shader, gpu.color_layout)
        };
        let camera = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("meshlet camera layout")))?;
        let fragment_entry = if occluded { "fs_occluded" } else { "fs" };
        let compare = if occluded { CompareFunction::Always } else { CompareFunction::GreaterEqual };
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: fragment_entry.into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: HDR_SCENE_FORMAT,
                layouts: vec![camera, layout],
                vertex_buffers: vec![VertexBufferLayout {
                    stride,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![VertexAttribute { shader_location: 0, offset: 0, format: VertexFormat::Float32x3 }],
                }],
                depth: Some(DepthState { format: TextureFormat::Depth32Float, write_enabled: false, compare }),
                // Opaque materials hide the back of a coarse shell. A two-sided material, such as the townshop canopy, keeps both sides.
                cull: if two_sided { CullMode::None } else { CullMode::Back },
                label: Some(format!("JARVIG.MeshletIds.Stride{stride}")),
            })
            .map_err(RenderError::Rhi)?;
        let gpu = self.meshlet_gpu.as_mut().expect("meshlet debug");
        if occluded {
            gpu.occluded_pipelines.push((stride, two_sided, pipeline));
        } else {
            gpu.pipelines.push((stride, two_sided, pipeline));
        }
        Ok(pipeline)
    }

    fn write_binding(&mut self, view: RenderViewId, instance: RenderInstanceId, bytes: &[u8; GpuTransforms::BYTES]) -> Result<(), RenderError> {
        if !self.bindings.iter().any(|binding| binding.view == view && binding.instance == instance) {
            let label = self.slot(view)?.label.clone();
            let layout = self.camera_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("pipeline layout")))?;
            let name = format!("{label}.Instance{}", instance.0);
            let uniform = self
                .device
                .create_buffer(&BufferDesc {
                    size: GpuTransforms::BYTES as u64,
                    usage: BufferUsage::Uniform,
                    label: Some(name.clone()),
                    contents: None,
                })
                .map_err(RenderError::Rhi)?;
            let group = self
                .device
                .create_bind_group(&BindGroupDesc {
                    layout,
                    entries: vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(uniform) }],
                    label: Some(name),
                })
                .map_err(RenderError::Rhi)?;
            self.bindings.push(InstanceBinding { view, instance, uniform, group, last_bytes: None });
        }
        let binding = self.bindings.iter_mut().find(|binding| binding.view == view && binding.instance == instance).expect("binding");
        if binding.last_bytes == Some(*bytes) {
            return Ok(());
        }
        let uniform = binding.uniform;
        self.device.write_buffer(uniform, 0, bytes).map_err(RenderError::Rhi)?;
        let binding = self.bindings.iter_mut().find(|binding| binding.view == view && binding.instance == instance).expect("binding");
        binding.last_bytes = Some(*bytes);
        self.instance_transform_uploads = self.instance_transform_uploads.saturating_add(1);
        Ok(())
    }

    fn release_view_bindings(&mut self, view: RenderViewId) -> Result<(), RenderError> {
        let mut kept = Vec::new();
        let mut dropped = Vec::new();
        for binding in std::mem::take(&mut self.bindings) {
            if binding.view == view {
                dropped.push(binding);
            } else {
                kept.push(binding);
            }
        }
        self.bindings = kept;
        for binding in &dropped {
            self.device.destroy(ResourceKind::BindGroup, binding.group.raw()).map_err(RenderError::Rhi)?;
        }
        for binding in dropped {
            self.device.destroy(ResourceKind::Buffer, binding.uniform.raw()).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn ensure_light_layout(&mut self) -> Result<BindGroupLayoutId, RenderError> {
        if let Some(layout) = self.light_layout {
            return Ok(layout);
        }
        let layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries: vec![
                    BindGroupLayoutEntry { binding: 0, kind: BindingType::UniformBuffer, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 1, kind: BindingType::StorageBuffer, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 2, kind: BindingType::UniformBuffer, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 3, kind: BindingType::UniformBuffer, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 4, kind: BindingType::TextureCube, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 5, kind: BindingType::Sampler, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 6, kind: BindingType::StorageBuffer, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 7, kind: BindingType::Texture, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 8, kind: BindingType::Texture, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 9, kind: BindingType::TextureCube, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 10, kind: BindingType::Sampler, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 11, kind: BindingType::TextureCube, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 12, kind: BindingType::Texture, stage: ShaderStage::Fragment },
                ],
                label: Some("JARVIG.Lights".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.light_layout = Some(layout);
        Ok(layout)
    }

    fn ensure_environment_buffer(&mut self, environment: &EnvironmentLight) -> Result<BufferId, RenderError> {
        let bytes = GpuEnvironmentPacket::from_light(environment).to_bytes();
        if let Some(gpu) = &self.environment_gpu {
            if gpu.last_bytes == bytes {
                return Ok(gpu.buffer);
            }
            let buffer = gpu.buffer;
            self.device.write_buffer(buffer, 0, &bytes).map_err(RenderError::Rhi)?;
            self.environment_uploads = self.environment_uploads.saturating_add(1);
            self.environment_gpu.as_mut().expect("environment").last_bytes = bytes;
            return Ok(buffer);
        }
        let buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: GpuEnvironmentPacket::BYTES as u64,
                usage: BufferUsage::Uniform,
                label: Some("JARVIG.Environment".into()),
                contents: None,
            })
            .map_err(RenderError::Rhi)?;
        self.device.write_buffer(buffer, 0, &bytes).map_err(RenderError::Rhi)?;
        self.environment_uploads = self.environment_uploads.saturating_add(1);
        self.environment_gpu = Some(EnvironmentGpu { buffer, last_bytes: bytes });
        Ok(buffer)
    }

    fn ensure_view_lights(
        &mut self,
        view: RenderViewId,
        camera: &ResolvedPose,
        lights: &[RenderLight],
        environment: EnvironmentLight,
        probes: &[jarvig_core::RenderReflectionProbe],
    ) -> Result<BindGroupId, RenderError> {
        if lights.len() > LIGHT_CAPACITY {
            return Err(RenderError::TooManyLights);
        }
        let environment_buffer = self.ensure_environment_buffer(&environment)?;
        let (probe_view, irradiance_view, probe_sampler) = self.probe_draw_binding(probes)?;
        let selected = select_reflection_probe(probes);
        let mip_count = selected.map(|probe| probe.mip_count).unwrap_or(REFLECTION_PROBE_MIP_COUNT);
        let probe_bytes = reflection_probe_packet(selected, camera.translation, mip_count, true).to_bytes();
        let debug = self.slot(view)?.settings.lighting;
        let mut mask = debug.gpu_mask();
        let channel = self.slot(view)?.settings.material_channel & 7;
        if channel != 0 {
            mask |= channel << 20;
        }
        if self.land_unlit {
            mask = LIGHTING_DEBUG_ACTIVE;
        }
        let include_direct = !self.land_unlit && (mask == 0 || mask & LIGHTING_DEBUG_DIRECT != 0);
        let layout = self.ensure_light_layout()?;
        let mut storage = Vec::with_capacity(lights.len() * GpuLightRecord::BYTES);
        let mut included = Vec::new();
        let mut direct_count = 0u32;
        for light in lights {
            if !include_direct || !debug.allows_kind(light.kind) {
                continue;
            }
            storage.extend_from_slice(&render_light_record(light, camera).to_bytes());
            included.push(*light);
            direct_count = direct_count.saturating_add(1);
        }
        let shadow_bytes = self.pack_shadow_records(view, &included, camera, debug.shadows);
        let settings = self.slot(view)?.settings;
        let contact_bits = if settings.contact_shadows { jarvig_core::CONTACT_SHADOW_DISTANCE_M.to_bits() } else { 0 };
        let cascade_debug = u32::from(settings.cascade_debug);
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&direct_count.to_le_bytes());
        header[4..8].copy_from_slice(&mask.to_le_bytes());
        header[8..12].copy_from_slice(&contact_bits.to_le_bytes());
        header[12..16].copy_from_slice(&cascade_debug.to_le_bytes());
        let view_label = self.slot(view)?.label.clone();
        if let Some(slot) = self.light_packets.iter().position(|packet| packet.view == view) {
            if self.light_packets[slot].probe_view != probe_view
                || self.light_packets[slot].irradiance_view != irradiance_view
                || self.light_packets[slot].probe_sampler != probe_sampler
            {
                let old = self.light_packets[slot].group;
                let header_buffer = self.light_packets[slot].header;
                let storage_buffer = self.light_packets[slot].storage;
                let shadow_buffer = self.light_packets[slot].shadow;
                let probe_buffer = self.light_packets[slot].probe;
                if old.is_valid() {
                    self.device.destroy(ResourceKind::BindGroup, old.raw()).map_err(RenderError::Rhi)?;
                }
                let group = self.light_bind_group(
                    layout,
                    header_buffer,
                    storage_buffer,
                    shadow_buffer,
                    environment_buffer,
                    probe_buffer,
                    probe_view,
                    irradiance_view,
                    probe_sampler,
                    view,
                    &view_label,
                )?;
                self.light_packets[slot].group = group;
                if self.light_packets[slot].unshadowed.is_valid() {
                    let stale = self.light_packets[slot].unshadowed;
                    self.device.destroy(ResourceKind::BindGroup, stale.raw()).map_err(RenderError::Rhi)?;
                    self.light_packets[slot].unshadowed = BindGroupId::INVALID;
                }
                self.light_packets[slot].probe_view = probe_view;
                self.light_packets[slot].irradiance_view = irradiance_view;
                self.light_packets[slot].probe_sampler = probe_sampler;
            }
            if self.light_packets[slot].last_header != header {
                let buffer = self.light_packets[slot].header;
                self.device.write_buffer(buffer, 0, &header).map_err(RenderError::Rhi)?;
                self.light_uploads = self.light_uploads.saturating_add(1);
                self.light_packets[slot].last_header = header;
            }
            if self.light_packets[slot].last_storage != storage && !storage.is_empty() {
                let buffer = self.light_packets[slot].storage;
                self.device.write_buffer(buffer, 0, &storage).map_err(RenderError::Rhi)?;
                self.light_uploads = self.light_uploads.saturating_add(1);
                self.light_packets[slot].last_storage = storage;
            } else if self.light_packets[slot].last_storage != storage {
                self.light_packets[slot].last_storage.clear();
            }
            if self.light_packets[slot].last_shadow != shadow_bytes && !shadow_bytes.is_empty() {
                let buffer = self.light_packets[slot].shadow;
                self.device.write_buffer(buffer, 0, &shadow_bytes).map_err(RenderError::Rhi)?;
                self.light_packets[slot].last_shadow = shadow_bytes;
            } else if self.light_packets[slot].last_shadow != shadow_bytes {
                self.light_packets[slot].last_shadow.clear();
            }
            if self.light_packets[slot].last_probe != probe_bytes {
                let buffer = self.light_packets[slot].probe;
                self.device.write_buffer(buffer, 0, &probe_bytes).map_err(RenderError::Rhi)?;
                self.light_packets[slot].last_probe = probe_bytes;
            }
            return Ok(self.light_packets[slot].group);
        }
        let label = self.slot(view)?.label.clone();
        let header_buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: 16,
                usage: BufferUsage::Uniform,
                label: Some(format!("{label}.LightHeader")),
                contents: None,
            })
            .map_err(RenderError::Rhi)?;
        let storage_buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: (LIGHT_CAPACITY * GpuLightRecord::BYTES) as u64,
                usage: BufferUsage::Storage,
                label: Some(format!("{label}.Lights")),
                contents: Some(vec![0u8; LIGHT_CAPACITY * GpuLightRecord::BYTES]),
            })
            .map_err(RenderError::Rhi)?;
        self.device.write_buffer(header_buffer, 0, &header).map_err(RenderError::Rhi)?;
        self.light_uploads = self.light_uploads.saturating_add(1);
        if !storage.is_empty() {
            self.device.write_buffer(storage_buffer, 0, &storage).map_err(RenderError::Rhi)?;
            self.light_uploads = self.light_uploads.saturating_add(1);
        }
        let shadow_buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: (LIGHT_CAPACITY * jarvig_core::GpuShadowRecord::BYTES) as u64,
                usage: BufferUsage::Storage,
                label: Some(format!("{label}.Shadows")),
                contents: Some(vec![0u8; LIGHT_CAPACITY * jarvig_core::GpuShadowRecord::BYTES]),
            })
            .map_err(RenderError::Rhi)?;
        if !shadow_bytes.is_empty() {
            self.device.write_buffer(shadow_buffer, 0, &shadow_bytes).map_err(RenderError::Rhi)?;
        }
        let probe_buffer = self
            .device
            .create_buffer(&BufferDesc {
                size: GpuReflectionProbePacket::BYTES as u64,
                usage: BufferUsage::Uniform,
                label: Some(format!("{label}.ReflectionProbe")),
                contents: None,
            })
            .map_err(RenderError::Rhi)?;
        self.device.write_buffer(probe_buffer, 0, &probe_bytes).map_err(RenderError::Rhi)?;
        let group = self.light_bind_group(
            layout,
            header_buffer,
            storage_buffer,
            shadow_buffer,
            environment_buffer,
            probe_buffer,
            probe_view,
            irradiance_view,
            probe_sampler,
            view,
            &label,
        )?;
        self.light_packets.push(ViewLightPacket {
            view,
            header: header_buffer,
            storage: storage_buffer,
            shadow: shadow_buffer,
            probe: probe_buffer,
            group,
            unshadowed: BindGroupId::INVALID,
            last_header: header,
            last_storage: storage,
            last_shadow: shadow_bytes,
            last_probe: probe_bytes,
            probe_view,
            irradiance_view,
            probe_sampler,
        });
        Ok(group)
    }

    fn ensure_unshadowed_lights(&mut self, view: RenderViewId) -> Result<BindGroupId, RenderError> {
        if let Some(packet) = self.light_packets.iter().find(|packet| packet.view == view) {
            if packet.unshadowed.is_valid() {
                return Ok(packet.unshadowed);
            }
        }
        let layout = self.light_layout.ok_or(RenderError::Rhi(RhiError::InvalidResource("light layout")))?;
        let (header, storage, probe, probe_view, irradiance_view, probe_sampler) = {
            let packet = self.light_packets.iter().find(|packet| packet.view == view).ok_or(RenderError::Rhi(RhiError::InvalidResource("lights")))?;
            (packet.header, packet.storage, packet.probe, packet.probe_view, packet.irradiance_view, packet.probe_sampler)
        };
        let disabled = self.shadow_sample_bindings(Some(view))?.disabled;
        let environment = self.environment_gpu.as_ref().ok_or(RenderError::Rhi(RhiError::InvalidResource("environment")))?.buffer;
        let group = self.light_bind_group(
            layout,
            header,
            storage,
            disabled,
            environment,
            probe,
            probe_view,
            irradiance_view,
            probe_sampler,
            view,
            "JARVIG.Unshadowed",
        )?;
        if let Some(packet) = self.light_packets.iter_mut().find(|packet| packet.view == view) {
            packet.unshadowed = group;
        }
        Ok(group)
    }

    fn light_bind_group(
        &mut self,
        layout: BindGroupLayoutId,
        header: BufferId,
        storage: BufferId,
        shadow: BufferId,
        environment: BufferId,
        probe: BufferId,
        probe_view: TextureViewId,
        irradiance_view: TextureViewId,
        probe_sampler: jarvig_rhi::SamplerId,
        view: RenderViewId,
        label: &str,
    ) -> Result<BindGroupId, RenderError> {
        let maps = self.shadow_sample_bindings(Some(view))?;
        let contact = self.contact_view().unwrap_or(maps.spot);
        self.device
            .create_bind_group(&BindGroupDesc {
                layout,
                entries: vec![
                    BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(header) },
                    BindGroupEntry { binding: 1, resource: jarvig_rhi::BindResource::Buffer(storage) },
                    BindGroupEntry { binding: 2, resource: jarvig_rhi::BindResource::Buffer(environment) },
                    BindGroupEntry { binding: 3, resource: jarvig_rhi::BindResource::Buffer(probe) },
                    BindGroupEntry { binding: 4, resource: jarvig_rhi::BindResource::TextureView(probe_view) },
                    BindGroupEntry { binding: 5, resource: jarvig_rhi::BindResource::Sampler(probe_sampler) },
                    BindGroupEntry { binding: 6, resource: jarvig_rhi::BindResource::Buffer(shadow) },
                    BindGroupEntry { binding: 7, resource: jarvig_rhi::BindResource::TextureView(maps.directional) },
                    BindGroupEntry { binding: 8, resource: jarvig_rhi::BindResource::TextureView(maps.spot) },
                    BindGroupEntry { binding: 9, resource: jarvig_rhi::BindResource::TextureView(maps.point) },
                    BindGroupEntry { binding: 10, resource: jarvig_rhi::BindResource::Sampler(maps.sampler) },
                    BindGroupEntry { binding: 11, resource: jarvig_rhi::BindResource::TextureView(irradiance_view) },
                    BindGroupEntry { binding: 12, resource: jarvig_rhi::BindResource::TextureView(contact) },
                ],
                label: Some(format!("{label}.Lights")),
            })
            .map_err(RenderError::Rhi)
    }

    fn detach_probe_view(&mut self, view: TextureViewId) -> Result<(), RenderError> {
        let stale: Vec<usize> = self
            .light_packets
            .iter()
            .enumerate()
            .filter(|(_, packet)| packet.probe_view == view || packet.irradiance_view == view)
            .map(|(index, _)| index)
            .collect();
        for index in stale {
            let group = self.light_packets[index].group;
            if group.is_valid() {
                self.device.destroy(ResourceKind::BindGroup, group.raw()).map_err(RenderError::Rhi)?;
            }
            self.light_packets[index].group = BindGroupId::INVALID;
            self.light_packets[index].probe_view = TextureViewId::INVALID;
            self.light_packets[index].irradiance_view = TextureViewId::INVALID;
        }
        Ok(())
    }

    fn destroy_probe_gpu(&mut self, probe: ProbeGpu) -> Result<(), RenderError> {
        self.device.destroy(ResourceKind::TextureView, probe.sample.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::TextureView, probe.irradiance_sample.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Texture, probe.texture.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Texture, probe.irradiance.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Sampler, probe.sampler.raw()).map_err(RenderError::Rhi)?;
        Ok(())
    }

    fn release_view_lights(&mut self, view: RenderViewId) -> Result<(), RenderError> {
        let mut kept = Vec::new();
        let mut dropped = Vec::new();
        for packet in std::mem::take(&mut self.light_packets) {
            if packet.view == view {
                dropped.push(packet);
            } else {
                kept.push(packet);
            }
        }
        self.light_packets = kept;
        for packet in &dropped {
            self.device.destroy(ResourceKind::BindGroup, packet.group.raw()).map_err(RenderError::Rhi)?;
            if packet.unshadowed.is_valid() {
                self.device.destroy(ResourceKind::BindGroup, packet.unshadowed.raw()).map_err(RenderError::Rhi)?;
            }
        }
        for packet in dropped {
            self.device.destroy(ResourceKind::Buffer, packet.header.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, packet.storage.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, packet.shadow.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, packet.probe.raw()).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn hdr_view(&self) -> Option<TextureViewId> {
        self.hdr.as_ref().map(|hdr| hdr.view)
    }

    pub fn hdr_scene_format(&self) -> &'static str {
        "rgba16float"
    }

    pub fn hdr_scene_size(&self) -> Option<(u32, u32)> {
        self.hdr.as_ref().map(|hdr| (hdr.width, hdr.height))
    }

    pub fn hdr_scene_target_count(&self) -> u32 {
        u32::from(self.hdr.is_some())
    }

    pub fn exposure_ev(&self, view: RenderViewId) -> Result<f32, RenderError> {
        Ok(self.slot(view)?.settings.exposure_ev)
    }

    pub fn lighting_debug(&self, view: RenderViewId) -> Result<LightingDebug, RenderError> {
        Ok(self.slot(view)?.settings.lighting)
    }

    pub fn tone_map_pipeline_count(&self) -> u32 {
        self.tone_map_pipelines
    }

    pub fn tone_map_parameter_upload_count(&self) -> u32 {
        self.tone_map_parameter_uploads
    }

    pub fn output_pass_count(&self) -> u32 {
        self.output_passes_last
    }

    fn sync_hdr(&mut self) -> Result<(), RenderError> {
        if self.width == 0 || self.height == 0 {
            self.drop_hdr()?;
            self.drop_contact()?;
            return Ok(());
        }
        if self.hdr.as_ref().is_some_and(|hdr| hdr.width == self.width && hdr.height == self.height) {
            return Ok(());
        }
        self.drop_hdr()?;
        let texture = self
            .device
            .create_texture(&TextureDesc {
                width: self.width,
                height: self.height,
                format: HDR_SCENE_FORMAT,
                usage: TextureUsage::ColorTarget,
                mip_count: 1,
                label: Some("JARVIG.HdrScene".into()),
            })
            .map_err(RenderError::Rhi)?;
        let view = self.device.create_texture_view(texture).map_err(RenderError::Rhi)?;
        self.hdr = Some(HdrScene { texture, view, width: self.width, height: self.height });
        self.sync_contact()?;
        Ok(())
    }

    fn sync_contact(&mut self) -> Result<(), RenderError> {
        if self.width == 0 || self.height == 0 {
            return self.drop_contact();
        }
        if self.contact.as_ref().is_some_and(|contact| contact.width == self.width && contact.height == self.height) {
            return Ok(());
        }
        self.drop_contact()?;
        let texture = self
            .device
            .create_texture(&TextureDesc {
                width: self.width,
                height: self.height,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsage::ColorTarget,
                mip_count: 1,
                label: Some("JARVIG.ContactDepth".into()),
            })
            .map_err(RenderError::Rhi)?;
        let view = self.device.create_texture_view(texture).map_err(RenderError::Rhi)?;
        self.contact = Some(ContactDepth { texture, view, width: self.width, height: self.height });
        Ok(())
    }

    fn drop_contact(&mut self) -> Result<(), RenderError> {
        let Some(contact) = self.contact.take() else { return Ok(()) };
        for packet in &mut self.light_packets {
            if packet.group.is_valid() {
                self.device.destroy(ResourceKind::BindGroup, packet.group.raw()).map_err(RenderError::Rhi)?;
                packet.group = BindGroupId::INVALID;
            }
            if packet.unshadowed.is_valid() {
                self.device.destroy(ResourceKind::BindGroup, packet.unshadowed.raw()).map_err(RenderError::Rhi)?;
                packet.unshadowed = BindGroupId::INVALID;
            }
            packet.probe_view = TextureViewId::INVALID;
        }
        self.device.destroy(ResourceKind::TextureView, contact.view.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Texture, contact.texture.raw()).map_err(RenderError::Rhi)?;
        self.device.flush().map_err(RenderError::Rhi)?;
        Ok(())
    }

    fn drop_hdr(&mut self) -> Result<(), RenderError> {
        self.drop_output_views()?;
        let Some(hdr) = self.hdr.take() else { return Ok(()) };
        self.device.destroy(ResourceKind::TextureView, hdr.view.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Texture, hdr.texture.raw()).map_err(RenderError::Rhi)?;
        self.device.flush().map_err(RenderError::Rhi)?;
        Ok(())
    }

    fn drop_output_views(&mut self) -> Result<(), RenderError> {
        let slots = std::mem::take(&mut self.output_views);
        for slot in &slots {
            self.device.destroy(ResourceKind::BindGroup, slot.group.raw()).map_err(RenderError::Rhi)?;
        }
        for slot in slots {
            self.device.destroy(ResourceKind::Buffer, slot.uniform.raw()).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn draw_output(
        &mut self,
        encoder: &mut dyn jarvig_rhi::CommandEncoder,
        swapchain: TextureViewId,
        pass: &Prepared,
    ) -> Result<(), RenderError> {
        let settings = self.slot(pass.id)?.settings;
        let bytes = exposure_uniform(&settings);
        self.ensure_output_view(pass.id)?;
        let uniform = self.output_views.iter().find(|slot| slot.view == pass.id).expect("output view").uniform;
        let stale = self.output_views.iter().find(|slot| slot.view == pass.id).expect("output view").last_bytes != bytes;
        if stale {
            self.device.write_buffer(uniform, 0, &bytes).map_err(RenderError::Rhi)?;
            self.output_views.iter_mut().find(|slot| slot.view == pass.id).expect("output view").last_bytes = bytes;
            self.tone_map_parameter_uploads = self.tone_map_parameter_uploads.saturating_add(1);
        }
        let group = self.output_views.iter().find(|slot| slot.view == pass.id).expect("output view").group;
        let pipeline = self.output.as_ref().expect("output").pipeline;
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("JARVIG.Output".into()),
                colors: vec![ColorAttachment { target: swapchain, load: LoadOp::Load, store: StoreOp::Store }],
                depth: None,
            })
            .map_err(RenderError::Rhi)?;
        encoder
            .set_viewport(Viewport {
                x: pass.rect.x as f32,
                y: pass.rect.y as f32,
                width: pass.rect.width as f32,
                height: pass.rect.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            })
            .map_err(RenderError::Rhi)?;
        encoder
            .set_scissor(ScissorRect { x: pass.rect.x, y: pass.rect.y, width: pass.rect.width, height: pass.rect.height })
            .map_err(RenderError::Rhi)?;
        encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(0, group).map_err(RenderError::Rhi)?;
        encoder.draw(3, 1, 0, 0).map_err(RenderError::Rhi)?;
        encoder.end_render_pass().map_err(RenderError::Rhi)?;
        self.output_passes_last = self.output_passes_last.saturating_add(1);
        Ok(())
    }

    fn ensure_output_view(&mut self, view: RenderViewId) -> Result<(), RenderError> {
        self.ensure_output_pipeline()?;
        let hdr = self.hdr_view().ok_or(RenderError::Rhi(RhiError::InvalidResource("hdr scene")))?;
        if self.output_views.iter().any(|slot| slot.view == view) {
            return Ok(());
        }
        let output = self.output.as_ref().expect("output");
        let uniform = self
            .device
            .create_buffer(&BufferDesc {
                size: 16,
                usage: BufferUsage::Uniform,
                label: Some("JARVIG.Exposure".into()),
                contents: None,
            })
            .map_err(RenderError::Rhi)?;
        let group = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout: output.layout,
                entries: vec![
                    BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::TextureView(hdr) },
                    BindGroupEntry { binding: 1, resource: jarvig_rhi::BindResource::Sampler(output.sampler) },
                    BindGroupEntry { binding: 2, resource: jarvig_rhi::BindResource::Buffer(uniform) },
                ],
                label: Some("JARVIG.Output".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.output_views.push(OutputView { view, uniform, group, last_bytes: [0; 16] });
        Ok(())
    }

    fn ensure_output_pipeline(&mut self) -> Result<(), RenderError> {
        if self.output.is_some() {
            return Ok(());
        }
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(output::OUTPUT_SHADER.into()),
                label: Some("JARVIG.Output".into()),
            })
            .map_err(RenderError::Rhi)?;
        let layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries: vec![
                    BindGroupLayoutEntry { binding: 0, kind: BindingType::Texture, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 1, kind: BindingType::Sampler, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 2, kind: BindingType::UniformBuffer, stage: ShaderStage::Fragment },
                ],
                label: Some("JARVIG.Output".into()),
            })
            .map_err(RenderError::Rhi)?;
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: self.color_format,
                layouts: vec![layout],
                vertex_buffers: Vec::new(),
                depth: None,
                cull: CullMode::None,
                label: Some("JARVIG.Output".into()),
            })
            .map_err(RenderError::Rhi)?;
        let sampler = self
            .device
            .create_sampler(&jarvig_rhi::SamplerDesc {
                min_filter: jarvig_rhi::FilterMode::Linear,
                mag_filter: jarvig_rhi::FilterMode::Linear,
                mip_filter: jarvig_rhi::FilterMode::Nearest,
                address_u: jarvig_rhi::AddressMode::ClampToEdge,
                address_v: jarvig_rhi::AddressMode::ClampToEdge,
                address_w: jarvig_rhi::AddressMode::ClampToEdge,
                anisotropy: 1,
                label: Some("JARVIG.HdrSampler".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.tone_map_pipelines = 1;
        self.output = Some(OutputGpu { shader, pipeline, layout, sampler });
        Ok(())
    }

    fn sync_depth(&mut self) -> Result<(), RenderError> {
        let width = self.width;
        let height = self.height;
        let ids = self.alive_ids();
        for id in ids {
            if width == 0 || height == 0 {
                self.drop_depth(id)?;
                continue;
            }
            if self.slot(id)?.depth_size == Some((width, height)) {
                continue;
            }
            self.drop_depth(id)?;
            self.create_depth(id, width, height)?;
        }
        Ok(())
    }

    fn drop_depth(&mut self, id: RenderViewId) -> Result<(), RenderError> {
        let mut retired = false;
        if let Some(view) = self.slot_mut(id)?.depth_view.take() {
            self.device.destroy(ResourceKind::TextureView, view.raw()).map_err(RenderError::Rhi)?;
            retired = true;
        }
        if let Some(texture) = self.slot_mut(id)?.depth.take() {
            self.device.destroy(ResourceKind::Texture, texture.raw()).map_err(RenderError::Rhi)?;
            retired = true;
        }
        self.slot_mut(id)?.depth_size = None;
        if retired {
            self.device.flush().map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn create_depth(&mut self, id: RenderViewId, width: u32, height: u32) -> Result<(), RenderError> {
        let label = self.slot(id)?.label.clone();
        let texture = self
            .device
            .create_texture(&TextureDesc {
                width,
                height,
                format: TextureFormat::Depth32Float,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: Some(format!("{label}.Depth")),
            })
            .map_err(RenderError::Rhi)?;
        let view = self.device.create_texture_view(texture).map_err(RenderError::Rhi)?;
        let slot = self.slot_mut(id)?;
        slot.depth = Some(texture);
        slot.depth_view = Some(view);
        slot.depth_size = Some((width, height));
        self.depth_creates += 1;
        Ok(())
    }

    fn release_output_view(&mut self, id: RenderViewId) -> Result<(), RenderError> {
        let mut kept = Vec::new();
        let mut dropped = Vec::new();
        for slot in std::mem::take(&mut self.output_views) {
            if slot.view == id {
                dropped.push(slot);
            } else {
                kept.push(slot);
            }
        }
        self.output_views = kept;
        for slot in &dropped {
            self.device.destroy(ResourceKind::BindGroup, slot.group.raw()).map_err(RenderError::Rhi)?;
        }
        for slot in dropped {
            self.device.destroy(ResourceKind::Buffer, slot.uniform.raw()).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn release_view_gpu(&mut self, view: RenderView) -> Result<(), RenderError> {
        if let Some(depth_view) = view.depth_view {
            self.device.destroy(ResourceKind::TextureView, depth_view.raw()).map_err(RenderError::Rhi)?;
        }
        if let Some(texture) = view.depth {
            self.device.destroy(ResourceKind::Texture, texture.raw()).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }
}

fn exposure_uniform(settings: &RenderViewSettings) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&output::exposure_multiplier(settings.exposure_ev).to_le_bytes());
    let dither = if settings.output_dither { 1.0f32 } else { 0.0 };
    bytes[4..8].copy_from_slice(&dither.to_le_bytes());
    bytes[8..12].copy_from_slice(&settings.presentation.as_f32().to_le_bytes());
    bytes
}

fn check_layout(layout: NormalizedRect) -> Result<(), RenderError> {
    let finite = layout.x.is_finite() && layout.y.is_finite() && layout.width.is_finite() && layout.height.is_finite();
    if finite && layout.x >= 0.0 && layout.y >= 0.0 && layout.width > 0.0 && layout.height > 0.0 && layout.x + layout.width <= 1.001 && layout.y + layout.height <= 1.001
    {
        Ok(())
    } else {
        Err(RenderError::Rhi(RhiError::Validation("view layout is outside the target".into())))
    }
}

/// Split parent vertices into pieces under 64 MB. This GPU rejects one buffer over 256 MB.
fn parent_vertex_groups(indices: &[u32], ranges: &[jarvig_core::ParentRange], vertex_count: usize) -> Vec<(u32, u32)> {
    const CHUNK_VERTS: u32 = ((64 * 1024 * 1024) / 60) as u32;
    let mut spans = Vec::new();
    for range in ranges {
        if range.index_count == 0 {
            continue;
        }
        let start = range.first_index as usize;
        let end = start.saturating_add(range.index_count as usize).min(indices.len());
        let Some(slice) = indices.get(start..end) else { continue };
        if slice.is_empty() {
            continue;
        }
        let min_vertex = slice.iter().copied().min().unwrap_or(0);
        let max_vertex = slice.iter().copied().max().unwrap_or(min_vertex).saturating_add(1);
        spans.push((min_vertex, max_vertex));
    }
    spans.sort_by_key(|span| span.0);
    let mut groups = Vec::new();
    let mut group_start = 0u32;
    let mut group_end = 0u32;
    let mut open = false;
    for (start, end) in spans {
        if !open {
            group_start = start;
            group_end = end;
            open = true;
            continue;
        }
        if end.saturating_sub(group_start) > CHUNK_VERTS && start > group_start {
            groups.push((group_start, group_end));
            group_start = start;
            group_end = end;
        } else {
            group_end = group_end.max(end);
        }
    }
    if open {
        groups.push((group_start, group_end));
    }
    if groups.is_empty() && vertex_count > 0 {
        let mut cursor = 0u32;
        let total = vertex_count as u32;
        while cursor < total {
            let end = cursor.saturating_add(CHUNK_VERTS).min(total);
            groups.push((cursor, end));
            cursor = end;
        }
    }
    groups
}

fn coalesce_spans(spans: &[(u32, u32)], flags: &[u32]) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut run_first = 0u32;
    let mut run_end = 0u32;
    let mut open = false;
    for (index, span) in spans.iter().enumerate() {
        let flag = flags.get(index).copied().unwrap_or(0);
        let visible = flag == 1 || flag == 4 || flag == 7;
        if !visible {
            if open {
                runs.push((run_first, run_end.saturating_sub(run_first)));
                open = false;
            }
            continue;
        }
        if open && span.0 == run_end {
            run_end = span.0.saturating_add(span.1);
        } else {
            if open {
                runs.push((run_first, run_end.saturating_sub(run_first)));
            }
            run_first = span.0;
            run_end = span.0.saturating_add(span.1);
            open = true;
        }
    }
    if open {
        runs.push((run_first, run_end.saturating_sub(run_first)));
    }
    runs
}


fn u32_bytes(values: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn pixel_rect(layout: NormalizedRect, width: u32, height: u32) -> Result<PixelRect, RenderError> {
    check_layout(layout)?;
    if width == 0 || height == 0 {
        return Err(RenderError::Rhi(RhiError::Validation("viewport is invalid".into())));
    }
    let x = (layout.x * width as f32).round().max(0.0) as u32;
    let y = (layout.y * height as f32).round().max(0.0) as u32;
    let right = ((layout.x + layout.width) * width as f32).round().max(0.0) as u32;
    let bottom = ((layout.y + layout.height) * height as f32).round().max(0.0) as u32;
    let rect_width = right.saturating_sub(x);
    let rect_height = bottom.saturating_sub(y);
    if rect_width == 0 || rect_height == 0 || x + rect_width > width || y + rect_height > height {
        return Err(RenderError::Rhi(RhiError::Validation("viewport is outside the target".into())));
    }
    Ok(PixelRect { x, y, width: rect_width, height: rect_height })
}

fn mesh_satisfies(mesh: &Mesh, compiled: &CompiledMaterial) -> bool {
    compiled.vertex.iter().all(|semantic| match semantic {
        VertexSemantic::Position => has_attribute(mesh, 0, MeshVertexFormat::Float32x3),
        VertexSemantic::Color0 => has_attribute(mesh, 1, MeshVertexFormat::Float32x3),
        VertexSemantic::TexCoord0 => has_attribute(mesh, 2, MeshVertexFormat::Float32x2),
        VertexSemantic::Normal => has_attribute(mesh, 3, MeshVertexFormat::Float32x3),
        VertexSemantic::Tangent => has_attribute(mesh, 4, MeshVertexFormat::Float32x4),
    })
}

fn map_filter(mode: jarvig_material::FilterMode) -> jarvig_rhi::FilterMode {
    match mode {
        jarvig_material::FilterMode::Nearest => jarvig_rhi::FilterMode::Nearest,
        jarvig_material::FilterMode::Linear => jarvig_rhi::FilterMode::Linear,
    }
}

fn map_address(mode: jarvig_material::AddressMode) -> jarvig_rhi::AddressMode {
    match mode {
        jarvig_material::AddressMode::ClampToEdge => jarvig_rhi::AddressMode::ClampToEdge,
        jarvig_material::AddressMode::Repeat => jarvig_rhi::AddressMode::Repeat,
    }
}

fn sampled_format(space: ColorSpace) -> TextureFormat {
    match space.rgba8_storage() {
        jarvig_material::Rgba8Storage::Srgb => TextureFormat::Rgba8UnormSrgb,
        jarvig_material::Rgba8Storage::Unorm => TextureFormat::Rgba8Unorm,
    }
}

fn has_attribute(mesh: &Mesh, location: u32, format: MeshVertexFormat) -> bool {
    mesh.streams()
        .iter()
        .flat_map(|stream| stream.attributes.iter())
        .any(|attribute| attribute.shader_location == location && attribute.format == format)
}

fn vertex_key(mesh: &Mesh) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for stream in mesh.streams() {
        hash = mix(hash, u64::from(stream.stride));
        for attribute in &stream.attributes {
            hash = mix(hash, u64::from(attribute.shader_location));
            hash = mix(hash, u64::from(attribute.offset));
            hash = mix(hash, u64::from(attribute.format.byte_size()));
        }
    }
    hash
}

fn mix(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn rhi_layout(mesh: &Mesh) -> Result<VertexBufferLayout, RenderError> {
    let stream = mesh.streams().first().ok_or(RenderError::Mesh(MeshError::Empty))?;
    let mut attributes = Vec::with_capacity(stream.attributes.len());
    for attribute in &stream.attributes {
        attributes.push(VertexAttribute {
            shader_location: attribute.shader_location,
            offset: attribute.offset as u64,
            format: match attribute.format {
                MeshVertexFormat::Float32x2 => VertexFormat::Float32x2,
                MeshVertexFormat::Float32x3 => VertexFormat::Float32x3,
                MeshVertexFormat::Float32x4 => VertexFormat::Float32x4,
            },
        });
    }
    Ok(VertexBufferLayout {
        stride: stream.stride as u64,
        step_mode: VertexStepMode::Vertex,
        attributes,
    })
}

pub fn default_clear() -> ClearColor {
    JARVIG_CLEAR
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvig_core::{
        bootstrap_pbr_textures, create_mesh, MeshDesc, MeshIndexFormat, MeshTopology, MeshVertexAttribute, MeshVertexFormat,
        Quat, RenderFrameId, ResolvedPose, SceneWorld, SubmeshDesc, TextureLibrary, Vec3, VertexStreamDesc, world_grid_phase,
    };

    fn grid_words(bytes: &[u8; 96]) -> [f32; 24] {
        let mut words = [0.0; 24];
        for (index, word) in words.iter_mut().enumerate() {
            let start = index * 4;
            *word = f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap());
        }
        words
    }

    #[test]
    fn terrain_grid_uniform_packs_strength_and_falloff() {
        let pose = ResolvedPose { translation: Vec3::new(1_000_000_000.25, 2.0, -4.5), rotation: Quat::IDENTITY };
        let desc = TerrainGridDesc {
            meshes: Vec::new(),
            minor_m: 1.0,
            major_m: 10.0,
            world: true,
            vertices: false,
            chunks: true,
            lod: false,
            lod_enabled: true,
            half_x: 256.0,
            half_z: 256.0,
            vertex_spacing: 1.0,
            chunk_m: 64.0,
            brush_x: 3.5,
            brush_z: -2.0,
            brush_radius: 4.0,
            brush_enabled: true,
            brush_strength: 0.35,
            brush_smooth: true,
            brush_color: [0.96, 0.86, 0.34, 1.0],
        };
        let words = grid_words(&super::terrain_grid_bytes(&pose, &desc));
        assert_eq!(words.len(), 24);
        assert!((words[0] - 0.25).abs() < 1.0e-4, "minor phase {}", words[0]);
        assert_eq!(words[0], world_grid_phase(pose.translation.x, 1.0));
        assert_eq!(words[4], 1.0);
        assert_eq!(words[5], 10.0);
        assert_eq!(words[6], 1.0);
        assert_eq!(words[7], 0.35);
        assert_eq!(words[16], 3.5);
        assert_eq!(words[17], -2.0);
        assert_eq!(words[18], 4.0);
        assert_eq!(words[19], 2.0);
        assert_eq!(words[23], 1.0);

        let mut linear = desc.clone();
        linear.brush_smooth = false;
        assert_eq!(grid_words(&super::terrain_grid_bytes(&pose, &linear))[19], 1.0);

        let mut off = desc.clone();
        off.brush_enabled = false;
        let hidden = grid_words(&super::terrain_grid_bytes(&pose, &off));
        assert_eq!(hidden[19], 0.0);
        assert_eq!(hidden[7], 0.35);
    }
    use jarvig_material::{ColorSpace, MaterialLibrary, ParameterValue, SamplerState};
    use jarvig_rhi::{create_null_instance, DeviceDesc, SurfaceDesc, SwapchainDesc, TextureFormat};

    fn rig() -> (SceneWorld, MaterialLibrary, TextureLibrary, Renderer, RenderTargetId, RenderViewId, RenderViewId) {
        let mut world = SceneWorld::bootstrap();
        let (textures, near_color, far_color, sampler) = bootstrap_pbr_textures();
        let orm = textures.neutral_orm().unwrap();
        let normal = textures.flat_normal().unwrap();
        let emissive = textures.white_srgb().unwrap();
        let (materials, near, far) = MaterialLibrary::bootstrap_standard(near_color, far_color, orm, normal, emissive, sampler);
        let objects: Vec<_> = world.objects().collect();
        world.bind_material(objects[0], 0, near).unwrap();
        world.bind_material(objects[1], 0, far).unwrap();
        let instance = create_null_instance();
        let adapter = instance.enumerate_adapters()[0];
        let mut device = instance
            .request_device(adapter, &DeviceDesc { label: Some("JARVIG.Device".into()) })
            .unwrap();
        let surface = device
            .create_surface(&SurfaceDesc {
                width: 32,
                height: 18,
                label: Some(Renderer::surface_label().into()),
            })
            .unwrap();
        let swapchain = device
            .create_swapchain(&SwapchainDesc {
                surface,
                format: TextureFormat::Rgba8Unorm,
                width: 32,
                height: 18,
                label: Some("JARVIG.MainSurface".into()),
            })
            .unwrap();
        let mut renderer = Renderer::new(device, swapchain, TextureFormat::Rgba8Unorm, 32, 18);
        let target = renderer.surface_target();
        let front = renderer
            .create_view(RenderViewDesc {
                label: "JARVIG.Perspective".into(),
                target,
                camera: world.front_camera(),
                layout: NormalizedRect::LEFT,
                settings: RenderViewSettings::default(),
            })
            .unwrap();
        let side = renderer
            .create_view(RenderViewDesc {
                label: "JARVIG.Alternate".into(),
                target,
                camera: world.side_camera(),
                layout: NormalizedRect::RIGHT,
                settings: RenderViewSettings { exposure_ev: 0.0, ..RenderViewSettings::default() },
            })
            .unwrap();
        (world, materials, textures, renderer, target, front, side)
    }

    fn present(
        renderer: &mut Renderer,
        world: &SceneWorld,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
        target: RenderTargetId,
        frame: u64,
    ) -> FrameOutcome {
        let snapshot = world.extract(RenderFrameId(frame)).unwrap();
        renderer.render_target(target, &snapshot, world.meshes(), materials, textures).unwrap()
    }

    #[test]
    fn two_views_share_one_snapshot_and_one_present() {
        let (world, materials, textures, mut renderer, target, front, side) = rig();
        assert_eq!(renderer.views_for(target), vec![front, side]);
        assert_eq!(renderer.target_kind(target).unwrap(), RenderTargetKind::Surface);
        assert_eq!(present(&mut renderer, &world, &materials, &textures, target, 1), FrameOutcome::Presented { frame: 1 });
        assert_eq!(present(&mut renderer, &world, &materials, &textures, target, 1), FrameOutcome::Presented { frame: 2 });
        assert_eq!(renderer.pipeline_count(), 7);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.tone_map_pipeline_count(), 1);
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(materials.master_count(), 1);
        assert_eq!(materials.instance_count(), 2);
        assert_eq!(renderer.material_parameter_upload_count(), 2);
        assert_eq!(renderer.buffer_count(), 29);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(world.mesh_count(), 2);
        assert_eq!(renderer.submitted_last(), 2);
        assert_eq!(renderer.view_count(), 2);
        assert_eq!(renderer.target_count(), 1);
        assert_eq!(renderer.indexed_draw_count(), 58);
        assert_eq!(renderer.acquires_last_frame(), 1);
        assert_eq!(renderer.presents_last_frame(), 1);
        assert_eq!(renderer.presented_frames(), 2);
        assert_eq!(renderer.resource_stats().alive_buffers, 24);
        assert_eq!(renderer.resource_stats().alive_layouts, 5);
        assert_eq!(renderer.resource_stats().alive_bind_groups, 12);
        assert_eq!(renderer.gpu_light_packet_count(), 2);
        assert_eq!(renderer.light_buffer_upload_count(), 4);
        assert_eq!(renderer.environment_packet_upload_count(), 1);
        assert_eq!(renderer.reflection_probe_count(), 1);
        assert_eq!(renderer.reflection_probe_active_count(), 1);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        assert_eq!(renderer.reflection_probe_selected_far(), 1);
        assert_eq!(renderer.reflection_probe_fallback_count(), 0);
        assert_eq!(renderer.reflection_probe_resolution(), 32);
        assert_eq!(renderer.reflection_probe_mip_count(), 6);
        assert_eq!(renderer.reflection_probe_texture_count(), 1);
        assert_eq!(renderer.material_cull(), Some(jarvig_material::CullMode::None));
        assert_eq!(renderer.material_two_sided(), Some(true));
        assert_eq!(renderer.resource_stats().alive_pipelines, 4);
        assert_eq!(renderer.resource_stats().alive_textures, 19);
        assert_eq!(renderer.hdr_scene_target_count(), 1);
        assert_eq!(renderer.hdr_scene_size(), Some((32, 18)));
        assert_eq!(renderer.output_pass_count(), 2);
        assert_eq!(renderer.texture_upload_count(), 5);
        assert_eq!(renderer.gpu_sampler_count(), 1);
        assert_eq!(sampled_format(ColorSpace::Srgb), TextureFormat::Rgba8UnormSrgb);
        assert_eq!(sampled_format(ColorSpace::Linear), TextureFormat::Rgba8Unorm);
        assert!(snapshot_has_logical_material(&world));
        assert_eq!(renderer.viewport(front).unwrap(), Some(PixelRect { x: 0, y: 0, width: 16, height: 18 }));
        assert_eq!(renderer.viewport(side).unwrap(), Some(PixelRect { x: 16, y: 0, width: 16, height: 18 }));
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        let instance = &snapshot.instances()[0];
        let front_pose = snapshot.camera(world.front_camera().frame).unwrap().pose;
        let side_pose = snapshot.camera(world.side_camera().frame).unwrap().pose;
        let front_delta = instance_gpu_transforms(instance, &front_pose, world.front_camera().vertical_fov_radians, world.front_camera().near_m, 1.0).unwrap().model.cols[3];
        let side_delta = instance_gpu_transforms(instance, &side_pose, world.side_camera().vertical_fov_radians, world.side_camera().near_m, 1.0).unwrap().model.cols[3];
        assert_ne!(front_delta, side_delta);
        assert!(front_delta[0].abs() < 20.0 && front_delta[2].abs() < 20.0);
        assert!(side_delta[0].abs() < 20.0 && side_delta[2].abs() < 20.0);
        assert_eq!(present(&mut renderer, &world, &materials, &textures, target, 1), FrameOutcome::Presented { frame: 3 });
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(renderer.material_parameter_upload_count(), 2);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.texture_upload_count(), 5);
        assert_eq!(renderer.buffer_count(), 29);
        assert_eq!(renderer.light_buffer_upload_count(), 4);
        assert_eq!(renderer.draw_count(), 138);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        assert_eq!(renderer.presented_frames(), 3);
    }

    #[test]
    fn lighting_debug_isolates_a_view_without_revising_the_world_or_rebuilding_meshes() {
        let (world, materials, textures, mut renderer, target, front, side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        let revision = world.revision();
        let compiles = materials.compile_count();
        let meshes = renderer.mesh_upload_count();
        let textures_uploaded = renderer.texture_upload_count();
        let pipelines = renderer.material_pipeline_count();
        assert_eq!(renderer.lighting_debug(front).unwrap().gpu_mask(), 0);
        assert_eq!(LightingDebug::default().gpu_mask(), 0);
        assert_eq!(LightingDebug::probe_specular_only().gpu_mask() & LIGHTING_DEBUG_PROBE, LIGHTING_DEBUG_PROBE);
        assert_eq!(LightingDebug::probe_specular_only().gpu_mask() & LIGHTING_DEBUG_ENV_SPECULAR, 0);
        assert_ne!(LightingDebug::directional_only().gpu_mask(), 0);
        for debug in [
            LightingDebug::direct_only(),
            LightingDebug::environment_diffuse_only(),
            LightingDebug::environment_specular_only(),
            LightingDebug::probe_specular_only(),
            LightingDebug::emissive_only(),
            LightingDebug::directional_only(),
            LightingDebug::point_only(),
            LightingDebug::spot_only(),
            LightingDebug::indirect_diffuse_only(),
            LightingDebug::default(),
        ] {
            renderer
                .update_view(front, RenderViewUpdate {
                    camera: None,
                    layout: None,
                    settings: Some(RenderViewSettings { exposure_ev: 0.0, lighting: debug, ..RenderViewSettings::default() }),
                    pose: None,
                })
                .unwrap();
            present(&mut renderer, &world, &materials, &textures, target, 2);
            assert_eq!(world.revision(), revision);
            assert_eq!(world.light_count(), 3);
            assert!(world.environment().enabled);
            assert_eq!(world.reflection_probe_count(), 1);
            assert!(world.reflection_probe_enabled());
            assert_eq!(materials.compile_count(), compiles);
            assert_eq!(renderer.mesh_upload_count(), meshes);
            assert_eq!(renderer.texture_upload_count(), textures_uploaded);
            assert_eq!(renderer.material_pipeline_count(), pipelines);
            assert_eq!(renderer.lighting_debug(front).unwrap(), debug);
            assert_eq!(renderer.lighting_debug(side).unwrap(), LightingDebug::default());
            assert_eq!(renderer.reflection_probe_capture_count(), 1);
        }
    }

    #[test]
    fn a_duplicated_probe_gets_its_own_cube_and_delete_retires_that_cube() {
        let (mut world, materials, textures, mut renderer, target, _front, _side) = rig();
        let compiles = materials.compile_count();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.reflection_probe_texture_count(), 1);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        let original = renderer.reflection_probe_resident_ids();
        assert_eq!(original.len(), 1);
        let probe = world.entity_outline().iter().find(|row| row.name == "Reflection Probe").unwrap().uuid;
        let copy = world.duplicate_authored(probe).unwrap();
        assert_eq!(renderer.reflection_probe_resident_ids(), original);
        present(&mut renderer, &world, &materials, &textures, target, 2);
        let residents = renderer.reflection_probe_resident_textures();
        assert_eq!(residents.len(), 2);
        assert_ne!((residents[0].1, residents[0].2), (residents[1].1, residents[1].2));
        assert_eq!(renderer.reflection_probe_capture_count(), 2);
        assert_eq!(materials.compile_count(), compiles);
        world.destroy_authored(probe).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 3);
        assert_eq!(renderer.reflection_probe_texture_count(), 1);
        assert_eq!(renderer.reflection_probe_resident_ids(), vec![world.entity_ownership(copy).unwrap().probe.unwrap()]);
        assert!(renderer.reflection_probe_resident_textures().iter().all(|resident| resident.0 != original[0]));
        world.destroy_authored(copy).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 4);
        assert_eq!(renderer.reflection_probe_texture_count(), 0);
        assert!(renderer.reflection_probe_resident_ids().is_empty());
        assert_eq!(materials.compile_count(), compiles);
    }

    #[test]
    fn exposure_does_not_rebuild_scene_resources_or_the_world() {
        let (world, materials, textures, mut renderer, target, front, _side) = rig();
        let revision = world.revision();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        let meshes = renderer.mesh_upload_count();
        let compiles = materials.compile_count();
        let textures_uploaded = renderer.texture_upload_count();
        let pipelines = renderer.tone_map_pipeline_count();
        let parameters = renderer.tone_map_parameter_upload_count();
        assert_eq!(pipelines, 1);
        assert_eq!(renderer.exposure_ev(front).unwrap(), 0.0);
        for ev in [2.0_f32, -2.0, 0.0] {
            renderer
                .update_view(front, RenderViewUpdate {
                    camera: None,
                    layout: None,
                    settings: Some(RenderViewSettings { exposure_ev: ev, ..RenderViewSettings::default() }),
                    pose: None,
                })
                .unwrap();
            present(&mut renderer, &world, &materials, &textures, target, 2);
            assert_eq!(world.revision(), revision);
            assert_eq!(renderer.mesh_upload_count(), meshes);
            assert_eq!(materials.compile_count(), compiles);
            assert_eq!(renderer.texture_upload_count(), textures_uploaded);
            assert_eq!(renderer.tone_map_pipeline_count(), pipelines);
            assert!(renderer.tone_map_parameter_upload_count() > parameters);
            assert!((renderer.exposure_ev(front).unwrap() - ev).abs() < 1.0e-6);
        }
    }

    #[test]
    fn evicting_a_gpu_mesh_keeps_the_logical_mesh() {
        let (world, materials, textures, mut renderer, target, _front, _side) = rig();
        assert_eq!(present(&mut renderer, &world, &materials, &textures, target, 1), FrameOutcome::Presented { frame: 1 });
        let far = world.extract(RenderFrameId(1)).unwrap().instances()[1].mesh;
        let stale = match &renderer.gpu_meshes.iter().find(|(id, _)| *id == far).unwrap().1 {
            GpuResidency::Resident(gpu) => gpu.vertices,
            GpuResidency::Evicted => panic!("far mesh should be resident"),
        };
        assert_eq!(renderer.resource_stats().alive_buffers, 24);
        renderer.evict_gpu_mesh(far).unwrap();
        assert_eq!(world.mesh_count(), 2);
        assert_eq!(renderer.resource_stats().alive_buffers, 22);
        assert!(matches!(renderer.device.write_buffer(stale, 0, &[0; 4]), Err(RhiError::InvalidHandle(_))));
        assert_eq!(present(&mut renderer, &world, &materials, &textures, target, 2), FrameOutcome::Presented { frame: 2 });
        assert_eq!(world.mesh_count(), 2);
        assert_eq!(renderer.mesh_upload_count(), 3);
        assert_eq!(renderer.resource_stats().alive_buffers, 24);
        assert!(matches!(renderer.device.write_buffer(stale, 0, &[0; 4]), Err(RhiError::InvalidHandle(_))));
    }

    #[test]
    fn a_recycled_mesh_id_does_not_keep_the_previous_worlds_buffers() {
        let (world, materials, textures, mut renderer, target, front, side) = rig();
        assert_eq!(present(&mut renderer, &world, &materials, &textures, target, 1), FrameOutcome::Presented { frame: 1 });
        let city = world.extract(RenderFrameId(1)).unwrap().instances()[0].mesh;
        assert_eq!(city.0, 1);
        let (stale, city_indices) = match &renderer.gpu_meshes.iter().find(|(id, _)| *id == city).unwrap().1 {
            GpuResidency::Resident(gpu) => (gpu.vertices, gpu.submeshes[0].index_count),
            GpuResidency::Evicted => panic!("first mesh should be resident"),
        };
        assert_eq!(city_indices, 3);
        let city_records = vec![
            jarvig_core::GpuMeshletRecord {
                center: [0.0; 3],
                radius: 1.0,
                bounds_min: [-1.0; 3],
                bounds_max: [1.0; 3],
                cone_axis: [0.0, 1.0, 0.0],
                cone_cutoff: 1.0,
                triangles: 64,
                vertices: 64,
                submesh: 0,
            };
            1044
        ];
        renderer.remember_meshlets(city, &city_records);
        assert_eq!(renderer.gpu_scene.remembered.iter().find(|(id, _)| *id == city).unwrap().1.len(), 1044);
        let pipelines = renderer.resource_stats().alive_pipelines;
        let uploads = renderer.mesh_upload_count();
        renderer.release_world_scene().unwrap();
        assert!(renderer.gpu_meshes.is_empty());
        assert!(renderer.gpu_scene.remembered.is_empty());
        assert!(renderer.parent_gpu.is_none());
        assert!(renderer.meshlet_gpu.is_none());
        assert!(renderer.entity_hidden.is_empty());
        assert!(renderer.views.iter().all(|slot| slot.view.as_ref().is_none_or(|view| view.cluster_flags.is_empty() && view.parent_indices.is_empty())));
        assert_eq!(renderer.resource_stats().alive_pipelines, pipelines);
        assert_eq!(renderer.gpu_scene_stats().meshlets, 0);
        assert_eq!(renderer.gpu_scene_stats().instances, 0);
        assert!(matches!(renderer.device.write_buffer(stale, 0, &[0; 4]), Err(RhiError::InvalidHandle(_))));

        let mut fresh = SceneWorld::new_session();
        let block = fresh
            .create_block(Vec3::new(0.0, 1.0, -4.0), jarvig_core::BlockRecord::standard([2.0, 2.0, 2.0]).unwrap())
            .unwrap();
        assert_eq!(fresh.block_count(), 1);
        assert_eq!(fresh.object_count(), 1);
        let near = world.material_instance(world.objects().next().unwrap(), 0).unwrap();
        let block_object = fresh.objects().next().unwrap();
        fresh.bind_material(block_object, 0, near).unwrap();
        let snapshot = fresh.extract(RenderFrameId(2)).unwrap();
        assert_eq!(snapshot.instances().len(), 1);
        assert_eq!(snapshot.instances()[0].mesh, city);
        assert_eq!(snapshot.instances()[0].entity, block);
        let mesh = fresh.meshes().get(city).unwrap();
        assert_eq!(mesh.vertex_count(), 24);
        assert_eq!(mesh.index_count(), 36);
        renderer
            .update_view(front, RenderViewUpdate { camera: Some(fresh.front_camera()), layout: None, settings: None, pose: None })
            .unwrap();
        renderer
            .update_view(side, RenderViewUpdate { camera: Some(fresh.side_camera()), layout: None, settings: None, pose: None })
            .unwrap();
        let derived = fresh.derived_meshlets(city).expect("the block builds clusters from its surface");
        let records: Vec<jarvig_core::GpuMeshletRecord> = derived.meshlets.iter().map(jarvig_core::GpuMeshletRecord::from_meshlet).collect();
        assert!(!records.is_empty());
        assert_ne!(records.len(), 1044);
        assert_eq!(records.iter().map(|record| record.triangles).sum::<u32>(), 12);
        renderer.remember_meshlets(city, &records);
        assert_eq!(present(&mut renderer, &fresh, &materials, &textures, target, 2), FrameOutcome::Presented { frame: 2 });
        assert!(renderer.mesh_upload_count() > uploads);
        let resident = match &renderer.gpu_meshes.iter().find(|(id, _)| *id == city).unwrap().1 {
            GpuResidency::Resident(gpu) => gpu,
            GpuResidency::Evicted => panic!("block mesh should be resident"),
        };
        assert_ne!(resident.vertices, stale);
        assert_eq!(resident.submeshes[0].index_count, 36);
        assert_eq!(renderer.gpu_scene.remembered.iter().find(|(id, _)| *id == city).unwrap().1.len(), records.len());
        let stats = renderer.gpu_scene_stats();
        assert_eq!(stats.instances, 1);
        assert_eq!(stats.geometries, 1);
        assert_eq!(stats.meshlets, records.len() as u32);
        assert_ne!(stats.meshlets, 1044);
        assert_eq!(stats.legacy_draw_triangles, 24);
        assert!(matches!(renderer.device.write_buffer(stale, 0, &[0; 4]), Err(RhiError::InvalidHandle(_))));

        let draw = jarvig_core::meshlet_draw(fresh.derived_meshlets(city).unwrap());
        renderer
            .set_meshlet_debug(Some(MeshletDebugBatch {
                mesh: city,
                indices: draw.indices,
                colors: draw.colors,
                ranges: draw.ranges.into_iter().map(|range| (range.first_index, range.index_count)).collect(),
                spans: draw.spans.into_iter().map(|range| (range.first_index, range.index_count)).collect(),
                span_source: draw.span_source,
                owners: draw.owners,
                show_ids: false,
                shade_clustered: true,
            }))
            .unwrap();
        renderer.set_microgeometry(true, 1, false);
        renderer.set_micro_compatible(true);
        renderer.set_micro_debug(jarvig_core::MicroDebugMode::Reject);
        assert_eq!(present(&mut renderer, &fresh, &materials, &textures, target, 3), FrameOutcome::Presented { frame: 3 });
        let drawn = renderer.gpu_scene_stats();
        assert_eq!(drawn.legacy_draw_triangles, 0);
        let front_flags = renderer.slot(front).unwrap().cluster_flags.clone();
        let side_flags = renderer.slot(side).unwrap().cluster_flags.clone();
        assert_eq!(front_flags.len(), records.len());
        assert_eq!(side_flags.len(), records.len());
        // The front camera frames the whole solid, so every derived cluster is submitted.
        // The side camera's half of the 32x18 target sees two triangles. Each cluster is one triangle.
        assert!(front_flags.iter().all(|flag| *flag == 1 || *flag == 4));
        let side_visible = side_flags.iter().filter(|flag| **flag == 1 || **flag == 4).count();
        assert_eq!(side_visible, 2);
        assert_eq!(drawn.meshlet_draw_triangles, (records.len() + side_visible) as u32);
        assert_eq!(drawn.meshlets, records.len() as u32);
        // The last view is the side camera. Every cluster is classified. The two on-screen
        // triangles project the 2 cm feature under 1 px, so Einstein builds no patches.
        assert_eq!(renderer.micro_reasons.len(), side_flags.len());
        for (reason, flag) in renderer.micro_reasons.iter().zip(side_flags.iter()) {
            match *flag {
                1 | 4 => assert_eq!(*reason, jarvig_core::DetailReject::BelowThreshold),
                3 => assert_eq!(*reason, jarvig_core::DetailReject::Occluded),
                _ => assert_eq!(*reason, jarvig_core::DetailReject::OffScreen),
            }
        }
        assert_eq!(drawn.micro_triangles, 0);
        assert!(renderer.gpu_scene.hierarchy_nodes.as_ref().is_some_and(|hierarchy| hierarchy.leaf_count as usize == records.len()));
    }

    #[test]
    fn resize_retires_depth_without_dropping_meshes_or_view_ids() {
        let (world, materials, textures, mut renderer, target, front, side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.resource_stats().alive_textures, 19);
        let before_depth = renderer.depth_target_count();
        renderer.resize(32, 18).unwrap();
        assert_eq!(renderer.depth_target_count(), before_depth);
        assert_eq!(renderer.configured_size(), (32, 18));
        renderer.resize(0, 0).unwrap();
        let stats = renderer.resource_stats();
        assert_eq!(renderer.hdr_scene_target_count(), 0);
        assert_eq!(stats.alive_textures, 15);
        assert_eq!(stats.alive_texture_views, 21);
        assert_eq!(stats.retired, 0);
        assert_eq!(world.mesh_count(), 2);
        assert_eq!(renderer.view_count(), 2);
        assert!(renderer.depth_size(front).unwrap().is_none());
        assert!(renderer.configured_aspect().is_none());
        renderer.resize(32, 18).unwrap();
        assert_eq!(renderer.resource_stats().alive_textures, 19);
        assert_eq!(renderer.hdr_scene_size(), Some((32, 18)));
        assert_eq!(renderer.depth_size(front).unwrap(), Some((32, 18)));
        assert_eq!(renderer.depth_size(side).unwrap(), Some((32, 18)));
        assert!((renderer.configured_aspect().unwrap() - (32.0 / 18.0)).abs() < 0.001);
        assert_eq!(present(&mut renderer, &world, &materials, &textures, target, 2), FrameOutcome::Presented { frame: 2 });
        assert_eq!(renderer.view_count(), 2);
    }

    #[test]
    fn a_view_pose_moves_lights_without_revising_the_world() {
        use jarvig_core::{render_light_record, LightKind, ResolvedPose, Vec3};
        let (world, materials, textures, mut renderer, target, front, side) = rig();
        let revision = world.revision();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        let home = snapshot.camera(world.front_camera().frame).unwrap().pose;
        let meshes = renderer.mesh_upload_count();
        let compiles = materials.compile_count();
        let lights_before = renderer.light_buffer_upload_count();
        let environment_before = renderer.environment_packet_upload_count();
        let moved = ResolvedPose {
            translation: Vec3::new(home.translation.x + 3.0, home.translation.y + 0.01, home.translation.z),
            rotation: home.rotation,
        };
        renderer
            .update_view(front, RenderViewUpdate { camera: None, layout: None, settings: None, pose: Some(moved) })
            .unwrap();
        assert_eq!(renderer.view_pose(front).unwrap().unwrap().translation.x, moved.translation.x);
        assert!(renderer.view_pose(side).unwrap().is_none());
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(world.revision(), revision);
        assert_eq!(renderer.mesh_upload_count(), meshes);
        assert_eq!(materials.compile_count(), compiles);
        assert!(renderer.light_buffer_upload_count() > lights_before);
        assert_eq!(renderer.environment_packet_upload_count(), environment_before);
        let point = snapshot.lights().iter().find(|light| light.kind == LightKind::Point).unwrap();
        let before = render_light_record(point, &home);
        let after = render_light_record(point, &moved);
        assert_ne!(before.position, after.position);
        assert!(after.position.iter().all(|component| component.is_finite() && component.abs() < 100.0));
        assert!((point.pose.translation.x - world.extract(RenderFrameId(3)).unwrap().lights().iter().find(|light| light.id == point.id).unwrap().pose.translation.x).abs() < 1.0e-9);
        assert_eq!(renderer.shadow_update_count(), 1);
    }

    #[test]
    fn shadows_redraw_for_the_world_and_not_for_the_camera() {
        use jarvig_core::{LightKind, Quat, Vec3};
        let (mut world, materials, textures, mut renderer, target, front, _side) = rig();
        let compiles = materials.compile_count();
        let objects: Vec<_> = world.objects().collect();
        let near = objects[0];
        let far = objects[1];
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.shadow_map_count(), 3);
        assert_eq!(renderer.shadow_update_count(), 1);
        assert_eq!(renderer.shadow_cull(), Some(jarvig_rhi::CullMode::None));
        assert_eq!(renderer.shadow_diagnostics().directional_resolution, 1024);
        assert_eq!(renderer.shadow_diagnostics().point_resolution, 256);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(materials.compile_count(), compiles);
        let home = renderer.shadow_record_bytes(front).unwrap().to_vec();
        assert!(home.len() >= 112);
        let translation = f32::from_le_bytes(home[48..52].try_into().unwrap());
        assert!(translation.is_finite() && translation.abs() < 1.0e5, "{translation}");
        let pose = world.extract(RenderFrameId(2)).unwrap().camera(world.front_camera().frame).unwrap().pose;
        let moved = jarvig_core::ResolvedPose {
            translation: Vec3::new(pose.translation.x + 4.0, pose.translation.y, pose.translation.z - 1.0),
            rotation: pose.rotation,
        };
        renderer
            .update_view(front, RenderViewUpdate { camera: None, layout: None, settings: None, pose: Some(moved) })
            .unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(renderer.shadow_update_count(), 1);
        assert_ne!(renderer.shadow_record_bytes(front).unwrap(), home.as_slice());
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(materials.compile_count(), compiles);
        world.set_object_local_translation(near, Vec3::new(0.35, 0.0, -2.0)).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 3);
        assert_eq!(renderer.shadow_update_count(), 2);
        world.set_object_local_translation(far, Vec3::new(0.2, 0.0, -5.4)).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 4);
        assert_eq!(renderer.shadow_update_count(), 3);
        let point = world.extract(RenderFrameId(5)).unwrap().lights().iter().find(|light| light.kind == LightKind::Point).unwrap().id;
        world.set_light_translation(point, Vec3::new(1.25, 0.7, -1.4)).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 5);
        assert_eq!(renderer.shadow_update_count(), 4);
        world.set_light_translation(point, Vec3::new(1.25, 0.7, -0.8)).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 6);
        assert_eq!(renderer.shadow_update_count(), 5);
        let entity = world.entity(near).unwrap();
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.35).unwrap();
        world.set_entity_local_rotation(entity, yaw).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 7);
        assert_eq!(renderer.shadow_update_count(), 6);
        world.set_entity_local_rotation(entity, Quat::IDENTITY).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 8);
        assert_eq!(renderer.shadow_update_count(), 7);
        world.set_object_local_translation(near, Vec3::new(0.0, 0.0, -2.0)).unwrap();
        world.set_object_local_translation(far, Vec3::new(0.0, 0.0, -5.0)).unwrap();
        let updates = renderer.shadow_update_count();
        renderer.resize(40, 22).unwrap();
        assert_eq!(renderer.shadow_map_count(), 3);
        assert_eq!(renderer.shadow_update_count(), updates);
        present(&mut renderer, &world, &materials, &textures, target, 9);
        assert!(renderer.shadow_update_count() > updates);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(materials.compile_count(), compiles);
        assert_eq!(renderer.material_pipeline_count(), 1);
        renderer
            .update_view(
                front,
                RenderViewUpdate {
                    camera: None,
                    layout: None,
                    settings: Some(RenderViewSettings { exposure_ev: 0.0, lighting: LightingDebug::shadows_disabled(), ..RenderViewSettings::default() }),
                    pose: None,
                },
            )
            .unwrap();
        let before = renderer.shadow_update_count();
        present(&mut renderer, &world, &materials, &textures, target, 10);
        assert_eq!(renderer.shadow_update_count(), before);
        assert_eq!(renderer.lighting_debug(front).unwrap().label(), "No Shadows");
        assert_eq!(LightingDebug::shadows_disabled().gpu_mask() & LIGHTING_DEBUG_SHADOWS, 0);
        assert_ne!(LightingDebug::directional_only().gpu_mask() & LIGHTING_DEBUG_SHADOWS, 0);
        assert_eq!(LightingDebug::default().gpu_mask(), 0);
    }

    #[test]
    fn destroyed_view_id_is_stale_and_does_not_drop_the_world() {
        let (world, materials, textures, mut renderer, target, front, side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        let generation = front.generation;
        renderer.destroy_view(front).unwrap();
        assert!(renderer.viewport(front).is_err());
        assert!(renderer.destroy_view(front).is_err());
        assert_eq!(world.mesh_count(), 2);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(renderer.view_count(), 1);
        assert_eq!(renderer.resource_stats().alive_buffers, 17);
        assert_eq!(renderer.gpu_light_packet_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.resource_stats().alive_textures, 18);
        let replacement = renderer
            .create_view(RenderViewDesc {
                label: "JARVIG.Perspective".into(),
                target,
                camera: world.front_camera(),
                layout: NormalizedRect::LEFT,
                settings: RenderViewSettings::default(),
            })
            .unwrap();
        assert_eq!(replacement.index, front.index);
        assert_ne!(replacement.generation, generation);
        assert!(renderer.update_view(front, RenderViewUpdate { camera: None, layout: None, settings: None, pose: None }).is_err());
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.views_for(target), vec![replacement, side]);
        assert_eq!(renderer.acquires_last_frame(), 1);
        assert_eq!(renderer.presents_last_frame(), 1);
    }

    #[test]
    fn shutdown_drops_gpu_objects_and_keeps_logical_meshes() {
        let (world, materials, textures, mut renderer, target, _front, _side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        renderer.shutdown().unwrap();
        assert_eq!(world.mesh_count(), 2);
        assert_eq!(materials.master_count(), 1);
        assert_eq!(materials.instance_count(), 2);
        let stats = renderer.resource_stats();
        assert_eq!(stats.alive_buffers, 0);
        assert_eq!(stats.alive_pipelines, 0);
        assert_eq!(stats.alive_textures, 0);
        assert_eq!(stats.alive_texture_views, 0);
        assert_eq!(stats.alive_bind_groups, 0);
        assert_eq!(stats.alive_shaders, 0);
        assert_eq!(stats.alive_layouts, 0);
        assert_eq!(stats.retired, 0);
        let snapshot = world.extract(RenderFrameId(2)).unwrap();
        assert!(renderer.render_target(target, &snapshot, world.meshes(), &materials, &textures).is_err());
    }

    #[test]
    fn zero_size_does_not_panic() {
        let (world, materials, textures, mut renderer, target, _front, _side) = rig();
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        renderer.resize(0, 0).unwrap();
        assert_eq!(renderer.render_target(target, &snapshot, world.meshes(), &materials, &textures).unwrap(), FrameOutcome::Minimized);
        assert_eq!(renderer.acquires_last_frame(), 0);
        assert_eq!(renderer.buffer_count(), 0);
        assert_eq!(renderer.depth_target_count(), 0);
        assert_eq!(renderer.view_count(), 2);
        renderer.resize(32, 18).unwrap();
        assert!(matches!(present(&mut renderer, &world, &materials, &textures, target, 1), FrameOutcome::Presented { .. }));
    }

    #[test]
    fn hiding_one_instance_drops_a_submission_without_dropping_the_mesh() {
        let (mut world, materials, textures, mut renderer, target, _front, _side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.submitted_last(), 2);
        let far = world.objects().nth(1).unwrap();
        world.set_visible(far, false).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(renderer.submitted_last(), 1);
        assert_eq!(renderer.indexed_draw_count(), 69);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(world.mesh_count(), 2);
        assert_eq!(materials.instance_count(), 2);
    }

    #[test]
    fn changing_a_light_does_not_recompile_or_upload_geometry() {
        let (mut world, materials, textures, mut renderer, target, _front, _side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.gpu_light_packet_count(), 2);
        assert_eq!(renderer.light_buffer_upload_count(), 4);
        assert_eq!(world.light_count(), 3);
        let point = world.light_ids().nth(1).unwrap();
        world.set_light_intensity(point, 40.0).unwrap();
        world.set_light_color(point, [0.1, 0.2, 1.0]).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(renderer.texture_upload_count(), 5);
        assert_eq!(renderer.light_buffer_upload_count(), 6);
        assert_eq!(renderer.gpu_light_packet_count(), 2);
        assert_eq!(renderer.buffer_count(), 29);
        assert_eq!(renderer.environment_packet_upload_count(), 1);
    }

    #[test]
    fn changing_the_environment_uploads_one_packet_and_does_not_rebuild() {
        let (mut world, materials, textures, mut renderer, target, front, _side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        let compiles = materials.compile_count();
        let meshes = renderer.mesh_upload_count();
        let textures_uploaded = renderer.texture_upload_count();
        let pipelines = renderer.tone_map_pipeline_count();
        let lights = renderer.light_buffer_upload_count();
        let buffers = renderer.buffer_count();
        let revision = world.revision();
        assert_eq!(renderer.environment_packet_upload_count(), 1);
        world.set_environment_intensity(0.05).unwrap();
        world.set_environment_upper([0.2, 0.3, 0.4]).unwrap();
        world.set_environment_lower([0.05, 0.04, 0.03]).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert!(world.revision() > revision);
        assert_eq!(materials.compile_count(), compiles);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.mesh_upload_count(), meshes);
        assert_eq!(renderer.texture_upload_count(), textures_uploaded);
        assert_eq!(renderer.tone_map_pipeline_count(), pipelines);
        assert_eq!(renderer.light_buffer_upload_count(), lights);
        assert_eq!(renderer.buffer_count(), buffers);
        assert_eq!(renderer.environment_packet_upload_count(), 2);
        world.set_environment_enabled(false).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 3);
        assert_eq!(renderer.environment_packet_upload_count(), 3);
        assert_eq!(world.light_count(), 3);
        assert_eq!(world.extract(RenderFrameId(4)).unwrap().light_count(), 3);
        let exposure_revision = world.revision();
        renderer
            .update_view(front, RenderViewUpdate {
                camera: None,
                layout: None,
                settings: Some(RenderViewSettings { exposure_ev: 1.0, ..RenderViewSettings::default() }),
                pose: None,
            })
            .unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 4);
        assert_eq!(world.revision(), exposure_revision);
        assert_eq!(renderer.environment_packet_upload_count(), 3);
        assert_eq!(renderer.tone_map_pipeline_count(), pipelines);
        assert_eq!(materials.compile_count(), compiles);
    }

    #[test]
    fn changing_tint_updates_one_instance_and_does_not_recompile() {
        let (world, mut materials, textures, mut renderer, target, _front, _side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.pipeline_count(), 7);
        assert_eq!(renderer.material_parameter_upload_count(), 2);
        assert_eq!(renderer.mesh_upload_count(), 2);
        let far = world.extract(RenderFrameId(1)).unwrap().instances()[1].material_for_slot(0).unwrap();
        let before = materials.parameter_bytes(far).unwrap().to_vec();
        materials.set_parameter(far, "RoughnessFactor", ParameterValue::Float(0.35)).unwrap();
        assert_ne!(materials.parameter_bytes(far).unwrap(), before.as_slice());
        assert_eq!(materials.compile_count(), 1);
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.pipeline_count(), 7);
        assert_eq!(renderer.material_parameter_upload_count(), 3);
        assert_eq!(renderer.mesh_upload_count(), 2);
        present(&mut renderer, &world, &materials, &textures, target, 3);
        assert_eq!(renderer.material_parameter_upload_count(), 3);
        assert_eq!(renderer.mesh_upload_count(), 2);
    }

    #[test]
    fn an_unbound_slot_is_skipped_and_a_bad_binding_is_not_drawn() {
        let (mut world, materials, textures, mut renderer, target, _front, _side) = rig();
        let far = world.objects().nth(1).unwrap();
        world.clear_material_slot(far, 0).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.indexed_draw_count(), 42);
        assert_eq!(renderer.unbound_material_skip_count(), 2);
        assert_eq!(renderer.material_pipeline_count(), 1);
        let near = world.objects().next().unwrap();
        world.bind_material(near, 0, jarvig_core::MaterialInstanceId(99)).unwrap();
        let snapshot = world.extract(RenderFrameId(2)).unwrap();
        let drawn = renderer.indexed_draw_count();
        let error = renderer.render_target(target, &snapshot, world.meshes(), &materials, &textures).unwrap_err();
        assert!(matches!(error, RenderError::UnknownMaterial));
        // The bad binding fails the view. Light-space maps still refresh because the world revision changed.
        assert_eq!(renderer.indexed_draw_count(), drawn + 30);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(materials.compile_count(), 1);
    }

    #[test]
    fn a_mesh_without_color_is_rejected() {
        let (mut world, materials, textures, mut renderer, target, _front, _side) = rig();
        let mesh = world.add_mesh(position_only_triangle());
        let near = world.objects().next().unwrap();
        world.set_object_mesh(near, mesh).unwrap();
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        let error = renderer.render_target(target, &snapshot, world.meshes(), &materials, &textures).unwrap_err();
        assert!(matches!(error, RenderError::IncompatibleMaterial));
        assert_eq!(renderer.material_pipeline_count(), 0);
        assert_eq!(renderer.pipeline_count(), 2);
        assert_eq!(renderer.indexed_draw_count(), 30);
        assert_eq!(materials.compile_count(), 1);
    }

    #[test]
    fn changing_a_texture_does_not_recompile_or_upload_the_mesh() {
        let (world, mut materials, mut textures, mut renderer, target, _front, _side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.texture_upload_count(), 5);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(renderer.gpu_sampler_count(), 1);
        let near = world.extract(RenderFrameId(1)).unwrap().instances()[0].material_for_slot(0).unwrap();
        let replacement = textures.insert(jarvig_core::checkerboard(
            ColorSpace::Srgb,
            [255, 255, 255, 255],
            [0, 0, 0, 255],
        ));
        materials.set_texture(near, "BaseColor", replacement).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.pipeline_count(), 7);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(renderer.texture_upload_count(), 6);
        assert_eq!(renderer.material_texture_binding_updates(), 1);
        assert_eq!(renderer.gpu_sampler_count(), 1);
        let clamp = textures.insert_sampler(SamplerState::linear_clamp());
        materials.set_sampler(near, "MaterialSampler", clamp).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 3);
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(renderer.gpu_sampler_count(), 2);
        assert_eq!(renderer.material_texture_binding_updates(), 2);
        assert_eq!(renderer.texture_upload_count(), 6);
    }

    #[test]
    fn a_mesh_without_texcoords_is_rejected() {
        let (mut world, materials, textures, mut renderer, target, _front, _side) = rig();
        let mesh = world.add_mesh(color_without_uv());
        let near = world.objects().next().unwrap();
        world.set_object_mesh(near, mesh).unwrap();
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        let error = renderer.render_target(target, &snapshot, world.meshes(), &materials, &textures).unwrap_err();
        assert!(matches!(error, RenderError::IncompatibleMaterial));
        assert_eq!(renderer.material_pipeline_count(), 0);
        assert_eq!(renderer.pipeline_count(), 2);
        assert_eq!(renderer.indexed_draw_count(), 30);
    }

    #[test]
    fn a_missing_texture_binding_uses_the_error_texture() {
        let (world, mut materials, textures, mut renderer, target, _front, _side) = rig();
        let near = world.extract(RenderFrameId(1)).unwrap().instances()[0].material_for_slot(0).unwrap();
        materials.set_texture(near, "BaseColor", jarvig_material::TextureId(0)).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.missing_texture_uses(), 0);
        assert_eq!(materials.compile_count(), 1);
        materials.set_texture(near, "Normal", jarvig_material::TextureId(0)).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(renderer.missing_texture_uses(), 0);
        materials.set_texture(near, "BaseColor", jarvig_material::TextureId(999)).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 3);
        assert_eq!(renderer.missing_texture_uses(), 1);
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert!(textures.error_color().is_some());
        materials.set_texture(near, "Normal", jarvig_material::TextureId(999)).unwrap();
        let snapshot = world.extract(RenderFrameId(4)).unwrap();
        let error = renderer.render_target(target, &snapshot, world.meshes(), &materials, &textures).unwrap_err();
        assert!(matches!(error, RenderError::UnknownTexture));
        assert_eq!(materials.compile_count(), 1);
        assert_eq!(renderer.material_pipeline_count(), 1);
    }

    #[test]
    fn an_object_rotation_reuploads_its_transform_and_not_the_mesh() {
        let (mut world, materials, textures, mut renderer, target, front, _side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        let meshes = renderer.mesh_upload_count();
        let compiles = materials.compile_count();
        let textures_uploaded = renderer.texture_upload_count();
        let lights = renderer.light_buffer_upload_count();
        let uploads = renderer.instance_transform_upload_count();
        assert_eq!(uploads, 4, "two views times two instances");
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(renderer.instance_transform_upload_count(), uploads, "an unchanged frame must not rewrite the uniform");
        let near = world.entity_outline()[0].uuid;
        let instance = world.extract(RenderFrameId(1)).unwrap().instances().iter().find(|item| item.entity == near).unwrap().id;
        let before = renderer.instance_transform_bytes(front, instance).unwrap();
        let turned = jarvig_core::Quat::from_axis_angle(jarvig_core::Vec3::new(1.0, 0.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        world.set_entity_local_rotation(near, turned).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 3);
        assert_eq!(renderer.instance_transform_upload_count(), uploads + 2, "both views rewrite the rotated instance only");
        assert_eq!(renderer.mesh_upload_count(), meshes);
        assert_eq!(materials.compile_count(), compiles);
        assert_eq!(renderer.texture_upload_count(), textures_uploaded);
        assert_eq!(renderer.light_buffer_upload_count(), lights, "an object rotation does not rewrite the light packet");
        assert_eq!(renderer.environment_packet_upload_count(), 1, "an object rotation does not rewrite the environment packet");
        let after = renderer.instance_transform_bytes(front, instance).unwrap();
        assert_eq!(&before[..128], &after[..128], "projection and view stay put when only the object turns");
        assert_ne!(&before[128..], &after[128..], "the model matrix must change on the next present");
        present(&mut renderer, &world, &materials, &textures, target, 4);
        assert_eq!(renderer.instance_transform_upload_count(), uploads + 2);
        world.set_entity_local_rotation(near, jarvig_core::Quat::IDENTITY).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 5);
        let restored = renderer.instance_transform_bytes(front, instance).unwrap();
        assert_eq!(restored, before);
        assert_eq!(renderer.mesh_upload_count(), meshes);
        assert_eq!(materials.compile_count(), compiles);
        assert_eq!(renderer.texture_upload_count(), textures_uploaded);
    }

    #[test]
    fn time_sliced_capture_finishes_across_frames_and_the_camera_does_not_reset_it() {
        use jarvig_core::{ProbeUpdatePolicy, ResolvedPose, Vec3};
        let (mut world, materials, textures, mut renderer, target, front, _side) = rig();
        world.set_probe_update_policy(ProbeUpdatePolicy::TimeSliced);
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.reflection_probe_capture_count(), 0);
        assert_eq!(renderer.reflection_probe_texture_count(), 0);
        assert_eq!(renderer.probe_capture_job_step(), 1);
        assert_eq!(renderer.probe_capture_job_steps(), 42);
        assert!(renderer.probe_capture_queue_len() >= 1);
        let home = world.extract(RenderFrameId(1)).unwrap().camera(world.front_camera().frame).unwrap().pose;
        renderer
            .update_view(front, RenderViewUpdate {
                camera: None,
                layout: None,
                settings: None,
                pose: Some(ResolvedPose {
                    translation: Vec3::new(home.translation.x + 3.0, home.translation.y, home.translation.z - 1.0),
                    rotation: home.rotation,
                }),
            })
            .unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(renderer.reflection_probe_capture_count(), 0);
        assert_eq!(renderer.probe_capture_job_step(), 2);
        assert_eq!(renderer.shadow_update_count(), 1);
        let mut frames = 2u32;
        while renderer.reflection_probe_capture_count() == 0 {
            frames += 1;
            present(&mut renderer, &world, &materials, &textures, target, frames as u64);
            assert!(frames < 50, "time sliced capture did not finish");
        }
        assert_eq!(frames, 42);
        assert_eq!(renderer.reflection_probe_resolution(), 32);
        assert_eq!(renderer.reflection_probe_mip_count(), 6);
        assert_eq!(renderer.probe_capture_queue_len(), 0);
        assert_eq!(renderer.probe_recapture_count(), 0);
        let shadows = renderer.shadow_update_count();
        present(&mut renderer, &world, &materials, &textures, target, 90);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        assert_eq!(renderer.shadow_update_count(), shadows);
    }

    #[test]
    fn a_later_probe_refresh_keeps_the_old_cube_until_the_budget_finishes() {
        use jarvig_core::{ProbeUpdatePolicy, ResolvedPose, Vec3};
        let (mut world, materials, textures, mut renderer, target, front, _side) = rig();
        present(&mut renderer, &world, &materials, &textures, target, 1);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        assert_eq!(renderer.reflection_probe_resolution(), 32);
        world.set_probe_update_policy(ProbeUpdatePolicy::OnTransformChange);
        let home = world.extract(RenderFrameId(2)).unwrap().camera(world.front_camera().frame).unwrap().pose;
        renderer
            .update_view(front, RenderViewUpdate {
                camera: None,
                layout: None,
                settings: None,
                pose: Some(ResolvedPose {
                    translation: Vec3::new(home.translation.x + 4.0, home.translation.y, home.translation.z),
                    rotation: home.rotation,
                }),
            })
            .unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 2);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        assert_eq!(renderer.probe_recapture_count(), 0);
        assert_eq!(renderer.probe_capture_queue_len(), 0);
        assert_eq!(renderer.shadow_update_count(), 1);
        let near = world.objects().next().unwrap();
        world.set_object_local_translation(near, Vec3::new(0.35, 0.0, -2.0)).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 3);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        assert_eq!(renderer.reflection_probe_resolution(), 32);
        assert_eq!(renderer.probe_capture_job_resolution(), 32);
        assert!(renderer.probe_capture_queue_len() >= 1);
        assert_eq!(renderer.shadow_update_count(), 2);
        world.set_probe_update_policy(ProbeUpdatePolicy::Static);
        world.request_probe_recapture();
        present(&mut renderer, &world, &materials, &textures, target, 4);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        assert_eq!(renderer.probe_capture_job_resolution(), 32);
        world.set_reflection_probe_resolution(128).unwrap();
        present(&mut renderer, &world, &materials, &textures, target, 5);
        assert_eq!(renderer.reflection_probe_resolution(), 32);
        assert_eq!(renderer.probe_capture_job_resolution(), 128);
        assert_eq!(renderer.reflection_probe_capture_count(), 1);
        assert_eq!(renderer.mesh_upload_count(), 2);
        assert_eq!(materials.compile_count(), 1);
    }

    fn snapshot_has_logical_material(world: &SceneWorld) -> bool {
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        snapshot.instances().iter().all(|instance| instance.material_for_slot(0).is_some())
    }

    fn color_without_uv() -> jarvig_core::Mesh {
        let mut bytes = Vec::new();
        for vertex in [
            [-0.5f32, -0.5, 0.0, 1.0, 0.0, 0.0],
            [0.5, -0.5, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.5, 0.0, 0.0, 0.0, 1.0],
        ] {
            for value in vertex {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut indices = Vec::new();
        for index in [0u16, 1, 2] {
            indices.extend_from_slice(&index.to_le_bytes());
        }
        create_mesh(MeshDesc {
            streams: vec![VertexStreamDesc {
                stride: 24,
                attributes: vec![
                    MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                    MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                ],
                bytes,
            }],
            index_format: MeshIndexFormat::Uint16,
            index_bytes: indices,
            submeshes: vec![SubmeshDesc {
                first_index: 0,
                index_count: 3,
                base_vertex: 0,
                topology: MeshTopology::TriangleList,
                material_slot: 0,
            }],
        })
        .unwrap()
    }

    fn position_only_triangle() -> jarvig_core::Mesh {
        let mut bytes = Vec::new();
        for vertex in [[-0.5f32, -0.5, 0.0], [0.5, -0.5, 0.0], [0.0, 0.5, 0.0]] {
            for value in vertex {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut indices = Vec::new();
        for index in [0u16, 1, 2] {
            indices.extend_from_slice(&index.to_le_bytes());
        }
        create_mesh(MeshDesc {
            streams: vec![VertexStreamDesc {
                stride: 12,
                attributes: vec![MeshVertexAttribute {
                    shader_location: 0,
                    offset: 0,
                    format: MeshVertexFormat::Float32x3,
                }],
                bytes,
            }],
            index_format: MeshIndexFormat::Uint16,
            index_bytes: indices,
            submeshes: vec![SubmeshDesc {
                first_index: 0,
                index_count: 3,
                base_vertex: 0,
                topology: MeshTopology::TriangleList,
                material_slot: 0,
            }],
        })
        .unwrap()
    }
}
