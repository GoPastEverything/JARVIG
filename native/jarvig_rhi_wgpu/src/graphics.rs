use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use jarvig_rhi::{
    select_graphics_adapter, AcquireFrame, AdapterId, AdapterInfo, AdapterKind, BackendKind, BindGroupDesc, BindGroupId,
    BindGroupLayoutDesc, BindGroupLayoutId, BindingType, BufferDesc, BufferId, BufferUsage, CommandBufferId, CommandEncoder,
    ComputePassDesc, ComputePipelineDesc, Device, DeviceCapabilities, FenceDesc, FenceId, GraphicsAdapterDesc, GraphicsApi,
    GraphicsDeviceConfig, GraphicsRequirements, GraphicsSelection, IndexFormat, PipelineId, QueryPoolDesc, QueryPoolId, QueueId,
    RawHandle, RenderPassDesc, RenderPipelineDesc, ResourceKind, ResourceStats, RhiError, SamplerDesc, SamplerId, ShaderModuleDesc,
    ShaderModuleId, SurfaceDesc, SurfaceId, SwapchainDesc, SwapchainId, TextureDesc, TextureId, TextureViewId, VertexFormat,
    VertexStepMode,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::error::{map_request, map_surface_error};

/// One queue. `submitted` advances on submit. `completed` advances only in `flush`,
/// after `poll(Wait)`, so a retired resource is not dropped while that queue can still read it.
struct Shared {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: Mutex<wgpu::SurfaceConfiguration>,
    current: Mutex<Option<wgpu::SurfaceTexture>>,
    frame_view: Mutex<Option<RawHandle>>,
    views: Mutex<jarvig_rhi::Registry<GpuView>>,
    pending: Mutex<Option<Pending>>,
    shaders: Mutex<jarvig_rhi::Registry<wgpu::ShaderModule>>,
    pipelines: Mutex<jarvig_rhi::Registry<GpuPipeline>>,
    layouts: Mutex<jarvig_rhi::Registry<GpuLayout>>,
    groups: Mutex<jarvig_rhi::Registry<GpuGroup>>,
    samplers: Mutex<jarvig_rhi::Registry<wgpu::Sampler>>,
    buffers: Mutex<jarvig_rhi::Registry<GpuBuffer>>,
    textures: Mutex<jarvig_rhi::Registry<GpuTexture>>,
    submitted: Arc<AtomicU64>,
    completed: Arc<AtomicU64>,
    draws: Mutex<u64>,
    indexed_draws: Mutex<u64>,
    pipeline_count: Mutex<u64>,
    buffer_count: Mutex<u64>,
}

struct GpuBuffer {
    buffer: wgpu::Buffer,
    usage: BufferUsage,
    size: u64,
}

struct GpuTexture {
    texture: wgpu::Texture,
    format: jarvig_rhi::TextureFormat,
    width: u32,
    height: u32,
    mip_count: u32,
    layers: u32,
}

enum GpuView {
    /// Swapchain image. It has no texture in the JARVIG registry.
    Color(wgpu::TextureView),
    Sampled {
        view: wgpu::TextureView,
        texture: RawHandle,
    },
    /// Offscreen color. It can be a render attachment and a sampled texture.
    Hdr {
        view: wgpu::TextureView,
        texture: RawHandle,
        width: u32,
        height: u32,
    },
    Depth {
        view: wgpu::TextureView,
        texture: RawHandle,
        format: jarvig_rhi::TextureFormat,
    },
    /// Whole cubemap. Sampled, never a render attachment.
    Cube {
        view: wgpu::TextureView,
        texture: RawHandle,
    },
}

fn color_attachment<'a>(view: &'a GpuView) -> Option<&'a wgpu::TextureView> {
    match view {
        GpuView::Color(view) | GpuView::Hdr { view, .. } => Some(view),
        GpuView::Sampled { .. } | GpuView::Depth { .. } | GpuView::Cube { .. } => None,
    }
}

struct Pending {
    command: wgpu::CommandBuffer,
    buffers: Vec<RawHandle>,
    views: Vec<RawHandle>,
    textures: Vec<RawHandle>,
    pipelines: Vec<RawHandle>,
    groups: Vec<RawHandle>,
    shaders: Vec<RawHandle>,
    layouts: Vec<RawHandle>,
    samplers: Vec<RawHandle>,
}

struct GpuPipeline {
    pipeline: wgpu::RenderPipeline,
    layouts: Vec<jarvig_rhi::RawHandle>,
    depth: Option<jarvig_rhi::TextureFormat>,
    shader: jarvig_rhi::RawHandle,
}

struct GpuLayout {
    layout: wgpu::BindGroupLayout,
    entries: Vec<(u32, BindingType)>,
}

struct GpuGroup {
    layout: RawHandle,
    buffers: Vec<RawHandle>,
    views: Vec<RawHandle>,
    samplers: Vec<RawHandle>,
    group: wgpu::BindGroup,
}

pub struct Attached {
    pub device: Box<dyn Device>,
    pub surface: SurfaceId,
    pub swapchain: SwapchainId,
    pub color_format: jarvig_rhi::TextureFormat,
    pub info: AdapterInfo,
    pub selection: GraphicsSelection,
    pub adapters: Vec<GraphicsAdapterDesc>,
}

pub struct AdapterSurvey {
    pub adapters: Vec<GraphicsAdapterDesc>,
    pub selection: Result<GraphicsSelection, String>,
}

pub fn describe_adapters(
    window: &(impl HasWindowHandle + HasDisplayHandle),
    config: &GraphicsDeviceConfig,
) -> Result<AdapterSurvey, RhiError> {
    let opened = open_surface(window)?;
    let adapters = describe_candidates(&opened.instance, &opened.surface);
    let selection = select_graphics_adapter(&adapters, config, GraphicsRequirements::PRESENTING).map_err(|error| error.to_string());
    Ok(AdapterSurvey { adapters, selection })
}

pub fn attach_window(
    window: &(impl HasWindowHandle + HasDisplayHandle),
    width: u32,
    height: u32,
    config: &GraphicsDeviceConfig,
) -> Result<Attached, RhiError> {
    let opened = open_surface(window)?;
    let instance = opened.instance;
    let surface = opened.surface;
    let found = enumerate_with_handles(&instance, &surface);
    let descriptions: Vec<_> = found.iter().map(|(_, desc)| desc.clone()).collect();
    let selection = select_graphics_adapter(&descriptions, config, GraphicsRequirements::PRESENTING)?;
    let adapter = found
        .into_iter()
        .find(|(_, desc)| desc.runtime_id == selection.adapter.runtime_id)
        .map(|(adapter, _)| adapter)
        .ok_or(RhiError::Validation("selected adapter disappeared before device creation".into()))?;
    let info = info_from_desc(&selection.adapter);
    let mut required_features = wgpu::Features::empty();
    if adapter.features().contains(wgpu::Features::SHADER_PRIMITIVE_INDEX) {
        required_features |= wgpu::Features::SHADER_PRIMITIVE_INDEX;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("JARVIG.Device"),
            required_features,
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .map_err(map_request)?;
    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|format| format.is_srgb())
        .or_else(|| caps.formats.first().copied())
        .ok_or(RhiError::Validation("surface exposes no color format".into()))?;
    let color_format = map_format(format)?;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: width.max(1),
        height: height.max(1),
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);
    let shared = Arc::new(Shared {
        device,
        queue,
        surface,
        config: Mutex::new(config),
        current: Mutex::new(None),
        frame_view: Mutex::new(None),
        views: Mutex::new(jarvig_rhi::Registry::default()),
        pending: Mutex::new(None),
        shaders: Mutex::new(jarvig_rhi::Registry::default()),
        pipelines: Mutex::new(jarvig_rhi::Registry::default()),
        layouts: Mutex::new(jarvig_rhi::Registry::default()),
        groups: Mutex::new(jarvig_rhi::Registry::default()),
        samplers: Mutex::new(jarvig_rhi::Registry::default()),
        buffers: Mutex::new(jarvig_rhi::Registry::default()),
        textures: Mutex::new(jarvig_rhi::Registry::default()),
        submitted: Arc::new(AtomicU64::new(0)),
        completed: Arc::new(AtomicU64::new(0)),
        draws: Mutex::new(0),
        indexed_draws: Mutex::new(0),
        pipeline_count: Mutex::new(0),
        buffer_count: Mutex::new(0),
    });
    Ok(Attached {
        device: Box::new(WgpuDevice {
            shared,
            presented: 0,
        }),
        surface: SurfaceId(1),
        swapchain: SwapchainId(1),
        color_format,
        info,
        selection,
        adapters: descriptions,
    })
}

struct OpenedSurface {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
}

fn open_surface(window: &(impl HasWindowHandle + HasDisplayHandle)) -> Result<OpenedSurface, RhiError> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    // SAFETY: the host drops this device before the window. The raw handle is only
    // valid for that lifetime. wgpu's safe surface target would force a winit dependency.
    let surface: wgpu::Surface<'static> = unsafe {
        let target = wgpu::SurfaceTargetUnsafe::from_window(window).map_err(|error| {
            RhiError::Validation(format!("window handle: {error}"))
        })?;
        let surface = instance
            .create_surface_unsafe(target)
            .map_err(|error| RhiError::Validation(format!("surface: {error}")))?;
        core::mem::transmute(surface)
    };
    Ok(OpenedSurface { instance, surface })
}

