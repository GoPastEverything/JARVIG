//! Conventional light-space shadow maps.
//!
//! One directional map, one spot map, and one point cubemap. Extra lights of the
//! same class stay unshadowed until a later atlas or virtual shadow map. The
//! material only calls `jarvig_direct_visibility`. Replacing these maps does not
//! require a new material graph.
//!
//! Depth is stored as reversed-Z in an `Rgba16Float` color target because the RHI
//! cannot sample `Depth32Float`. Near is 1. A cleared texel is 0, which is farther
//! than every receiver, so an empty map leaves the light unoccluded.
//!
//! `.r` alone is an fp16 step of about 0.001 near 1, which is centimeters on a wide
//! cascade and bands a flat floor. The color target keeps the same format. `.r` holds
//! `floor(z * 1024) / 1024` and `.g` holds the remainder times 1024. The material
//! reads `r + g / 1024`. Clear `(0, 0)` still unpacks to 0.
//!
//! The maps are anchored on the light, or on the caster centroid for a directional
//! light. A camera move rewrites the sampling matrix in the view packet. It does
//! not redraw the maps. `world_revision` does: object or light motion, rotation,
//! and geometry edits already bump that revision.
//!
//! The shadow pipeline culls nothing. The bootstrap cards are two-sided, and the
//! geometric triangle is the occluder from either side. The shading-normal flip is
//! not a second caster. Equal depth plus the constant bias stays lit, so a card
//! does not shadow itself.
//!
//! Probe capture binds these textures with every record disabled. The accepted
//! reflection image is not suddenly shadowed.

use jarvig_core::{
    camera_relative_f32, clip_matrix_for_camera, fit_directional_cascades, point_shadow_face_view_proj, spot_shadow_view_proj, CascadeSlice,
    DirectionalCascadeSet, GpuShadowRecord, LightKind, Mat4, MeshId, MeshLibrary, RenderInstance, RenderLight, RenderSceneSnapshot,
    ResolvedPose, ShadowMapClass, Vec3, CASCADE_RESOLUTION, DEPTH_CLEAR, DIRECTIONAL_ATLAS_RESOLUTION, POINT_SHADOW_RESOLUTION,
    PUNCTUAL_LIGHT_RADIUS_M, SHADOW_DEPTH_BIAS_M, SHADOW_FAR_M, SHADOW_MAP_RESOLUTION, SHADOW_NEAR_M, SHADOW_PASS_BUDGET, SUN_ANGULAR_TAN,
};
use jarvig_rhi::{
    AddressMode, BindGroupDesc, BindGroupEntry, BindGroupId, BindGroupLayoutDesc, BindGroupLayoutEntry, BindGroupLayoutId, BindingType,
    BufferDesc, BufferId, BufferUsage, ClearColor, ColorAttachment, CompareFunction, CullMode, DepthAttachment, DepthLoadOp, DepthState,
    FilterMode, LoadOp, PipelineId, PrimitiveTopology, RenderPassDesc, RenderPipelineDesc, ResourceKind, SamplerDesc, ScissorRect,
    ShaderModuleDesc, ShaderModuleId, ShaderSource, ShaderStage, StoreOp, TextureDesc, TextureFormat, TextureId, TextureUsage, TextureViewId,
    VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode, Viewport,
};

use super::{RenderError, RenderViewId, Renderer, LIGHT_CAPACITY};

