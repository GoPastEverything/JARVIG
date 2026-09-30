use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::{
    AdapterId, AdapterInfo, BackendKind, BindGroupDesc, BindGroupId, BindGroupLayoutDesc, BindGroupLayoutEntry,
    BindGroupLayoutId, BindingType, BufferDesc, BufferId, BufferUsage, CommandBufferId, CommandEncoder,
    ComputePassDesc, ComputePipelineDesc, Device, DeviceCapabilities, DeviceDesc, FenceDesc, FenceId, Instance,
    PipelineId, QueryKind, QueryPoolDesc, QueryPoolId, QueueId, RawHandle, RenderPassDesc, RenderPipelineDesc,
    ResourceKind, ResourceStats, RhiError, SamplerDesc, SamplerId, ShaderModuleDesc, ShaderModuleId, SurfaceDesc,
    SurfaceId, SwapchainDesc, SwapchainId, TextureDesc, TextureId, TextureViewId,
};

struct Slot {
    label: Option<String>,
}

#[derive(Default)]
struct Table {
    next: u64,
    live: HashMap<u64, Slot>,
}

impl Table {
    fn insert(&mut self, label: Option<String>) -> u64 {
        self.next += 1;
        let id = self.next;
        self.live.insert(id, Slot { label });
        id
    }

    fn remove(&mut self, id: u64) -> Result<Slot, RhiError> {
        self.live.remove(&id).ok_or(RhiError::InvalidResource("already destroyed or unknown"))
    }

    fn contains(&self, id: u64) -> bool {
        self.live.contains_key(&id)
    }
}

struct GpuLog {
    draws: u64,
    indexed_draws: u64,
    pipelines: u64,
    buffers: u64,
}

#[derive(Clone)]
struct BufferRecord {
    usage: BufferUsage,
    size: u64,
}

#[derive(Clone)]
struct PipelineInfo {
    shader: RawHandle,
    layouts: Vec<RawHandle>,
    depth: Option<crate::TextureFormat>,
    /// The null rasterizer does not discard triangles. The field records the request.
    #[allow(dead_code)]
    cull: crate::CullMode,
}

#[derive(Clone)]
struct GroupInfo {
    layout: RawHandle,
    buffers: Vec<RawHandle>,
    views: Vec<RawHandle>,
    samplers: Vec<RawHandle>,
}

struct NullEncoder {
    buffer_id: u64,
    parent_textures: Vec<RawHandle>,
    parent_views: HashMap<RawHandle, RawHandle>,
    pipelines: Vec<RawHandle>,
    pipeline_layouts: HashMap<RawHandle, Vec<RawHandle>>,
    group_layouts: HashMap<RawHandle, RawHandle>,
    buffers: HashMap<RawHandle, BufferRecord>,
    texture_formats: HashMap<RawHandle, crate::TextureFormat>,
    view_extent: HashMap<RawHandle, (u32, u32)>,
    cube_samples: HashSet<RawHandle>,
    pipeline_depth: HashMap<RawHandle, Option<crate::TextureFormat>>,
    depth_format: Option<crate::TextureFormat>,
    attachment: Option<(u32, u32)>,
    texture_sizes: HashMap<RawHandle, (u32, u32)>,
    bound: Option<RawHandle>,
    groups_set: HashMap<u32, RawHandle>,
    vertex_bound: bool,
    index_bound: bool,
    in_render: bool,
    in_compute: bool,
    queries_supported: bool,
    query_counts: HashMap<u64, u32>,
    log: Rc<RefCell<GpuLog>>,
}

impl NullEncoder {
    fn require_group(&self) -> Result<(), RhiError> {
        let pipeline = self.bound.ok_or(RhiError::Validation("a pipeline must be bound before draw".into()))?;
        let expected = self.pipeline_layouts.get(&pipeline).cloned().unwrap_or_default();
        for (index, layout) in expected.iter().enumerate() {
            let group = self
                .groups_set
                .get(&(index as u32))
                .copied()
                .ok_or(RhiError::Validation("a bind group must be set before draw".into()))?;
            let actual = self.group_layouts.get(&group).copied().ok_or(RhiError::InvalidResource("bind group"))?;
            if actual != *layout {
                return Err(RhiError::Validation("bind group layout does not match the pipeline".into()));
            }
        }
        Ok(())
    }

    fn require_depth(&self) -> Result<(), RhiError> {
        let pipeline = self.bound.ok_or(RhiError::Validation("a pipeline must be bound before draw".into()))?;
        let required = self.pipeline_depth.get(&pipeline).copied().flatten();
        match (required, self.depth_format) {
            (None, None) => Ok(()),
            (None, Some(_)) => Err(RhiError::Validation("a color pipeline cannot use a depth attachment".into())),
            (Some(_), None) => Err(RhiError::Validation("a depth pipeline requires a depth attachment".into())),
            (Some(expected), Some(actual)) if expected != actual => {
                Err(RhiError::Validation("depth format does not match the pipeline".into()))
            }
            (Some(_), Some(_)) => Ok(()),
        }
    }

    fn bind_buffer(&self, buffer: crate::BufferId, usage: BufferUsage, offset: u64, align: u64) -> Result<(), RhiError> {
        if !self.in_render {
            return Err(RhiError::Validation("a buffer is bound inside a render pass".into()));
        }
        let record = self.buffers.get(&buffer.raw()).ok_or(RhiError::InvalidHandle("buffer"))?;
        if record.usage != usage {
            return Err(RhiError::Validation("buffer usage does not match the binding".into()));
        }
        if offset > record.size || offset % align != 0 {
            return Err(RhiError::Validation("buffer offset is out of range or misaligned".into()));
        }
        Ok(())
    }
}

impl CommandEncoder for NullEncoder {
    fn begin_render_pass(&mut self, desc: &RenderPassDesc) -> Result<(), RhiError> {
        if self.in_render || self.in_compute {
            return Err(RhiError::Validation("render pass is not nestable".into()));
        }
        if desc.colors.is_empty() {
            return Err(RhiError::Validation("render pass needs a color attachment".into()));
        }
        for color in &desc.colors {
            let texture = self
                .parent_views
                .get(&color.target.raw())
                .copied()
                .ok_or(RhiError::InvalidResource("texture view"))?;
            if !self.parent_textures.contains(&texture) {
                return Err(RhiError::InvalidResource("texture"));
            }
        }
        self.bound = None;
        self.groups_set.clear();
        self.vertex_bound = false;
        self.index_bound = false;
        self.depth_format = None;
        if let Some(depth) = &desc.depth {
            let texture = self
                .parent_views
                .get(&depth.target.raw())
                .copied()
                .ok_or(RhiError::InvalidResource("depth view"))?;
            if !self.parent_textures.contains(&texture) {
                return Err(RhiError::InvalidResource("depth texture"));
            }
            let format = self
                .texture_formats
                .get(&texture)
                .copied()
                .ok_or(RhiError::InvalidResource("depth texture"))?;
            if format != crate::TextureFormat::Depth32Float {
                return Err(RhiError::Validation("depth attachment is not a depth texture".into()));
            }
            self.depth_format = Some(format);
        }
        let view = desc.colors[0].target.raw();
        if self.cube_samples.contains(&view) {
            return Err(RhiError::Validation("a cube sample view cannot be a render target".into()));
        }
        let texture = self.parent_views.get(&view).copied().ok_or(RhiError::InvalidResource("texture view"))?;
        self.attachment = Some(
            self.view_extent
                .get(&view)
                .copied()
                .or_else(|| self.texture_sizes.get(&texture).copied())
                .ok_or(RhiError::InvalidResource("texture size"))?,
        );
        self.in_render = true;
        Ok(())
    }

    fn set_viewport(&mut self, viewport: crate::Viewport) -> Result<(), RhiError> {
        if !self.in_render {
            return Err(RhiError::Validation("a viewport is set inside a render pass".into()));
        }
        let (width, height) = self.attachment.ok_or(RhiError::InvalidResource("texture size"))?;
        viewport.check(width, height)?;
        Ok(())
    }