fn describe_candidates(instance: &wgpu::Instance, surface: &wgpu::Surface<'_>) -> Vec<GraphicsAdapterDesc> {
    enumerate_with_handles(instance, surface).into_iter().map(|(_, desc)| desc).collect()
}

fn enumerate_with_handles(instance: &wgpu::Instance, surface: &wgpu::Surface<'_>) -> Vec<(wgpu::Adapter, GraphicsAdapterDesc)> {
    instance
        .enumerate_adapters(wgpu::Backends::all())
        .into_iter()
        .enumerate()
        .map(|(index, adapter)| {
            let desc = describe_adapter(index, &adapter, surface);
            (adapter, desc)
        })
        .collect()
}

fn describe_adapter(index: usize, adapter: &wgpu::Adapter, surface: &wgpu::Surface<'_>) -> GraphicsAdapterDesc {
    let info = adapter.get_info();
    let limits = adapter.limits();
    let surface_compatible = adapter.is_surface_supported(surface) && !surface.get_capabilities(adapter).formats.is_empty();
    let mut limit_notes = Vec::new();
    let mut limits_ok = true;
    wgpu::Limits::default().check_limits_with_fail_fn(&limits, false, |name, requested, allowed| {
        limits_ok = false;
        if limit_notes.len() < 3 {
            limit_notes.push(format!("{name} needs {requested}, adapter allows {allowed}"));
        }
    });
    let texture_ok = limits.max_texture_dimension_2d >= GraphicsRequirements::PRESENTING.min_texture_dimension_2d;
    let storage_ok = limits.max_storage_buffers_per_shader_stage >= GraphicsRequirements::PRESENTING.min_storage_buffers_per_shader_stage;
    let meets_requirements = limits_ok && texture_ok && storage_ok;
    let kind = match info.device_type {
        wgpu::DeviceType::IntegratedGpu => AdapterKind::IntegratedGpu,
        wgpu::DeviceType::DiscreteGpu => AdapterKind::DiscreteGpu,
        wgpu::DeviceType::VirtualGpu => AdapterKind::VirtualGpu,
        wgpu::DeviceType::Cpu => AdapterKind::Cpu,
        wgpu::DeviceType::Other => AdapterKind::Unknown,
    };
    let software = kind == AdapterKind::Cpu;
    let rejection = if !surface_compatible {
        Some("cannot present to the window surface".into())
    } else if !meets_requirements {
        Some(if limit_notes.is_empty() {
            "graphics limits are below the engine requirement".into()
        } else {
            limit_notes.join("; ")
        })
    } else {
        None
    };
    GraphicsAdapterDesc {
        runtime_id: AdapterId(index as u64 + 1),
        name: info.name,
        vendor_id: info.vendor,
        device_id: info.device,
        kind,
        backend: BackendKind::Wgpu,
        api: map_api(info.backend),
        driver: info.driver,
        driver_info: info.driver_info,
        max_texture_dimension_2d: limits.max_texture_dimension_2d,
        max_storage_buffers_per_shader_stage: limits.max_storage_buffers_per_shader_stage,
        timestamp_queries: false,
        surface_compatible,
        meets_requirements,
        software,
        rejection,
    }
}

fn map_api(backend: wgpu::Backend) -> GraphicsApi {
    match backend {
        wgpu::Backend::Dx12 => GraphicsApi::Dx12,
        wgpu::Backend::Vulkan => GraphicsApi::Vulkan,
        wgpu::Backend::Metal => GraphicsApi::Metal,
        wgpu::Backend::Gl => GraphicsApi::OpenGl,
        wgpu::Backend::BrowserWebGpu => GraphicsApi::BrowserWebGpu,
        wgpu::Backend::Empty => GraphicsApi::Unknown,
    }
}

fn info_from_desc(desc: &GraphicsAdapterDesc) -> AdapterInfo {
    AdapterInfo {
        id: desc.runtime_id,
        name: desc.name.clone(),
        vendor_id: desc.vendor_id,
        device_id: desc.device_id,
        backend: desc.backend,
        api: desc.api,
        kind: desc.kind,
        driver: desc.driver.clone(),
        driver_info: desc.driver_info.clone(),
        timestamp_queries: desc.timestamp_queries,
        max_texture_dimension_2d: desc.max_texture_dimension_2d,
        max_storage_buffers_per_shader_stage: desc.max_storage_buffers_per_shader_stage,
        software: desc.software,
    }
}

fn map_format(format: wgpu::TextureFormat) -> Result<jarvig_rhi::TextureFormat, RhiError> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => Ok(jarvig_rhi::TextureFormat::Rgba8Unorm),
        wgpu::TextureFormat::Rgba8UnormSrgb => Ok(jarvig_rhi::TextureFormat::Rgba8UnormSrgb),
        wgpu::TextureFormat::Bgra8Unorm => Ok(jarvig_rhi::TextureFormat::Bgra8Unorm),
        wgpu::TextureFormat::Bgra8UnormSrgb => Ok(jarvig_rhi::TextureFormat::Bgra8UnormSrgb),
        other => Err(RhiError::Validation(format!("unmapped surface format {other:?}"))),
    }
}

fn unmap_format(format: jarvig_rhi::TextureFormat) -> Result<wgpu::TextureFormat, RhiError> {
    match format {
        jarvig_rhi::TextureFormat::Rgba8Unorm => Ok(wgpu::TextureFormat::Rgba8Unorm),
        jarvig_rhi::TextureFormat::Rgba8UnormSrgb => Ok(wgpu::TextureFormat::Rgba8UnormSrgb),
        jarvig_rhi::TextureFormat::Bgra8Unorm => Ok(wgpu::TextureFormat::Bgra8Unorm),
        jarvig_rhi::TextureFormat::Bgra8UnormSrgb => Ok(wgpu::TextureFormat::Bgra8UnormSrgb),
        jarvig_rhi::TextureFormat::Rgba16Float => Ok(wgpu::TextureFormat::Rgba16Float),
        jarvig_rhi::TextureFormat::Depth32Float => Err(RhiError::Validation("depth is not a color target".into())),
    }
}

fn map_vertex_format(format: VertexFormat) -> wgpu::VertexFormat {
    match format {
        VertexFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
        VertexFormat::Float32x3 => wgpu::VertexFormat::Float32x3,
        VertexFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
    }
}

fn sampler_anisotropy(desc: &jarvig_rhi::SamplerDesc) -> u16 {
    let requested = desc.anisotropy.clamp(1, 16);
    if requested == 1 {
        return 1;
    }
    if !matches!(desc.min_filter, jarvig_rhi::FilterMode::Linear)
        || !matches!(desc.mag_filter, jarvig_rhi::FilterMode::Linear)
        || !matches!(desc.mip_filter, jarvig_rhi::FilterMode::Linear)
    {
        return 1;
    }
    requested
}

fn map_filter(mode: jarvig_rhi::FilterMode) -> wgpu::FilterMode {
    match mode {
        jarvig_rhi::FilterMode::Nearest => wgpu::FilterMode::Nearest,
        jarvig_rhi::FilterMode::Linear => wgpu::FilterMode::Linear,
    }
}

fn map_address(mode: jarvig_rhi::AddressMode) -> wgpu::AddressMode {
    match mode {
        jarvig_rhi::AddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        jarvig_rhi::AddressMode::Repeat => wgpu::AddressMode::Repeat,
    }
}

fn map_stage(stage: jarvig_rhi::ShaderStage) -> wgpu::ShaderStages {
    match stage {
        jarvig_rhi::ShaderStage::Vertex => wgpu::ShaderStages::VERTEX,
        jarvig_rhi::ShaderStage::Fragment => wgpu::ShaderStages::FRAGMENT,
        jarvig_rhi::ShaderStage::VertexFragment => wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
    }
}

fn map_compare(compare: jarvig_rhi::CompareFunction) -> wgpu::CompareFunction {
    match compare {
        jarvig_rhi::CompareFunction::Never => wgpu::CompareFunction::Never,
        jarvig_rhi::CompareFunction::Less => wgpu::CompareFunction::Less,
        jarvig_rhi::CompareFunction::LessEqual => wgpu::CompareFunction::LessEqual,
        jarvig_rhi::CompareFunction::Equal => wgpu::CompareFunction::Equal,
        jarvig_rhi::CompareFunction::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
        jarvig_rhi::CompareFunction::Greater => wgpu::CompareFunction::Greater,
        jarvig_rhi::CompareFunction::NotEqual => wgpu::CompareFunction::NotEqual,
        jarvig_rhi::CompareFunction::Always => wgpu::CompareFunction::Always,
    }
}

fn map_index_format(format: IndexFormat) -> wgpu::IndexFormat {
    match format {
        IndexFormat::Uint16 => wgpu::IndexFormat::Uint16,
        IndexFormat::Uint32 => wgpu::IndexFormat::Uint32,
    }
}

fn align_buffer_size(size: u64) -> u64 {
    size.saturating_add(3) & !3
}

struct WgpuDevice {
    shared: Arc<Shared>,
    presented: u64,
}

enum Recorded {
    SetPipeline(RawHandle),
    SetBindGroup { index: u32, group: RawHandle },
    SetVertexBuffer { slot: u32, buffer: RawHandle, offset: u64 },
    SetIndexBuffer { buffer: RawHandle, format: IndexFormat, offset: u64 },
    SetViewport(jarvig_rhi::Viewport),
    SetScissor(jarvig_rhi::ScissorRect),
    Draw { vertices: u32, instances: u32, first_vertex: u32, first_instance: u32 },
    DrawIndexed { indices: u32, instances: u32, first_index: u32, base_vertex: i32, first_instance: u32 },
}