const SHADOW_SHADER: &str = r#"
struct ShadowCamera {
    clip_from_local: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> shadow_camera: ShadowCamera;

struct ShadowIn {
    @location(0) position: vec3<f32>,
}

@vertex
fn vs(input: ShadowIn) -> @builtin(position) vec4<f32> {
    return shadow_camera.clip_from_local * vec4<f32>(input.position, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let z = clamp(pos.z, 0.0, 1.0);
    let hi = floor(z * 1024.0) / 1024.0;
    let lo = (z - hi) * 1024.0;
    return vec4<f32>(hi, lo, 0.0, 1.0);
}
"#;

/// What the status line and the self-test report. Not a GPU handle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowDiagnostics {
    pub map_count: u32,
    pub update_count: u32,
    pub directional_resolution: u32,
    pub spot_resolution: u32,
    pub point_resolution: u32,
    pub directional_cascades: u32,
    pub maps_updated: u32,
    pub shadow_draws: u32,
    pub shadow_casters: u32,
    pub contact_shadow_enabled: u32,
    /// CPU time spent recording shadow passes. GPU timestamps are not enabled on this RHI.
    pub shadow_pass_ms: f32,
    /// 1 is PCSS for directional and spot. 0 is fixed PCF.
    pub filter_pcss: u32,
}

pub(super) struct ShadowSampleBindings {
    pub directional: TextureViewId,
    pub spot: TextureViewId,
    pub point: TextureViewId,
    pub sampler: jarvig_rhi::SamplerId,
    pub disabled: BufferId,
}

struct ShadowMap2D {
    color: TextureId,
    color_view: TextureViewId,
    depth: TextureId,
    depth_view: TextureViewId,
}

struct ShadowPipeline {
    stride: u64,
    pipeline: PipelineId,
}

struct ShadowCaster {
    uniform: BufferId,
    group: BindGroupId,
}

struct DirectionalAtlas {
    view: super::RenderViewId,
    map: ShadowMap2D,
    fingerprint: u64,
    set: DirectionalCascadeSet,
}

pub(super) struct ShadowGpu {
    atlases: Vec<DirectionalAtlas>,
    spot: ShadowMap2D,
    point_texture: TextureId,
    point_sample: TextureViewId,
    point_faces: [TextureViewId; 6],
    point_depth: TextureId,
    point_depth_view: TextureViewId,
    sampler: jarvig_rhi::SamplerId,
    disabled: BufferId,
    shader: ShaderModuleId,
    layout: BindGroupLayoutId,
    pipelines: Vec<ShadowPipeline>,
    casters: Vec<ShadowCaster>,
    spot_vp: Mat4,
    spot_anchor: Vec3,
    spot_far: f32,
    point_anchor: Vec3,
    point_far: f32,
    cull: CullMode,
    passes_budget: u32,
}

impl Renderer {
    pub fn shadow_diagnostics(&self) -> ShadowDiagnostics {
        let ready = self.shadow.is_some();
        let cascades = self
            .shadow
            .as_ref()
            .and_then(|shadow| shadow.atlases.first())
            .map(|atlas| atlas.set.count)
            .unwrap_or(if ready { jarvig_core::DEFAULT_CASCADE_COUNT } else { 0 });
        ShadowDiagnostics {
            map_count: if ready { 3 } else { 0 },
            update_count: self.shadow_update_count,
            directional_resolution: if ready { CASCADE_RESOLUTION } else { 0 },
            spot_resolution: if ready { SHADOW_MAP_RESOLUTION } else { 0 },
            point_resolution: if ready { POINT_SHADOW_RESOLUTION } else { 0 },
            directional_cascades: cascades,
            maps_updated: self.shadow_maps_updated,
            shadow_draws: self.shadow_draws_frame,
            shadow_casters: self.shadow_casters,
            contact_shadow_enabled: u32::from(self.contact_requested && self.contact.is_some()),
            shadow_pass_ms: self.shadow_pass_ms,
            filter_pcss: 1,
        }
    }

    pub fn shadow_update_count(&self) -> u32 {
        self.shadow_update_count
    }

    pub fn shadow_map_count(&self) -> u32 {
        self.shadow_diagnostics().map_count
    }

    /// Persistent shadow textures. Each view's directional atlas is a color target plus a depth target.
    pub fn shadow_texture_count(&self) -> u32 {
        let Some(shadow) = &self.shadow else { return 0 };
        (shadow.atlases.len() as u32) * 2 + 4
    }

    pub fn shadow_cull(&self) -> Option<CullMode> {
        self.shadow.as_ref().map(|shadow| shadow.cull)
    }

    pub fn shadow_record_bytes(&self, view: RenderViewId) -> Option<&[u8]> {
        self.light_packets.iter().find(|packet| packet.view == view).map(|packet| packet.last_shadow.as_slice())
    }

    pub(super) fn shadow_sample_bindings(&self, view: Option<super::RenderViewId>) -> Result<ShadowSampleBindings, RenderError> {
        let shadow = self.shadow.as_ref().ok_or(RenderError::Rhi(jarvig_rhi::RhiError::InvalidResource("shadow maps")))?;
        let directional = view
            .and_then(|view| shadow.atlases.iter().find(|atlas| atlas.view == view))
            .or_else(|| shadow.atlases.first())
            .map(|atlas| atlas.map.color_view)
            .unwrap_or(shadow.spot.color_view);
        Ok(ShadowSampleBindings {
            directional,
            spot: shadow.spot.color_view,
            point: shadow.point_sample,
            sampler: shadow.sampler,
            disabled: shadow.disabled,
        })
    }