    fn set_scissor(&mut self, scissor: crate::ScissorRect) -> Result<(), RhiError> {
        if !self.in_render {
            return Err(RhiError::Validation("a scissor is set inside a render pass".into()));
        }
        let (width, height) = self.attachment.ok_or(RhiError::InvalidResource("texture size"))?;
        scissor.check(width, height)?;
        Ok(())
    }

    fn set_pipeline(&mut self, pipeline: crate::PipelineId) -> Result<(), RhiError> {
        if !self.in_render {
            return Err(RhiError::Validation("a pipeline is bound inside a render pass".into()));
        }
        if !self.pipelines.contains(&pipeline.raw()) {
            return Err(RhiError::InvalidHandle("pipeline"));
        }
        self.bound = Some(pipeline.raw());
        Ok(())
    }

    fn set_bind_group(&mut self, index: u32, group: BindGroupId) -> Result<(), RhiError> {
        if !self.in_render {
            return Err(RhiError::Validation("a bind group is set inside a render pass".into()));
        }
        if index > 3 {
            return Err(RhiError::Unsupported("bind group index is outside 0..=3"));
        }
        if !self.group_layouts.contains_key(&group.raw()) {
            return Err(RhiError::InvalidHandle("bind group"));
        }
        self.groups_set.insert(index, group.raw());
        Ok(())
    }

    fn set_vertex_buffer(&mut self, _slot: u32, buffer: crate::BufferId, offset: u64) -> Result<(), RhiError> {
        self.bind_buffer(buffer, BufferUsage::Vertex, offset, 4)?;
        self.vertex_bound = true;
        Ok(())
    }

    fn set_index_buffer(&mut self, buffer: crate::BufferId, format: crate::IndexFormat, offset: u64) -> Result<(), RhiError> {
        let align = match format {
            crate::IndexFormat::Uint16 => 2,
            crate::IndexFormat::Uint32 => 4,
        };
        self.bind_buffer(buffer, BufferUsage::Index, offset, align)?;
        self.index_bound = true;
        Ok(())
    }

    fn draw(&mut self, vertex_count: u32, instance_count: u32, _first_vertex: u32, _first_instance: u32) -> Result<(), RhiError> {
        if !self.in_render {
            return Err(RhiError::Validation("draw is only valid inside a render pass".into()));
        }
        if self.bound.is_none() {
            return Err(RhiError::Validation("a pipeline must be bound before draw".into()));
        }
        self.require_group()?;
        self.require_depth()?;
        if vertex_count == 0 || instance_count == 0 {
            return Err(RhiError::Validation("draw counts must be non-zero".into()));
        }
        self.log.borrow_mut().draws += 1;
        Ok(())
    }

    fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        _first_index: u32,
        _base_vertex: i32,
        _first_instance: u32,
    ) -> Result<(), RhiError> {
        if !self.in_render {
            return Err(RhiError::Validation("draw is only valid inside a render pass".into()));
        }
        if self.bound.is_none() {
            return Err(RhiError::Validation("a pipeline must be bound before draw".into()));
        }
        if !self.index_bound {
            return Err(RhiError::Validation("an index buffer must be bound before an indexed draw".into()));
        }
        if !self.vertex_bound {
            return Err(RhiError::Validation("a vertex buffer must be bound before an indexed draw".into()));
        }
        self.require_group()?;
        self.require_depth()?;
        if index_count == 0 || instance_count == 0 {
            return Err(RhiError::Validation("draw counts must be non-zero".into()));
        }
        let mut log = self.log.borrow_mut();
        log.draws += 1;
        log.indexed_draws += 1;
        Ok(())
    }

    fn end_render_pass(&mut self) -> Result<(), RhiError> {
        if !self.in_render {
            return Err(RhiError::Validation("no render pass is open".into()));
        }
        self.in_render = false;
        self.bound = None;
        self.groups_set.clear();
        self.vertex_bound = false;
        self.index_bound = false;
        Ok(())
    }

    fn begin_compute_pass(&mut self, _desc: &ComputePassDesc) -> Result<(), RhiError> {
        if self.in_render || self.in_compute {
            return Err(RhiError::Validation("compute pass is not nestable".into()));
        }
        self.in_compute = true;
        Ok(())
    }

    fn end_compute_pass(&mut self) -> Result<(), RhiError> {
        if !self.in_compute {
            return Err(RhiError::Validation("no compute pass is open".into()));
        }
        self.in_compute = false;
        Ok(())
    }

    fn write_timestamp(&mut self, pool: crate::QueryPoolId, index: u32) -> Result<(), RhiError> {
        if !self.queries_supported {
            return Err(RhiError::Unsupported("timestamp queries"));
        }
        let count = self.query_counts.get(&pool.0).copied().ok_or(RhiError::InvalidResource("query pool"))?;
        if index >= count {
            return Err(RhiError::Validation("timestamp index out of range".into()));
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<CommandBufferId, RhiError> {
        if self.in_render || self.in_compute {
            return Err(RhiError::Validation("a pass is still open".into()));
        }
        Ok(CommandBufferId(self.buffer_id))
    }
}

struct NullDevice {
    label: Option<String>,
    capabilities: DeviceCapabilities,
    buffers: crate::Registry<BufferRecord>,
    textures: crate::Registry<crate::TextureFormat>,
    views: crate::Registry<RawHandle>,
    texture_sizes: HashMap<RawHandle, (u32, u32)>,
    texture_mips: HashMap<RawHandle, u32>,
    view_extent: HashMap<RawHandle, (u32, u32)>,
    cube_samples: HashSet<RawHandle>,
    cube_meta: HashMap<RawHandle, (u32, u32)>,
    samplers: crate::Registry<()>,
    shaders: crate::Registry<()>,
    layouts: crate::Registry<Vec<BindGroupLayoutEntry>>,
    pipelines: crate::Registry<PipelineInfo>,
    groups: crate::Registry<GroupInfo>,
    fences: Table,
    queries: Table,
    query_counts: HashMap<u64, u32>,
    surfaces: Table,
    swapchains: HashMap<u64, SwapchainState>,
    frame_texture: Option<RawHandle>,
    frame_view: Option<RawHandle>,
    next_view: u64,
    next_encoder: u64,
    submitted: u64,
    completed: u64,
    presented: u64,
    signaled: HashMap<u64, bool>,
    log: Rc<RefCell<GpuLog>>,
}

impl NullDevice {
    fn collect(&mut self) {
        self.buffers.collect(self.completed);
        self.textures.collect(self.completed);
        self.views.collect(self.completed);
        self.samplers.collect(self.completed);
        self.shaders.collect(self.completed);
        self.layouts.collect(self.completed);
        self.pipelines.collect(self.completed);
        self.groups.collect(self.completed);
    }

    /// Frame color targets are transient. Retire the view before the texture it references.
    fn retire_frame(&mut self) {
        if let Some(view) = self.frame_view.take() {
            let _ = self.views.retire(view);
        }
        if let Some(texture) = self.frame_texture.take() {
            self.texture_sizes.remove(&texture);
            let _ = self.textures.retire(texture);
        }
        self.collect();
    }
}

impl Device for NullDevice {
    fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities
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
        let handle = self.buffers.insert(
            BufferRecord { usage: desc.usage, size: desc.size },
            desc.label.clone(),
            desc.size,
        );
        self.log.borrow_mut().buffers += 1;
        Ok(BufferId(handle))
    }

    fn create_texture(&mut self, desc: &TextureDesc) -> Result<TextureId, RhiError> {
        if desc.width == 0 || desc.height == 0 || desc.mip_count == 0 || desc.width > self.capabilities.max_texture_dimension_2d {
            return Err(RhiError::Validation("texture dimensions are invalid".into()));
        }
        let bytes = u64::from(desc.width) * u64::from(desc.height) * 4;
        let handle = self.textures.insert(desc.format, desc.label.clone(), bytes);
        self.texture_sizes.insert(handle, (desc.width, desc.height));
        self.texture_mips.insert(handle, desc.mip_count);
        Ok(TextureId(handle))
    }

    fn create_texture_view(&mut self, texture: TextureId) -> Result<TextureViewId, RhiError> {
        self.textures.get(texture.raw()).map_err(|_| RhiError::InvalidHandle("texture"))?;
        let handle = self.views.insert(texture.raw(), None, 0);
        let extent = self.texture_sizes.get(&texture.raw()).copied().unwrap_or((1, 1));
        self.view_extent.insert(handle, extent);
        Ok(TextureViewId(handle))
    }

    fn create_cube_texture(&mut self, desc: &crate::CubeTextureDesc) -> Result<TextureId, RhiError> {
        if desc.size == 0 || !desc.size.is_power_of_two() || desc.mip_count == 0 || desc.size > self.capabilities.max_texture_dimension_2d {
            return Err(RhiError::Validation("cube texture dimensions are invalid".into()));
        }
        let max_mips = desc.size.ilog2() + 1;
        if desc.mip_count > max_mips {
            return Err(RhiError::Validation("cube mip count is invalid".into()));
        }
        if desc.format != crate::TextureFormat::Rgba16Float || desc.usage != crate::TextureUsage::ColorTarget {
            return Err(RhiError::Unsupported("only an HDR color cube is implemented"));
        }
        let bytes = u64::from(desc.size) * u64::from(desc.size) * 8 * 6;
        let handle = self.textures.insert(desc.format, desc.label.clone(), bytes);
        let id = TextureId(handle);
        self.texture_sizes.insert(id.raw(), (desc.size, desc.size));
        self.cube_meta.insert(id.raw(), (desc.size, desc.mip_count));
        Ok(id)
    }

    fn create_cube_face_view(&mut self, texture: TextureId, face: u32, mip: u32) -> Result<TextureViewId, RhiError> {
        let (size, mips) = *self.cube_meta.get(&texture.raw()).ok_or(RhiError::InvalidResource("cube texture"))?;
        if face >= 6 || mip >= mips {
            return Err(RhiError::Validation("cube face view is out of range".into()));
        }
        let edge = (size >> mip).max(1);
        let handle = self.views.insert(texture.raw(), None, 0);
        let view = TextureViewId(handle);
        self.view_extent.insert(view.raw(), (edge, edge));
        Ok(view)
    }

    fn create_cube_sample_view(&mut self, texture: TextureId) -> Result<TextureViewId, RhiError> {
        let (size, _) = *self.cube_meta.get(&texture.raw()).ok_or(RhiError::InvalidResource("cube texture"))?;
        let handle = self.views.insert(texture.raw(), None, 0);
        let view = TextureViewId(handle);
        self.view_extent.insert(view.raw(), (size, size));
        self.cube_samples.insert(view.raw());
        Ok(view)
    }

    fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<SamplerId, RhiError> {
        Ok(SamplerId(self.samplers.insert((), desc.label.clone(), 0)))
    }

    fn create_shader_module(&mut self, desc: &ShaderModuleDesc) -> Result<ShaderModuleId, RhiError> {
        let crate::ShaderSource::Wgsl(source) = &desc.source;
        if source.is_empty() {
            return Err(RhiError::Validation("shader source is empty".into()));
        }
        Ok(ShaderModuleId(self.shaders.insert((), desc.label.clone(), 0)))
    }

    fn create_bind_group_layout(&mut self, desc: &BindGroupLayoutDesc) -> Result<BindGroupLayoutId, RhiError> {
        let handle = self.layouts.insert(desc.entries.clone(), desc.label.clone(), 0);
        Ok(BindGroupLayoutId(handle))
    }

    fn create_bind_group(&mut self, desc: &BindGroupDesc) -> Result<BindGroupId, RhiError> {
        let entries = self.layouts.get(desc.layout.raw()).map_err(|_| RhiError::InvalidHandle("bind group layout"))?;
        if desc.entries.len() != entries.len() {
            return Err(RhiError::Validation("bind group entries do not match the layout".into()));
        }
        let mut buffers = Vec::new();
        let mut views = Vec::new();
        let mut samplers = Vec::new();
        for entry in &desc.entries {
            let expected = entries
                .iter()
                .find(|item| item.binding == entry.binding)
                .ok_or(RhiError::Validation("bind group binding is not in the layout".into()))?;
            match entry.resource {
                crate::BindResource::Buffer(buffer) => {
                    let record = self.buffers.get(buffer.raw()).map_err(|_| RhiError::InvalidHandle("buffer"))?;
                    let usage_ok = match expected.kind {
                        BindingType::UniformBuffer => record.usage == BufferUsage::Uniform,
                        BindingType::StorageBuffer => record.usage == BufferUsage::Storage,
                        _ => false,
                    };
                    if !usage_ok {
                        return Err(RhiError::Validation("bind group buffer does not match the layout".into()));
                    }
                    buffers.push(buffer.raw());
                }
                crate::BindResource::TextureView(view) => {
                    let cube = self.cube_samples.contains(&view.raw());
                    let kind_ok = match expected.kind {
                        BindingType::Texture => !cube,
                        BindingType::TextureCube => cube,
                        _ => false,
                    };
                    if !kind_ok {
                        return Err(RhiError::Validation("bind group entry does not match the layout".into()));
                    }
                    self.views.get(view.raw()).map_err(|_| RhiError::InvalidHandle("texture view"))?;
                    views.push(view.raw());
                }
                crate::BindResource::Sampler(sampler) => {
                    if expected.kind != BindingType::Sampler {
                        return Err(RhiError::Validation("bind group entry does not match the layout".into()));
                    }
                    self.samplers.get(sampler.raw()).map_err(|_| RhiError::InvalidHandle("sampler"))?;
                    samplers.push(sampler.raw());
                }
            }
        }
        let handle = self.groups.insert(GroupInfo { layout: desc.layout.raw(), buffers, views, samplers }, desc.label.clone(), 0);
        Ok(BindGroupId(handle))
    }

    fn write_buffer(&mut self, buffer: BufferId, offset: u64, data: &[u8]) -> Result<(), RhiError> {
        let record = self.buffers.get(buffer.raw())?;
        if !matches!(record.usage, BufferUsage::Uniform | BufferUsage::Storage) {
            return Err(RhiError::Validation("only a uniform or storage buffer can be written".into()));
        }
        if data.is_empty() || offset % 4 != 0 || data.len() % 4 != 0 || offset + data.len() as u64 > record.size {
            return Err(RhiError::Validation("buffer write is empty, misaligned, or out of range".into()));
        }
        Ok(())
    }

    fn write_texture(&mut self, texture: crate::TextureId, data: &[u8]) -> Result<(), RhiError> {
        self.write_texture_mip(texture, 0, data)
    }

    fn write_texture_mip(&mut self, texture: crate::TextureId, mip: u32, data: &[u8]) -> Result<(), RhiError> {
        let format = *self.textures.get(texture.raw()).map_err(|_| RhiError::InvalidHandle("texture"))?;
        if !matches!(format, crate::TextureFormat::Rgba8Unorm | crate::TextureFormat::Rgba8UnormSrgb) {
            return Err(RhiError::Validation("only an RGBA8 texture can be written".into()));
        }
        let mips = self.texture_mips.get(&texture.raw()).copied().unwrap_or(1);
        if mip >= mips {
            return Err(RhiError::Validation("texture mip is out of range".into()));
        }
        let (width, height) = self.texture_sizes.get(&texture.raw()).copied().ok_or(RhiError::InvalidResource("texture"))?;
        let level_width = (width >> mip).max(1);
        let level_height = (height >> mip).max(1);
        if data.len() != level_width as usize * level_height as usize * 4 {
            return Err(RhiError::Validation("texture write is not tightly packed RGBA8".into()));
        }
        Ok(())
    }

    fn create_render_pipeline(&mut self, desc: &RenderPipelineDesc) -> Result<PipelineId, RhiError> {
        for layout in &desc.layouts {
            self.layouts.get(layout.raw()).map_err(|_| RhiError::InvalidHandle("bind group layout"))?;
        }
        self.shaders.get(desc.shader.raw()).map_err(|_| RhiError::InvalidHandle("shader"))?;
        if desc.vertex_entry.is_empty() || desc.fragment_entry.is_empty() {
            return Err(RhiError::InvalidResource("pipeline shader"));
        }
        for layout in &desc.vertex_buffers {
            layout.check()?;
        }
        if let Some(depth) = desc.depth {
            if depth.format != crate::TextureFormat::Depth32Float {
                return Err(RhiError::Validation("depth state format is not a depth format".into()));
            }
        }
        let handle = self.pipelines.insert(
            PipelineInfo {
                shader: desc.shader.raw(),
                layouts: desc.layouts.iter().map(|layout| layout.raw()).collect(),
                depth: desc.depth.map(|depth| depth.format),
                cull: desc.cull,
            },
            desc.label.clone(),
            0,
        );
        self.log.borrow_mut().pipelines += 1;
        Ok(PipelineId(handle))
    }

    fn create_compute_pipeline(&mut self, desc: &ComputePipelineDesc) -> Result<PipelineId, RhiError> {
        self.layouts.get(desc.layout.raw()).map_err(|_| RhiError::InvalidHandle("bind group layout"))?;
        self.shaders.get(desc.shader.raw()).map_err(|_| RhiError::InvalidHandle("shader"))?;
        let handle = self.pipelines.insert(
            PipelineInfo { shader: desc.shader.raw(), layouts: vec![desc.layout.raw()], depth: None, cull: crate::CullMode::None },
            desc.label.clone(),
            0,
        );
        Ok(PipelineId(handle))
    }

    fn create_command_encoder(&mut self, _label: Option<&str>) -> Result<Box<dyn CommandEncoder>, RhiError> {
        self.next_encoder += 1;
        let buffer_id = self.next_encoder;
        Ok(Box::new(NullEncoder {
            buffer_id,
            parent_textures: self.textures.alive_handles(),
            parent_views: self.views.alive_values().into_iter().map(|(id, texture)| (id, *texture)).collect(),
            pipelines: self.pipelines.alive_handles(),
            pipeline_layouts: self.pipelines.alive_values().into_iter().map(|(id, info)| (id, info.layouts.clone())).collect(),
            group_layouts: self.groups.alive_values().into_iter().map(|(id, info)| (id, info.layout)).collect(),
            buffers: self.buffers.alive_values().into_iter().map(|(id, record)| (id, record.clone())).collect(),
            texture_formats: self.textures.alive_values().into_iter().map(|(id, format)| (id, *format)).collect(),
            view_extent: self.view_extent.clone(),
            cube_samples: self.cube_samples.clone(),
            pipeline_depth: self.pipelines.alive_values().into_iter().map(|(id, info)| (id, info.depth)).collect(),
            depth_format: None,
            attachment: None,
            texture_sizes: self.texture_sizes.clone(),
            bound: None,
            groups_set: HashMap::new(),
            vertex_bound: false,
            index_bound: false,
            in_render: false,
            in_compute: false,
            queries_supported: self.capabilities.timestamp_queries,
            query_counts: self.query_counts.clone(),
            log: Rc::clone(&self.log),
        }))
    }

    fn create_fence(&mut self, desc: &FenceDesc) -> Result<FenceId, RhiError> {
        let id = self.fences.insert(desc.label.clone());
        self.signaled.insert(id, false);
        Ok(FenceId(id))
    }

    fn create_query_pool(&mut self, desc: &QueryPoolDesc) -> Result<QueryPoolId, RhiError> {
        if desc.kind == QueryKind::Timestamp && !self.capabilities.timestamp_queries {
            return Err(RhiError::Unsupported("timestamp queries"));
        }
        if desc.count == 0 {
            return Err(RhiError::Validation("query pool count must be non-zero".into()));
        }
        let id = self.queries.insert(desc.label.clone());
        self.query_counts.insert(id, desc.count);
        Ok(QueryPoolId(id))
    }

    fn create_surface(&mut self, desc: &SurfaceDesc) -> Result<SurfaceId, RhiError> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RhiError::Validation("surface size must be non-zero".into()));
        }
        Ok(SurfaceId(self.surfaces.insert(desc.label.clone())))
    }

    fn create_swapchain(&mut self, desc: &SwapchainDesc) -> Result<SwapchainId, RhiError> {
        if !self.surfaces.contains(desc.surface.0) || desc.width == 0 || desc.height == 0 {
            return Err(RhiError::Validation("swapchain is invalid".into()));
        }
        let id = self.next_view + self.swapchains.len() as u64 + 1;
        self.swapchains.insert(
            id,
            SwapchainState {
                surface: desc.surface.0,
                width: desc.width,
                height: desc.height,
            },
        );
        Ok(SwapchainId(id))
    }

    fn configure_swapchain(&mut self, swapchain: SwapchainId, width: u32, height: u32) -> Result<(), RhiError> {
        let state = self
            .swapchains
            .get_mut(&swapchain.0)
            .ok_or(RhiError::InvalidResource("swapchain"))?;
        state.width = width;
        state.height = height;
        Ok(())
    }

    fn acquire_frame(&mut self, swapchain: SwapchainId) -> Result<crate::AcquireFrame, RhiError> {
        let (width, height) = {
            let state = self
                .swapchains
                .get(&swapchain.0)
                .ok_or(RhiError::InvalidResource("swapchain"))?;
            (state.width, state.height)
        };
        if width == 0 || height == 0 {
            return Ok(crate::AcquireFrame::Minimized);
        }
        // A missed present must not leave the previous fake frame resident.
        self.retire_frame();
        let texture = self.textures.insert(crate::TextureFormat::Rgba8Unorm, Some("JARVIG.NullFrame".into()), 4);
        self.texture_sizes.insert(texture, (width, height));
        let view = self.views.insert(texture, Some("JARVIG.NullFrame".into()), 0);
        self.frame_texture = Some(texture);
        self.frame_view = Some(view);
        Ok(crate::AcquireFrame::Ready { view: crate::TextureViewId(view) })
    }

    fn destroy(&mut self, kind: ResourceKind, handle: RawHandle) -> Result<(), RhiError> {
        let retired = match kind {
            ResourceKind::Buffer => {
                let referenced = self.groups.alive_values().iter().any(|(_, group)| group.buffers.contains(&handle));
                if referenced {
                    return Err(RhiError::Validation("buffer is still referenced by a bind group".into()));
                }
                self.buffers.retire(handle)
            }
            ResourceKind::Texture => {
                let referenced = self.views.alive_values().iter().any(|(_, texture)| **texture == handle);
                if referenced {
                    return Err(RhiError::Validation("texture is still referenced by a texture view".into()));
                }
                self.texture_sizes.remove(&handle);
                self.cube_meta.remove(&handle);
                self.textures.retire(handle)
            }
            ResourceKind::TextureView => {
                let referenced = self.groups.alive_values().iter().any(|(_, group)| group.views.contains(&handle));
                if referenced {
                    return Err(RhiError::Validation("texture view is still referenced by a bind group".into()));
                }
                self.view_extent.remove(&handle);
                self.cube_samples.remove(&handle);
                self.views.retire(handle)
            }
            ResourceKind::Sampler => {
                let referenced = self.groups.alive_values().iter().any(|(_, group)| group.samplers.contains(&handle));
                if referenced {
                    return Err(RhiError::Validation("sampler is still referenced by a bind group".into()));
                }
                self.samplers.retire(handle)
            }
            ResourceKind::ShaderModule => {
                let referenced = self.pipelines.alive_values().iter().any(|(_, pipeline)| pipeline.shader == handle);
                if referenced {
                    return Err(RhiError::Validation("shader is still referenced by a pipeline".into()));
                }
                self.shaders.retire(handle)
            }
            ResourceKind::BindGroupLayout => {
                let referenced = self.groups.alive_values().iter().any(|(_, group)| group.layout == handle)
                    || self.pipelines.alive_values().iter().any(|(_, pipeline)| pipeline.layouts.contains(&handle));
                if referenced {
                    return Err(RhiError::Validation("bind group layout is still referenced".into()));
                }
                self.layouts.retire(handle)
            }
            ResourceKind::BindGroup => self.groups.retire(handle),
            ResourceKind::Pipeline => self.pipelines.retire(handle),
            ResourceKind::Fence => {
                self.fences.remove(handle.index as u64)?;
                self.signaled.remove(&(handle.index as u64));
                return Ok(());
            }
            ResourceKind::QueryPool => {
                self.queries.remove(handle.index as u64)?;
                self.query_counts.remove(&(handle.index as u64));
                return Ok(());
            }
            ResourceKind::Surface => {
                self.surfaces.remove(handle.index as u64)?;
                return Ok(());
            }
            ResourceKind::Swapchain => {
                self.swapchains.remove(&(handle.index as u64)).ok_or(RhiError::InvalidResource("swapchain"))?;
                return Ok(());
            }
            ResourceKind::CommandBuffer => return Err(RhiError::Validation("that resource is not destroyed by id".into())),
        };
        retired?;
        self.collect();
        Ok(())
    }

    fn debug_label(&self, kind: ResourceKind, handle: RawHandle) -> Option<String> {
        match kind {
            ResourceKind::Buffer => self.buffers.label(handle),
            ResourceKind::Texture => self.textures.label(handle),
            ResourceKind::Pipeline => self.pipelines.label(handle),
            ResourceKind::ShaderModule => self.shaders.label(handle),
            ResourceKind::Surface => self.surfaces.live.get(&(handle.index as u64)).and_then(|slot| slot.label.clone()),
            _ => None,
        }
    }

    fn flush(&mut self) -> Result<(), RhiError> {
        self.completed = self.completed.max(self.submitted);
        self.collect();
        Ok(())
    }

    fn resource_stats(&self) -> ResourceStats {
        let (buffers, retired_b, buffer_bytes) = self.buffers.counts();
        let (textures, retired_t, texture_bytes) = self.textures.counts();
        let (views, retired_v, _) = self.views.counts();
        let (shaders, retired_s, _) = self.shaders.counts();
        let (pipelines, retired_p, _) = self.pipelines.counts();
        let (groups, retired_g, _) = self.groups.counts();
        let (layouts, retired_l, _) = self.layouts.counts();
        let (samplers, retired_sam, _) = self.samplers.counts();
        ResourceStats {
            alive_buffers: buffers,
            alive_textures: textures,
            alive_texture_views: views,
            alive_shaders: shaders,
            alive_pipelines: pipelines,
            alive_bind_groups: groups,
            alive_layouts: layouts,
            alive_samplers: samplers,
            retired: retired_b + retired_t + retired_v + retired_s + retired_p + retired_g + retired_l + retired_sam,
            estimated_bytes: buffer_bytes + texture_bytes,
        }
    }

    fn submit(
        &mut self,
        queue: QueueId,
        buffers: &[CommandBufferId],
        signal: Option<FenceId>,
    ) -> Result<(), RhiError> {
        if queue != QueueId(0) {
            return Err(RhiError::InvalidResource("queue"));
        }
        if buffers.is_empty() {
            return Err(RhiError::Validation("submit needs a command buffer".into()));
        }
        if let Some(fence) = signal {
            if !self.fences.contains(fence.0) {
                return Err(RhiError::InvalidResource("fence"));
            }
            self.signaled.insert(fence.0, true);
        }
        self.submitted += buffers.len() as u64;
        self.completed = self.submitted;
        self.collect();
        Ok(())
    }

    fn wait(&mut self, fence: FenceId) -> Result<(), RhiError> {
        if !self.signaled.get(&fence.0).copied().unwrap_or(false) {
            return Err(RhiError::InvalidResource("fence is not signaled"));
        }
        Ok(())
    }

    fn present(&mut self, queue: QueueId, swapchain: SwapchainId) -> Result<u64, RhiError> {
        if queue != QueueId(0) || !self.swapchains.contains_key(&swapchain.0) {
            return Err(RhiError::InvalidResource("present"));
        }
        // The null device has already completed submit. The frame targets are not resident after present.
        self.retire_frame();
        self.presented += 1;
        Ok(self.presented)
    }

    fn submitted_buffers(&self) -> u64 {
        self.submitted
    }

    fn presented_frames(&self) -> u64 {
        self.presented
    }

    fn draw_count(&self) -> u64 {
        self.log.borrow().draws
    }

    fn indexed_draw_count(&self) -> u64 {
        self.log.borrow().indexed_draws
    }

    fn pipeline_count(&self) -> u64 {
        self.log.borrow().pipelines
    }

    fn buffer_count(&self) -> u64 {
        self.log.borrow().buffers
    }
}

