use std::fmt;

macro_rules! plain_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub u64);
    };
}

/// Index plus generation. Generation 0 is never a live resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawHandle {
    pub index: u32,
    pub generation: u32,
}

impl RawHandle {
    pub const INVALID: Self = Self { index: 0, generation: 0 };

    pub fn is_valid(self) -> bool {
        self.generation != 0
    }
}

macro_rules! gpu_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub RawHandle);

        impl $name {
            pub const INVALID: Self = Self(RawHandle::INVALID);

            pub fn raw(self) -> RawHandle {
                self.0
            }

            pub fn is_valid(self) -> bool {
                self.0.is_valid()
            }
        }
    };
}

plain_id!(AdapterId);
plain_id!(QueueId);
gpu_id!(BufferId);
gpu_id!(TextureId);
gpu_id!(TextureViewId);
gpu_id!(SamplerId);
gpu_id!(ShaderModuleId);
gpu_id!(BindGroupLayoutId);
gpu_id!(BindGroupId);
gpu_id!(PipelineId);
plain_id!(CommandEncoderId);
plain_id!(CommandBufferId);
plain_id!(FenceId);
plain_id!(QueryPoolId);
plain_id!(SurfaceId);
plain_id!(SwapchainId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RhiError {
    Unsupported(&'static str),
    InvalidResource(&'static str),
    /// The handle is null, already retired, or a stale generation.
    InvalidHandle(&'static str),
    Validation(String),
    DeviceLost,
    /// The window has no drawable area. Callers skip the frame. This is not fatal.
    Minimized,
    /// The surface must be reconfigured, usually after a resize.
    SurfaceOutdated,
    /// The surface must be recreated.
    SurfaceLost,
    /// The GPU did not deliver a frame in time. Skip it.
    SurfaceTimeout,
}

impl fmt::Display for RhiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(formatter, "unsupported: {what}"),
            Self::InvalidResource(what) => write!(formatter, "invalid resource: {what}"),
            Self::InvalidHandle(what) => write!(formatter, "invalid handle: {what}"),
            Self::Validation(message) => write!(formatter, "{message}"),
            Self::DeviceLost => write!(formatter, "device lost"),
            Self::Minimized => write!(formatter, "window has no drawable size"),
            Self::SurfaceOutdated => write!(formatter, "surface outdated"),
            Self::SurfaceLost => write!(formatter, "surface lost"),
            Self::SurfaceTimeout => write!(formatter, "surface timeout"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    Unknown,
    Cpu,
    IntegratedGpu,
    DiscreteGpu,
    VirtualGpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// No GPU. Tests, headless checks, and server builds use this.
    Null,
    /// The private wgpu backend. This is not a wgpu type.
    Wgpu,
}

/// Native graphics API behind the backend. Not a wgpu enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphicsApi {
    Dx12,
    Vulkan,
    Metal,
    OpenGl,
    BrowserWebGpu,
    Null,
    Unknown,
}

impl GraphicsApi {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "dx12" | "d3d12" => Some(Self::Dx12),
            "vulkan" | "vk" => Some(Self::Vulkan),
            "metal" => Some(Self::Metal),
            "opengl" | "gl" => Some(Self::OpenGl),
            "webgpu" => Some(Self::BrowserWebGpu),
            "null" => Some(Self::Null),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dx12 => "dx12",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
            Self::OpenGl => "opengl",
            Self::BrowserWebGpu => "webgpu",
            Self::Null => "null",
            Self::Unknown => "unknown",
        }
    }
}

/// How a presenting host asks for a GPU. Auto on a native desktop host is high performance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsPreference {
    Auto,
    HighPerformance,
    LowPower,
    Specific(AdapterFingerprint),
}

impl GraphicsPreference {
    pub fn label(&self) -> String {
        match self {
            Self::Auto => "Auto".into(),
            Self::HighPerformance => "HighPerformance".into(),
            Self::LowPower => "LowPower".into(),
            Self::Specific(fingerprint) => format!(
                "Specific {:04x}:{:04x}:{}",
                fingerprint.vendor_id, fingerprint.device_id, fingerprint.api.label()
            ),
        }
    }
}

/// Stable-enough preference key. Not a hardware UUID and not a runtime index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterFingerprint {
    pub vendor_id: u32,
    pub device_id: u32,
    pub api: GraphicsApi,
    pub kind: Option<AdapterKind>,
    pub name: Option<String>,
}

