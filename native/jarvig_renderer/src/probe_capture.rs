//! Static reflection-probe capture.
//!
//! One HDR cubemap per probe, six faces, rendered with the scene material and the scene
//! lights. The probe uniform is disabled for these draws so the capture cannot
//! sample itself. Editor overlay is not a scene pass and is not drawn here.
//! The cube is not tone-mapped. Higher mips are a GGX importance-sample prefilter.
//! Mip 0 stays the sharp capture. Mip `i` is roughness `i / (mip_count - 1)`.
//!
//! Direct-light shadow maps are bound for layout compatibility and left disabled.
//! The capture does not darken with visibility. A later recapture policy can turn them on.
//!
//! The first cube under Static, On Demand, On Transform, or On Lighting finishes in that
//! frame, so the bootstrap self-test still captures once. Every later refresh, and every
//! Time Sliced capture, spends one face of the frame budget. The cube already on screen
//! stays bound until the new one is complete. Camera movement does not dirty a probe.
//! A later dynamic mode uses this budget. It does not recapture every probe every frame.

use jarvig_core::{
    instance_gpu_transforms, reflection_cube_faces, reflection_probe_influence, render_light_record,
    rotation_looking_toward, select_reflection_probe, GpuReflectionProbePacket, GpuTransforms, MeshLibrary, RenderInstanceId,
    RenderSceneSnapshot, ResolvedPose, TextureLibrary, REFLECTION_PROBE_RESOLUTION, DEPTH_CLEAR,
};
use jarvig_material::{MaterialLibrary, ShadingModel};
use jarvig_rhi::{
    AddressMode, BindGroupDesc, BindGroupEntry, BindGroupId, BindGroupLayoutDesc, BindGroupLayoutEntry, BindGroupLayoutId,
    BindingType, BufferDesc, BufferId, BufferUsage, ClearColor, ColorAttachment, CubeTextureDesc, DepthAttachment, DepthLoadOp,
    FilterMode, LoadOp, PipelineId, PrimitiveTopology, RenderPassDesc, RenderPipelineDesc, ResourceKind, SamplerDesc,
    ScissorRect, ShaderModuleDesc, ShaderModuleId, ShaderSource, ShaderStage, StoreOp, TextureDesc, TextureFormat, TextureId,
    TextureUsage, TextureViewId, Viewport, CullMode,
};

use super::{PreparedDraw, RenderError, Renderer, LIGHT_CAPACITY};

const CAPTURE_NEAR_M: f32 = 0.05;

const PREFILTER_SHADER: &str = r#"
struct Prefilter {
    face: u32,
    sample_count: u32,
    roughness: f32,
    source_resolution: f32,
    max_mip: f32,
    dest_edge: f32,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var env_cube: texture_cube<f32>;
@group(0) @binding(1) var env_sampler: sampler;
@group(0) @binding(2) var<uniform> prefilter: Prefilter;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index) % 2) * 4.0 - 1.0;
    let y = f32(i32(index) / 2) * 4.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn radical_inverse(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}

fn face_direction(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let s = uv.x * 2.0 - 1.0;
    let t = uv.y * 2.0 - 1.0;
    var dir = vec3<f32>(0.0, 0.0, 1.0);
    if (face == 0u) { dir = vec3<f32>(1.0, -t, -s); }
    else if (face == 1u) { dir = vec3<f32>(-1.0, -t, s); }
    else if (face == 2u) { dir = vec3<f32>(s, 1.0, t); }
    else if (face == 3u) { dir = vec3<f32>(s, -1.0, -t); }
    else if (face == 4u) { dir = vec3<f32>(s, -t, 1.0); }
    else { dir = vec3<f32>(-s, -t, -1.0); }
    return normalize(dir);
}

fn ggx_direction(xi: vec2<f32>, roughness: f32, n: vec3<f32>) -> vec3<f32> {
    let alpha = roughness * roughness;
    let phi = 6.28318530718 * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / (1.0 + (alpha * alpha - 1.0) * xi.y));
    let sin_theta = sqrt(max(0.0, 1.0 - cos_theta * cos_theta));
    let h = vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos_theta);
    var up = vec3<f32>(0.0, 0.0, 1.0);
    if (abs(n.z) >= 0.999) { up = vec3<f32>(1.0, 0.0, 0.0); }
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    return normalize(tangent * h.x + bitangent * h.y + n * h.z);
}

fn sample_mip(n_dot_h: f32, h_dot_v: f32, roughness: f32) -> f32 {
    if (roughness < 0.001 || h_dot_v <= 0.00001) { return 0.0; }
    let alpha = roughness * roughness;
    let alpha2 = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
    let distribution = alpha2 / (3.14159265 * denom * denom);
    let pdf = distribution * n_dot_h / (4.0 * h_dot_v) + 0.0001;
    let resolution = prefilter.source_resolution;
    let sa_texel = 12.5663706144 / (6.0 * resolution * resolution);
    let sa_sample = 1.0 / (f32(prefilter.sample_count) * pdf);
    return clamp(0.5 * log2(sa_sample / sa_texel), 0.0, prefilter.max_mip);
}

@fragment
fn ggx_fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = pos.xy / vec2<f32>(prefilter.dest_edge);
    let n = face_direction(prefilter.face, uv);
    var color = vec3<f32>(0.0);
    var weight = 0.0;
    let count = prefilter.sample_count;
    for (var i = 0u; i < count; i = i + 1u) {
        let xi = vec2<f32>(f32(i) / f32(count), radical_inverse(i));
        let h = ggx_direction(xi, prefilter.roughness, n);
        let l = normalize(2.0 * dot(n, h) * h - n);
        let n_dot_l = saturate(dot(n, l));
        if (n_dot_l > 0.0) {
            let n_dot_h = saturate(dot(n, h));
            let mip = sample_mip(n_dot_h, n_dot_h, prefilter.roughness);
            // The cube sample selects the face from the direction, including across an edge.
            color = color + textureSampleLevel(env_cube, env_sampler, l, mip).rgb * n_dot_l;
            weight = weight + n_dot_l;
        }
    }
    if (weight > 0.0) {
        return vec4<f32>(color / weight, 1.0);
    }
    return vec4<f32>(textureSampleLevel(env_cube, env_sampler, n, 0.0).rgb, 1.0);
}
"#;

const COPY_SHADER: &str = r#"
@group(0) @binding(0) var copy_src: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index) % 2) * 4.0 - 1.0;
    let y = f32(i32(index) / 2) * 4.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn copy_fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(copy_src, vec2<i32>(pos.xy), 0);
}
"#;

/// Cosine convolution of the captured cube. Not a GGX mip and not a second capture.
/// The PDF is `cos / π`, so the stored value is the average sample. The material multiplies albedo.
const IRRADIANCE_SHADER: &str = r#"
struct Irradiance {
    face: u32,
    sample_count: u32,
}

@group(0) @binding(0) var env_cube: texture_cube<f32>;
@group(0) @binding(1) var env_sampler: sampler;
@group(0) @binding(2) var<uniform> irradiance: Irradiance;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index) % 2) * 4.0 - 1.0;
    let y = f32(i32(index) / 2) * 4.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn radical_inverse(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}

fn face_direction(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let s = uv.x * 2.0 - 1.0;
    let t = uv.y * 2.0 - 1.0;
    var dir = vec3<f32>(0.0, 0.0, 1.0);
    if (face == 0u) { dir = vec3<f32>(1.0, -t, -s); }
    else if (face == 1u) { dir = vec3<f32>(-1.0, -t, s); }
    else if (face == 2u) { dir = vec3<f32>(s, 1.0, t); }
    else if (face == 3u) { dir = vec3<f32>(s, -1.0, -t); }
    else if (face == 4u) { dir = vec3<f32>(s, -t, 1.0); }
    else { dir = vec3<f32>(-s, -t, -1.0); }
    return normalize(dir);
}