struct SwapchainState {
    #[allow(dead_code)]
    surface: u64,
    width: u32,
    height: u32,
}

struct NullInstance;

impl Instance for NullInstance {
    fn backend_name(&self) -> &'static str {
        "null"
    }

    fn enumerate_adapters(&self) -> Vec<AdapterId> {
        vec![AdapterId(1)]
    }

    fn adapter_info(&self, adapter: AdapterId) -> Result<AdapterInfo, RhiError> {
        if adapter != AdapterId(1) {
            return Err(RhiError::InvalidResource("adapter"));
        }
        Ok(AdapterInfo {
            id: adapter,
            name: "JARVIG Null".into(),
            vendor_id: 0,
            device_id: 0,
            backend: BackendKind::Null,
            api: crate::GraphicsApi::Null,
            kind: crate::AdapterKind::Cpu,
            driver: String::new(),
            driver_info: String::new(),
            timestamp_queries: false,
            max_texture_dimension_2d: 8192,
            max_storage_buffers_per_shader_stage: 0,
            software: true,
        })
    }

    fn request_device(&self, adapter: AdapterId, desc: &DeviceDesc) -> Result<Box<dyn Device>, RhiError> {
        let info = self.adapter_info(adapter)?;
        Ok(Box::new(NullDevice {
            label: desc.label.clone(),
            capabilities: DeviceCapabilities {
                timestamp_queries: info.timestamp_queries,
                max_texture_dimension_2d: info.max_texture_dimension_2d,
            },
            buffers: crate::Registry::default(),
            textures: crate::Registry::default(),
            views: crate::Registry::default(),
            texture_sizes: HashMap::new(),
            texture_mips: HashMap::new(),
            view_extent: HashMap::new(),
            cube_samples: HashSet::new(),
            cube_meta: HashMap::new(),
            samplers: crate::Registry::default(),
            shaders: crate::Registry::default(),
            layouts: crate::Registry::default(),
            pipelines: crate::Registry::default(),
            groups: crate::Registry::default(),
            fences: Table::default(),
            queries: Table::default(),
            query_counts: HashMap::new(),
            surfaces: Table::default(),
            swapchains: HashMap::new(),
            frame_texture: None,
            frame_view: None,
            next_view: 0,
            next_encoder: 0,
            submitted: 0,
            completed: 0,
            presented: 0,
            signaled: HashMap::new(),
            log: Rc::new(RefCell::new(GpuLog { draws: 0, indexed_draws: 0, pipelines: 0, buffers: 0 })),
        }))
    }
}