    pub(super) fn contact_view(&self) -> Option<TextureViewId> {
        self.contact.as_ref().map(|contact| contact.view)
    }

    /// Records in the same order as the filtered direct-light list. Index 0 is the first included light.
    pub(super) fn pack_shadow_records(
        &self,
        view: super::RenderViewId,
        lights: &[RenderLight],
        camera: &ResolvedPose,
        shadows_enabled: bool,
    ) -> Vec<u8> {
        let Some(shadow) = &self.shadow else {
            return Vec::new();
        };
        let cascades = shadow.atlases.iter().find(|atlas| atlas.view == view).map(|atlas| atlas.set);
        let mut claimed = [false; 3];
        let mut bytes = Vec::with_capacity(lights.len() * GpuShadowRecord::BYTES);
        for light in lights {
            let class = match light.kind {
                LightKind::Directional => 0,
                LightKind::Spot => 1,
                LightKind::Point => 2,
            };
            let first = !claimed[class];
            claimed[class] = true;
            let settings = light.shadow;
            let mut record = GpuShadowRecord::off();
            record.bias = settings.depth_bias_m;
            record.pad = [settings.slope_bias_m, settings.filter_radius.max(1.0), settings.normal_bias_m];
            record.near_m = SHADOW_NEAR_M;
            record.atlas_scale = [1.0, 1.0];
            record.extra = [1.0, jarvig_core::CASCADE_BLEND, 1.0, if light.kind == LightKind::Directional { SUN_ANGULAR_TAN } else { PUNCTUAL_LIGHT_RADIUS_M }];
            match light.kind {
                LightKind::Directional => {
                    record.kind = ShadowMapClass::Directional.gpu_tag();
                    record.far_m = settings.effective_distance_m();
                    if let Some(set) = cascades {
                        record.extra[0] = set.count as f32;
                        for (index, slice) in set.slices.iter().take(set.count as usize).enumerate() {
                            let clip = clip_matrix_for_camera(slice.clip, camera.translation, slice.anchor);
                            match index {
                                0 => record.clip_from_camera_relative = clip,
                                1 => record.clip1 = clip,
                                2 => record.clip2 = clip,
                                _ => record.clip3 = clip,
                            }
                            record.splits[index] = slice.split_end_m;
                            record.half_extents[index] = slice.half_extent_m;
                        }
                    }
                }
                LightKind::Spot => {
                    record.kind = ShadowMapClass::Spot.gpu_tag();
                    record.far_m = shadow.spot_far;
                    record.clip_from_camera_relative = clip_matrix_for_camera(shadow.spot_vp, camera.translation, shadow.spot_anchor);
                }
                LightKind::Point => {
                    record.kind = ShadowMapClass::Point.gpu_tag();
                    record.far_m = shadow.point_far;
                    record.extra[2] = 0.0;
                }
            }
            record.enabled = if shadows_enabled && first && settings.cast && light_has_map(light.kind, cascades.is_some()) { 1.0 } else { 0.0 };
            bytes.extend_from_slice(&record.to_bytes());
        }
        bytes
    }