fn cosine_direction(xi: vec2<f32>, n: vec3<f32>) -> vec3<f32> {
    let phi = 6.28318530718 * xi.x;
    let cos_theta = sqrt(xi.y);
    let sin_theta = sqrt(max(0.0, 1.0 - xi.y));
    let local = vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos_theta);
    var up = vec3<f32>(0.0, 0.0, 1.0);
    if (abs(n.z) >= 0.999) { up = vec3<f32>(1.0, 0.0, 0.0); }
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    return normalize(tangent * local.x + bitangent * local.y + n * local.z);
}

@fragment
fn irradiance_fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let edge = vec2<f32>(textureDimensions(env_cube));
    let n = face_direction(irradiance.face, pos.xy / edge);
    var color = vec3<f32>(0.0);
    let count = max(irradiance.sample_count, 1u);
    for (var i = 0u; i < count; i = i + 1u) {
        let xi = vec2<f32>(f32(i) / f32(count), radical_inverse(i));
        let sample_dir = cosine_direction(xi, n);
        color = color + textureSampleLevel(env_cube, env_sampler, sample_dir, 0.0).rgb;
    }
    return vec4<f32>(color / f32(count), 1.0);
}
"#;

struct CaptureTemp {
    groups: Vec<BindGroupId>,
    buffers: Vec<BufferId>,
    views: Vec<TextureViewId>,
    textures: Vec<TextureId>,
    pipelines: Vec<PipelineId>,
    shaders: Vec<ShaderModuleId>,
    layouts: Vec<BindGroupLayoutId>,
}

impl CaptureTemp {
    fn new() -> Self {
        Self {
            groups: Vec::new(),
            buffers: Vec::new(),
            views: Vec::new(),
            textures: Vec::new(),
            pipelines: Vec::new(),
            shaders: Vec::new(),
            layouts: Vec::new(),
        }
    }