pub fn create_null_instance() -> Box<dyn Instance> {
    Box::new(NullInstance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BindingType, BindGroupLayoutEntry, BufferUsage, ClearColor, ColorAttachment, ComputePassDesc, ShaderStage,
        ComputePipelineDesc, FenceDesc, QueryKind, QueryPoolDesc, RenderPassDesc, RenderPipelineDesc,
        ResourceKind, SamplerDesc, ShaderModuleDesc, ShaderSource, SurfaceDesc, SwapchainDesc, TextureDesc,
        TextureFormat, TextureUsage,
    };

    fn device() -> Box<dyn crate::Device> {
        let instance = create_null_instance();
        let adapter = instance.enumerate_adapters()[0];
        instance
            .request_device(adapter, &crate::DeviceDesc { label: Some("test".into()) })
            .unwrap()
    }

    #[test]
    fn clear_and_present_records_a_frame_without_a_gpu() {
        let mut device = device();
        assert_eq!(device.label(), Some("test"));
        assert!(!device.capabilities().timestamp_queries);
        let texture = device
            .create_texture(&TextureDesc {
                width: 8,
                height: 8,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: Some("color".into()),
            })
            .unwrap();
        let view = device.create_texture_view(texture).unwrap();
        let surface = device
            .create_surface(&SurfaceDesc { width: 8, height: 8, label: Some("view".into()) })
            .unwrap();
        let swapchain = device
            .create_swapchain(&SwapchainDesc {
                surface,
                format: TextureFormat::Rgba8Unorm,
                width: 8,
                height: 8,
                label: None,
            })
            .unwrap();
        let mut encoder = device.create_command_encoder(Some("frame")).unwrap();
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("clear".into()),
                colors: vec![ColorAttachment {
                    target: view,
                    load: crate::LoadOp::Clear(ClearColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                    store: crate::StoreOp::Store,
                }],
                depth: None,
            })
            .unwrap();
        encoder.end_render_pass().unwrap();
        encoder.begin_compute_pass(&ComputePassDesc { label: None }).unwrap();
        encoder.end_compute_pass().unwrap();
        assert!(encoder.write_timestamp(crate::QueryPoolId(1), 0).is_err());
        let buffer = encoder.finish().unwrap();
        let fence = device.create_fence(&FenceDesc { label: Some("frame-done".into()) }).unwrap();
        device.submit(device.graphics_queue(), &[buffer], Some(fence)).unwrap();
        device.wait(fence).unwrap();
        assert_eq!(device.present(device.graphics_queue(), swapchain).unwrap(), 1);
        assert_eq!(device.presented_frames(), 1);
        assert_eq!(device.debug_label(ResourceKind::Surface, RawHandle { index: surface.0 as u32, generation: 1 }).as_deref(), Some("view"));
        assert!(device.destroy(ResourceKind::Texture, texture.raw()).is_err());
        device.destroy(ResourceKind::TextureView, view.raw()).unwrap();
        device.destroy(ResourceKind::Texture, texture.raw()).unwrap();
        assert!(device.destroy(ResourceKind::Texture, texture.raw()).is_err());
    }

    #[test]
    fn resources_and_pipelines_validate() {
        let mut device = device();
        let buffer = device
            .create_buffer(&crate::BufferDesc {
                size: 16,
                usage: BufferUsage::Vertex,
                label: Some("vertices".into()),
                contents: None,
            })
            .unwrap();
        let shader = device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl("fn main() {}".into()),
                label: None,
            })
            .unwrap();
        let layout = device
            .create_bind_group_layout(&crate::BindGroupLayoutDesc {
                entries: vec![BindGroupLayoutEntry { binding: 0, kind: BindingType::UniformBuffer, stage: ShaderStage::Vertex }],
                label: None,
            })
            .unwrap();
        let pipeline = device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: crate::PrimitiveTopology::TriangleList,
                color_format: crate::TextureFormat::Rgba8Unorm,
                layouts: vec![layout],
                vertex_buffers: Vec::new(),
                depth: None,
                cull: crate::CullMode::None,
                label: Some("mesh".into()),
            })
            .unwrap();
        device
            .create_compute_pipeline(&ComputePipelineDesc { layout, shader, label: None })
            .unwrap();
        device.create_sampler(&SamplerDesc::linear_repeat(None)).unwrap();
        assert!(device
            .create_query_pool(&QueryPoolDesc { kind: QueryKind::Timestamp, count: 1, label: None })
            .is_err());
        assert_eq!(device.debug_label(ResourceKind::Buffer, buffer.raw()).as_deref(), Some("vertices"));
        assert_eq!(device.debug_label(ResourceKind::Pipeline, pipeline.raw()).as_deref(), Some("mesh"));
    }

    #[test]
    fn draw_requires_a_bound_pipeline_and_records_one_draw() {
        let mut device = device();
        let shader = device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl("fn vs() {}".into()),
                label: Some("JARVIG.FirstTriangle".into()),
            })
            .unwrap();
        let pipeline = device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: crate::PrimitiveTopology::TriangleList,
                color_format: TextureFormat::Rgba8Unorm,
                layouts: Vec::new(),
                vertex_buffers: Vec::new(),
                depth: None,
                cull: crate::CullMode::None,
                label: Some("JARVIG.FirstTriangle".into()),
            })
            .unwrap();
        assert_eq!(device.pipeline_count(), 1);
        let texture = device
            .create_texture(&TextureDesc {
                width: 4,
                height: 4,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: None,
            })
            .unwrap();
        let view = device.create_texture_view(texture).unwrap();
        let surface = device
            .create_surface(&SurfaceDesc { width: 4, height: 4, label: None })
            .unwrap();
        let swapchain = device
            .create_swapchain(&SwapchainDesc {
                surface,
                format: TextureFormat::Rgba8Unorm,
                width: 4,
                height: 4,
                label: None,
            })
            .unwrap();
        let mut encoder = device.create_command_encoder(Some("order")).unwrap();
        assert!(encoder.draw(3, 1, 0, 0).is_err());
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("triangle".into()),
                colors: vec![ColorAttachment {
                    target: view,
                    load: crate::LoadOp::Clear(ClearColor { r: 0.051, g: 0.090, b: 0.141, a: 1.0 }),
                    store: crate::StoreOp::Store,
                }],
                depth: None,
            })
            .unwrap();
        assert!(encoder.draw(3, 1, 0, 0).is_err());
        assert!(encoder.set_pipeline(crate::PipelineId::INVALID).is_err());
        encoder.set_pipeline(pipeline).unwrap();
        assert!(encoder.draw(0, 1, 0, 0).is_err());
        encoder.draw(3, 1, 0, 0).unwrap();
        encoder.end_render_pass().unwrap();
        assert!(encoder.draw(3, 1, 0, 0).is_err());
        let buffer = encoder.finish().unwrap();
        device.submit(device.graphics_queue(), &[buffer], None).unwrap();
        assert_eq!(device.present(device.graphics_queue(), swapchain).unwrap(), 1);
        assert_eq!(device.draw_count(), 1);
        assert_eq!(device.pipeline_count(), 1);
        assert!(device.destroy(ResourceKind::ShaderModule, shader.raw()).is_err());
        device.destroy(ResourceKind::Pipeline, pipeline.raw()).unwrap();
        device.destroy(ResourceKind::ShaderModule, shader.raw()).unwrap();
        assert!(device.destroy(ResourceKind::Pipeline, pipeline.raw()).is_err());
    }

    #[test]
    fn indexed_draw_requires_a_pipeline_and_both_buffers() {
        let mut device = device();
        assert!(device
            .create_buffer(&crate::BufferDesc {
                size: 4,
                usage: BufferUsage::Vertex,
                label: None,
                contents: Some(vec![1, 2]),
            })
            .is_err());
        let vertices = device
            .create_buffer(&crate::BufferDesc {
                size: 24,
                usage: BufferUsage::Vertex,
                label: Some("JARVIG.FirstTriangle.Vertices".into()),
                contents: Some(vec![0; 24]),
            })
            .unwrap();
        let indices = device
            .create_buffer(&crate::BufferDesc {
                size: 6,
                usage: BufferUsage::Index,
                label: Some("JARVIG.FirstTriangle.Indices".into()),
                contents: Some(vec![0, 0, 1, 0, 2, 0]),
            })
            .unwrap();
        let shader = device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl("fn vs() {}".into()),
                label: None,
            })
            .unwrap();
        let pipeline = device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: crate::PrimitiveTopology::TriangleList,
                color_format: TextureFormat::Rgba8Unorm,
                layouts: Vec::new(),
                vertex_buffers: vec![crate::VertexBufferLayout {
                    stride: 24,
                    step_mode: crate::VertexStepMode::Vertex,
                    attributes: vec![
                        crate::VertexAttribute {
                            shader_location: 0,
                            offset: 0,
                            format: crate::VertexFormat::Float32x3,
                        },
                        crate::VertexAttribute {
                            shader_location: 1,
                            offset: 12,
                            format: crate::VertexFormat::Float32x3,
                        },
                    ],
                }],
                depth: None,
                cull: crate::CullMode::None,
                label: None,
            })
            .unwrap();
        let texture = device
            .create_texture(&TextureDesc {
                width: 4,
                height: 4,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: None,
            })
            .unwrap();
        let view = device.create_texture_view(texture).unwrap();
        let mut encoder = device.create_command_encoder(Some("indexed")).unwrap();
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: None,
                colors: vec![ColorAttachment {
                    target: view,
                    load: crate::LoadOp::Load,
                    store: crate::StoreOp::Store,
                }],
                depth: None,
            })
            .unwrap();
        assert!(encoder.draw_indexed(3, 1, 0, 0, 0).is_err());
        encoder.set_pipeline(pipeline).unwrap();
        assert!(encoder.draw_indexed(3, 1, 0, 0, 0).is_err());
        assert!(encoder.set_vertex_buffer(0, crate::BufferId::INVALID, 0).is_err());
        assert!(encoder.set_index_buffer(crate::BufferId::INVALID, crate::IndexFormat::Uint16, 0).is_err());
        assert!(encoder.set_vertex_buffer(0, indices, 0).is_err());
        assert!(encoder.set_index_buffer(vertices, crate::IndexFormat::Uint32, 0).is_err());
        encoder.set_index_buffer(indices, crate::IndexFormat::Uint32, 0).unwrap();
        encoder.set_vertex_buffer(0, vertices, 0).unwrap();
        encoder.set_index_buffer(indices, crate::IndexFormat::Uint16, 0).unwrap();
        encoder.draw_indexed(3, 1, 0, 0, 0).unwrap();
        encoder.end_render_pass().unwrap();
        let command = encoder.finish().unwrap();
        device.submit(device.graphics_queue(), &[command], None).unwrap();
        assert_eq!(device.buffer_count(), 2);
        assert_eq!(device.indexed_draw_count(), 1);
        assert_eq!(device.draw_count(), 1);
        let mut again = device.create_command_encoder(Some("reuse")).unwrap();
        again
            .begin_render_pass(&RenderPassDesc {
                label: None,
                colors: vec![ColorAttachment {
                    target: view,
                    load: crate::LoadOp::Load,
                    store: crate::StoreOp::Store,
                }],
                depth: None,
            })
            .unwrap();
        again.set_pipeline(pipeline).unwrap();
        again.set_vertex_buffer(0, vertices, 0).unwrap();
        again.set_index_buffer(indices, crate::IndexFormat::Uint16, 0).unwrap();
        again.draw_indexed(3, 1, 0, 0, 0).unwrap();
        again.end_render_pass().unwrap();
        let command = again.finish().unwrap();
        device.submit(device.graphics_queue(), &[command], None).unwrap();
        assert_eq!(device.buffer_count(), 2);
        assert_eq!(device.indexed_draw_count(), 2);
        device.destroy(ResourceKind::Buffer, vertices.raw()).unwrap();
        assert!(device.destroy(ResourceKind::Buffer, vertices.raw()).is_err());
    }

    #[test]
    fn generations_do_not_alias_a_reused_slot() {
        let mut device = device();
        let first = device
            .create_buffer(&crate::BufferDesc {
                size: 16,
                usage: BufferUsage::Vertex,
                label: Some("first".into()),
                contents: None,
            })
            .unwrap();
        device.destroy(ResourceKind::Buffer, first.raw()).unwrap();
        let second = device
            .create_buffer(&crate::BufferDesc {
                size: 16,
                usage: BufferUsage::Vertex,
                label: Some("second".into()),
                contents: None,
            })
            .unwrap();
        assert_eq!(first.raw().index, second.raw().index);
        assert_ne!(first.raw().generation, second.raw().generation);
        assert!(device.write_buffer(first, 0, &[0; 16]).is_err());
        assert_eq!(device.debug_label(ResourceKind::Buffer, second.raw()).as_deref(), Some("second"));
        let stats = device.resource_stats();
        assert_eq!(stats.alive_buffers, 1);
        assert_eq!(stats.retired, 0);
        assert!(stats.estimated_bytes >= 16);
    }

    #[test]
    fn uniform_binding_is_required_when_the_pipeline_has_a_layout() {
        let mut device = device();
        let uniform = device
            .create_buffer(&crate::BufferDesc {
                size: 192,
                usage: BufferUsage::Uniform,
                label: Some("JARVIG.Camera".into()),
                contents: None,
            })
            .unwrap();
        let vertices = device
            .create_buffer(&crate::BufferDesc {
                size: 16,
                usage: BufferUsage::Vertex,
                label: None,
                contents: None,
            })
            .unwrap();
        assert!(device.write_buffer(vertices, 0, &[0; 16]).is_err());
        assert!(device.write_buffer(uniform, 0, &[0; 192]).is_ok());
        assert!(device.write_buffer(crate::BufferId::INVALID, 0, &[0; 16]).is_err());
        let layout = device
            .create_bind_group_layout(&crate::BindGroupLayoutDesc {
                entries: vec![BindGroupLayoutEntry { binding: 0, kind: BindingType::UniformBuffer, stage: ShaderStage::Vertex }],
                label: Some("JARVIG.Camera".into()),
            })
            .unwrap();
        assert!(device
            .create_bind_group(&crate::BindGroupDesc {
                layout,
                entries: vec![crate::BindGroupEntry { binding: 0, resource: crate::BindResource::Buffer(vertices) }],
                label: None,
            })
            .is_err());
        let group = device
            .create_bind_group(&crate::BindGroupDesc {
                layout,
                entries: vec![crate::BindGroupEntry { binding: 0, resource: crate::BindResource::Buffer(uniform) }],
                label: None,
            })
            .unwrap();
        let shader = device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl("fn vs() {}".into()),
                label: None,
            })
            .unwrap();
        let pipeline = device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: crate::PrimitiveTopology::TriangleList,
                color_format: TextureFormat::Rgba8Unorm,
                layouts: vec![layout],
                vertex_buffers: Vec::new(),
                depth: None,
                cull: crate::CullMode::None,
                label: None,
            })
            .unwrap();
        let texture = device
            .create_texture(&TextureDesc {
                width: 4,
                height: 4,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: None,
            })
            .unwrap();
        let view = device.create_texture_view(texture).unwrap();
        let mut encoder = device.create_command_encoder(Some("camera")).unwrap();
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: None,
                colors: vec![ColorAttachment {
                    target: view,
                    load: crate::LoadOp::Load,
                    store: crate::StoreOp::Store,
                }],
                depth: None,
            })
            .unwrap();
        encoder.set_pipeline(pipeline).unwrap();
        assert!(encoder.draw(3, 1, 0, 0).is_err());
        assert!(encoder.set_bind_group(0, BindGroupId::INVALID).is_err());
        encoder.set_bind_group(0, group).unwrap();
        encoder.draw(3, 1, 0, 0).unwrap();
        encoder.end_render_pass().unwrap();
    }

    #[test]
    fn a_pipeline_with_two_groups_requires_both() {
        let mut device = device();
        let transform = device
            .create_buffer(&crate::BufferDesc {
                size: 192,
                usage: BufferUsage::Uniform,
                label: None,
                contents: None,
            })
            .unwrap();
        let material = device
            .create_buffer(&crate::BufferDesc {
                size: 16,
                usage: BufferUsage::Uniform,
                label: None,
                contents: None,
            })
            .unwrap();
        let group0 = device
            .create_bind_group_layout(&crate::BindGroupLayoutDesc {
                entries: vec![BindGroupLayoutEntry { binding: 0, kind: BindingType::UniformBuffer, stage: ShaderStage::Vertex }],
                label: Some("group0".into()),
            })
            .unwrap();
        let group1 = device
            .create_bind_group_layout(&crate::BindGroupLayoutDesc {
                entries: vec![BindGroupLayoutEntry {
                    binding: 0,
                    kind: BindingType::UniformBuffer,
                    stage: ShaderStage::Fragment,
                }],
                label: Some("group1".into()),
            })
            .unwrap();
        let bound0 = device
            .create_bind_group(&crate::BindGroupDesc {
                layout: group0,
                entries: vec![crate::BindGroupEntry { binding: 0, resource: crate::BindResource::Buffer(transform) }],
                label: None,
            })
            .unwrap();
        let bound1 = device
            .create_bind_group(&crate::BindGroupDesc {
                layout: group1,
                entries: vec![crate::BindGroupEntry { binding: 0, resource: crate::BindResource::Buffer(material) }],
                label: None,
            })
            .unwrap();
        let shader = device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl("fn vs() {}".into()),
                label: None,
            })
            .unwrap();
        let pipeline = device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: crate::PrimitiveTopology::TriangleList,
                color_format: TextureFormat::Rgba8Unorm,
                layouts: vec![group0, group1],
                vertex_buffers: Vec::new(),
                depth: None,
                cull: crate::CullMode::None,
                label: None,
            })
            .unwrap();
        let texture = device
            .create_texture(&TextureDesc {
                width: 4,
                height: 4,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: None,
            })
            .unwrap();
        let view = device.create_texture_view(texture).unwrap();
        let mut encoder = device.create_command_encoder(Some("material")).unwrap();
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: None,
                colors: vec![ColorAttachment {
                    target: view,
                    load: crate::LoadOp::Load,
                    store: crate::StoreOp::Store,
                }],
                depth: None,
            })
            .unwrap();
        encoder.set_pipeline(pipeline).unwrap();
        assert!(encoder.set_bind_group(4, bound0).is_err());
        encoder.set_bind_group(0, bound0).unwrap();
        assert!(encoder.draw(3, 1, 0, 0).is_err());
        encoder.set_bind_group(1, bound1).unwrap();
        encoder.draw(3, 1, 0, 0).unwrap();
        encoder.end_render_pass().unwrap();
    }

    #[test]
    fn a_sampled_texture_is_packed_and_a_view_blocks_destruction() {
        let mut device = device();
        let texture = device
            .create_texture(&TextureDesc {
                width: 4,
                height: 4,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: TextureUsage::Sampled,
                mip_count: 1,
                label: Some("color".into()),
            })
            .unwrap();
        assert!(device.write_texture(texture, &[0; 64]).is_ok());
        assert!(device.write_texture(texture, &[0; 256]).is_err());
        let linear = device
            .create_texture(&TextureDesc {
                width: 4,
                height: 4,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsage::Sampled,
                mip_count: 1,
                label: Some("data".into()),
            })
            .unwrap();
        assert!(device.write_texture(linear, &[7; 64]).is_ok());
        let view = device.create_texture_view(texture).unwrap();
        let sampler = device.create_sampler(&SamplerDesc::linear_repeat(None)).unwrap();
        let layout = device
            .create_bind_group_layout(&crate::BindGroupLayoutDesc {
                entries: vec![
                    BindGroupLayoutEntry { binding: 0, kind: BindingType::Texture, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 1, kind: BindingType::Sampler, stage: ShaderStage::Fragment },
                ],
                label: None,
            })
            .unwrap();
        let group = device
            .create_bind_group(&crate::BindGroupDesc {
                layout,
                entries: vec![
                    crate::BindGroupEntry { binding: 0, resource: crate::BindResource::TextureView(view) },
                    crate::BindGroupEntry { binding: 1, resource: crate::BindResource::Sampler(sampler) },
                ],
                label: None,
            })
            .unwrap();
        assert!(device.destroy(ResourceKind::Texture, texture.raw()).is_err());
        assert!(device.destroy(ResourceKind::TextureView, view.raw()).is_err());
        assert!(device.destroy(ResourceKind::Sampler, sampler.raw()).is_err());
        device.destroy(ResourceKind::BindGroup, group.raw()).unwrap();
        device.destroy(ResourceKind::TextureView, view.raw()).unwrap();
        device.destroy(ResourceKind::Texture, texture.raw()).unwrap();
        device.destroy(ResourceKind::Sampler, sampler.raw()).unwrap();
    }

    #[test]
    fn depth_pipeline_requires_a_matching_depth_attachment() {
        let mut device = device();
        let shader = device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl("fn vs() {}".into()),
                label: None,
            })
            .unwrap();
        let depth = device
            .create_texture(&TextureDesc {
                width: 4,
                height: 4,
                format: TextureFormat::Depth32Float,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: Some("JARVIG.MainDepth".into()),
            })
            .unwrap();
        assert!(device
            .create_texture(&TextureDesc {
                width: 0,
                height: 4,
                format: TextureFormat::Depth32Float,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: None,
            })
            .is_err());
        let depth_view = device.create_texture_view(depth).unwrap();
        let color = device
            .create_texture(&TextureDesc {
                width: 4,
                height: 4,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: None,
            })
            .unwrap();
        let color_view = device.create_texture_view(color).unwrap();
        let color_pipeline = device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: crate::PrimitiveTopology::TriangleList,
                color_format: TextureFormat::Rgba8Unorm,
                layouts: Vec::new(),
                vertex_buffers: Vec::new(),
                depth: None,
                cull: crate::CullMode::None,
                label: None,
            })
            .unwrap();
        let depth_pipeline = device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: "fs".into(),
                topology: crate::PrimitiveTopology::TriangleList,
                color_format: TextureFormat::Rgba8Unorm,
                layouts: Vec::new(),
                vertex_buffers: Vec::new(),
                depth: Some(crate::DepthState {
                    format: TextureFormat::Depth32Float,
                    write_enabled: true,
                    compare: crate::CompareFunction::GreaterEqual,
                }),
                cull: crate::CullMode::Back,
                label: None,
            })
            .unwrap();
        let mut encoder = device.create_command_encoder(Some("depth")).unwrap();
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("JARVIG.MainOpaquePass".into()),
                colors: vec![ColorAttachment {
                    target: color_view,
                    load: crate::LoadOp::Load,
                    store: crate::StoreOp::Store,
                }],
                depth: None,
            })
            .unwrap();
        encoder.set_pipeline(depth_pipeline).unwrap();
        assert!(encoder.draw(3, 1, 0, 0).is_err());
        encoder.end_render_pass().unwrap();
        assert!(encoder
            .begin_render_pass(&RenderPassDesc {
                label: None,
                colors: vec![ColorAttachment {
                    target: color_view,
                    load: crate::LoadOp::Load,
                    store: crate::StoreOp::Store,
                }],
                depth: Some(crate::DepthAttachment {
                    target: color_view,
                    load: crate::DepthLoadOp::Clear(0.0),
                    store: crate::StoreOp::Store,
                }),
            })
            .is_err());
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: None,
                colors: vec![ColorAttachment {
                    target: color_view,
                    load: crate::LoadOp::Load,
                    store: crate::StoreOp::Store,
                }],
                depth: Some(crate::DepthAttachment {
                    target: depth_view,
                    load: crate::DepthLoadOp::Clear(0.0),
                    store: crate::StoreOp::Store,
                }),
            })
            .unwrap();
        encoder.set_pipeline(color_pipeline).unwrap();
        assert!(encoder.draw(3, 1, 0, 0).is_err());
        encoder.set_pipeline(depth_pipeline).unwrap();
        encoder.draw(3, 1, 0, 0).unwrap();
        encoder.end_render_pass().unwrap();
    }

    #[test]
    fn viewport_and_scissor_must_fit_the_target() {
        let mut device = device();
        let texture = device
            .create_texture(&TextureDesc {
                width: 8,
                height: 8,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: None,
            })
            .unwrap();
        let view = device.create_texture_view(texture).unwrap();
        let mut encoder = device.create_command_encoder(Some("viewport")).unwrap();
        assert!(encoder
            .set_viewport(crate::Viewport { x: 0.0, y: 0.0, width: 8.0, height: 8.0, min_depth: 0.0, max_depth: 1.0 })
            .is_err());
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: None,
                colors: vec![ColorAttachment {
                    target: view,
                    load: crate::LoadOp::Load,
                    store: crate::StoreOp::Store,
                }],
                depth: None,
            })
            .unwrap();
        assert!(encoder
            .set_viewport(crate::Viewport { x: 0.0, y: 0.0, width: 0.0, height: 8.0, min_depth: 0.0, max_depth: 1.0 })
            .is_err());
        assert!(encoder
            .set_viewport(crate::Viewport { x: 0.0, y: 0.0, width: 9.0, height: 8.0, min_depth: 0.0, max_depth: 1.0 })
            .is_err());
        assert!(encoder.set_scissor(crate::ScissorRect { x: 0, y: 0, width: 9, height: 1 }).is_err());
        encoder
            .set_viewport(crate::Viewport { x: 0.0, y: 0.0, width: 4.0, height: 8.0, min_depth: 0.0, max_depth: 1.0 })
            .unwrap();
        encoder.set_scissor(crate::ScissorRect { x: 0, y: 0, width: 4, height: 8 }).unwrap();
        encoder.end_render_pass().unwrap();
    }
}