struct WgpuEncoder {
    shared: Arc<Shared>,
    encoder: Option<wgpu::CommandEncoder>,
    label: String,
    view: Option<RawHandle>,
    load: Option<jarvig_rhi::LoadOp>,
    store: jarvig_rhi::StoreOp,
    commands: Vec<Recorded>,
    pass_open: bool,
    depth_view: Option<RawHandle>,
    depth_clear: Option<f32>,
    depth_store: jarvig_rhi::StoreOp,
    attachment_width: u32,
    attachment_height: u32,
    accumulated: Used,
}

impl WgpuEncoder {
    fn bind_buffer(&self, buffer: BufferId, usage: BufferUsage, offset: u64, align: u64) -> Result<(), RhiError> {
        if !self.pass_open {
            return Err(RhiError::Validation("a buffer is bound inside a render pass".into()));
        }
        let buffers = lock(&self.shared.buffers)?;
        let record = buffers.get(buffer.raw()).map_err(|_| RhiError::InvalidHandle("buffer"))?;
        if record.usage != usage {
            return Err(RhiError::Validation("buffer usage does not match the binding".into()));
        }
        if offset > record.size || offset % align != 0 {
            return Err(RhiError::Validation("buffer offset is out of range or misaligned".into()));
        }
        Ok(())
    }

    fn require_draw(&self, count: u32, instances: u32, indexed: bool) -> Result<(), RhiError> {
        if !self.pass_open {
            return Err(RhiError::Validation("draw is only valid inside a render pass".into()));
        }
        let pipeline = self.commands.iter().rev().find_map(|command| match command {
            Recorded::SetPipeline(id) => Some(*id),
            _ => None,
        });
        let Some(pipeline) = pipeline else {
            return Err(RhiError::Validation("a pipeline must be bound before draw".into()));
        };
        let (expected_layouts, required) = {
            let pipelines = lock(&self.shared.pipelines)?;
            let info = pipelines.get(pipeline).map_err(|_| RhiError::InvalidHandle("pipeline"))?;
            (info.layouts.clone(), info.depth)
        };
        for (index, layout) in expected_layouts.iter().enumerate() {
            let group = self.commands.iter().rev().find_map(|command| match command {
                Recorded::SetBindGroup { index: bound, group } if *bound == index as u32 => Some(*group),
                _ => None,
            });
            let Some(group) = group else {
                return Err(RhiError::Validation("a bind group must be set before draw".into()));
            };
            let bound = lock(&self.shared.groups)?.get(group).map_err(|_| RhiError::InvalidHandle("bind group"))?.layout;
            if bound != *layout {
                return Err(RhiError::Validation("bind group layout does not match the pipeline".into()));
            }
        }
        if indexed {
            let index = self.commands.iter().any(|command| matches!(command, Recorded::SetIndexBuffer { .. }));
            let vertex = self.commands.iter().any(|command| matches!(command, Recorded::SetVertexBuffer { .. }));
            if !index {
                return Err(RhiError::Validation("an index buffer must be bound before an indexed draw".into()));
            }
            if !vertex {
                return Err(RhiError::Validation("a vertex buffer must be bound before an indexed draw".into()));
            }
        }
        let attached = match self.depth_view {
            Some(id) => match lock(&self.shared.views)?.get(id).map_err(|_| RhiError::InvalidHandle("depth view"))? {
                GpuView::Depth { format, .. } => Some(*format),
                GpuView::Sampled { .. } | GpuView::Color(_) | GpuView::Hdr { .. } | GpuView::Cube { .. } => {
                    return Err(RhiError::InvalidResource("depth view"))
                }
            },
            None => None,
        };
        match (required, attached) {
            (None, None) => {}
            (None, Some(_)) => return Err(RhiError::Validation("a color pipeline cannot use a depth attachment".into())),
            (Some(_), None) => return Err(RhiError::Validation("a depth pipeline requires a depth attachment".into())),
            (Some(expected), Some(actual)) if expected != actual => {
                return Err(RhiError::Validation("depth format does not match the pipeline".into()));
            }
            (Some(_), Some(_)) => {}
        }
        if count == 0 || instances == 0 {
            return Err(RhiError::Validation("draw counts must be non-zero".into()));
        }
        Ok(())
    }
}

impl CommandEncoder for WgpuEncoder {
    fn begin_render_pass(&mut self, desc: &RenderPassDesc) -> Result<(), RhiError> {
        if self.pass_open {
            return Err(RhiError::Validation("render pass is already open".into()));
        }
        let color = desc.colors.first().ok_or(RhiError::Validation("render pass needs a color attachment".into()))?;
        let color_handle = color.target.raw();
        let swapchain_sized = match lock(&self.shared.views)?.get(color_handle).map_err(|_| RhiError::InvalidHandle("frame view"))? {
            GpuView::Color(_) => true,
            GpuView::Hdr { width, height, .. } => {
                self.attachment_width = *width;
                self.attachment_height = *height;
                false
            }
            GpuView::Sampled { .. } | GpuView::Depth { .. } | GpuView::Cube { .. } => return Err(RhiError::InvalidResource("frame view")),
        };
        self.view = Some(color_handle);
        self.load = Some(color.load);
        self.store = color.store;
        self.label = desc.label.clone().unwrap_or_else(|| "JARVIG.MainClearPass".into());
        self.commands.clear();
        self.depth_view = None;
        self.depth_clear = None;
        if let Some(depth) = &desc.depth {
            match lock(&self.shared.views)?.get(depth.target.raw()).map_err(|_| RhiError::InvalidHandle("depth view"))? {
                GpuView::Depth { format, .. } if *format == jarvig_rhi::TextureFormat::Depth32Float => {}
                GpuView::Depth { .. } => {
                    return Err(RhiError::Validation("depth attachment is not a depth texture".into()));
                }
                GpuView::Sampled { .. } | GpuView::Color(_) | GpuView::Hdr { .. } | GpuView::Cube { .. } => {
                    return Err(RhiError::InvalidResource("depth view"))
                }
            }
            self.depth_view = Some(depth.target.raw());
            self.depth_clear = match depth.load {
                jarvig_rhi::DepthLoadOp::Clear(value) => Some(value),
                jarvig_rhi::DepthLoadOp::Load => None,
            };
            self.depth_store = depth.store;
        }
        if swapchain_sized {
            let (width, height) = {
                let config = lock(&self.shared.config)?;
                (config.width, config.height)
            };
            self.attachment_width = width;
            self.attachment_height = height;
        }
        self.pass_open = true;
        Ok(())
    }

    fn set_viewport(&mut self, viewport: jarvig_rhi::Viewport) -> Result<(), RhiError> {
        if !self.pass_open {
            return Err(RhiError::Validation("a viewport is set inside a render pass".into()));
        }
        viewport.check(self.attachment_width, self.attachment_height)?;
        self.commands.push(Recorded::SetViewport(viewport));
        Ok(())
    }

    fn set_scissor(&mut self, scissor: jarvig_rhi::ScissorRect) -> Result<(), RhiError> {
        if !self.pass_open {
            return Err(RhiError::Validation("a scissor is set inside a render pass".into()));
        }
        scissor.check(self.attachment_width, self.attachment_height)?;
        self.commands.push(Recorded::SetScissor(scissor));
        Ok(())
    }

    fn set_pipeline(&mut self, pipeline: jarvig_rhi::PipelineId) -> Result<(), RhiError> {
        if !self.pass_open {
            return Err(RhiError::Validation("a pipeline is bound inside a render pass".into()));
        }
        lock(&self.shared.pipelines)?.get(pipeline.raw()).map_err(|_| RhiError::InvalidHandle("pipeline"))?;
        self.commands.push(Recorded::SetPipeline(pipeline.raw()));
        Ok(())
    }

    fn set_bind_group(&mut self, index: u32, group: BindGroupId) -> Result<(), RhiError> {
        if !self.pass_open {
            return Err(RhiError::Validation("a bind group is set inside a render pass".into()));
        }
        if index > 3 {
            return Err(RhiError::Unsupported("bind group index is outside 0..=3"));
        }
        lock(&self.shared.groups)?.get(group.raw()).map_err(|_| RhiError::InvalidHandle("bind group"))?;
        self.commands.push(Recorded::SetBindGroup { index, group: group.raw() });
        Ok(())
    }

    fn set_vertex_buffer(&mut self, slot: u32, buffer: BufferId, offset: u64) -> Result<(), RhiError> {
        self.bind_buffer(buffer, BufferUsage::Vertex, offset, 4)?;
        self.commands.push(Recorded::SetVertexBuffer { slot, buffer: buffer.raw(), offset });
        Ok(())
    }

    fn set_index_buffer(&mut self, buffer: BufferId, format: IndexFormat, offset: u64) -> Result<(), RhiError> {
        let align = match format {
            IndexFormat::Uint16 => 2,
            IndexFormat::Uint32 => 4,
        };
        self.bind_buffer(buffer, BufferUsage::Index, offset, align)?;
        self.commands.push(Recorded::SetIndexBuffer { buffer: buffer.raw(), format, offset });
        Ok(())
    }

    fn draw(&mut self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32) -> Result<(), RhiError> {
        self.require_draw(vertex_count, instance_count, false)?;
        self.commands.push(Recorded::Draw {
            vertices: vertex_count,
            instances: instance_count,
            first_vertex,
            first_instance,
        });
        Ok(())
    }

    fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    ) -> Result<(), RhiError> {
        self.require_draw(index_count, instance_count, true)?;
        self.commands.push(Recorded::DrawIndexed {
            indices: index_count,
            instances: instance_count,
            first_index,
            base_vertex,
            first_instance,
        });
        Ok(())
    }

    fn end_render_pass(&mut self) -> Result<(), RhiError> {
        if !self.pass_open {
            return Err(RhiError::Validation("no render pass is open".into()));
        }
        let view_id = self.view.ok_or(RhiError::InvalidResource("frame view"))?;
        let views = lock(&self.shared.views)?;
        let view = match views.get(view_id).map_err(|_| RhiError::InvalidHandle("frame view"))? {
            view => color_attachment(view).cloned().ok_or(RhiError::InvalidResource("frame view"))?,
        };
        let depth_view = match self.depth_view {
            Some(id) => match views.get(id).map_err(|_| RhiError::InvalidHandle("depth view"))? {
                GpuView::Depth { view, .. } => Some(view.clone()),
                GpuView::Sampled { .. } | GpuView::Color(_) | GpuView::Hdr { .. } | GpuView::Cube { .. } => {
                    return Err(RhiError::InvalidResource("depth view"))
                }
            },
            None => None,
        };
        drop(views);
        let depth_attachment = depth_view.as_ref().map(|depth| {
            let load = match self.depth_clear {
                Some(value) => wgpu::LoadOp::Clear(value),
                None => wgpu::LoadOp::Load,
            };
            let store = match self.depth_store {
                jarvig_rhi::StoreOp::Store => wgpu::StoreOp::Store,
                jarvig_rhi::StoreOp::Discard => wgpu::StoreOp::Discard,
            };
            wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations { load, store }),
                stencil_ops: None,
            }
        });
        let load = match self.load.unwrap_or(jarvig_rhi::LoadOp::Load) {
            jarvig_rhi::LoadOp::Clear(color) => wgpu::LoadOp::Clear(wgpu::Color {
                r: color.r as f64,
                g: color.g as f64,
                b: color.b as f64,
                a: color.a as f64,
            }),
            jarvig_rhi::LoadOp::Load => wgpu::LoadOp::Load,
        };
        let store = match self.store {
            jarvig_rhi::StoreOp::Store => wgpu::StoreOp::Store,
            jarvig_rhi::StoreOp::Discard => wgpu::StoreOp::Discard,
        };
        let encoder = self.encoder.as_mut().ok_or(RhiError::Validation("encoder finished".into()))?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&self.label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations { load, store },
            })],
            depth_stencil_attachment: depth_attachment,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let pipelines = lock(&self.shared.pipelines)?;
        let groups = lock(&self.shared.groups)?;
        let gpu_buffers = lock(&self.shared.buffers)?;
        let mut draws = 0u64;
        let mut indexed = 0u64;
        for command in &self.commands {
            match command {
                Recorded::SetPipeline(id) => {
                    let pipeline = pipelines.get(*id).map_err(|_| RhiError::InvalidHandle("pipeline"))?;
                    pass.set_pipeline(&pipeline.pipeline);
                }
                Recorded::SetBindGroup { index, group } => {
                    let group = groups.get(*group).map_err(|_| RhiError::InvalidHandle("bind group"))?;
                    pass.set_bind_group(*index, &group.group, &[]);
                }
                Recorded::SetVertexBuffer { slot, buffer, offset } => {
                    let gpu = gpu_buffers.get(*buffer).map_err(|_| RhiError::InvalidHandle("buffer"))?;
                    pass.set_vertex_buffer(*slot, gpu.buffer.slice(*offset..));
                }
                Recorded::SetIndexBuffer { buffer, format, offset } => {
                    let gpu = gpu_buffers.get(*buffer).map_err(|_| RhiError::InvalidHandle("buffer"))?;
                    pass.set_index_buffer(gpu.buffer.slice(*offset..), map_index_format(*format));
                }
                Recorded::SetViewport(viewport) => {
                    pass.set_viewport(viewport.x, viewport.y, viewport.width, viewport.height, viewport.min_depth, viewport.max_depth);
                }
                Recorded::SetScissor(scissor) => {
                    pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
                }
                Recorded::Draw { vertices, instances, first_vertex, first_instance } => {
                    pass.draw(
                        *first_vertex..first_vertex.saturating_add(*vertices),
                        *first_instance..first_instance.saturating_add(*instances),
                    );
                    draws += 1;
                }
                Recorded::DrawIndexed { indices, instances, first_index, base_vertex, first_instance } => {
                    pass.draw_indexed(
                        *first_index..first_index.saturating_add(*indices),
                        *base_vertex,
                        *first_instance..first_instance.saturating_add(*instances),
                    );
                    draws += 1;
                    indexed += 1;
                }
            }
        }
        drop(pass);
        drop(pipelines);
        drop(groups);
        drop(gpu_buffers);
        *lock(&self.shared.draws)? += draws;
        *lock(&self.shared.indexed_draws)? += indexed;
        let used = self.used_resources()?;
        self.accumulated.append(used);
        self.pass_open = false;
        Ok(())
    }

    fn begin_compute_pass(&mut self, _desc: &ComputePassDesc) -> Result<(), RhiError> {
        Err(RhiError::Unsupported("compute pass"))
    }

    fn end_compute_pass(&mut self) -> Result<(), RhiError> {
        Err(RhiError::Unsupported("compute pass"))
    }

    fn write_timestamp(&mut self, _pool: QueryPoolId, _index: u32) -> Result<(), RhiError> {
        Err(RhiError::Unsupported("timestamp queries"))
    }

    fn finish(mut self: Box<Self>) -> Result<CommandBufferId, RhiError> {
        if self.pass_open {
            return Err(RhiError::Validation("a pass is still open".into()));
        }
        let used = std::mem::take(&mut self.accumulated);
        let encoder = self.encoder.take().ok_or(RhiError::Validation("encoder finished".into()))?;
        *lock(&self.shared.pending)? = Some(Pending {
            command: encoder.finish(),
            buffers: used.buffers,
            views: used.views,
            textures: used.textures,
            pipelines: used.pipelines,
            groups: used.groups,
            shaders: used.shaders,
            layouts: used.layouts,
            samplers: used.samplers,
        });
        Ok(CommandBufferId(1))
    }
}

struct Used {
    buffers: Vec<RawHandle>,
    views: Vec<RawHandle>,
    textures: Vec<RawHandle>,
    pipelines: Vec<RawHandle>,
    groups: Vec<RawHandle>,
    shaders: Vec<RawHandle>,
    layouts: Vec<RawHandle>,
    samplers: Vec<RawHandle>,
}

impl WgpuEncoder {
    /// Handles this submission must keep alive until `flush` advances the completed serial.
    fn used_resources(&self) -> Result<Used, RhiError> {
        let mut buffers = Vec::new();
        let mut pipelines = Vec::new();
        let mut groups = Vec::new();
        for command in &self.commands {
            match command {
                Recorded::SetPipeline(id) => pipelines.push(*id),
                Recorded::SetBindGroup { group, .. } => groups.push(*group),
                Recorded::SetVertexBuffer { buffer, .. } | Recorded::SetIndexBuffer { buffer, .. } => buffers.push(*buffer),
                Recorded::SetViewport(_) | Recorded::SetScissor(_) | Recorded::Draw { .. } | Recorded::DrawIndexed { .. } => {}
            }
        }
        let mut shaders = Vec::new();
        let mut layouts = Vec::new();
        let mut views = Vec::new();
        let mut samplers = Vec::new();
        {
            let table = lock(&self.shared.pipelines)?;
            for pipeline in &pipelines {
                if let Ok(info) = table.get(*pipeline) {
                    shaders.push(info.shader);
                    layouts.extend(info.layouts.iter().copied());
                }
            }
        }
        {
            let table = lock(&self.shared.groups)?;
            for group in &groups {
                if let Ok(info) = table.get(*group) {
                    buffers.extend(info.buffers.iter().copied());
                    views.extend(info.views.iter().copied());
                    samplers.extend(info.samplers.iter().copied());
                }
            }
        }
        let mut textures = Vec::new();
        if let Some(view) = self.view {
            views.push(view);
        }
        if let Some(view) = self.depth_view {
            views.push(view);
        }
        {
            let table = lock(&self.shared.views)?;
            for view in &views {
                if let Ok(GpuView::Sampled { texture, .. } | GpuView::Depth { texture, .. } | GpuView::Hdr { texture, .. } | GpuView::Cube { texture, .. }) =
                    table.get(*view)
                {
                    textures.push(*texture);
                }
            }
        }
        Ok(Used { buffers, views, textures, pipelines, groups, shaders, layouts, samplers })
    }
}

impl Used {
    fn append(&mut self, extra: Used) {
        self.buffers.extend(extra.buffers);
        self.views.extend(extra.views);
        self.textures.extend(extra.textures);
        self.pipelines.extend(extra.pipelines);
        self.groups.extend(extra.groups);
        self.shaders.extend(extra.shaders);
        self.layouts.extend(extra.layouts);
        self.samplers.extend(extra.samplers);
    }
}

impl Default for Used {
    fn default() -> Self {
        Self {
            buffers: Vec::new(),
            views: Vec::new(),
            textures: Vec::new(),
            pipelines: Vec::new(),
            groups: Vec::new(),
            shaders: Vec::new(),
            layouts: Vec::new(),
            samplers: Vec::new(),
        }
    }
}

impl Device for WgpuDevice {
    fn label(&self) -> Option<&str> {
        Some("JARVIG.Device")
    }