    pub(super) fn ensure_shadow_targets(&mut self) -> Result<(), RenderError> {
        if self.shadow.is_some() {
            return Ok(());
        }
        let spot = self.create_shadow_map_2d(SHADOW_MAP_RESOLUTION, "JARVIG.Shadow.Spot")?;
        let point_texture = self
            .device
            .create_cube_texture(&jarvig_rhi::CubeTextureDesc {
                size: POINT_SHADOW_RESOLUTION,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsage::ColorTarget,
                mip_count: 1,
                label: Some("JARVIG.Shadow.Point".into()),
            })
            .map_err(RenderError::Rhi)?;
        let point_sample = self.device.create_cube_sample_view(point_texture).map_err(RenderError::Rhi)?;
        let mut point_faces = [TextureViewId::INVALID; 6];
        for face in 0..6 {
            point_faces[face] = self.device.create_cube_face_view(point_texture, face as u32, 0).map_err(RenderError::Rhi)?;
        }
        let (point_depth, point_depth_view) = self.create_shadow_depth(POINT_SHADOW_RESOLUTION, "JARVIG.Shadow.Point.Depth")?;
        let sampler = self
            .device
            .create_sampler(&SamplerDesc {
                min_filter: FilterMode::Nearest,
                mag_filter: FilterMode::Nearest,
                mip_filter: FilterMode::Nearest,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
                address_w: AddressMode::ClampToEdge,
                anisotropy: 1,
                label: Some("JARVIG.Shadow.Sampler".into()),
            })
            .map_err(RenderError::Rhi)?;
        let disabled = self
            .device
            .create_buffer(&BufferDesc {
                size: (LIGHT_CAPACITY * GpuShadowRecord::BYTES) as u64,
                usage: BufferUsage::Storage,
                label: Some("JARVIG.Shadow.Disabled".into()),
                contents: Some(vec![0u8; LIGHT_CAPACITY * GpuShadowRecord::BYTES]),
            })
            .map_err(RenderError::Rhi)?;
        let shader = self
            .device
            .create_shader_module(&ShaderModuleDesc {
                source: ShaderSource::Wgsl(SHADOW_SHADER.into()),
                label: Some("JARVIG.Shadow".into()),
            })
            .map_err(RenderError::Rhi)?;
        let layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDesc {
                entries: vec![BindGroupLayoutEntry {
                    binding: 0,
                    kind: BindingType::UniformBuffer,
                    stage: ShaderStage::Vertex,
                }],
                label: Some("JARVIG.Shadow.Layout".into()),
            })
            .map_err(RenderError::Rhi)?;
        self.shadow = Some(ShadowGpu {
            atlases: Vec::new(),
            spot,
            point_texture,
            point_sample,
            point_faces,
            point_depth,
            point_depth_view,
            sampler,
            disabled,
            shader,
            layout,
            pipelines: Vec::new(),
            casters: Vec::new(),
            spot_vp: Mat4::IDENTITY,
            spot_anchor: Vec3::ZERO,
            spot_far: SHADOW_FAR_M,
            point_anchor: Vec3::ZERO,
            point_far: SHADOW_FAR_M,
            cull: CullMode::None,
            passes_budget: SHADOW_PASS_BUDGET,
        });
        Ok(())
    }

    /// Point and spot maps. A camera move does not call this again. Directional cascades are separate.
    pub(super) fn update_shadow_maps(&mut self, snapshot: &RenderSceneSnapshot, meshes: &MeshLibrary) -> Result<(), RenderError> {
        self.ensure_shadow_targets()?;
        self.shadow_maps_updated = 0;
        self.shadow_draws_frame = 0;
        self.shadow_pass_ms = 0.0;
        if self.shadow_update_count > 0 && self.shadow_world_revision == snapshot.world_revision {
            return Ok(());
        }
        let started = std::time::Instant::now();
        let spot = snapshot.lights().iter().find(|light| light.kind == LightKind::Spot && light.shadow.cast).copied();
        let point = snapshot.lights().iter().find(|light| light.kind == LightKind::Point && light.shadow.cast).copied();
        let (spot_vp, spot_far, spot_anchor) = match spot {
            Some(light) => {
                let far = punctual_far(&light);
                let outer = light.cos_outer.clamp(-1.0, 1.0).acos();
                (spot_shadow_view_proj(light.pose.rotation, outer, far).map_err(RenderError::Space)?, far, light.pose.translation)
            }
            None => (Mat4::IDENTITY, SHADOW_FAR_M, Vec3::ZERO),
        };
        let (point_far, point_anchor) = match point {
            Some(light) => (punctual_far(&light), light.pose.translation),
            None => (SHADOW_FAR_M, Vec3::ZERO),
        };
        {
            let shadow = self.shadow.as_mut().expect("shadow targets");
            shadow.spot_vp = spot_vp;
            shadow.spot_anchor = spot_anchor;
            shadow.spot_far = spot_far;
            shadow.point_anchor = point_anchor;
            shadow.point_far = point_far;
            shadow.passes_budget = SHADOW_PASS_BUDGET;
        }
        let hidden: Vec<jarvig_core::EntityId> = self.entity_hidden.iter().copied().collect();
        let visible = visible_ids(snapshot, meshes, &hidden);
        self.shadow_casters = visible.len() as u32;
        self.ensure_caster_slots(visible.len())?;
        if spot.is_some() && self.shadow_maps_updated < SHADOW_PASS_BUDGET {
            self.draw_shadow_2d(snapshot, meshes, &visible, false)?;
            self.shadow_maps_updated = self.shadow_maps_updated.saturating_add(1);
        } else {
            self.clear_shadow_target(false, None)?;
        }
        for face in 0..6 {
            if point.is_some() && self.shadow_maps_updated < SHADOW_PASS_BUDGET {
                self.draw_point_face(snapshot, meshes, &visible, face)?;
                self.shadow_maps_updated = self.shadow_maps_updated.saturating_add(1);
            } else if point.is_none() {
                self.clear_shadow_target(false, Some(face))?;
            }
        }
        self.shadow_world_revision = snapshot.world_revision;
        self.shadow_update_count = self.shadow_update_count.saturating_add(1);
        self.shadow_pass_ms = started.elapsed().as_secs_f32() * 1000.0;
        Ok(())
    }