    fn destroy(self, renderer: &mut Renderer) -> Result<(), RenderError> {
        for group in &self.groups {
            renderer.device.destroy(ResourceKind::BindGroup, group.raw()).map_err(RenderError::Rhi)?;
        }
        for pipeline in &self.pipelines {
            renderer.device.destroy(ResourceKind::Pipeline, pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        for shader in &self.shaders {
            renderer.device.destroy(ResourceKind::ShaderModule, shader.raw()).map_err(RenderError::Rhi)?;
        }
        for layout in &self.layouts {
            renderer.device.destroy(ResourceKind::BindGroupLayout, layout.raw()).map_err(RenderError::Rhi)?;
        }
        for view in &self.views {
            renderer.device.destroy(ResourceKind::TextureView, view.raw()).map_err(RenderError::Rhi)?;
        }
        for texture in &self.textures {
            renderer.device.destroy(ResourceKind::Texture, texture.raw()).map_err(RenderError::Rhi)?;
        }
        for buffer in &self.buffers {
            renderer.device.destroy(ResourceKind::Buffer, buffer.raw()).map_err(RenderError::Rhi)?;
        }
        renderer.device.flush().map_err(RenderError::Rhi)?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct CaptureObject {
    instance: RenderInstanceId,
    mesh: jarvig_core::MeshId,
    submesh: usize,
    material: BindGroupId,
    pipeline: PipelineId,
    uniform: BufferId,
    group: BindGroupId,
    vertex_stride: u32,
}

struct OpenCapture {
    objects: Vec<CaptureObject>,
    depth_view: TextureViewId,
    header: BufferId,
    storage: BufferId,
    probe_buffer: BufferId,
    lights: BindGroupId,
}

enum CaptureWork {
    Scene(u32),
    Prefilter { mip: u32, face: u32 },
    Irradiance(u32),
}

struct ProbeFilterState {
    prefilter_pipeline: PipelineId,
    copy_pipeline: PipelineId,
    irradiance_pipeline: Option<PipelineId>,
    prefilter_layout: BindGroupLayoutId,
    prefilter_group: BindGroupId,
    copy_layout: BindGroupLayoutId,
    scratch_view: TextureViewId,
    scratch_edge: u32,
}

/// One probe rebuild spread across frames. The destination is not sampled until it is finished.
pub(super) struct ProbeCaptureJob {
    id: jarvig_core::ProbeId,
    destination: super::ProbeGpu,
    temp: CaptureTemp,
    objects: Vec<CaptureObject>,
    depth_view: TextureViewId,
    header: BufferId,
    storage: BufferId,
    probe_buffer: BufferId,
    lights: BindGroupId,
    resolution: u32,
    mip_count: u32,
    filter: Option<ProbeFilterState>,
    step: u32,
    steps: u32,
    face_ms: u32,
    prefilter_ms: u32,
    target_world: u64,
    target_lighting: u64,
    target_serial: u64,
}

impl Default for CaptureTemp {
    fn default() -> Self {
        Self::new()
    }
}

/// One cubemap face per frame after the first synchronous cube. Not a full recapture.
const REFLECTION_CAPTURE_STEPS_PER_FRAME: u32 = 1;

fn elapsed_ms(started: std::time::Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

fn capture_step_count(mip_count: u32) -> u32 {
    6 + mip_count.saturating_sub(1) * 6 + 6
}

fn capture_work(step: u32, mip_count: u32) -> Option<CaptureWork> {
    let scene = 6u32;
    let prefilter = mip_count.saturating_sub(1).saturating_mul(6);
    if step < scene {
        return Some(CaptureWork::Scene(step));
    }
    let after = step - scene;
    if after < prefilter {
        return Some(CaptureWork::Prefilter { mip: after / 6 + 1, face: after % 6 });
    }
    let irradiance = after - prefilter;
    if irradiance < 6 {
        Some(CaptureWork::Irradiance(irradiance))
    } else {
        None
    }
}

impl Renderer {
    pub fn reflection_probe_count(&self) -> u32 {
        self.probe_count
    }

    pub fn reflection_probe_active_count(&self) -> u32 {
        self.probe_active_count
    }

    pub fn reflection_probe_capture_count(&self) -> u32 {
        self.probe_capture_count
    }

    pub fn reflection_probe_selected_far(&self) -> u32 {
        self.probe_selected_far
    }

    pub fn reflection_probe_fallback_count(&self) -> u32 {
        self.probe_fallback_count
    }

    pub fn reflection_probe_resolution(&self) -> u32 {
        self.probe_residents.first().map(|probe| probe.gpu.resolution).unwrap_or(0)
    }

    pub fn reflection_probe_mip_count(&self) -> u32 {
        self.probe_residents.first().map(|probe| probe.gpu.mip_count).unwrap_or(0)
    }

    /// Captured cubemaps only. Not the unbound shader fallback, not scene color, not a material texture.
    pub fn reflection_probe_texture_count(&self) -> u32 {
        self.probe_residents.len() as u32
    }

    /// Probe ids that own a cube. Order is capture order. Not a shared texture.
    pub fn reflection_probe_resident_ids(&self) -> Vec<jarvig_core::ProbeId> {
        self.probe_residents.iter().map(|probe| probe.id).collect()
    }

    pub fn reflection_probe_resident_textures(&self) -> Vec<(jarvig_core::ProbeId, u32, u32)> {
        self.probe_residents
            .iter()
            .map(|probe| {
                let raw = probe.gpu.texture.raw();
                (probe.id, raw.index, raw.generation)
            })
            .collect()
    }

    pub(super) fn note_reflection_probes(&mut self, snapshot: &RenderSceneSnapshot) {
        let probes = snapshot.reflection_probes();
        self.probe_count = probes.len() as u32;
        self.probe_active_count = probes.iter().filter(|probe| probe.enabled && probe.radius_m > 0.0).count() as u32;
        self.probe_fallback_count = probes_outside(snapshot);
        self.probe_selected_far = u32::from(
            snapshot.instances().get(1).is_some_and(|instance| reflection_probe_influence(probes, instance.pose.translation) > 0.0),
        );
    }

    fn create_probe_gpu(&mut self, label: &str, resolution: u32) -> Result<super::ProbeGpu, RenderError> {
        let mip_count = jarvig_core::reflection_probe_mip_count_for(resolution);
        let texture = self
            .device
            .create_cube_texture(&CubeTextureDesc {
                size: resolution,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsage::ColorTarget,
                mip_count,
                label: Some(label.into()),
            })
            .map_err(RenderError::Rhi)?;
        let sample = self.device.create_cube_sample_view(texture).map_err(RenderError::Rhi)?;
        let irradiance = self
            .device
            .create_cube_texture(&CubeTextureDesc {
                size: resolution,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsage::ColorTarget,
                mip_count: 1,
                label: Some(format!("{label}.Irradiance")),
            })
            .map_err(RenderError::Rhi)?;
        let irradiance_sample = self.device.create_cube_sample_view(irradiance).map_err(RenderError::Rhi)?;
        let sampler = self
            .device
            .create_sampler(&SamplerDesc {
                min_filter: FilterMode::Linear,
                mag_filter: FilterMode::Linear,
                mip_filter: FilterMode::Linear,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
                address_w: AddressMode::ClampToEdge,
                anisotropy: 1,
                label: Some(label.into()),
            })
            .map_err(RenderError::Rhi)?;
        Ok(super::ProbeGpu {
            texture,
            sample,
            sampler,
            resolution,
            mip_count,
            irradiance,
            irradiance_sample,
        })
    }

    fn retire_absent_probes(&mut self, snapshot: &RenderSceneSnapshot) -> Result<(), RenderError> {
        let live: Vec<jarvig_core::ProbeId> = snapshot.reflection_probes().iter().map(|probe| probe.id).collect();
        let stale: Vec<jarvig_core::ProbeId> = self.probe_residents.iter().map(|probe| probe.id).filter(|id| !live.contains(id)).collect();
        for id in stale {
            if let Some(index) = self.probe_residents.iter().position(|probe| probe.id == id) {
                let resident = self.probe_residents.remove(index);
                self.detach_probe_view(resident.gpu.sample)?;
                self.destroy_probe_gpu(resident.gpu)?;
            }
        }
        Ok(())
    }

    /// The selected probe's cube, or another resident cube, or a one-off unbound cube.
    /// The unbound cube is not counted as probe residency. The irradiance view matches that cube.
    pub(super) fn probe_draw_binding(
        &mut self,
        probes: &[jarvig_core::RenderReflectionProbe],
    ) -> Result<(TextureViewId, TextureViewId, jarvig_rhi::SamplerId), RenderError> {
        let selected = select_reflection_probe(probes).map(|probe| probe.id);
        let id = selected.or_else(|| self.probe_residents.first().map(|probe| probe.id));
        if let Some(id) = id {
            if let Some(resident) = self.probe_residents.iter().find(|probe| probe.id == id) {
                return Ok((resident.gpu.sample, resident.gpu.irradiance_sample, resident.gpu.sampler));
            }
        }
        if self.probe_unbound.is_none() {
            self.probe_unbound = Some(self.create_probe_gpu("JARVIG.ReflectionProbe.Unbound", REFLECTION_PROBE_RESOLUTION)?);
        }
        let probe = self.probe_unbound.as_ref().expect("unbound probe");
        Ok((probe.sample, probe.irradiance_sample, probe.sampler))
    }

    pub fn indirect_diffuse_texture_count(&self) -> u32 {
        self.probe_residents.len() as u32
    }

    pub fn probe_recapture_count(&self) -> u32 {
        self.probe_recapture_count
    }

    pub fn irradiance_build_count(&self) -> u32 {
        self.irradiance_build_count
    }

    pub fn last_probe_capture_ms(&self) -> u32 {
        self.last_capture_ms
    }

    /// GGX mip faces plus the cosine irradiance build. Not the six scene faces.
    pub fn last_probe_prefilter_ms(&self) -> u32 {
        self.last_prefilter_ms
    }

    /// Dirty probes, including the one currently being rebuilt. Camera motion does not add one.
    pub fn probe_capture_queue_len(&self) -> u32 {
        self.probe_queue_len
    }

    /// Face size of the cube being built. Zero when no rebuild is in flight.
    pub fn probe_capture_job_resolution(&self) -> u32 {
        self.probe_job.as_ref().map(|job| job.resolution).unwrap_or(0)
    }

    pub fn probe_capture_job_step(&self) -> u32 {
        self.probe_job.as_ref().map(|job| job.step).unwrap_or(0)
    }

    pub fn probe_capture_job_steps(&self) -> u32 {
        self.probe_job.as_ref().map(|job| job.steps).unwrap_or(0)
    }

    /// Specular chain plus the irradiance cube. Not the unbound fallback and not shadow maps.
    pub fn reflection_probe_memory_bytes(&self) -> u64 {
        self.probe_residents.iter().map(|resident| cube_memory_bytes(resident.gpu.resolution, resident.gpu.mip_count) + cube_memory_bytes(resident.gpu.resolution, 1)).sum()
    }

    /// Each enabled probe without a cube is captured in this frame, except under Time Sliced.
    /// A dirty cube already on screen is rebuilt one face per frame into a side cube, then swapped.
    /// Deleting a probe drops that cube. It does not reuse another probe's texture.
    pub(super) fn capture_static_probe(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
    ) -> Result<(), RenderError> {
        self.retire_absent_probes(snapshot)?;
        let probes: Vec<jarvig_core::RenderReflectionProbe> = snapshot
            .reflection_probes()
            .iter()
            .copied()
            .filter(|probe| probe.enabled && probe.radius_m > 0.0)
            .collect();
        self.drop_job_unless_current(&probes)?;
        if probes.is_empty() {
            self.probe_queue_len = 0;
            return Ok(());
        }
        if self.probe_job.is_some() {
            self.note_job_scene(snapshot);
            self.advance_probe_job(snapshot, meshes, materials, textures)?;
            self.probe_queue_len = self.reflection_queue_len(snapshot, &probes);
            return Ok(());
        }
        let environment = self.ensure_environment_buffer(&snapshot.environment())?;
        for probe in &probes {
            let exists = self.probe_residents.iter().any(|resident| resident.id == probe.id);
            let refresh = exists
                && self
                    .probe_residents
                    .iter()
                    .find(|resident| resident.id == probe.id)
                    .is_some_and(|resident| probe_needs_refresh(resident, snapshot, probe));
            if exists && !refresh {
                continue;
            }
            let budget = capture_step_budget(snapshot.probe_policy, !exists);
            if budget == u32::MAX {
                self.capture_probe_immediate(snapshot, meshes, materials, textures, probe, environment)?;
            } else {
                self.begin_probe_job(snapshot, meshes, materials, textures, probe, environment)?;
                if self.probe_job.is_some() {
                    self.advance_probe_job(snapshot, meshes, materials, textures)?;
                }
                break;
            }
        }
        self.probe_queue_len = self.reflection_queue_len(snapshot, &probes);
        Ok(())
    }

    fn capture_probe_immediate(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
        probe: &jarvig_core::RenderReflectionProbe,
        environment: BufferId,
    ) -> Result<(), RenderError> {
        let gpu = self.create_probe_gpu("JARVIG.ReflectionProbe", probe.resolution)?;
        let texture = gpu.texture;
        let sampler = gpu.sampler;
        let irradiance = gpu.irradiance;
        let irradiance_view = gpu.irradiance_sample;
        self.probe_residents.push(super::ProbeResident {
            id: probe.id,
            gpu,
            captured_world: 0,
            captured_lighting: 0,
            captured_serial: u64::MAX,
        });
        let mut temp = CaptureTemp::new();
        let timed = self.capture_faces(
            snapshot,
            meshes,
            materials,
            textures,
            probe,
            environment,
            sampler,
            texture,
            irradiance,
            irradiance_view,
            &mut temp,
        );
        temp.destroy(self)?;
        let (face_ms, prefilter_ms) = match timed {
            Ok(times) => times,
            Err(error) => {
                if let Some(index) = self.probe_residents.iter().position(|resident| resident.id == probe.id) {
                    let resident = self.probe_residents.remove(index);
                    self.destroy_probe_gpu(resident.gpu)?;
                }
                return Err(error);
            }
        };
        self.probe_capture_count = self.probe_capture_count.saturating_add(1);
        self.irradiance_build_count = self.irradiance_build_count.saturating_add(1);
        self.last_capture_ms = face_ms;
        self.last_prefilter_ms = prefilter_ms;
        if let Some(resident) = self.probe_residents.iter_mut().find(|resident| resident.id == probe.id) {
            resident.captured_world = snapshot.world_revision;
            resident.captured_lighting = snapshot.lighting_revision;
            resident.captured_serial = snapshot.probe_recapture_serial;
        }
        Ok(())
    }

    fn capture_faces(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
        probe: &jarvig_core::RenderReflectionProbe,
        environment: BufferId,
        sampler: jarvig_rhi::SamplerId,
        cube: TextureId,
        irradiance: TextureId,
        irradiance_view: TextureViewId,
        temp: &mut CaptureTemp,
    ) -> Result<(u32, u32), RenderError> {
        let Some(open) = self.open_capture(snapshot, meshes, materials, textures, probe, environment, sampler, irradiance_view, temp)? else {
            return Ok((0, 0));
        };
        let faces = reflection_cube_faces();
        let face_started = std::time::Instant::now();
        for (face_index, (forward, up)) in faces.iter().enumerate() {
            let rotation = rotation_looking_toward(*forward, *up).map_err(RenderError::Space)?;
            let camera = ResolvedPose { translation: probe.translation, rotation };
            for object in &open.objects {
                let instance = snapshot.instances().iter().find(|instance| instance.id == object.instance).expect("instance");
                let packet = instance_gpu_transforms(instance, &camera, std::f64::consts::FRAC_PI_2, CAPTURE_NEAR_M, 1.0).map_err(RenderError::Space)?;
                self.device.write_buffer(object.uniform, 0, &packet.to_bytes()).map_err(RenderError::Rhi)?;
            }
            self.write_capture_lights(snapshot, &camera, open.header, open.storage)?;
            let face = self.device.create_cube_face_view(cube, face_index as u32, 0).map_err(RenderError::Rhi)?;
            temp.views.push(face);
            self.draw_capture_face(face, open.depth_view, open.lights, open.objects.as_slice(), face_clear(snapshot, forward.y), probe.resolution)?;
            self.device.flush().map_err(RenderError::Rhi)?;
        }
        let face_ms = elapsed_ms(face_started);
        let prefilter_started = std::time::Instant::now();
        self.downsample_cube(cube, sampler, open.probe_buffer, irradiance, probe.resolution, probe.mip_count, temp)?;
        Ok((face_ms, elapsed_ms(prefilter_started)))
    }

    fn open_capture(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
        probe: &jarvig_core::RenderReflectionProbe,
        environment: BufferId,
        sampler: jarvig_rhi::SamplerId,
        irradiance_view: TextureViewId,
        temp: &mut CaptureTemp,
    ) -> Result<Option<OpenCapture>, RenderError> {
        let objects = self.capture_objects(snapshot, meshes, materials, textures, temp)?;
        if objects.is_empty() {
            return Ok(None);
        }
        let dummy = self
            .device
            .create_cube_texture(&CubeTextureDesc {
                size: 1,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsage::ColorTarget,
                mip_count: 1,
                label: Some("JARVIG.ReflectionProbe.Unbound".into()),
            })
            .map_err(RenderError::Rhi)?;
        let dummy_view = self.device.create_cube_sample_view(dummy).map_err(RenderError::Rhi)?;
        temp.views.push(dummy_view);
        temp.textures.push(dummy);
        let depth = self
            .device
            .create_texture(&TextureDesc {
                width: probe.resolution,
                height: probe.resolution,
                format: TextureFormat::Depth32Float,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: Some("JARVIG.ReflectionProbe.Depth".into()),
            })
            .map_err(RenderError::Rhi)?;
        let depth_view = self.device.create_texture_view(depth).map_err(RenderError::Rhi)?;
        temp.views.push(depth_view);
        temp.textures.push(depth);
        let layout = self.light_layout.ok_or(RenderError::Rhi(jarvig_rhi::RhiError::InvalidResource("probe layout")))?;
        let header = self.uniform_buffer(16, "JARVIG.ReflectionProbe.Header", temp)?;
        let storage = self.storage_buffer((LIGHT_CAPACITY * jarvig_core::GpuLightRecord::BYTES) as u64, temp)?;
        let disabled = GpuReflectionProbePacket::disabled().to_bytes();
        let probe_buffer = self.uniform_buffer(GpuReflectionProbePacket::BYTES as u64, "JARVIG.ReflectionProbe.Off", temp)?;
        self.device.write_buffer(probe_buffer, 0, &disabled).map_err(RenderError::Rhi)?;
        let maps = self.shadow_sample_bindings(None)?;
        let contact = self.contact_view().unwrap_or(maps.spot);
        let lights = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout,
                entries: vec![
                    BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(header) },
                    BindGroupEntry { binding: 1, resource: jarvig_rhi::BindResource::Buffer(storage) },
                    BindGroupEntry { binding: 2, resource: jarvig_rhi::BindResource::Buffer(environment) },
                    BindGroupEntry { binding: 3, resource: jarvig_rhi::BindResource::Buffer(probe_buffer) },
                    BindGroupEntry { binding: 4, resource: jarvig_rhi::BindResource::TextureView(dummy_view) },
                    BindGroupEntry { binding: 5, resource: jarvig_rhi::BindResource::Sampler(sampler) },
                    BindGroupEntry { binding: 6, resource: jarvig_rhi::BindResource::Buffer(maps.disabled) },
                    BindGroupEntry { binding: 7, resource: jarvig_rhi::BindResource::TextureView(maps.directional) },
                    BindGroupEntry { binding: 8, resource: jarvig_rhi::BindResource::TextureView(maps.spot) },
                    BindGroupEntry { binding: 9, resource: jarvig_rhi::BindResource::TextureView(maps.point) },
                    BindGroupEntry { binding: 10, resource: jarvig_rhi::BindResource::Sampler(maps.sampler) },
                    BindGroupEntry { binding: 11, resource: jarvig_rhi::BindResource::TextureView(irradiance_view) },
                    BindGroupEntry { binding: 12, resource: jarvig_rhi::BindResource::TextureView(contact) },
                ],
                label: Some("JARVIG.ReflectionProbe.CaptureLights".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.groups.push(lights);
        Ok(Some(OpenCapture { objects, depth_view, header, storage, probe_buffer, lights }))
    }

    fn capture_objects(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
        temp: &mut CaptureTemp,
    ) -> Result<Vec<CaptureObject>, RenderError> {
        let layout = self.camera_layout.ok_or(RenderError::Rhi(jarvig_rhi::RhiError::InvalidResource("camera layout")))?;
        let mut objects = Vec::new();
        for instance in snapshot.instances().iter().filter(|instance| instance.visible) {
            let mesh = meshes.get(instance.mesh).ok_or(RenderError::Mesh(jarvig_core::MeshError::Empty))?;
            for (submesh_index, submesh) in mesh.submeshes().iter().enumerate() {
                let Some(material_id) = instance.material_for_slot(submesh.material_slot) else { continue };
                let master = materials.master_of(material_id).map_err(|_| RenderError::UnknownMaterial)?;
                let compiled = materials.compiled(master).map_err(|_| RenderError::UnknownMaterial)?.clone();
                if compiled.shading != ShadingModel::StandardMetalRough {
                    continue;
                }
                if !super::mesh_satisfies(mesh, &compiled) {
                    return Err(RenderError::IncompatibleMaterial);
                }
                let pipeline = self.ensure_gpu_master(master, &compiled, mesh)?;
                let material = self.ensure_gpu_material(material_id, master, &compiled, materials, textures)?;
                let uniform = self.uniform_buffer(GpuTransforms::BYTES as u64, "JARVIG.ReflectionProbe.Transform", temp)?;
                let group = self
                    .device
                    .create_bind_group(&BindGroupDesc {
                        layout,
                        entries: vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(uniform) }],
                        label: Some("JARVIG.ReflectionProbe.Transform".into()),
                    })
                    .map_err(RenderError::Rhi)?;
                temp.groups.push(group);
                objects.push(CaptureObject {
                    instance: instance.id,
                    mesh: instance.mesh,
                    submesh: submesh_index,
                    material,
                    pipeline,
                    uniform,
                    group,
                    vertex_stride: mesh.streams().first().map(|stream| stream.stride).unwrap_or(0),
                });
            }
        }
        Ok(objects)
    }

    fn write_capture_lights(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        camera: &ResolvedPose,
        header: BufferId,
        storage: BufferId,
    ) -> Result<(), RenderError> {
        let mut bytes = Vec::new();
        for light in snapshot.lights() {
            bytes.extend_from_slice(&render_light_record(light, camera).to_bytes());
        }
        let mut packed = [0u8; 16];
        packed[0..4].copy_from_slice(&(snapshot.lights().len() as u32).to_le_bytes());
        self.device.write_buffer(header, 0, &packed).map_err(RenderError::Rhi)?;
        if !bytes.is_empty() {
            self.device.write_buffer(storage, 0, &bytes).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn draw_capture_face(
        &mut self,
        face: TextureViewId,
        depth: TextureViewId,
        lights: BindGroupId,
        objects: &[CaptureObject],
        clear: ClearColor,
        resolution: u32,
    ) -> Result<(), RenderError> {
        let mut encoder = self.device.create_command_encoder(Some("JARVIG.ReflectionProbe.Face")).map_err(RenderError::Rhi)?;
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("JARVIG.ReflectionProbe.Face".into()),
                colors: vec![ColorAttachment { target: face, load: LoadOp::Clear(clear), store: StoreOp::Store }],
                depth: Some(DepthAttachment { target: depth, load: DepthLoadOp::Clear(DEPTH_CLEAR), store: StoreOp::Store }),
            })
            .map_err(RenderError::Rhi)?;
        let edge = resolution as f32;
        encoder
            .set_viewport(Viewport { x: 0.0, y: 0.0, width: edge, height: edge, min_depth: 0.0, max_depth: 1.0 })
            .map_err(RenderError::Rhi)?;
        encoder
            .set_scissor(ScissorRect { x: 0, y: 0, width: resolution, height: resolution })
            .map_err(RenderError::Rhi)?;
        for object in objects {
            self.draw_prepared(
                &mut *encoder,
                &PreparedDraw {
                    mesh: object.mesh,
                    submesh: object.submesh,
                    transform: object.group,
                    material: object.material,
                    lights: Some(lights),
                    pipeline: object.pipeline,
                    vertex_stride: object.vertex_stride,
                },
                false,
            )?;
        }
        encoder.end_render_pass().map_err(RenderError::Rhi)?;
        let buffer = encoder.finish().map_err(RenderError::Rhi)?;
        self.device.submit(self.device.graphics_queue(), &[buffer], None).map_err(RenderError::Rhi)
    }

    fn downsample_cube(
        &mut self,
        cube: TextureId,
        sampler: jarvig_rhi::SamplerId,
        params: BufferId,
        irradiance: TextureId,
        resolution: u32,
        mip_count: u32,
        temp: &mut CaptureTemp,
    ) -> Result<(), RenderError> {
        let prefilter_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries: vec![
                    BindGroupLayoutEntry { binding: 0, kind: BindingType::TextureCube, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 1, kind: BindingType::Sampler, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 2, kind: BindingType::UniformBuffer, stage: ShaderStage::Fragment },
                ],
                label: Some("JARVIG.ReflectionProbe.Prefilter".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.layouts.push(prefilter_layout);
        let copy_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries: vec![BindGroupLayoutEntry { binding: 0, kind: BindingType::Texture, stage: ShaderStage::Fragment }],
                label: Some("JARVIG.ReflectionProbe.Copy".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.layouts.push(copy_layout);
        let prefilter_shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(PREFILTER_SHADER.into()),
                label: Some("JARVIG.ReflectionProbe.Prefilter".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.shaders.push(prefilter_shader);
        let copy_shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(COPY_SHADER.into()),
                label: Some("JARVIG.ReflectionProbe.Copy".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.shaders.push(copy_shader);
        let prefilter_pipeline = self.downsample_pipeline(prefilter_shader, prefilter_layout, "ggx_fs", temp)?;
        let copy_pipeline = self.downsample_pipeline(copy_shader, copy_layout, "copy_fs", temp)?;
        let cube_view = self.device.create_cube_sample_view(cube).map_err(RenderError::Rhi)?;
        temp.views.push(cube_view);
        let prefilter_group = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout: prefilter_layout,
                entries: vec![
                    BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::TextureView(cube_view) },
                    BindGroupEntry { binding: 1, resource: jarvig_rhi::BindResource::Sampler(sampler) },
                    BindGroupEntry { binding: 2, resource: jarvig_rhi::BindResource::Buffer(params) },
                ],
                label: Some("JARVIG.ReflectionProbe.Prefilter".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.groups.push(prefilter_group);
        for mip in 1..mip_count {
            let dest = (resolution >> mip).max(1);
            let roughness = jarvig_core::reflection_probe_prefilter_roughness(mip, mip_count);
            let scratch = self
                .device
                .create_texture(&TextureDesc {
                    width: dest,
                    height: dest,
                    format: TextureFormat::Rgba16Float,
                    usage: TextureUsage::ColorTarget,
                    mip_count: 1,
                    label: Some("JARVIG.ReflectionProbe.Scratch".into()),
                })
                .map_err(RenderError::Rhi)?;
            let scratch_view = self.device.create_texture_view(scratch).map_err(RenderError::Rhi)?;
            temp.views.push(scratch_view);
            temp.textures.push(scratch);
            let samples = jarvig_core::reflection_probe_prefilter_sample_count(resolution, mip, mip_count);
            for face in 0..6u32 {
                self.write_prefilter_uniform(params, face, roughness, (mip - 1) as f32, dest, resolution, samples)?;
                let target = self.device.create_cube_face_view(cube, face, mip).map_err(RenderError::Rhi)?;
                temp.views.push(target);
                self.fullscreen(prefilter_pipeline, prefilter_group, scratch_view, dest)?;
                self.device.flush().map_err(RenderError::Rhi)?;
                let read_scratch = self.texture_group(copy_layout, scratch_view, temp)?;
                self.fullscreen(copy_pipeline, read_scratch, target, dest)?;
                self.device.flush().map_err(RenderError::Rhi)?;
            }
        }
        self.convolve_irradiance(prefilter_layout, prefilter_group, params, irradiance, resolution, temp)?;
        Ok(())
    }

    fn convolve_irradiance(
        &mut self,
        layout: BindGroupLayoutId,
        group: BindGroupId,
        params: BufferId,
        irradiance: TextureId,
        resolution: u32,
        temp: &mut CaptureTemp,
    ) -> Result<(), RenderError> {
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(IRRADIANCE_SHADER.into()),
                label: Some("JARVIG.IndirectDiffuse".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.shaders.push(shader);
        let pipeline = self.downsample_pipeline(shader, layout, "irradiance_fs", temp)?;
        let edge = resolution;
        for face in 0..6u32 {
            self.write_prefilter_uniform(params, face, 0.0, 0.0, edge, resolution, jarvig_core::INDIRECT_DIFFUSE_SAMPLES)?;
            let target = self.device.create_cube_face_view(irradiance, face, 0).map_err(RenderError::Rhi)?;
            temp.views.push(target);
            self.fullscreen(pipeline, group, target, edge)?;
            self.device.flush().map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn write_prefilter_uniform(
        &mut self,
        buffer: BufferId,
        face: u32,
        roughness: f32,
        max_mip: f32,
        dest_edge: u32,
        source_resolution: u32,
        sample_count: u32,
    ) -> Result<(), RenderError> {
        let mut bytes = [0u8; 32];
        bytes[0..4].copy_from_slice(&face.to_le_bytes());
        bytes[4..8].copy_from_slice(&sample_count.to_le_bytes());
        bytes[8..12].copy_from_slice(&roughness.to_le_bytes());
        bytes[12..16].copy_from_slice(&(source_resolution as f32).to_le_bytes());
        bytes[16..20].copy_from_slice(&max_mip.to_le_bytes());
        bytes[20..24].copy_from_slice(&(dest_edge as f32).to_le_bytes());
        self.device.write_buffer(buffer, 0, &bytes).map_err(RenderError::Rhi)
    }

    fn downsample_pipeline(
        &mut self,
        shader: ShaderModuleId,
        layout: BindGroupLayoutId,
        entry: &str,
        temp: &mut CaptureTemp,
    ) -> Result<PipelineId, RenderError> {
        let pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDesc {
                shader,
                vertex_entry: "vs".into(),
                fragment_entry: entry.into(),
                topology: PrimitiveTopology::TriangleList,
                color_format: TextureFormat::Rgba16Float,
                layouts: vec![layout],
                vertex_buffers: Vec::new(),
                depth: None,
                cull: CullMode::None,
                label: Some(format!("JARVIG.ReflectionProbe.{entry}")),
            })
            .map_err(RenderError::Rhi)?;
        temp.pipelines.push(pipeline);
        Ok(pipeline)
    }

    fn fullscreen(&mut self, pipeline: PipelineId, group: BindGroupId, target: TextureViewId, edge: u32) -> Result<(), RenderError> {
        let mut encoder = self.device.create_command_encoder(Some("JARVIG.ReflectionProbe.Mip")).map_err(RenderError::Rhi)?;
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some("JARVIG.ReflectionProbe.Mip".into()),
                colors: vec![ColorAttachment {
                    target,
                    load: LoadOp::Clear(ClearColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                    store: StoreOp::Store,
                }],
                depth: None,
            })
            .map_err(RenderError::Rhi)?;
        let size = edge as f32;
        encoder
            .set_viewport(Viewport { x: 0.0, y: 0.0, width: size, height: size, min_depth: 0.0, max_depth: 1.0 })
            .map_err(RenderError::Rhi)?;
        encoder.set_scissor(ScissorRect { x: 0, y: 0, width: edge, height: edge }).map_err(RenderError::Rhi)?;
        encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(0, group).map_err(RenderError::Rhi)?;
        encoder.draw(3, 1, 0, 0).map_err(RenderError::Rhi)?;
        encoder.end_render_pass().map_err(RenderError::Rhi)?;
        let buffer = encoder.finish().map_err(RenderError::Rhi)?;
        self.device.submit(self.device.graphics_queue(), &[buffer], None).map_err(RenderError::Rhi)
    }

    fn texture_group(&mut self, layout: BindGroupLayoutId, view: TextureViewId, temp: &mut CaptureTemp) -> Result<BindGroupId, RenderError> {
        let group = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout,
                entries: vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::TextureView(view) }],
                label: Some("JARVIG.ReflectionProbe.MipSource".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.groups.push(group);
        Ok(group)
    }

    fn uniform_buffer(&mut self, size: u64, label: &str, temp: &mut CaptureTemp) -> Result<BufferId, RenderError> {
        let buffer = self
            .device
            .create_buffer(&BufferDesc { size, usage: BufferUsage::Uniform, label: Some(label.into()), contents: None })
            .map_err(RenderError::Rhi)?;
        temp.buffers.push(buffer);
        Ok(buffer)
    }

    fn storage_buffer(&mut self, size: u64, temp: &mut CaptureTemp) -> Result<BufferId, RenderError> {
        let buffer = self
            .device
            .create_buffer(&BufferDesc {
                size,
                usage: BufferUsage::Storage,
                label: Some("JARVIG.ReflectionProbe.Lights".into()),
                contents: Some(vec![0u8; size as usize]),
            })
            .map_err(RenderError::Rhi)?;
        temp.buffers.push(buffer);
        Ok(buffer)
    }

    pub(super) fn cancel_probe_job(&mut self) -> Result<(), RenderError> {
        let Some(job) = self.probe_job.take() else {
            return Ok(());
        };
        let ProbeCaptureJob { destination, temp, .. } = job;
        temp.destroy(self)?;
        self.destroy_probe_gpu(destination)
    }

    fn drop_job_unless_current(&mut self, probes: &[jarvig_core::RenderReflectionProbe]) -> Result<(), RenderError> {
        let stale = match self.probe_job.as_ref() {
            None => false,
            Some(job) => match probes.iter().find(|probe| probe.id == job.id) {
                Some(probe) => probe.resolution != job.resolution,
                None => true,
            },
        };
        if stale {
            self.cancel_probe_job()?;
        }
        Ok(())
    }

    fn note_job_scene(&mut self, snapshot: &RenderSceneSnapshot) {
        let Some(job) = self.probe_job.as_mut() else {
            return;
        };
        if job.target_world == snapshot.world_revision
            && job.target_lighting == snapshot.lighting_revision
            && job.target_serial == snapshot.probe_recapture_serial
        {
            return;
        }
        job.step = 0;
        job.face_ms = 0;
        job.prefilter_ms = 0;
        job.target_world = snapshot.world_revision;
        job.target_lighting = snapshot.lighting_revision;
        job.target_serial = snapshot.probe_recapture_serial;
    }

    fn reflection_queue_len(&self, snapshot: &RenderSceneSnapshot, probes: &[jarvig_core::RenderReflectionProbe]) -> u32 {
        probes
            .iter()
            .filter(|probe| {
                if self.probe_job.as_ref().is_some_and(|job| job.id == probe.id) {
                    return true;
                }
                match self.probe_residents.iter().find(|resident| resident.id == probe.id) {
                    None => true,
                    Some(resident) => probe_needs_refresh(resident, snapshot, probe),
                }
            })
            .count() as u32
    }

    fn begin_probe_job(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        materials: &MaterialLibrary,
        textures: &TextureLibrary,
        probe: &jarvig_core::RenderReflectionProbe,
        environment: BufferId,
    ) -> Result<(), RenderError> {
        let gpu = self.create_probe_gpu("JARVIG.ReflectionProbe.Pending", probe.resolution)?;
        let sampler = gpu.sampler;
        let irradiance_view = gpu.irradiance_sample;
        let mip_count = gpu.mip_count;
        let mut temp = CaptureTemp::new();
        let opened = self.open_capture(snapshot, meshes, materials, textures, probe, environment, sampler, irradiance_view, &mut temp);
        let open = match opened {
            Ok(open) => open,
            Err(error) => {
                temp.destroy(self)?;
                self.destroy_probe_gpu(gpu)?;
                return Err(error);
            }
        };
        let Some(open) = open else {
            temp.destroy(self)?;
            self.install_finished_cube(probe.id, gpu, snapshot, true, 0, 0)?;
            return Ok(());
        };
        self.probe_job = Some(ProbeCaptureJob {
            id: probe.id,
            destination: gpu,
            temp,
            objects: open.objects,
            depth_view: open.depth_view,
            header: open.header,
            storage: open.storage,
            probe_buffer: open.probe_buffer,
            lights: open.lights,
            resolution: probe.resolution,
            mip_count,
            filter: None,
            step: 0,
            steps: capture_step_count(mip_count),
            face_ms: 0,
            prefilter_ms: 0,
            target_world: snapshot.world_revision,
            target_lighting: snapshot.lighting_revision,
            target_serial: snapshot.probe_recapture_serial,
        });
        Ok(())
    }

    fn advance_probe_job(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        _meshes: &MeshLibrary,
        _materials: &MaterialLibrary,
        _textures: &TextureLibrary,
    ) -> Result<(), RenderError> {
        let Some(job) = self.probe_job.as_ref() else {
            return Ok(());
        };
        let step = job.step;
        let mip_count = job.mip_count;
        let Some(work) = capture_work(step, mip_count) else {
            return self.finish_probe_job(snapshot);
        };
        let started = std::time::Instant::now();
        match work {
            CaptureWork::Scene(face) => self.job_scene_face(snapshot, face)?,
            CaptureWork::Prefilter { mip, face } => self.job_prefilter_face(mip, face)?,
            CaptureWork::Irradiance(face) => self.job_irradiance_face(face)?,
        }
        if self.probe_job.is_none() {
            return Ok(());
        }
        let elapsed = elapsed_ms(started);
        let job = self.probe_job.as_mut().expect("probe job");
        match work {
            CaptureWork::Scene(_) => job.face_ms = job.face_ms.saturating_add(elapsed),
            _ => job.prefilter_ms = job.prefilter_ms.saturating_add(elapsed),
        }
        job.step = job.step.saturating_add(1);
        if job.step >= job.steps {
            self.finish_probe_job(snapshot)?;
        }
        Ok(())
    }

    fn job_scene_face(&mut self, snapshot: &RenderSceneSnapshot, face_index: u32) -> Result<(), RenderError> {
        let (forward, up) = reflection_cube_faces()[face_index as usize];
        let probe_id = self.probe_job.as_ref().expect("probe job").id;
        let Some(probe) = snapshot.reflection_probes().iter().find(|probe| probe.id == probe_id).copied() else {
            return self.cancel_probe_job();
        };
        let rotation = rotation_looking_toward(forward, up).map_err(RenderError::Space)?;
        let camera = ResolvedPose { translation: probe.translation, rotation };
        let (objects, header, storage, lights, depth, cube, resolution) = {
            let job = self.probe_job.as_ref().expect("probe job");
            (job.objects.clone(), job.header, job.storage, job.lights, job.depth_view, job.destination.texture, job.resolution)
        };
        for object in &objects {
            let Some(instance) = snapshot.instances().iter().find(|instance| instance.id == object.instance) else {
                return self.cancel_probe_job();
            };
            let packet = instance_gpu_transforms(instance, &camera, std::f64::consts::FRAC_PI_2, CAPTURE_NEAR_M, 1.0).map_err(RenderError::Space)?;
            self.device.write_buffer(object.uniform, 0, &packet.to_bytes()).map_err(RenderError::Rhi)?;
        }
        self.write_capture_lights(snapshot, &camera, header, storage)?;
        let mut temp = std::mem::take(&mut self.probe_job.as_mut().expect("probe job").temp);
        let face = match self.device.create_cube_face_view(cube, face_index, 0) {
            Ok(face) => face,
            Err(error) => {
                self.probe_job.as_mut().expect("probe job").temp = temp;
                return Err(RenderError::Rhi(error));
            }
        };
        temp.views.push(face);
        let drawn = self.draw_capture_face(face, depth, lights, &objects, face_clear(snapshot, forward.y), resolution);
        self.probe_job.as_mut().expect("probe job").temp = temp;
        drawn
    }

    fn job_prefilter_face(&mut self, mip: u32, face: u32) -> Result<(), RenderError> {
        self.ensure_job_filter()?;
        let (pipeline, copy_pipeline, group, params, cube, resolution, mip_count, copy_layout) = {
            let job = self.probe_job.as_ref().expect("probe job");
            let filter = job.filter.as_ref().expect("prefilter");
            (
                filter.prefilter_pipeline,
                filter.copy_pipeline,
                filter.prefilter_group,
                job.probe_buffer,
                job.destination.texture,
                job.resolution,
                job.mip_count,
                filter.copy_layout,
            )
        };
        let dest = (resolution >> mip).max(1);
        self.ensure_job_scratch(dest)?;
        let scratch_view = self.probe_job.as_ref().expect("probe job").filter.as_ref().expect("prefilter").scratch_view;
        let roughness = jarvig_core::reflection_probe_prefilter_roughness(mip, mip_count);
        let samples = jarvig_core::reflection_probe_prefilter_sample_count(resolution, mip, mip_count);
        self.write_prefilter_uniform(params, face, roughness, (mip - 1) as f32, dest, resolution, samples)?;
        let mut temp = std::mem::take(&mut self.probe_job.as_mut().expect("probe job").temp);
        let target = match self.device.create_cube_face_view(cube, face, mip) {
            Ok(target) => target,
            Err(error) => {
                self.probe_job.as_mut().expect("probe job").temp = temp;
                return Err(RenderError::Rhi(error));
            }
        };
        temp.views.push(target);
        let filtered = self.fullscreen(pipeline, group, scratch_view, dest);
        let copied = filtered.and_then(|_| {
            let read_scratch = self.texture_group(copy_layout, scratch_view, &mut temp)?;
            self.fullscreen(copy_pipeline, read_scratch, target, dest)
        });
        self.probe_job.as_mut().expect("probe job").temp = temp;
        copied
    }

    fn job_irradiance_face(&mut self, face: u32) -> Result<(), RenderError> {
        self.ensure_job_irradiance_pipeline()?;
        let (pipeline, group, params, irradiance, resolution) = {
            let job = self.probe_job.as_ref().expect("probe job");
            let filter = job.filter.as_ref().expect("prefilter");
            (
                filter.irradiance_pipeline.expect("irradiance pipeline"),
                filter.prefilter_group,
                job.probe_buffer,
                job.destination.irradiance,
                job.resolution,
            )
        };
        self.write_prefilter_uniform(params, face, 0.0, 0.0, resolution, resolution, jarvig_core::INDIRECT_DIFFUSE_SAMPLES)?;
        let mut temp = std::mem::take(&mut self.probe_job.as_mut().expect("probe job").temp);
        let target = match self.device.create_cube_face_view(irradiance, face, 0) {
            Ok(target) => target,
            Err(error) => {
                self.probe_job.as_mut().expect("probe job").temp = temp;
                return Err(RenderError::Rhi(error));
            }
        };
        temp.views.push(target);
        let drawn = self.fullscreen(pipeline, group, target, resolution);
        self.probe_job.as_mut().expect("probe job").temp = temp;
        drawn
    }

    fn ensure_job_filter(&mut self) -> Result<(), RenderError> {
        if self.probe_job.as_ref().is_some_and(|job| job.filter.is_some()) {
            return Ok(());
        }
        let (cube, sampler, params) = {
            let job = self.probe_job.as_ref().expect("probe job");
            (job.destination.texture, job.destination.sampler, job.probe_buffer)
        };
        let mut temp = std::mem::take(&mut self.probe_job.as_mut().expect("probe job").temp);
        let built = self.build_prefilter_state(cube, sampler, params, &mut temp);
        self.probe_job.as_mut().expect("probe job").temp = temp;
        let filter = built?;
        self.probe_job.as_mut().expect("probe job").filter = Some(filter);
        Ok(())
    }

    fn build_prefilter_state(
        &mut self,
        cube: TextureId,
        sampler: jarvig_rhi::SamplerId,
        params: BufferId,
        temp: &mut CaptureTemp,
    ) -> Result<ProbeFilterState, RenderError> {
        let prefilter_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries: vec![
                    BindGroupLayoutEntry { binding: 0, kind: BindingType::TextureCube, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 1, kind: BindingType::Sampler, stage: ShaderStage::Fragment },
                    BindGroupLayoutEntry { binding: 2, kind: BindingType::UniformBuffer, stage: ShaderStage::Fragment },
                ],
                label: Some("JARVIG.ReflectionProbe.Prefilter".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.layouts.push(prefilter_layout);
        let copy_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries: vec![BindGroupLayoutEntry { binding: 0, kind: BindingType::Texture, stage: ShaderStage::Fragment }],
                label: Some("JARVIG.ReflectionProbe.Copy".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.layouts.push(copy_layout);
        let prefilter_shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(PREFILTER_SHADER.into()),
                label: Some("JARVIG.ReflectionProbe.Prefilter".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.shaders.push(prefilter_shader);
        let copy_shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(COPY_SHADER.into()),
                label: Some("JARVIG.ReflectionProbe.Copy".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.shaders.push(copy_shader);
        let prefilter_pipeline = self.downsample_pipeline(prefilter_shader, prefilter_layout, "ggx_fs", temp)?;
        let copy_pipeline = self.downsample_pipeline(copy_shader, copy_layout, "copy_fs", temp)?;
        let cube_view = self.device.create_cube_sample_view(cube).map_err(RenderError::Rhi)?;
        temp.views.push(cube_view);
        let prefilter_group = self
            .device
            .create_bind_group(&BindGroupDesc {
                layout: prefilter_layout,
                entries: vec![
                    BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::TextureView(cube_view) },
                    BindGroupEntry { binding: 1, resource: jarvig_rhi::BindResource::Sampler(sampler) },
                    BindGroupEntry { binding: 2, resource: jarvig_rhi::BindResource::Buffer(params) },
                ],
                label: Some("JARVIG.ReflectionProbe.Prefilter".into()),
            })
            .map_err(RenderError::Rhi)?;
        temp.groups.push(prefilter_group);
        Ok(ProbeFilterState {
            prefilter_pipeline,
            copy_pipeline,
            irradiance_pipeline: None,
            prefilter_layout,
            prefilter_group,
            copy_layout,
            scratch_view: TextureViewId::INVALID,
            scratch_edge: 0,
        })
    }

    fn ensure_job_scratch(&mut self, edge: u32) -> Result<(), RenderError> {
        if self.probe_job.as_ref().is_some_and(|job| job.filter.as_ref().is_some_and(|filter| filter.scratch_edge == edge)) {
            return Ok(());
        }
        let mut temp = std::mem::take(&mut self.probe_job.as_mut().expect("probe job").temp);
        let scratch = match self.device.create_texture(&TextureDesc {
            width: edge,
            height: edge,
            format: TextureFormat::Rgba16Float,
            usage: TextureUsage::ColorTarget,
            mip_count: 1,
            label: Some("JARVIG.ReflectionProbe.Scratch".into()),
        }) {
            Ok(scratch) => scratch,
            Err(error) => {
                self.probe_job.as_mut().expect("probe job").temp = temp;
                return Err(RenderError::Rhi(error));
            }
        };
        temp.textures.push(scratch);
        let scratch_view = match self.device.create_texture_view(scratch) {
            Ok(view) => view,
            Err(error) => {
                self.probe_job.as_mut().expect("probe job").temp = temp;
                return Err(RenderError::Rhi(error));
            }
        };
        temp.views.push(scratch_view);
        self.probe_job.as_mut().expect("probe job").temp = temp;
        let filter = self.probe_job.as_mut().expect("probe job").filter.as_mut().expect("prefilter");
        filter.scratch_view = scratch_view;
        filter.scratch_edge = edge;
        Ok(())
    }

    fn ensure_job_irradiance_pipeline(&mut self) -> Result<(), RenderError> {
        self.ensure_job_filter()?;
        if self.probe_job.as_ref().is_some_and(|job| job.filter.as_ref().is_some_and(|filter| filter.irradiance_pipeline.is_some())) {
            return Ok(());
        }
        let layout = self.probe_job.as_ref().expect("probe job").filter.as_ref().expect("prefilter").prefilter_layout;
        let mut temp = std::mem::take(&mut self.probe_job.as_mut().expect("probe job").temp);
        let shader = match self.device.create_shader_module(&ShaderModuleDesc {
            source: ShaderSource::Wgsl(IRRADIANCE_SHADER.into()),
            label: Some("JARVIG.IndirectDiffuse".into()),
        }) {
            Ok(shader) => shader,
            Err(error) => {
                self.probe_job.as_mut().expect("probe job").temp = temp;
                return Err(RenderError::Rhi(error));
            }
        };
        temp.shaders.push(shader);
        let pipeline = self.downsample_pipeline(shader, layout, "irradiance_fs", &mut temp);
        self.probe_job.as_mut().expect("probe job").temp = temp;
        let pipeline = pipeline?;
        self.probe_job.as_mut().expect("probe job").filter.as_mut().expect("prefilter").irradiance_pipeline = Some(pipeline);
        Ok(())
    }

    fn finish_probe_job(&mut self, snapshot: &RenderSceneSnapshot) -> Result<(), RenderError> {
        let Some(job) = self.probe_job.take() else {
            return Ok(());
        };
        let ProbeCaptureJob { id, destination, temp, face_ms, prefilter_ms, .. } = job;
        if let Err(error) = temp.destroy(self) {
            let _ = self.destroy_probe_gpu(destination);
            return Err(error);
        }
        self.install_finished_cube(id, destination, snapshot, true, face_ms, prefilter_ms)
    }

    fn install_finished_cube(
        &mut self,
        id: jarvig_core::ProbeId,
        gpu: super::ProbeGpu,
        snapshot: &RenderSceneSnapshot,
        count_recapture_when_replacing: bool,
        face_ms: u32,
        prefilter_ms: u32,
    ) -> Result<(), RenderError> {
        let replacing = self.probe_residents.iter().any(|resident| resident.id == id);
        if let Some(index) = self.probe_residents.iter().position(|resident| resident.id == id) {
            let old = self.probe_residents.remove(index);
            self.detach_probe_view(old.gpu.sample)?;
            self.detach_probe_view(old.gpu.irradiance_sample)?;
            self.destroy_probe_gpu(old.gpu)?;
        }
        self.probe_residents.push(super::ProbeResident {
            id,
            gpu,
            captured_world: snapshot.world_revision,
            captured_lighting: snapshot.lighting_revision,
            captured_serial: snapshot.probe_recapture_serial,
        });
        if replacing && count_recapture_when_replacing {
            self.probe_recapture_count = self.probe_recapture_count.saturating_add(1);
        }
        self.probe_capture_count = self.probe_capture_count.saturating_add(1);
        self.irradiance_build_count = self.irradiance_build_count.saturating_add(1);
        self.last_capture_ms = face_ms;
        self.last_prefilter_ms = prefilter_ms;
        Ok(())
    }
}

fn cube_memory_bytes(resolution: u32, mip_count: u32) -> u64 {
    let mut bytes = 0u64;
    for mip in 0..mip_count {
        let edge = u64::from((resolution >> mip).max(1));
        bytes = bytes.saturating_add(edge * edge * 6 * 8);
    }
    bytes
}

fn probe_needs_refresh(resident: &super::ProbeResident, snapshot: &RenderSceneSnapshot, probe: &jarvig_core::RenderReflectionProbe) -> bool {
    if resident.gpu.resolution != probe.resolution || resident.captured_serial != snapshot.probe_recapture_serial {
        return true;
    }
    match snapshot.probe_policy {
        jarvig_core::ProbeUpdatePolicy::Static | jarvig_core::ProbeUpdatePolicy::OnDemand => false,
        jarvig_core::ProbeUpdatePolicy::OnTransformChange => resident.captured_world != snapshot.world_revision,
        jarvig_core::ProbeUpdatePolicy::OnLightingChange => resident.captured_lighting != snapshot.lighting_revision,
        jarvig_core::ProbeUpdatePolicy::TimeSliced => {
            resident.captured_world != snapshot.world_revision || resident.captured_lighting != snapshot.lighting_revision
        }
    }
}

/// The first cube still finishes in the frame that creates it, except Time Sliced.
/// That keeps the bootstrap self-test at one capture after one present.
/// Every later dirty probe, including a Static recapture or a resolution change, gets one face.
/// Static does not become dirty from the camera, a transform, or a light edit.
fn capture_step_budget(policy: jarvig_core::ProbeUpdatePolicy, first_capture: bool) -> u32 {
    if first_capture && !matches!(policy, jarvig_core::ProbeUpdatePolicy::TimeSliced) {
        u32::MAX
    } else {
        let _ = policy;
        REFLECTION_CAPTURE_STEPS_PER_FRAME
    }
}

fn probes_outside(snapshot: &RenderSceneSnapshot) -> u32 {
    snapshot
        .instances()
        .iter()
        .filter(|instance| reflection_probe_influence(snapshot.reflection_probes(), instance.pose.translation) == 0.0)
        .count() as u32
}

fn face_clear(snapshot: &RenderSceneSnapshot, forward_y: f64) -> ClearColor {
    let environment = snapshot.environment();
    if !environment.enabled || environment.intensity <= 0.0 {
        return ClearColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    }
    let sky = (forward_y as f32).clamp(-1.0, 1.0) * 0.5 + 0.5;
    let mix = |lower: f32, upper: f32| (lower + (upper - lower) * sky) * environment.intensity;
    ClearColor {
        r: mix(environment.lower_hemisphere_linear_rgb[0], environment.upper_hemisphere_linear_rgb[0]),
        g: mix(environment.lower_hemisphere_linear_rgb[1], environment.upper_hemisphere_linear_rgb[1]),
        b: mix(environment.lower_hemisphere_linear_rgb[2], environment.upper_hemisphere_linear_rgb[2]),
        a: 1.0,
    }
}