    fn capabilities(&self) -> DeviceCapabilities {
        DeviceCapabilities {
            timestamp_queries: false,
            max_texture_dimension_2d: self.shared.device.limits().max_texture_dimension_2d,
        }
    }

    fn graphics_queue(&self) -> QueueId {
        QueueId(0)
    }

    fn create_buffer(&mut self, desc: &BufferDesc) -> Result<BufferId, RhiError> {
        if desc.size == 0 {
            return Err(RhiError::Validation("buffer size must be non-zero".into()));
        }
        if let Some(contents) = &desc.contents {
            if contents.len() as u64 != desc.size {
                return Err(RhiError::Validation("buffer contents do not match size".into()));
            }
        }
        let usage = match desc.usage {
            BufferUsage::Vertex => wgpu::BufferUsages::VERTEX,
            BufferUsage::Index => wgpu::BufferUsages::INDEX,
            BufferUsage::Uniform => wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            BufferUsage::Storage => wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            BufferUsage::CopySrc | BufferUsage::CopyDst => {
                return Err(RhiError::Unsupported("only vertex, index, uniform, and storage buffers are implemented"));
            }
        };
        let padded = align_buffer_size(desc.size);
        let buffer = self.shared.device.create_buffer(&wgpu::BufferDescriptor {
            label: desc.label.as_deref(),
            size: padded,
            usage,
            mapped_at_creation: desc.contents.is_some(),
        });
        if let Some(contents) = &desc.contents {
            let mut bytes = contents.clone();
            bytes.resize(padded as usize, 0);
            buffer.slice(..).get_mapped_range_mut().copy_from_slice(&bytes);
            buffer.unmap();
        }
        let handle = lock(&self.shared.buffers)?.insert(
            GpuBuffer { buffer, usage: desc.usage, size: desc.size },
            desc.label.clone(),
            desc.size,
        );
        *lock(&self.shared.buffer_count)? += 1;
        Ok(BufferId(handle))
    }
    fn create_texture(&mut self, desc: &TextureDesc) -> Result<TextureId, RhiError> {
        if desc.width == 0 || desc.height == 0 || desc.mip_count == 0 {
            return Err(RhiError::Validation("texture dimensions are invalid".into()));
        }
        let (format, usage) = match (desc.format, desc.usage) {
            (jarvig_rhi::TextureFormat::Depth32Float, jarvig_rhi::TextureUsage::RenderAttachment) => {
                (wgpu::TextureFormat::Depth32Float, wgpu::TextureUsages::RENDER_ATTACHMENT)
            }
            (jarvig_rhi::TextureFormat::Rgba8Unorm, jarvig_rhi::TextureUsage::Sampled) => {
                (wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
            }
            (jarvig_rhi::TextureFormat::Rgba8UnormSrgb, jarvig_rhi::TextureUsage::Sampled) => {
                (wgpu::TextureFormat::Rgba8UnormSrgb, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
            }
            (jarvig_rhi::TextureFormat::Rgba16Float, jarvig_rhi::TextureUsage::ColorTarget) => (
                wgpu::TextureFormat::Rgba16Float,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            _ => return Err(RhiError::Unsupported("that texture format and usage are not implemented")),
        };
        let texture = self.shared.device.create_texture(&wgpu::TextureDescriptor {
            label: desc.label.as_deref(),
            size: wgpu::Extent3d { width: desc.width, height: desc.height, depth_or_array_layers: 1 },
            mip_level_count: desc.mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let texel_bytes = if desc.format == jarvig_rhi::TextureFormat::Rgba16Float { 8 } else { 4 };
        let bytes = u64::from(desc.width) * u64::from(desc.height) * texel_bytes;
        let handle = lock(&self.shared.textures)?.insert(
            GpuTexture { texture, format: desc.format, width: desc.width, height: desc.height, mip_count: desc.mip_count, layers: 1 },
            desc.label.clone(),
            bytes,
        );
        Ok(TextureId(handle))
    }
    fn create_texture_view(&mut self, texture: TextureId) -> Result<TextureViewId, RhiError> {
        let textures = lock(&self.shared.textures)?;
        let gpu = textures.get(texture.raw()).map_err(|_| RhiError::InvalidHandle("texture"))?;
        let format = gpu.format;
        let texture_handle = texture.raw();
        let width = gpu.width;
        let height = gpu.height;
        let view = gpu.texture.create_view(&wgpu::TextureViewDescriptor::default());
        drop(textures);
        let gpu_view = match format {
            jarvig_rhi::TextureFormat::Depth32Float => GpuView::Depth { view, texture: texture_handle, format },
            jarvig_rhi::TextureFormat::Rgba8Unorm | jarvig_rhi::TextureFormat::Rgba8UnormSrgb => {
                GpuView::Sampled { view, texture: texture_handle }
            }
            jarvig_rhi::TextureFormat::Rgba16Float => GpuView::Hdr { view, texture: texture_handle, width, height },
            jarvig_rhi::TextureFormat::Bgra8Unorm | jarvig_rhi::TextureFormat::Bgra8UnormSrgb => {
                return Err(RhiError::Unsupported("that texture cannot be viewed"))
            }
        };
        let handle = lock(&self.shared.views)?.insert(gpu_view, None, 0);
        Ok(TextureViewId(handle))
    }

    fn create_cube_texture(&mut self, desc: &jarvig_rhi::CubeTextureDesc) -> Result<TextureId, RhiError> {
        if desc.size == 0 || !desc.size.is_power_of_two() || desc.mip_count == 0 {
            return Err(RhiError::Validation("cube texture dimensions are invalid".into()));
        }
        if desc.mip_count > desc.size.ilog2() + 1 {
            return Err(RhiError::Validation("cube mip count is invalid".into()));
        }
        if desc.format != jarvig_rhi::TextureFormat::Rgba16Float || desc.usage != jarvig_rhi::TextureUsage::ColorTarget {
            return Err(RhiError::Unsupported("only an HDR color cube is implemented"));
        }
        let texture = self.shared.device.create_texture(&wgpu::TextureDescriptor {
            label: desc.label.as_deref(),
            size: wgpu::Extent3d { width: desc.size, height: desc.size, depth_or_array_layers: 6 },
            mip_level_count: desc.mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let bytes = u64::from(desc.size) * u64::from(desc.size) * 8 * 6;
        let handle = lock(&self.shared.textures)?.insert(
            GpuTexture {
                texture,
                format: desc.format,
                width: desc.size,
                height: desc.size,
                mip_count: desc.mip_count,
                layers: 6,
            },
            desc.label.clone(),
            bytes,
        );
        Ok(TextureId(handle))
    }

    fn create_cube_face_view(&mut self, texture: TextureId, face: u32, mip: u32) -> Result<TextureViewId, RhiError> {
        let textures = lock(&self.shared.textures)?;
        let gpu = textures.get(texture.raw()).map_err(|_| RhiError::InvalidHandle("texture"))?;
        if gpu.layers != 6 || face >= 6 || mip >= gpu.mip_count {
            return Err(RhiError::Validation("cube face view is out of range".into()));
        }
        let edge = (gpu.width >> mip).max(1);
        let format = gpu.format;
        let texture_handle = texture.raw();
        let view = gpu.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("JARVIG.ReflectionProbe.Face"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            format: Some(wgpu::TextureFormat::Rgba16Float),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: mip,
            mip_level_count: Some(1),
            base_array_layer: face,
            array_layer_count: Some(1),
            usage: None,
        });
        drop(textures);
        if format != jarvig_rhi::TextureFormat::Rgba16Float {
            return Err(RhiError::Unsupported("that cube cannot be rendered"));
        }
        let handle = lock(&self.shared.views)?.insert(
            GpuView::Hdr { view, texture: texture_handle, width: edge, height: edge },
            None,
            0,
        );
        Ok(TextureViewId(handle))
    }

    fn create_cube_sample_view(&mut self, texture: TextureId) -> Result<TextureViewId, RhiError> {
        let textures = lock(&self.shared.textures)?;
        let gpu = textures.get(texture.raw()).map_err(|_| RhiError::InvalidHandle("texture"))?;
        if gpu.layers != 6 {
            return Err(RhiError::InvalidResource("cube texture"));
        }
        let mips = gpu.mip_count;
        let texture_handle = texture.raw();
        let view = gpu.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("JARVIG.ReflectionProbe.Cube"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            format: Some(wgpu::TextureFormat::Rgba16Float),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(mips),
            base_array_layer: 0,
            array_layer_count: Some(6),
            usage: None,
        });
        drop(textures);
        let handle = lock(&self.shared.views)?.insert(GpuView::Cube { view, texture: texture_handle }, None, 0);
        Ok(TextureViewId(handle))
    }
    fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<SamplerId, RhiError> {
        let anisotropy = sampler_anisotropy(desc);
        let sampler = self.shared.device.create_sampler(&wgpu::SamplerDescriptor {
            label: desc.label.as_deref(),
            address_mode_u: map_address(desc.address_u),
            address_mode_v: map_address(desc.address_v),
            address_mode_w: map_address(desc.address_w),
            mag_filter: map_filter(desc.mag_filter),
            min_filter: map_filter(desc.min_filter),
            mipmap_filter: map_filter(desc.mip_filter),
            anisotropy_clamp: anisotropy,
            ..Default::default()
        });
        let handle = lock(&self.shared.samplers)?.insert(sampler, desc.label.clone(), 0);
        Ok(SamplerId(handle))
    }
    fn create_shader_module(&mut self, desc: &ShaderModuleDesc) -> Result<ShaderModuleId, RhiError> {
        let jarvig_rhi::ShaderSource::Wgsl(source) = &desc.source;
        if source.is_empty() {
            return Err(RhiError::Validation("shader source is empty".into()));
        }
        self.shared.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self.shared.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: desc.label.as_deref(),
            source: wgpu::ShaderSource::Wgsl(source.clone().into()),
        });
        if let Some(error) = pollster::block_on(self.shared.device.pop_error_scope()) {
            drop(module);
            return Err(RhiError::Validation(error.to_string()));
        }
        let handle = lock(&self.shared.shaders)?.insert(module, desc.label.clone(), 0);
        Ok(ShaderModuleId(handle))
    }
    fn create_bind_group_layout(&mut self, desc: &BindGroupLayoutDesc) -> Result<BindGroupLayoutId, RhiError> {
        if desc.entries.is_empty() {
            return Err(RhiError::Validation("bind group layout needs an entry".into()));
        }
        let mut entries = Vec::with_capacity(desc.entries.len());
        let mut recorded = Vec::with_capacity(desc.entries.len());
        for entry in &desc.entries {
            let ty = match entry.kind {
                BindingType::UniformBuffer => wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                BindingType::Texture => wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                BindingType::TextureCube => wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false,
                },
                BindingType::Sampler => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                BindingType::StorageBuffer => wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            };
            recorded.push((entry.binding, entry.kind));
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: entry.binding,
                visibility: map_stage(entry.stage),
                ty,
                count: None,
            });
        }
        let layout = self.shared.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: desc.label.as_deref(),
            entries: &entries,
        });
        let handle = lock(&self.shared.layouts)?.insert(GpuLayout { layout, entries: recorded }, desc.label.clone(), 0);
        Ok(BindGroupLayoutId(handle))
    }

    fn create_bind_group(&mut self, desc: &BindGroupDesc) -> Result<BindGroupId, RhiError> {
        let layouts = lock(&self.shared.layouts)?;
        let gpu_layout = layouts.get(desc.layout.raw()).map_err(|_| RhiError::InvalidHandle("bind group layout"))?.layout.clone();
        let expected = layouts.get(desc.layout.raw()).map_err(|_| RhiError::InvalidHandle("bind group layout"))?.entries.clone();
        drop(layouts);
        if desc.entries.len() != expected.len() {
            return Err(RhiError::Validation("bind group entries do not match the layout".into()));
        }
        let mut held_buffers = Vec::new();
        let mut held_views = Vec::new();
        let mut held_samplers = Vec::new();
        let mut buffer_handles = Vec::new();
        let mut view_handles = Vec::new();
        let mut sampler_handles = Vec::new();
        let mut order = Vec::new();
        for entry in &desc.entries {
            let kind = expected
                .iter()
                .find(|(binding, _)| *binding == entry.binding)
                .map(|(_, kind)| *kind)
                .ok_or(RhiError::Validation("bind group binding is not in the layout".into()))?;
            match entry.resource {
                jarvig_rhi::BindResource::Buffer(buffer) => {
                    let buffers = lock(&self.shared.buffers)?;
                    let record = buffers.get(buffer.raw()).map_err(|_| RhiError::InvalidHandle("buffer"))?;
                    let usage_ok = match kind {
                        BindingType::UniformBuffer => record.usage == BufferUsage::Uniform,
                        BindingType::StorageBuffer => record.usage == BufferUsage::Storage,
                        _ => false,
                    };
                    if !usage_ok {
                        return Err(RhiError::Validation("bind group buffer does not match the layout".into()));
                    }
                    held_buffers.push((entry.binding, record.buffer.clone()));
                    drop(buffers);
                    buffer_handles.push(buffer.raw());
                    order.push(0u8);
                }
                jarvig_rhi::BindResource::TextureView(view) => {
                    let views = lock(&self.shared.views)?;
                    let gpu = match (kind, views.get(view.raw()).map_err(|_| RhiError::InvalidHandle("texture view"))?) {
                        (BindingType::Texture, GpuView::Sampled { view, .. } | GpuView::Hdr { view, .. }) => view.clone(),
                        (BindingType::TextureCube, GpuView::Cube { view, .. }) => view.clone(),
                        _ => return Err(RhiError::Validation("bind group entry does not match the layout".into())),
                    };
                    drop(views);
                    held_views.push((entry.binding, gpu));
                    view_handles.push(view.raw());
                    order.push(1u8);
                }
                jarvig_rhi::BindResource::Sampler(sampler) => {
                    if kind != BindingType::Sampler {
                        return Err(RhiError::Validation("bind group entry does not match the layout".into()));
                    }
                    let samplers = lock(&self.shared.samplers)?;
                    let gpu = samplers.get(sampler.raw()).map_err(|_| RhiError::InvalidHandle("sampler"))?.clone();
                    drop(samplers);
                    held_samplers.push((entry.binding, gpu));
                    sampler_handles.push(sampler.raw());
                    order.push(2u8);
                }
            }
        }
        let mut buffer_cursor = 0usize;
        let mut view_cursor = 0usize;
        let mut sampler_cursor = 0usize;
        let mut resources = Vec::with_capacity(order.len());
        for kind in order {
            match kind {
                0 => {
                    let (binding, buffer) = &held_buffers[buffer_cursor];
                    buffer_cursor += 1;
                    resources.push(wgpu::BindGroupEntry { binding: *binding, resource: buffer.as_entire_binding() });
                }
                1 => {
                    let (binding, view) = &held_views[view_cursor];
                    view_cursor += 1;
                    resources.push(wgpu::BindGroupEntry { binding: *binding, resource: wgpu::BindingResource::TextureView(view) });
                }
                _ => {
                    let (binding, sampler) = &held_samplers[sampler_cursor];
                    sampler_cursor += 1;
                    resources.push(wgpu::BindGroupEntry { binding: *binding, resource: wgpu::BindingResource::Sampler(sampler) });
                }
            }
        }
        let group = self.shared.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: desc.label.as_deref(),
            layout: &gpu_layout,
            entries: &resources,
        });
        let handle = lock(&self.shared.groups)?.insert(
            GpuGroup { layout: desc.layout.raw(), buffers: buffer_handles, views: view_handles, samplers: sampler_handles, group },
            desc.label.clone(),
            0,
        );
        Ok(BindGroupId(handle))
    }

    fn write_buffer(&mut self, buffer: BufferId, offset: u64, data: &[u8]) -> Result<(), RhiError> {
        let buffers = lock(&self.shared.buffers)?;
        let record = buffers.get(buffer.raw()).map_err(|_| RhiError::InvalidHandle("buffer"))?;
        if !matches!(record.usage, BufferUsage::Uniform | BufferUsage::Storage) {
            return Err(RhiError::Validation("only a uniform or storage buffer can be written".into()));
        }
        if data.is_empty() || offset % 4 != 0 || data.len() % 4 != 0 || offset + data.len() as u64 > record.size {
            return Err(RhiError::Validation("buffer write is empty, misaligned, or out of range".into()));
        }
        self.shared.queue.write_buffer(&record.buffer, offset, data);
        Ok(())
    }

    fn write_texture(&mut self, texture: TextureId, data: &[u8]) -> Result<(), RhiError> {
        self.write_texture_mip(texture, 0, data)
    }

    fn write_texture_mip(&mut self, texture: TextureId, mip: u32, data: &[u8]) -> Result<(), RhiError> {
        let textures = lock(&self.shared.textures)?;
        let record = textures.get(texture.raw()).map_err(|_| RhiError::InvalidHandle("texture"))?;
        if !matches!(record.format, jarvig_rhi::TextureFormat::Rgba8Unorm | jarvig_rhi::TextureFormat::Rgba8UnormSrgb) {
            return Err(RhiError::Validation("only an RGBA8 texture can be written".into()));
        }
        if mip >= record.mip_count {
            return Err(RhiError::Validation("texture mip is out of range".into()));
        }
        let width = (record.width >> mip).max(1);
        let height = (record.height >> mip).max(1);
        let packed_row = width.saturating_mul(4);
        if data.len() != packed_row as usize * height as usize {
            return Err(RhiError::Validation("texture write is not tightly packed RGBA8".into()));
        }
        let stride = packed_row.saturating_add(255) & !255;
        let upload = if stride == packed_row {
            data.to_vec()
        } else {
            let mut padded = vec![0u8; stride as usize * height as usize];
            for row in 0..height as usize {
                let from = row * packed_row as usize;
                let to = row * stride as usize;
                padded[to..to + packed_row as usize].copy_from_slice(&data[from..from + packed_row as usize]);
            }
            padded
        };
        let gpu = record.texture.clone();
        drop(textures);
        self.shared.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &gpu, mip_level: mip, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &upload,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        Ok(())
    }

    fn create_render_pipeline(&mut self, desc: &RenderPipelineDesc) -> Result<PipelineId, RhiError> {
        if desc.topology != jarvig_rhi::PrimitiveTopology::TriangleList {
            return Err(RhiError::Unsupported("only triangle lists are implemented"));
        }
        let mut attribute_sets = Vec::with_capacity(desc.vertex_buffers.len());
        for layout in &desc.vertex_buffers {
            layout.check()?;
            let mut attributes = Vec::with_capacity(layout.attributes.len());
            for attribute in &layout.attributes {
                attributes.push(wgpu::VertexAttribute {
                    format: map_vertex_format(attribute.format),
                    offset: attribute.offset,
                    shader_location: attribute.shader_location,
                });
            }
            attribute_sets.push((layout.stride, layout.step_mode, attributes));
        }
        let vertex_layouts: Vec<wgpu::VertexBufferLayout> = attribute_sets
            .iter()
            .map(|(stride, step, attributes)| wgpu::VertexBufferLayout {
                array_stride: *stride,
                step_mode: match step {
                    VertexStepMode::Vertex => wgpu::VertexStepMode::Vertex,
                    VertexStepMode::Instance => wgpu::VertexStepMode::Instance,
                },
                attributes,
            })
            .collect();
        let mut gpu_layouts = Vec::with_capacity(desc.layouts.len());
        {
            let layouts = self.shared.layouts.lock().map_err(|_| RhiError::DeviceLost)?;
            for layout in &desc.layouts {
                gpu_layouts.push(
                    layouts
                        .get(layout.raw())
                        .map_err(|_| RhiError::InvalidHandle("bind group layout"))?
                        .layout
                        .clone(),
                );
            }
        }
        let layout_refs: Vec<&wgpu::BindGroupLayout> = gpu_layouts.iter().collect();
        let pipeline_layout = if layout_refs.is_empty() {
            None
        } else {
            Some(self.shared.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: desc.label.as_deref(),
                bind_group_layouts: &layout_refs,
                push_constant_ranges: &[],
            }))
        };
        let shaders = self.shared.shaders.lock().map_err(|_| RhiError::DeviceLost)?;
        let module = shaders.get(desc.shader.raw()).map_err(|_| RhiError::InvalidHandle("shader"))?;
        let format = unmap_format(desc.color_format)?;
        let depth_state = match desc.depth {
            Some(depth) => {
                if depth.format != jarvig_rhi::TextureFormat::Depth32Float {
                    return Err(RhiError::Validation("depth state format is not a depth format".into()));
                }
                Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: depth.write_enabled,
                    depth_compare: map_compare(depth.compare),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                })
            }
            None => None,
        };
        self.shared.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = self.shared.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: desc.label.as_deref(),
            layout: pipeline_layout.as_ref(),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(&desc.vertex_entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_layouts,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: match desc.cull {
                    jarvig_rhi::CullMode::Back => Some(wgpu::Face::Back),
                    jarvig_rhi::CullMode::Front => Some(wgpu::Face::Front),
                    jarvig_rhi::CullMode::None => None,
                },
                ..Default::default()
            },
            depth_stencil: depth_state,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(&desc.fragment_entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        drop(shaders);
        self.shared.device.poll(wgpu::Maintain::Wait);
        if let Some(error) = pollster::block_on(self.shared.device.pop_error_scope()) {
            return Err(RhiError::Validation(error.to_string()));
        }
        let handle = lock(&self.shared.pipelines)?.insert(
            GpuPipeline {
                pipeline,
                layouts: desc.layouts.iter().map(|layout| layout.raw()).collect(),
                depth: desc.depth.map(|depth| depth.format),
                shader: desc.shader.raw(),
            },
            desc.label.clone(),
            0,
        );
        *lock(&self.shared.pipeline_count)? += 1;
        Ok(PipelineId(handle))
    }
    fn create_compute_pipeline(&mut self, _desc: &ComputePipelineDesc) -> Result<PipelineId, RhiError> {
        Err(RhiError::Unsupported("pipelines are a later ticket"))
    }

    fn create_command_encoder(&mut self, label: Option<&str>) -> Result<Box<dyn CommandEncoder>, RhiError> {
        let encoder = self.shared.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some(label.unwrap_or("JARVIG.MainClearPass")),
        });
        Ok(Box::new(WgpuEncoder {
            shared: Arc::clone(&self.shared),
            encoder: Some(encoder),
            label: "JARVIG.MainClearPass".into(),
            view: None,
            load: None,
            store: jarvig_rhi::StoreOp::Store,
            commands: Vec::new(),
            pass_open: false,
            depth_view: None,
            depth_clear: None,
            depth_store: jarvig_rhi::StoreOp::Store,
            attachment_width: 0,
            attachment_height: 0,
            accumulated: Used::default(),
        }))
    }

    fn create_fence(&mut self, _desc: &FenceDesc) -> Result<FenceId, RhiError> {
        Err(RhiError::Unsupported("fences are a later ticket"))
    }
    fn create_query_pool(&mut self, _desc: &QueryPoolDesc) -> Result<QueryPoolId, RhiError> {
        Err(RhiError::Unsupported("timestamp queries"))
    }
    fn create_surface(&mut self, _desc: &SurfaceDesc) -> Result<SurfaceId, RhiError> {
        Err(RhiError::Unsupported("use attach_window for the native surface"))
    }
    fn create_swapchain(&mut self, _desc: &SwapchainDesc) -> Result<SwapchainId, RhiError> {
        Err(RhiError::Unsupported("the window swapchain already exists"))
    }

    fn configure_swapchain(&mut self, swapchain: SwapchainId, width: u32, height: u32) -> Result<(), RhiError> {
        if swapchain != SwapchainId(1) {
            return Err(RhiError::InvalidResource("swapchain"));
        }
        let previous = {
            let config = self.shared.config.lock().map_err(|_| RhiError::DeviceLost)?;
            (config.width, config.height, config.clone())
        };
        if previous.0 == width && previous.1 == height {
            return Ok(());
        }
        if width == 0 || height == 0 {
            // Do not configure a 0×0 swapchain. Mark the target suspended.
            self.release_swapchain_buffers()?;
            let mut config = self.shared.config.lock().map_err(|_| RhiError::DeviceLost)?;
            config.width = 0;
            config.height = 0;
            return Ok(());
        }
        // One queue wait so the previous submission drops its swapchain view.
        // Debt: a fence per resize would be enough. This is not per splitter pixel.
        self.shared.device.poll(wgpu::Maintain::Wait);
        self.release_swapchain_buffers()?;
        let mut next = previous.2;
        next.width = width;
        next.height = height;
        self.shared.device.push_error_scope(wgpu::ErrorFilter::Validation);
        self.shared.surface.configure(&self.shared.device, &next);
        if let Some(error) = pollster::block_on(self.shared.device.pop_error_scope()) {
            return Err(RhiError::Validation(format!(
                "surface configure {width}x{height} from {}x{} failed: {error}",
                previous.0, previous.1
            )));
        }
        *self.shared.config.lock().map_err(|_| RhiError::DeviceLost)? = next;
        Ok(())
    }

    fn acquire_frame(&mut self, swapchain: SwapchainId) -> Result<AcquireFrame, RhiError> {
        if swapchain != SwapchainId(1) {
            return Err(RhiError::InvalidResource("swapchain"));
        }
        let config = self.shared.config.lock().map_err(|_| RhiError::DeviceLost)?;
        if config.width == 0 || config.height == 0 {
            return Ok(AcquireFrame::Minimized);
        }
        drop(config);
        let frame = match self.shared.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(error) => return Err(map_surface_error(error)),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("JARVIG.MainSurface"),
            ..Default::default()
        });
        if let Some(previous) = lock(&self.shared.frame_view)?.take() {
            let _ = lock(&self.shared.views)?.retire(previous);
        }
        let handle = lock(&self.shared.views)?.insert(GpuView::Color(view), Some("JARVIG.MainSurface".into()), 0);
        *lock(&self.shared.frame_view)? = Some(handle);
        *lock(&self.shared.current)? = Some(frame);
        Ok(AcquireFrame::Ready { view: TextureViewId(handle) })
    }

    fn destroy(&mut self, kind: ResourceKind, handle: RawHandle) -> Result<(), RhiError> {
        match kind {
            ResourceKind::Buffer => {
                let referenced = lock(&self.shared.groups)?.alive_values().iter().any(|(_, group)| group.buffers.contains(&handle));
                if referenced {
                    return Err(RhiError::Validation("buffer is still referenced by a bind group".into()));
                }
                lock(&self.shared.buffers)?.retire(handle)?;
            }
            ResourceKind::Texture => {
                let referenced = lock(&self.shared.views)?.alive_values().iter().any(|(_, view)| match view {
                    GpuView::Depth { texture, .. }
                    | GpuView::Sampled { texture, .. }
                    | GpuView::Hdr { texture, .. }
                    | GpuView::Cube { texture, .. } => *texture == handle,
                    GpuView::Color(_) => false,
                });
                if referenced {
                    return Err(RhiError::Validation("texture is still referenced by a texture view".into()));
                }
                lock(&self.shared.textures)?.retire(handle)?;
            }
            ResourceKind::TextureView => {
                let referenced = lock(&self.shared.groups)?.alive_values().iter().any(|(_, group)| group.views.contains(&handle));
                if referenced {
                    return Err(RhiError::Validation("texture view is still referenced by a bind group".into()));
                }
                lock(&self.shared.views)?.retire(handle)?;
            }
            ResourceKind::Sampler => {
                let referenced = lock(&self.shared.groups)?.alive_values().iter().any(|(_, group)| group.samplers.contains(&handle));
                if referenced {
                    return Err(RhiError::Validation("sampler is still referenced by a bind group".into()));
                }
                lock(&self.shared.samplers)?.retire(handle)?;
            }
            ResourceKind::ShaderModule => {
                let referenced = lock(&self.shared.pipelines)?.alive_values().iter().any(|(_, pipeline)| pipeline.shader == handle);
                if referenced {
                    return Err(RhiError::Validation("shader is still referenced by a pipeline".into()));
                }
                lock(&self.shared.shaders)?.retire(handle)?;
            }
            ResourceKind::BindGroupLayout => {
                let referenced = lock(&self.shared.groups)?.alive_values().iter().any(|(_, group)| group.layout == handle)
                    || lock(&self.shared.pipelines)?.alive_values().iter().any(|(_, pipeline)| pipeline.layouts.contains(&handle));
                if referenced {
                    return Err(RhiError::Validation("bind group layout is still referenced".into()));
                }
                lock(&self.shared.layouts)?.retire(handle)?;
            }
            ResourceKind::BindGroup => {
                lock(&self.shared.groups)?.retire(handle)?;
            }
            ResourceKind::Pipeline => {
                lock(&self.shared.pipelines)?.retire(handle)?;
            }
            ResourceKind::CommandBuffer => {
                return Err(RhiError::Validation("that resource is not destroyed by id".into()));
            }
            ResourceKind::Fence | ResourceKind::QueryPool | ResourceKind::Surface | ResourceKind::Swapchain => {
                return Err(RhiError::InvalidHandle("resource"));
            }
        }
        let completed = self.shared.completed.load(Ordering::Acquire);
        self.collect(completed)
    }

    fn debug_label(&self, kind: ResourceKind, handle: RawHandle) -> Option<String> {
        match kind {
            ResourceKind::Buffer => lock(&self.shared.buffers).ok()?.label(handle),
            ResourceKind::Texture => lock(&self.shared.textures).ok()?.label(handle),
            ResourceKind::TextureView => lock(&self.shared.views).ok()?.label(handle),
            ResourceKind::ShaderModule => lock(&self.shared.shaders).ok()?.label(handle),
            ResourceKind::BindGroupLayout => lock(&self.shared.layouts).ok()?.label(handle),
            ResourceKind::BindGroup => lock(&self.shared.groups).ok()?.label(handle),
            ResourceKind::Pipeline => lock(&self.shared.pipelines).ok()?.label(handle),
            ResourceKind::Surface | ResourceKind::Swapchain => Some("JARVIG.MainSurface".into()),
            _ => None,
        }
    }

    fn flush(&mut self) -> Result<(), RhiError> {
        // Wait completes every command buffer previously submitted to this single queue.
        // This is conservative. It is not an N-frame guess, and it does not overlap deletion with GPU work.
        self.shared.device.poll(wgpu::Maintain::Wait);
        let submitted = self.shared.submitted.load(Ordering::Acquire);
        self.shared.completed.store(submitted, Ordering::Release);
        self.collect(submitted)
    }

    fn resource_stats(&self) -> ResourceStats {
        let (alive_buffers, retired_b, buffer_bytes) = lock(&self.shared.buffers).map(|registry| registry.counts()).unwrap_or((0, 0, 0));
        let (alive_textures, retired_t, texture_bytes) = lock(&self.shared.textures).map(|registry| registry.counts()).unwrap_or((0, 0, 0));
        let (alive_texture_views, retired_v, _) = lock(&self.shared.views).map(|registry| registry.counts()).unwrap_or((0, 0, 0));
        let (alive_shaders, retired_s, _) = lock(&self.shared.shaders).map(|registry| registry.counts()).unwrap_or((0, 0, 0));
        let (alive_pipelines, retired_p, _) = lock(&self.shared.pipelines).map(|registry| registry.counts()).unwrap_or((0, 0, 0));
        let (alive_bind_groups, retired_g, _) = lock(&self.shared.groups).map(|registry| registry.counts()).unwrap_or((0, 0, 0));
        let (alive_layouts, retired_l, _) = lock(&self.shared.layouts).map(|registry| registry.counts()).unwrap_or((0, 0, 0));
        let (alive_samplers, retired_sam, _) = lock(&self.shared.samplers).map(|registry| registry.counts()).unwrap_or((0, 0, 0));
        ResourceStats {
            alive_buffers,
            alive_textures,
            alive_texture_views,
            alive_shaders,
            alive_pipelines,
            alive_bind_groups,
            alive_layouts,
            alive_samplers,
            retired: retired_b + retired_t + retired_v + retired_s + retired_p + retired_g + retired_l + retired_sam,
            estimated_bytes: buffer_bytes + texture_bytes,
        }
    }

    fn submit(
        &mut self,
        queue: QueueId,
        buffers: &[CommandBufferId],
        _signal: Option<FenceId>,
    ) -> Result<(), RhiError> {
        if queue != QueueId(0) || buffers.is_empty() {
            return Err(RhiError::Validation("submit needs the graphics queue".into()));
        }
        let pending = lock(&self.shared.pending)?.take().ok_or(RhiError::Validation("submit without a command buffer".into()))?;
        let serial = self.shared.submitted.fetch_add(1, Ordering::AcqRel) + 1;
        self.stamp(&pending, serial)?;
        self.shared.queue.submit(std::iter::once(pending.command));
        Ok(())
    }

    fn wait(&mut self, _fence: FenceId) -> Result<(), RhiError> {
        self.flush()
    }

    fn present(&mut self, queue: QueueId, swapchain: SwapchainId) -> Result<u64, RhiError> {
        if queue != QueueId(0) || swapchain != SwapchainId(1) {
            return Err(RhiError::InvalidResource("present"));
        }
        let frame = lock(&self.shared.current)?.take().ok_or(RhiError::Validation("present without an acquired frame".into()))?;
        frame.present();
        self.presented += 1;
        if let Some(handle) = lock(&self.shared.frame_view)?.take() {
            lock(&self.shared.views)?.retire(handle)?;
        }
        Ok(self.presented)
    }

    fn submitted_buffers(&self) -> u64 {
        self.shared.submitted.load(Ordering::Acquire)
    }

    fn presented_frames(&self) -> u64 {
        self.presented
    }

    fn draw_count(&self) -> u64 {
        self.shared.draws.lock().map(|draws| *draws).unwrap_or(0)
    }

    fn indexed_draw_count(&self) -> u64 {
        self.shared.indexed_draws.lock().map(|draws| *draws).unwrap_or(0)
    }

    fn pipeline_count(&self) -> u64 {
        self.shared.pipeline_count.lock().map(|count| *count).unwrap_or(0)
    }

    fn buffer_count(&self) -> u64 {
        self.shared.buffer_count.lock().map(|count| *count).unwrap_or(0)
    }
}