    /// One view's directional atlas. Stationary local lights are not touched.
    pub(super) fn update_directional_cascades(
        &mut self,
        view: super::RenderViewId,
        camera: &ResolvedPose,
        fov_y: f64,
        aspect: f32,
        near_m: f32,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
    ) -> Result<(), RenderError> {
        self.ensure_shadow_targets()?;
        let Some(light) = snapshot.lights().iter().find(|light| light.kind == LightKind::Directional && light.shadow.cast).copied() else {
            return Ok(());
        };
        let settings = light.shadow;
        let set = fit_directional_cascades(
            *camera,
            light.pose.rotation,
            fov_y,
            f64::from(aspect.max(0.01)),
            f64::from(near_m.max(SHADOW_NEAR_M)),
            f64::from(settings.effective_distance_m()),
            settings.effective_cascades(),
            f64::from(settings.cascade_distribution),
            settings.effective_resolution(CASCADE_RESOLUTION),
            snapshot.world_revision,
        )
        .map_err(RenderError::Space)?;
        if self.shadow.as_ref().expect("shadow").atlases.iter().any(|atlas| atlas.view == view && atlas.fingerprint == set.fingerprint) {
            return Ok(());
        }
        self.ensure_directional_atlas(view)?;
        let hidden: Vec<jarvig_core::EntityId> = self.entity_hidden.iter().copied().collect();
        let visible = visible_ids(snapshot, meshes, &hidden);
        self.shadow_casters = self.shadow_casters.max(visible.len() as u32);
        self.ensure_caster_slots(visible.len())?;
        let started = std::time::Instant::now();
        for index in 0..set.count as usize {
            if self.shadow_maps_updated >= SHADOW_PASS_BUDGET {
                return Ok(());
            }
            let slice = set.slices[index];
            self.draw_cascade_quadrant(snapshot, meshes, &visible, view, &slice, index, index == 0)?;
            self.shadow_maps_updated = self.shadow_maps_updated.saturating_add(1);
        }
        if let Some(atlas) = self.shadow.as_mut().expect("shadow").atlases.iter_mut().find(|atlas| atlas.view == view) {
            atlas.fingerprint = set.fingerprint;
            atlas.set = set;
        }
        self.shadow_pass_ms += started.elapsed().as_secs_f32() * 1000.0;
        Ok(())
    }

    fn ensure_directional_atlas(&mut self, view: super::RenderViewId) -> Result<(), RenderError> {
        if self.shadow.as_ref().expect("shadow").atlases.iter().any(|atlas| atlas.view == view) {
            return Ok(());
        }
        let map = self.create_shadow_map_2d(DIRECTIONAL_ATLAS_RESOLUTION, "JARVIG.Shadow.Directional")?;
        self.shadow.as_mut().expect("shadow").atlases.push(DirectionalAtlas {
            view,
            map,
            fingerprint: 0,
            set: DirectionalCascadeSet::default(),
        });
        Ok(())
    }

    pub(super) fn destroy_shadow_resources(&mut self) -> Result<(), RenderError> {
        let Some(shadow) = self.shadow.take() else {
            return Ok(());
        };
        for caster in &shadow.casters {
            self.device.destroy(ResourceKind::BindGroup, caster.group.raw()).map_err(RenderError::Rhi)?;
        }
        for pipeline in &shadow.pipelines {
            self.device.destroy(ResourceKind::Pipeline, pipeline.pipeline.raw()).map_err(RenderError::Rhi)?;
        }
        self.device.destroy(ResourceKind::ShaderModule, shadow.shader.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::BindGroupLayout, shadow.layout.raw()).map_err(RenderError::Rhi)?;
        for caster in &shadow.casters {
            self.device.destroy(ResourceKind::Buffer, caster.uniform.raw()).map_err(RenderError::Rhi)?;
        }
        self.device.destroy(ResourceKind::Buffer, shadow.disabled.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Sampler, shadow.sampler.raw()).map_err(RenderError::Rhi)?;
        for atlas in shadow.atlases {
            self.destroy_shadow_map_2d(atlas.map)?;
        }
        self.destroy_shadow_map_2d(shadow.spot)?;
        for face in shadow.point_faces {
            self.device.destroy(ResourceKind::TextureView, face.raw()).map_err(RenderError::Rhi)?;
        }
        self.device.destroy(ResourceKind::TextureView, shadow.point_sample.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::TextureView, shadow.point_depth_view.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Texture, shadow.point_texture.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Texture, shadow.point_depth.raw()).map_err(RenderError::Rhi)?;
        Ok(())
    }