/// Host-supplied startup choice. The renderer does not parse CLI text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsDeviceConfig {
    pub preference: GraphicsPreference,
    /// Software is eligible only after every hardware candidate has been rejected.
    pub allow_software_fallback: bool,
}

impl Default for GraphicsDeviceConfig {
    fn default() -> Self {
        Self { preference: GraphicsPreference::Auto, allow_software_fallback: true }
    }
}

/// One enumerated adapter. `runtime_id` is valid for this process only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsAdapterDesc {
    pub runtime_id: AdapterId,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub kind: AdapterKind,
    pub backend: BackendKind,
    pub api: GraphicsApi,
    pub driver: String,
    pub driver_info: String,
    pub max_texture_dimension_2d: u32,
    pub max_storage_buffers_per_shader_stage: u32,
    pub timestamp_queries: bool,
    pub surface_compatible: bool,
    pub meets_requirements: bool,
    pub software: bool,
    pub rejection: Option<String>,
}

impl GraphicsAdapterDesc {
    pub fn fingerprint(&self) -> AdapterFingerprint {
        AdapterFingerprint {
            vendor_id: self.vendor_id,
            device_id: self.device_id,
            api: self.api,
            kind: Some(self.kind),
            name: Some(self.name.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInfo {
    pub id: AdapterId,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub backend: BackendKind,
    pub api: GraphicsApi,
    pub kind: AdapterKind,
    pub driver: String,
    pub driver_info: String,
    pub timestamp_queries: bool,
    pub max_texture_dimension_2d: u32,
    pub max_storage_buffers_per_shader_stage: u32,
    pub software: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceDesc {
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCapabilities {
    pub timestamp_queries: bool,
    pub max_texture_dimension_2d: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    Vertex,
    Index,
    Uniform,
    Storage,
    CopySrc,
    CopyDst,
}

/// Opaque buffer handle. Backend objects stay in the backend.
pub type BufferHandle = BufferId;

#[derive(Debug, Clone)]
pub struct BufferDesc {
    pub size: u64,
    pub usage: BufferUsage,
    pub label: Option<String>,
    /// Copied once at creation. This is not a per-frame upload and not a staging allocator.
    pub contents: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexFormat {
    Uint16,
    Uint32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexStepMode {
    Vertex,
    Instance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
}

impl VertexFormat {
    pub fn byte_size(self) -> u64 {
        match self {
            Self::Float32x2 => 8,
            Self::Float32x3 => 12,
            Self::Float32x4 => 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexAttribute {
    pub shader_location: u32,
    pub offset: u64,
    pub format: VertexFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexBufferLayout {
    pub stride: u64,
    pub step_mode: VertexStepMode,
    pub attributes: Vec<VertexAttribute>,
}

impl VertexBufferLayout {
    pub fn check(&self) -> Result<(), RhiError> {
        if self.stride == 0 || self.stride % 4 != 0 {
            return Err(RhiError::Validation("vertex stride must be a non-zero multiple of 4".into()));
        }
        if self.attributes.is_empty() {
            return Err(RhiError::Validation("vertex layout needs an attribute".into()));
        }
        let mut locations = Vec::new();
        for attribute in &self.attributes {
            if attribute.offset % 4 != 0 {
                return Err(RhiError::Validation("vertex attribute offset must be a multiple of 4".into()));
            }
            if attribute.offset.saturating_add(attribute.format.byte_size()) > self.stride {
                return Err(RhiError::Validation("vertex attribute exceeds stride".into()));
            }
            if locations.contains(&attribute.shader_location) {
                return Err(RhiError::Validation("vertex attribute locations must be unique".into()));
            }
            locations.push(attribute.shader_location);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Rgba8Unorm,
    Rgba8UnormSrgb,
    Bgra8Unorm,
    Bgra8UnormSrgb,
    /// Linear HDR scene color. Not a swapchain format and not a depth format.
    Rgba16Float,
    Depth32Float,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureUsage {
    RenderAttachment,
    Sampled,
    /// Rendered in one pass and sampled in a later pass. HDR scene color.
    ColorTarget,
    Storage,
    Copy,
}

#[derive(Debug, Clone)]
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub usage: TextureUsage,
    /// At least 1. The first sampled texture uses 1. Higher counts are metadata, not a mip generator.
    pub mip_count: u32,
    pub label: Option<String>,
}

/// Six square faces, one HDR image. Not a 2D texture with a hidden dimension field.
#[derive(Debug, Clone)]
pub struct CubeTextureDesc {
    pub size: u32,
    pub format: TextureFormat,
    pub usage: TextureUsage,
    pub mip_count: u32,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
}

/// JARVIG sampler state. Not a wgpu descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamplerDesc {
    pub min_filter: FilterMode,
    pub mag_filter: FilterMode,
    pub mip_filter: FilterMode,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub address_w: AddressMode,
    /// 1 disables anisotropy. Values above 1 require linear min, mag, and mip filters.
    pub anisotropy: u16,
    pub label: Option<String>,
}

impl SamplerDesc {
    pub fn linear_repeat(label: Option<String>) -> Self {
        Self {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mip_filter: FilterMode::Linear,
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
            address_w: AddressMode::Repeat,
            anisotropy: 1,
            label,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindResource {
    Buffer(BufferId),
    TextureView(TextureViewId),
    Sampler(SamplerId),
}

/// Bootstrap shader carrier. WGSL text is not the final JARVIG shader architecture.
/// Materials will compile to an IR, and each backend will generate its own target.
/// This variant exists so the first triangle can reach the wgpu backend without a compiler.
#[derive(Debug, Clone)]
pub enum ShaderSource {
    Wgsl(String),
}

#[derive(Debug, Clone)]
pub struct ShaderModuleDesc {
    pub source: ShaderSource,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingType {
    UniformBuffer,
    StorageBuffer,
    Texture,
    /// `texture_cube`. Not a 2D face view.
    TextureCube,
    Sampler,
}

/// Which shader stages read a binding. Not a wgpu type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    VertexFragment,
}

#[derive(Debug, Clone)]
pub struct BindGroupLayoutEntry {
    pub binding: u32,
    pub kind: BindingType,
    pub stage: ShaderStage,
}

#[derive(Debug, Clone)]
pub struct BindGroupLayoutDesc {
    pub entries: Vec<BindGroupLayoutEntry>,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BindGroupEntry {
    pub binding: u32,
    pub resource: BindResource,
}

#[derive(Debug, Clone)]
pub struct BindGroupDesc {
    pub layout: BindGroupLayoutId,
    pub entries: Vec<BindGroupEntry>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineKind {
    Render,
    Compute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveTopology {
    TriangleList,
}

/// Which triangle faces the rasterizer discards. Not a wgpu type.
/// Counter-clockwise is the front face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CullMode {
    Back,
    Front,
    None,
}

#[derive(Debug, Clone)]
pub struct RenderPipelineDesc {
    pub shader: ShaderModuleId,
    pub vertex_entry: String,
    pub fragment_entry: String,
    pub topology: PrimitiveTopology,
    pub color_format: TextureFormat,
    /// Group index is the vector index. Empty means the pipeline binds nothing.
    /// Group 0 is the view and object transform. Group 1 is material parameters.
    pub layouts: Vec<BindGroupLayoutId>,
    /// Empty means the shader generates positions. The indexed triangle supplies one layout.
    pub vertex_buffers: Vec<VertexBufferLayout>,
    /// None keeps the pass color-only. Opaque geometry sets this.
    pub depth: Option<DepthState>,
    /// Counter-clockwise is front. Opaque materials cull back faces.
    pub cull: CullMode,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareFunction {
    Never,
    Less,
    LessEqual,
    Equal,
    GreaterEqual,
    Greater,
    NotEqual,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthState {
    pub format: TextureFormat,
    pub write_enabled: bool,
    pub compare: CompareFunction,
}

#[derive(Debug, Clone)]
pub struct ComputePipelineDesc {
    pub layout: BindGroupLayoutId,
    pub shader: ShaderModuleId,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClearColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Deterministic first-frame clear. Not a material and not a scene.
pub const JARVIG_CLEAR: ClearColor = ClearColor {
    r: 0.051,
    g: 0.090,
    b: 0.141,
    a: 1.0,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoadOp {
    Clear(ClearColor),
    Load,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOp {
    Store,
    Discard,
}

#[derive(Debug, Clone)]
pub struct ColorAttachment {
    pub target: TextureViewId,
    pub load: LoadOp,
    pub store: StoreOp,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DepthLoadOp {
    Clear(f32),
    Load,
}

#[derive(Debug, Clone)]
pub struct DepthAttachment {
    pub target: TextureViewId,
    pub load: DepthLoadOp,
    pub store: StoreOp,
}

#[derive(Debug, Clone)]
pub struct RenderPassDesc {
    pub label: Option<String>,
    pub colors: Vec<ColorAttachment>,
    pub depth: Option<DepthAttachment>,
}

/// Rectangle inside a render target. Origin is the top-left of that target. Not a wgpu viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Viewport {
    pub fn check(self, target_width: u32, target_height: u32) -> Result<(), RhiError> {
        let finite = self.x.is_finite() && self.y.is_finite() && self.width.is_finite() && self.height.is_finite();
        if !finite || self.width <= 0.0 || self.height <= 0.0 || target_width == 0 || target_height == 0 {
            return Err(RhiError::Validation("viewport is invalid".into()));
        }
        if !(0.0..=1.0).contains(&self.min_depth) || !(0.0..=1.0).contains(&self.max_depth) || self.min_depth > self.max_depth {
            return Err(RhiError::Validation("viewport depth range is invalid".into()));
        }
        let limit = 0.01;
        if self.x < -limit || self.y < -limit || self.x + self.width > target_width as f32 + limit || self.y + self.height > target_height as f32 + limit {
            return Err(RhiError::Validation("viewport is outside the target".into()));
        }
        Ok(())
    }
}

/// Pixel rectangle inside a render target. Origin is the top-left. Not a wgpu scissor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScissorRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ScissorRect {
    pub fn check(self, target_width: u32, target_height: u32) -> Result<(), RhiError> {
        if self.width == 0 || self.height == 0 || target_width == 0 || target_height == 0 {
            return Err(RhiError::Validation("scissor is invalid".into()));
        }
        let right = self.x.checked_add(self.width);
        let bottom = self.y.checked_add(self.height);
        if right.is_none_or(|right| right > target_width) || bottom.is_none_or(|bottom| bottom > target_height) {
            return Err(RhiError::Validation("scissor is outside the target".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ComputePassDesc {
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FenceDesc {
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    Timestamp,
}

#[derive(Debug, Clone)]
pub struct QueryPoolDesc {
    pub kind: QueryKind,
    pub count: u32,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SurfaceDesc {
    pub width: u32,
    pub height: u32,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwapchainDesc {
    pub surface: SurfaceId,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquireFrame {
    Ready { view: TextureViewId },
    Minimized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Buffer,
    Texture,
    TextureView,
    Sampler,
    ShaderModule,
    BindGroupLayout,
    BindGroup,
    Pipeline,
    CommandBuffer,
    Fence,
    QueryPool,
    Surface,
    Swapchain,
}

pub trait Instance {
    fn backend_name(&self) -> &'static str;
    fn enumerate_adapters(&self) -> Vec<AdapterId>;
    fn adapter_info(&self, adapter: AdapterId) -> Result<AdapterInfo, RhiError>;
    fn request_device(&self, adapter: AdapterId, desc: &DeviceDesc) -> Result<Box<dyn Device>, RhiError>;
}

pub trait Device {
    fn label(&self) -> Option<&str>;
    fn capabilities(&self) -> DeviceCapabilities;
    fn graphics_queue(&self) -> QueueId;

    fn create_buffer(&mut self, desc: &BufferDesc) -> Result<BufferId, RhiError>;
    fn create_texture(&mut self, desc: &TextureDesc) -> Result<TextureId, RhiError>;
    fn create_texture_view(&mut self, texture: TextureId) -> Result<TextureViewId, RhiError>;
    fn create_cube_texture(&mut self, desc: &CubeTextureDesc) -> Result<TextureId, RhiError>;
    /// One mip of one face. A color attachment, not a cube sample.
    fn create_cube_face_view(&mut self, texture: TextureId, face: u32, mip: u32) -> Result<TextureViewId, RhiError>;
    /// All faces and mips. Bound as `texture_cube`. Not a render target.
    fn create_cube_sample_view(&mut self, texture: TextureId) -> Result<TextureViewId, RhiError>;
    fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<SamplerId, RhiError>;
    fn create_shader_module(&mut self, desc: &ShaderModuleDesc) -> Result<ShaderModuleId, RhiError>;
    fn create_bind_group_layout(&mut self, desc: &BindGroupLayoutDesc) -> Result<BindGroupLayoutId, RhiError>;
    fn create_bind_group(&mut self, desc: &BindGroupDesc) -> Result<BindGroupId, RhiError>;
    fn write_buffer(&mut self, buffer: BufferId, offset: u64, data: &[u8]) -> Result<(), RhiError>;
    /// Packed RGBA8 rows of mip 0. The backend aligns the copy. Callers do not pad to 256.
    fn write_texture(&mut self, texture: TextureId, data: &[u8]) -> Result<(), RhiError>;
    /// One mip. `mip` 0 is the base. The byte length is that level's tightly packed RGBA8.
    fn write_texture_mip(&mut self, texture: TextureId, mip: u32, data: &[u8]) -> Result<(), RhiError>;
    fn create_render_pipeline(&mut self, desc: &RenderPipelineDesc) -> Result<PipelineId, RhiError>;
    fn create_compute_pipeline(&mut self, desc: &ComputePipelineDesc) -> Result<PipelineId, RhiError>;
    fn create_command_encoder(&mut self, label: Option<&str>) -> Result<Box<dyn CommandEncoder>, RhiError>;
    fn create_fence(&mut self, desc: &FenceDesc) -> Result<FenceId, RhiError>;
    fn create_query_pool(&mut self, desc: &QueryPoolDesc) -> Result<QueryPoolId, RhiError>;
    fn create_surface(&mut self, desc: &SurfaceDesc) -> Result<SurfaceId, RhiError>;
    fn create_swapchain(&mut self, desc: &SwapchainDesc) -> Result<SwapchainId, RhiError>;
    fn configure_swapchain(&mut self, swapchain: SwapchainId, width: u32, height: u32) -> Result<(), RhiError>;
    fn acquire_frame(&mut self, swapchain: SwapchainId) -> Result<AcquireFrame, RhiError>;

    fn destroy(&mut self, kind: ResourceKind, handle: RawHandle) -> Result<(), RhiError>;
    fn debug_label(&self, kind: ResourceKind, handle: RawHandle) -> Option<String>;
    /// Block until submitted GPU work is finished, then destroy retired resources that are safe.
    fn flush(&mut self) -> Result<(), RhiError>;
    fn resource_stats(&self) -> ResourceStats;

    fn submit(
        &mut self,
        queue: QueueId,
        buffers: &[CommandBufferId],
        signal: Option<FenceId>,
    ) -> Result<(), RhiError>;
    fn wait(&mut self, fence: FenceId) -> Result<(), RhiError>;
    fn present(&mut self, queue: QueueId, swapchain: SwapchainId) -> Result<u64, RhiError>;

    fn submitted_buffers(&self) -> u64;
    fn presented_frames(&self) -> u64;
    fn draw_count(&self) -> u64;
    fn indexed_draw_count(&self) -> u64;
    fn pipeline_count(&self) -> u64;
    fn buffer_count(&self) -> u64;
}

/// Counts for the resource inspector. Not a VRAM budget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceStats {
    pub alive_buffers: u32,
    pub alive_textures: u32,
    pub alive_texture_views: u32,
    pub alive_shaders: u32,
    pub alive_pipelines: u32,
    pub alive_bind_groups: u32,
    pub alive_layouts: u32,
    pub alive_samplers: u32,
    pub retired: u32,
    pub estimated_bytes: u64,
}

pub trait CommandEncoder {
    fn begin_render_pass(&mut self, desc: &RenderPassDesc) -> Result<(), RhiError>;
    fn set_pipeline(&mut self, pipeline: PipelineId) -> Result<(), RhiError>;
    fn set_bind_group(&mut self, index: u32, group: BindGroupId) -> Result<(), RhiError>;
    fn set_vertex_buffer(&mut self, slot: u32, buffer: BufferId, offset: u64) -> Result<(), RhiError>;
    fn set_index_buffer(&mut self, buffer: BufferId, format: IndexFormat, offset: u64) -> Result<(), RhiError>;
    fn set_viewport(&mut self, viewport: Viewport) -> Result<(), RhiError>;
    fn set_scissor(&mut self, scissor: ScissorRect) -> Result<(), RhiError>;
    fn draw(&mut self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32) -> Result<(), RhiError>;
    fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    ) -> Result<(), RhiError>;
    fn end_render_pass(&mut self) -> Result<(), RhiError>;
    fn begin_compute_pass(&mut self, desc: &ComputePassDesc) -> Result<(), RhiError>;
    fn end_compute_pass(&mut self) -> Result<(), RhiError>;
    fn write_timestamp(&mut self, pool: QueryPoolId, index: u32) -> Result<(), RhiError>;
    fn finish(self: Box<Self>) -> Result<CommandBufferId, RhiError>;
}