impl WgpuDevice {
    fn release_swapchain_buffers(&self) -> Result<(), RhiError> {
        let views = lock(&self.shared.views)?.extract_where(|view| matches!(view, GpuView::Color(_)));
        drop(views);
        drop(lock(&self.shared.current)?.take());
        *lock(&self.shared.frame_view)? = None;
        Ok(())
    }

    fn stamp(&self, pending: &Pending, serial: u64) -> Result<(), RhiError> {
        for handle in &pending.buffers {
            lock(&self.shared.buffers)?.stamp(*handle, serial);
        }
        for handle in &pending.views {
            lock(&self.shared.views)?.stamp(*handle, serial);
        }
        for handle in &pending.textures {
            lock(&self.shared.textures)?.stamp(*handle, serial);
        }
        for handle in &pending.pipelines {
            lock(&self.shared.pipelines)?.stamp(*handle, serial);
        }
        for handle in &pending.groups {
            lock(&self.shared.groups)?.stamp(*handle, serial);
        }
        for handle in &pending.shaders {
            lock(&self.shared.shaders)?.stamp(*handle, serial);
        }
        for handle in &pending.layouts {
            lock(&self.shared.layouts)?.stamp(*handle, serial);
        }
        for handle in &pending.samplers {
            lock(&self.shared.samplers)?.stamp(*handle, serial);
        }
        Ok(())
    }