    fn create_shadow_map_2d(&mut self, resolution: u32, label: &str) -> Result<ShadowMap2D, RenderError> {
        let color = self
            .device
            .create_texture(&TextureDesc {
                width: resolution,
                height: resolution,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsage::ColorTarget,
                mip_count: 1,
                label: Some(label.into()),
            })
            .map_err(RenderError::Rhi)?;
        let color_view = self.device.create_texture_view(color).map_err(RenderError::Rhi)?;
        let (depth, depth_view) = self.create_shadow_depth(resolution, &format!("{label}.Depth"))?;
        Ok(ShadowMap2D { color, color_view, depth, depth_view })
    }

    fn create_shadow_depth(&mut self, resolution: u32, label: &str) -> Result<(TextureId, TextureViewId), RenderError> {
        let depth = self
            .device
            .create_texture(&TextureDesc {
                width: resolution,
                height: resolution,
                format: TextureFormat::Depth32Float,
                usage: TextureUsage::RenderAttachment,
                mip_count: 1,
                label: Some(label.into()),
            })
            .map_err(RenderError::Rhi)?;
        let view = self.device.create_texture_view(depth).map_err(RenderError::Rhi)?;
        Ok((depth, view))
    }

    fn destroy_shadow_map_2d(&mut self, map: ShadowMap2D) -> Result<(), RenderError> {
        self.device.destroy(ResourceKind::TextureView, map.color_view.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::TextureView, map.depth_view.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Texture, map.color.raw()).map_err(RenderError::Rhi)?;
        self.device.destroy(ResourceKind::Texture, map.depth.raw()).map_err(RenderError::Rhi)?;
        Ok(())
    }

    fn ensure_caster_slots(&mut self, count: usize) -> Result<(), RenderError> {
        let layout = self.shadow.as_ref().expect("shadow").layout;
        let have = self.shadow.as_ref().expect("shadow").casters.len();
        for index in have..count {
            let uniform = self
                .device
                .create_buffer(&BufferDesc {
                    size: 64,
                    usage: BufferUsage::Uniform,
                    label: Some(format!("JARVIG.Shadow.Caster{index}")),
                    contents: None,
                })
                .map_err(RenderError::Rhi)?;
            let group = self
                .device
                .create_bind_group(&BindGroupDesc {
                    layout,
                    entries: vec![BindGroupEntry { binding: 0, resource: jarvig_rhi::BindResource::Buffer(uniform) }],
                    label: Some(format!("JARVIG.Shadow.Caster{index}")),
                })
                .map_err(RenderError::Rhi)?;
            self.shadow.as_mut().expect("shadow").casters.push(ShadowCaster { uniform, group });
        }
        Ok(())
    }

    fn write_caster_clips(&mut self, snapshot: &RenderSceneSnapshot, visible: &[usize], vp: Mat4, anchor: Vec3) -> Result<(), RenderError> {
        for (slot, instance_index) in visible.iter().enumerate() {
            let instance = &snapshot.instances()[*instance_index];
            let clip = vp.mul(light_model(instance, anchor));
            let mut bytes = [0u8; 64];
            clip.write_column_major(&mut bytes);
            let uniform = self.shadow.as_ref().expect("shadow").casters[slot].uniform;
            self.device.write_buffer(uniform, 0, &bytes).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn draw_shadow_2d(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        visible: &[usize],
        _directional: bool,
    ) -> Result<(), RenderError> {
        let (vp, anchor, color, depth) = {
            let shadow = self.shadow.as_ref().expect("shadow");
            (shadow.spot_vp, shadow.spot_anchor, shadow.spot.color_view, shadow.spot.depth_view)
        };
        self.write_caster_clips(snapshot, visible, vp, anchor)?;
        self.encode_shadow_draws(snapshot, meshes, visible, color, depth, 0, 0, SHADOW_MAP_RESOLUTION, true, "JARVIG.Shadow.Spot")
    }

    fn draw_cascade_quadrant(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        visible: &[usize],
        view: super::RenderViewId,
        slice: &CascadeSlice,
        index: usize,
        clear: bool,
    ) -> Result<(), RenderError> {
        self.write_caster_clips(snapshot, visible, slice.clip, slice.anchor)?;
        let (color, depth) = {
            let shadow = self.shadow.as_ref().expect("shadow");
            let atlas = shadow.atlases.iter().find(|atlas| atlas.view == view).expect("cascade atlas");
            (atlas.map.color_view, atlas.map.depth_view)
        };
        let col = (index % 2) as u32 * CASCADE_RESOLUTION;
        let row = (index / 2) as u32 * CASCADE_RESOLUTION;
        self.encode_shadow_draws(snapshot, meshes, visible, color, depth, col, row, CASCADE_RESOLUTION, clear, "JARVIG.Shadow.Cascade")
    }

    fn clear_shadow_target(&mut self, _directional: bool, point_face: Option<usize>) -> Result<(), RenderError> {
        let (color, depth, resolution, label) = {
            let shadow = self.shadow.as_ref().expect("shadow");
            if let Some(face) = point_face {
                (shadow.point_faces[face], shadow.point_depth_view, POINT_SHADOW_RESOLUTION, "JARVIG.Shadow.Point")
            } else {
                (shadow.spot.color_view, shadow.spot.depth_view, SHADOW_MAP_RESOLUTION, "JARVIG.Shadow.Spot")
            }
        };
        self.submit_shadow_pass(color, depth, 0, 0, resolution, true, label, &[])
    }

    fn draw_point_face(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        visible: &[usize],
        face: usize,
    ) -> Result<(), RenderError> {
        let (anchor, far, color, depth) = {
            let shadow = self.shadow.as_ref().expect("shadow");
            (shadow.point_anchor, shadow.point_far, shadow.point_faces[face], shadow.point_depth_view)
        };
        let vp = point_shadow_face_view_proj(face as u32, far).map_err(RenderError::Space)?;
        self.write_caster_clips(snapshot, visible, vp, anchor)?;
        self.encode_shadow_draws(snapshot, meshes, visible, color, depth, 0, 0, POINT_SHADOW_RESOLUTION, true, "JARVIG.Shadow.Point")
    }

    fn encode_shadow_draws(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        meshes: &MeshLibrary,
        visible: &[usize],
        color: TextureViewId,
        depth: TextureViewId,
        origin_x: u32,
        origin_y: u32,
        resolution: u32,
        clear: bool,
        label: &str,
    ) -> Result<(), RenderError> {
        let mut prepared = Vec::with_capacity(visible.len());
        for (slot, instance_index) in visible.iter().enumerate() {
            let instance = &snapshot.instances()[*instance_index];
            let mesh = meshes.get(instance.mesh).ok_or(RenderError::Mesh(jarvig_core::MeshError::Empty))?;
            let stride = position_stride(mesh)?;
            let pipeline = self.ensure_shadow_pipeline(stride)?;
            let group = self.shadow.as_ref().expect("shadow").casters[slot].group;
            prepared.push((instance.mesh, pipeline, group));
        }
        self.shadow_draws_frame = self.shadow_draws_frame.saturating_add(prepared.len() as u32);
        self.submit_shadow_pass(color, depth, origin_x, origin_y, resolution, clear, label, &prepared)
    }

    fn submit_shadow_pass(
        &mut self,
        color: TextureViewId,
        depth: TextureViewId,
        origin_x: u32,
        origin_y: u32,
        resolution: u32,
        clear: bool,
        label: &str,
        prepared: &[(MeshId, PipelineId, BindGroupId)],
    ) -> Result<(), RenderError> {
        let mut encoder = self.device.create_command_encoder(Some(label)).map_err(RenderError::Rhi)?;
        let color_load = if clear {
            LoadOp::Clear(ClearColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 })
        } else {
            LoadOp::Load
        };
        let depth_load = if clear { DepthLoadOp::Clear(DEPTH_CLEAR) } else { DepthLoadOp::Load };
        encoder
            .begin_render_pass(&RenderPassDesc {
                label: Some(label.into()),
                colors: vec![ColorAttachment { target: color, load: color_load, store: StoreOp::Store }],
                depth: Some(DepthAttachment { target: depth, load: depth_load, store: StoreOp::Store }),
            })
            .map_err(RenderError::Rhi)?;
        let edge = resolution as f32;
        encoder
            .set_viewport(Viewport { x: origin_x as f32, y: origin_y as f32, width: edge, height: edge, min_depth: 0.0, max_depth: 1.0 })
            .map_err(RenderError::Rhi)?;
        encoder
            .set_scissor(ScissorRect { x: origin_x, y: origin_y, width: resolution, height: resolution })
            .map_err(RenderError::Rhi)?;
        for &(mesh, pipeline, group) in prepared {
            self.draw_shadow_mesh(&mut *encoder, mesh, pipeline, group)?;
        }
        encoder.end_render_pass().map_err(RenderError::Rhi)?;
        let buffer = encoder.finish().map_err(RenderError::Rhi)?;
        self.device.submit(self.device.graphics_queue(), &[buffer], None).map_err(RenderError::Rhi)?;
        self.device.flush().map_err(RenderError::Rhi)
    }

    fn draw_shadow_mesh(
        &self,
        encoder: &mut dyn jarvig_rhi::CommandEncoder,
        mesh: MeshId,
        pipeline: PipelineId,
        group: BindGroupId,
    ) -> Result<(), RenderError> {
        let gpu = self
            .gpu_meshes
            .iter()
            .find(|(id, _)| *id == mesh)
            .and_then(|(_, slot)| match slot {
                super::GpuResidency::Resident(gpu) => Some(gpu),
                super::GpuResidency::Evicted => None,
            })
            .ok_or(RenderError::Mesh(jarvig_core::MeshError::Empty))?;
        encoder.set_pipeline(pipeline).map_err(RenderError::Rhi)?;
        encoder.set_bind_group(0, group).map_err(RenderError::Rhi)?;
        encoder.set_vertex_buffer(0, gpu.vertices, 0).map_err(RenderError::Rhi)?;
        encoder.set_index_buffer(gpu.indices, gpu.index_format, 0).map_err(RenderError::Rhi)?;
        for range in &gpu.submeshes {
            encoder.draw_indexed(range.index_count, 1, range.first_index, range.base_vertex, 0).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    fn ensure_shadow_pipeline(&mut self, stride: u64) -> Result<PipelineId, RenderError> {
        if let Some(found) = self.shadow.as_ref().expect("shadow").pipelines.iter().find(|pipeline| pipeline.stride == stride) {
            return Ok(found.pipeline);
        }
        let shader = self.shadow.as_ref().expect("shadow").shader;
        let layout = self.shadow.as_ref().expect("shadow").layout;
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
                cull: CullMode::None,
                label: Some(format!("JARVIG.Shadow.Stride{stride}")),
            })
            .map_err(RenderError::Rhi)?;
        self.shadow.as_mut().expect("shadow").pipelines.push(ShadowPipeline { stride, pipeline });
        Ok(pipeline)
    }
}

fn punctual_far(light: &RenderLight) -> f32 {
    let range = if light.range_m > SHADOW_NEAR_M { light.range_m } else { SHADOW_FAR_M };
    range.min(light.shadow.effective_distance_m())
}

fn light_has_map(kind: LightKind, cascades_ready: bool) -> bool {
    match kind {
        LightKind::Directional => cascades_ready,
        LightKind::Spot | LightKind::Point => true,
    }
}

fn visible_ids(snapshot: &RenderSceneSnapshot, meshes: &jarvig_core::MeshLibrary, hidden: &[jarvig_core::EntityId]) -> Vec<usize> {
    snapshot
        .instances()
        .iter()
        .enumerate()
        .filter(|(_, instance)| {
            instance.visible
                && !hidden.contains(&instance.entity)
                && instance.cast_shadows
                && jarvig_core::casts_into_shadow_map(meshes.get(instance.mesh).map(|mesh| mesh.index_count() / 3).unwrap_or(0))
        })
        .map(|(index, _)| index)
        .collect()
}

fn light_model(instance: &RenderInstance, anchor: Vec3) -> Mat4 {
    let relative = camera_relative_f32(instance.pose.translation, anchor);
    let scale = [instance.scale.x as f32, instance.scale.y as f32, instance.scale.z as f32];
    Mat4::from_rotation_translation_scale(instance.pose.rotation, relative, scale)
}

fn position_stride(mesh: &jarvig_core::Mesh) -> Result<u64, RenderError> {
    let stream = mesh.streams().first().ok_or(RenderError::Mesh(jarvig_core::MeshError::Empty))?;
    let position = stream
        .attributes
        .iter()
        .find(|attribute| attribute.shader_location == 0)
        .ok_or(RenderError::Mesh(jarvig_core::MeshError::Empty))?;
    if position.offset != 0 {
        return Err(RenderError::Mesh(jarvig_core::MeshError::Empty));
    }
    Ok(u64::from(stream.stride))
}

const _: f32 = SHADOW_DEPTH_BIAS_M;