    fn collect(&self, completed: u64) -> Result<(), RhiError> {
        lock(&self.shared.buffers)?.collect(completed);
        lock(&self.shared.textures)?.collect(completed);
        lock(&self.shared.views)?.collect(completed);
        lock(&self.shared.shaders)?.collect(completed);
        lock(&self.shared.pipelines)?.collect(completed);
        lock(&self.shared.groups)?.collect(completed);
        lock(&self.shared.layouts)?.collect(completed);
        lock(&self.shared.samplers)?.collect(completed);
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, RhiError> {
    mutex.lock().map_err(|_| RhiError::DeviceLost)
}

fn guard<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // Declaration order drops the device and the surface before `current`.
        // wgpu then panics in SurfaceTexture::drop: the surface id is already gone.
        // That panic aborts a Win32 window procedure. Release the image first.
        self.device.poll(wgpu::Maintain::Wait);
        drop(guard(&self.pending).take());
        let groups = std::mem::take(&mut *guard(&self.groups));
        let pipelines = std::mem::take(&mut *guard(&self.pipelines));
        let views = std::mem::take(&mut *guard(&self.views));
        let samplers = std::mem::take(&mut *guard(&self.samplers));
        let buffers = std::mem::take(&mut *guard(&self.buffers));
        let textures = std::mem::take(&mut *guard(&self.textures));
        let shaders = std::mem::take(&mut *guard(&self.shaders));
        let layouts = std::mem::take(&mut *guard(&self.layouts));
        drop(groups);
        drop(pipelines);
        drop(views);
        drop(samplers);
        drop(buffers);
        drop(textures);
        drop(shaders);
        drop(layouts);
        let _ = guard(&self.frame_view).take();
        drop(guard(&self.current).take());
    }
}

// The instance trait is not used for window attach. Keep a named backend for logs.
#[allow(dead_code)]
fn _backend_name() -> BackendKind {
    BackendKind::Wgpu
}
