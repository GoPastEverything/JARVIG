//! Logical materials. A graph is not WGSL. The IR is not a shader string.
//! A master owns the program. An instance overrides parameters.
//! This crate does not know wgpu, winit, or the RHI.

mod basis;
mod brdf;
mod compile;
mod graph;
mod ir;
mod library;
mod surface;

pub use basis::{apply_visible_side, decode_normal, shade_normal, view_direction, Mat3};
pub use brdf::{
    directional_illuminance, emissive_radiance, environment_diffuse, environment_specular, evaluate_direct, perceptual_alpha, punctual_illuminance,
    reflected_direct,
    spot_angular, DIELECTRIC_F0, MIN_LIGHT_DISTANCE_M, MIN_PERCEPTUAL_ROUGHNESS,
};
pub use compile::{compile, CompiledMaterial, CompiledResource, ShaderTarget};
pub use surface::{MaterialSurface, DEFAULT_AMBIENT_OCCLUSION, DEFAULT_BASE_COLOR, DEFAULT_EMISSIVE, DEFAULT_METALLIC, DEFAULT_NORMAL_TS, DEFAULT_ROUGHNESS};
pub use graph::{standard_metal_rough, textured_unlit, vertex_color_tint, GraphLink, GraphNode, MaterialGraph, NodeId, NodeKind, SocketId, SwizzleMask};
pub use ir::{lower, IrOp, MaterialIr, ResourceClass, VertexSemantic, MATERIAL_IR_VERSION};
pub use library::{
    MasterMaterialId, MaterialError, MaterialInstanceId, MaterialLibrary, MaterialParameter, ParameterClass, ParameterId,
    ParameterValue,
};

/// Authored domain. Only Surface is implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialDomain {
    Surface,
}

/// Unlit stays. StandardMetalRough is the surface model. Lights are JRV-0054.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingModel {
    Unlit,
    StandardMetalRough,
}

/// What a texture means. Color space follows this, not the shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureSemantic {
    BaseColor,
    Emissive,
    Metallic,
    Roughness,
    AmbientOcclusion,
    Normal,
    /// Linear data for a later displacement path. Not a BRDF input.
    Height,
    Orm,
    Data,
    UnlitColor,
}

/// Only opaque is implemented. Masked, translucent, and additive are later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Opaque,
}

/// Which faces the rasterizer keeps. Counter-clockwise is front.
/// This is material policy. The backend maps it. It is not a wgpu type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CullMode {
    Back,
    Front,
    None,
}

/// Canonical material value. Not a WGSL type. Textures and samplers are not uniform floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Float,
    Float2,
    Float3,
    Float4,
    Texture2D,
    Sampler,
}

/// How stored RGBA8 bytes are sampled. Not a wgpu format.
///
/// `Srgb` bytes are sRGB-encoded. The GPU format decodes them to linear on sample.
/// `Unorm` bytes are data. Sampling must not apply that decode.
/// ADR-0027.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rgba8Storage {
    Unorm,
    Srgb,
}

/// Color meaning of a texture. The shader does not guess this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    /// Data: roughness, metallic, AO, normal, height, masks. Sampled as stored.
    Linear,
    /// Color: base color and other display color. Stored sRGB, sampled as linear.
    Srgb,
}

impl ColorSpace {
    pub fn rgba8_storage(self) -> Rgba8Storage {
        match self {
            Self::Linear => Rgba8Storage::Unorm,
            Self::Srgb => Rgba8Storage::Srgb,
        }
    }
}

/// Only Texture2D is implemented. Array, cube, and 3D stay open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureDimension {
    Texture2D,
}

/// Logical texture identity. Not an RHI texture and not a GPU object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(pub u64);

/// Logical sampler identity. Not an RHI sampler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SamplerId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterMode {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
}

/// Anisotropic samples on the repeating material sampler. Clamped to the device limit.
/// Shadow and probe samplers stay at 1.
pub const MATERIAL_ANISOTROPY: u8 = 8;

/// Sampler state shared by instances. Identical state can share one GPU sampler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SamplerState {
    pub min_filter: FilterMode,
    pub mag_filter: FilterMode,
    pub mip_filter: FilterMode,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub address_w: AddressMode,
    /// 1 disables anisotropy. The material repeat sampler uses [`MATERIAL_ANISOTROPY`].
    pub anisotropy: u8,
}

impl SamplerState {
    pub fn linear_repeat() -> Self {
        Self {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mip_filter: FilterMode::Linear,
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
            address_w: AddressMode::Repeat,
            anisotropy: MATERIAL_ANISOTROPY,
        }
    }

    pub fn linear_clamp() -> Self {
        Self {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mip_filter: FilterMode::Linear,
            address_u: AddressMode::ClampToEdge,
            address_v: AddressMode::ClampToEdge,
            address_w: AddressMode::ClampToEdge,
            anisotropy: 1,
        }
    }
}

impl ValueType {
    pub fn components(self) -> u8 {
        match self {
            Self::Float => 1,
            Self::Float2 => 2,
            Self::Float3 => 3,
            Self::Float4 => 4,
            Self::Texture2D | Self::Sampler => 0,
        }
    }

    /// std140-style size used by numeric uniforms. Textures and samplers are not packed.
    pub fn packed_size(self) -> u32 {
        match self {
            Self::Float => 4,
            Self::Float2 => 8,
            Self::Float3 => 16,
            Self::Float4 => 16,
            Self::Texture2D | Self::Sampler => 0,
        }
    }

    pub fn is_uniform(self) -> bool {
        !matches!(self, Self::Texture2D | Self::Sampler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_crate_does_not_depend_on_a_gpu_or_window_library() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("wgpu"));
        assert!(!manifest.contains("winit"));
        assert!(!manifest.contains("jarvig_rhi"));
    }

    #[test]
    fn a_tint_graph_lowers_once_and_instances_do_not_recompile() {
        let graph = vertex_color_tint("Tint", [1.0, 1.0, 1.0, 1.0]);
        let ir = lower(&graph).unwrap();
        assert_eq!(ir.version(), MATERIAL_IR_VERSION);
        assert!(ir.vertex().contains(&VertexSemantic::Color0));
        assert!(ir.vertex().contains(&VertexSemantic::Position));
        assert_eq!(ir.parameters().len(), 1);
        assert_eq!(ir.parameters()[0].name, "Tint");
        let again = lower(&graph).unwrap();
        assert_eq!(ir.ops(), again.ops());
        let compiled = compile(&ir).unwrap();
        assert_eq!(compiled.shading, ShadingModel::Unlit);
        assert_eq!(compiled.blend, BlendMode::Opaque);
        assert_eq!(compiled.target, ShaderTarget::Wgsl);
        assert!(compiled.artifact.contains("material.tint"));
        assert!(!compiled.artifact.contains("fn fs_hardcoded"));
        assert_eq!(compiled.parameters[0].offset, 0);
        assert_eq!(compiled.parameters[0].size, 16);

        let mut library = MaterialLibrary::new();
        let master = library.add_master(graph).unwrap();
        assert_eq!(library.compile_count(), 1);
        let white = library.create_instance(master, &[("Tint", ParameterValue::Float4([1.0, 1.0, 1.0, 1.0]))]).unwrap();
        let cool = library.create_instance(master, &[("Tint", ParameterValue::Float4([0.45, 0.7, 1.0, 1.0]))]).unwrap();
        assert_eq!(library.master_count(), 1);
        assert_eq!(library.instance_count(), 2);
        assert_ne!(library.parameter_bytes(white).unwrap(), library.parameter_bytes(cool).unwrap());
        assert!(library.create_instance(master, &[("Tint", ParameterValue::Float(1.0))]).is_err());
        assert!(library.create_instance(master, &[("Missing", ParameterValue::Float4([1.0; 4]))]).is_err());
        let before = library.compile_count();
        let revision = library.instance_revision(cool).unwrap();
        library.set_parameter(cool, "Tint", ParameterValue::Float4([0.2, 0.2, 1.0, 1.0])).unwrap();
        assert_eq!(library.compile_count(), before);
        assert!(library.instance_revision(cool).unwrap() > revision);
        assert_eq!(library.master_of(white).unwrap(), master);
        assert_eq!(library.master_of(cool).unwrap(), master);
        let schema = library.schema(master).unwrap();
        assert_eq!(schema[0].id, ParameterId(0));
        assert_eq!(schema[0].class, ParameterClass::RuntimeUniform);
        assert_eq!(library.master_revision(master).unwrap(), 1);
        assert_eq!(ColorSpace::Srgb.rgba8_storage(), Rgba8Storage::Srgb);
        assert_eq!(ColorSpace::Linear.rgba8_storage(), Rgba8Storage::Unorm);
        assert_ne!(ColorSpace::Srgb.rgba8_storage(), ColorSpace::Linear.rgba8_storage());
    }

    #[test]
    fn a_texture_sample_is_an_ir_instruction_and_not_a_second_compile() {
        let graph = textured_unlit("Tint", [1.0, 1.0, 1.0, 1.0]);
        let ir = lower(&graph).unwrap();
        assert!(ir.vertex().contains(&VertexSemantic::TexCoord0));
        assert!(ir.vertex().contains(&VertexSemantic::Color0));
        assert!(ir.resources().iter().any(|resource| resource.name == "BaseTexture"));
        assert!(ir.resources().iter().any(|resource| resource.name == "BaseSampler"));
        assert!(ir.ops().iter().any(|op| matches!(op, IrOp::SampleTexture2D { .. })));
        let compiled = compile(&ir).unwrap();
        assert!(compiled.artifact.contains("textureSample("));
        assert!(compiled.artifact.contains("color-space is a texture format"));
        assert!(!compiled.artifact.contains("pow("));
        assert_eq!(compiled.resources.len(), 2);
        assert_eq!(compiled.resources[0].binding, 1);
        assert_eq!(compiled.resources[1].binding, 2);
        assert_eq!(compiled.parameters[0].name, "Tint");

        let mut library = MaterialLibrary::new();
        let master = library.add_master(graph).unwrap();
        let near = library
            .create_instance(
                master,
                &[
                    ("Tint", ParameterValue::Float4([1.0, 1.0, 1.0, 1.0])),
                    ("BaseTexture", ParameterValue::Texture(TextureId(1))),
                    ("BaseSampler", ParameterValue::Sampler(SamplerId(1))),
                ],
            )
            .unwrap();
        let far = library
            .create_instance(
                master,
                &[
                    ("Tint", ParameterValue::Float4([0.45, 0.7, 1.0, 1.0])),
                    ("BaseTexture", ParameterValue::Texture(TextureId(2))),
                    ("BaseSampler", ParameterValue::Sampler(SamplerId(1))),
                ],
            )
            .unwrap();
        assert_eq!(library.compile_count(), 1);
        assert_ne!(library.texture_binding(near, "BaseTexture").unwrap(), library.texture_binding(far, "BaseTexture").unwrap());
        library.set_texture(near, "BaseTexture", TextureId(3)).unwrap();
        assert_eq!(library.compile_count(), 1);
        assert_eq!(library.texture_binding(near, "BaseTexture").unwrap(), TextureId(3));
        library.set_sampler(near, "BaseSampler", SamplerId(4)).unwrap();
        assert_eq!(library.compile_count(), 1);
        assert_eq!(library.sampler_binding(near, "BaseSampler").unwrap(), SamplerId(4));
        assert!(library.set_texture(near, "Tint", TextureId(1)).is_err());
        assert!(library.set_parameter(near, "BaseTexture", ParameterValue::Float4([1.0; 4])).is_err());
    }

    #[test]
    fn standard_surface_lowers_without_baking_a_light() {
        let ir = lower(&standard_metal_rough()).unwrap();
        assert_eq!(ir.shading, ShadingModel::StandardMetalRough);
        assert!(ir.vertex().contains(&VertexSemantic::Normal));
        assert!(ir.vertex().contains(&VertexSemantic::Tangent));
        assert!(ir.ops().iter().any(|op| matches!(op, IrOp::WriteMetallic { .. })));
        assert!(ir.ops().iter().any(|op| matches!(op, IrOp::Swizzle { .. })));
        assert!(ir.ops().iter().any(|op| matches!(op, IrOp::NormalDecode { .. })));
        assert!(!format!("{:?}", ir.ops()).contains("textureSample"));
        let compiled = compile(&ir).unwrap();
        assert!(compiled.artifact.contains("fn evaluate_material"));
        assert!(compiled.artifact.contains("@group(2)"));
        assert!(compiled.artifact.contains("jarvig_direct"));
        assert!(compiled.artifact.contains("jarvig_environment_diffuse"));
        assert!(compiled.artifact.contains("jarvig_environment_specular"));
        assert!(compiled.artifact.contains("jarvig_env_brdf"));
        assert!(compiled.artifact.contains("jarvig_hemisphere"));
        assert!(compiled.artifact.contains("jarvig_probe_weight"));
        assert!(compiled.artifact.contains("jarvig_term_on"));
        assert!(compiled.artifact.contains("textureSampleLevel"));
        assert!(compiled.artifact.contains("texture_cube"));
        assert!(compiled.artifact.contains("@group(2) @binding(3)"));
        assert!(compiled.artifact.contains("@group(2) @binding(4)"));
        assert!(compiled.artifact.contains("@group(2) @binding(5)"));
        assert!(compiled.artifact.contains("fn jarvig_direct_visibility"));
        assert!(compiled.artifact.contains("@group(2) @binding(6)"));
        assert!(compiled.artifact.contains("@group(2) @binding(10)"));
        assert!(compiled.artifact.contains("@group(2) @binding(11)"));
        assert!(compiled.artifact.contains("fn jarvig_indirect_diffuse"));
        let bounce = compiled.artifact.split("fn jarvig_indirect_diffuse").nth(1).unwrap();
        let bounce = bounce.split("@fragment").next().unwrap();
        assert!(!bounce.contains("jarvig_direct_visibility"));
        assert!(bounce.contains("irradiance_cube"));
        assert!(bounce.contains("ambient_occlusion"));
        let fragment = compiled.artifact.split("@fragment").nth(1).unwrap();
        assert!(fragment.contains("jarvig_direct_visibility"));
        let after_direct = fragment.split("jarvig_environment_diffuse").nth(1).unwrap();
        assert!(!after_direct.contains("jarvig_direct_visibility"));
        let specular = compiled.artifact.split("fn jarvig_environment_specular").nth(1).unwrap();
        let specular = specular.split("fn jarvig_indirect_diffuse").next().unwrap();
        assert!(!specular.contains("ambient_occlusion"));
        assert!(compiled.artifact.contains("@group(2) @binding(2)"));
        let diffuse = compiled.artifact.split("fn jarvig_environment_diffuse").nth(1).unwrap();
        let diffuse = diffuse.split("fn jarvig_env_brdf").next().unwrap();
        assert!(!diffuse.contains("reflection_cube"));
        let direct = compiled.artifact.split("fn jarvig_direct").nth(1).unwrap();
        let direct = direct.split("fn jarvig_environment_diffuse").next().unwrap();
        assert!(!direct.contains("ambient_occlusion"));
        assert!(compiled.artifact.contains("surface.ambient_occlusion"));
        assert!(compiled.artifact.contains("surface.emissive"));
        assert!(compiled.artifact.contains(&format!("{:.8}", MIN_PERCEPTUAL_ROUGHNESS)));
        assert!(compiled.artifact.contains(&format!("{:.8}", DIELECTRIC_F0)));
        assert!(compiled.artifact.contains(&format!("{:.8}", MIN_LIGHT_DISTANCE_M)));
        assert_eq!(compiled.cull, CullMode::Back);
        assert!(!compiled.two_sided);
        assert!(!compiled.artifact.contains("front_facing"));
        assert!(compiled.artifact.contains("jarvig_shade_normal"));
        assert!(compiled.artifact.contains("fn jarvig_specular_roughness"));
        assert!(compiled.artifact.contains("fn jarvig_reflection_lod"));
        assert!(compiled.artifact.contains("dpdx("));
        assert!(compiled.artifact.contains("jarvig_shadow_depth"));
        assert!(!compiled.artifact.contains("for (var y: i32 = -4"));
        assert!(compiled.artifact.contains("jarvig_normal_matrix"));
        assert!(compiled.artifact.contains("mix("));
        assert!(!compiled.artifact.contains("inverse("));
        assert!(!compiled.artifact.contains("2.2"));
        let unlit = compile(&lower(&vertex_color_tint("Tint", [1.0; 4])).unwrap()).unwrap();
        assert!(!unlit.artifact.contains("@group(2)"));
        assert_eq!(compiled.parameters.iter().find(|parameter| parameter.name == "RoughnessFactor").unwrap().size, 16);
        let mut library = MaterialLibrary::new();
        let master = library.add_master(standard_metal_rough()).unwrap();
        let rough = library
            .create_instance(master, &[("RoughnessFactor", ParameterValue::Float(0.85)), ("MetallicFactor", ParameterValue::Float(0.0))])
            .unwrap();
        library.set_parameter(rough, "MetallicFactor", ParameterValue::Float(1.0)).unwrap();
        assert_eq!(library.compile_count(), 1);
        assert!(TextureSemantic::BaseColor.color_space() == ColorSpace::Srgb);
        assert!(TextureSemantic::Height.color_space() == ColorSpace::Linear);
        assert!(!TextureSemantic::Height.feeds_standard_brdf());
        assert!(TextureSemantic::Normal.feeds_standard_brdf());
    }

    #[test]
    fn the_shared_standard_master_draws_both_sides() {
        let (library, near, _) = MaterialLibrary::bootstrap_standard(TextureId(1), TextureId(2), TextureId(3), TextureId(4), TextureId(5), SamplerId(1));
        let compiled = library.compiled(library.master_of(near).unwrap()).unwrap();
        assert!(compiled.two_sided);
        assert_eq!(compiled.cull, CullMode::None);
    }

    #[test]
    fn two_sided_flips_the_visible_normal_and_one_sided_culls_the_back() {
        let mut graph = standard_metal_rough();
        graph.set_two_sided(true);
        let compiled = compile(&lower(&graph).unwrap()).unwrap();
        assert!(compiled.two_sided);
        assert_eq!(compiled.cull, CullMode::None);
        assert!(compiled.artifact.contains("@builtin(front_facing)"));
        assert!(compiled.artifact.contains("geometric = -geometric"));
        assert!(compiled.artifact.contains("tangent.w = -tangent.w"));
        graph.cull = CullMode::Back;
        assert!(matches!(lower(&graph), Err(MaterialError::TwoSidedMustNotCull)));
        let identity = Mat3::scale(1.0, 1.0, 1.0);
        let geometric = [0.0, 1.0, 0.0];
        let tangent = [1.0, 0.0, 0.0, 1.0];
        let (front_n, front_t) = apply_visible_side(geometric, tangent, true);
        assert_eq!(front_n, geometric);
        assert_eq!(front_t[3], 1.0);
        let (back_n, back_t) = apply_visible_side(geometric, tangent, false);
        assert_eq!(back_n, [0.0, -1.0, 0.0]);
        assert_eq!(back_t[3], -1.0);
        let shaded = shade_normal(identity, back_n, back_t, [0.0, 0.0, 1.0]);
        assert!(shaded.iter().all(|channel| channel.is_finite()));
        assert!(shaded[1] < -0.99, "{shaded:?}");
        let toward_camera = [0.0, -1.0, 0.0];
        let lit = evaluate_direct(&MaterialSurface::default(), shaded, toward_camera, toward_camera);
        assert!(lit.direct()[0] > 0.0);
        let stale = evaluate_direct(&MaterialSurface::default(), geometric, toward_camera, toward_camera);
        assert_eq!(stale.direct(), [0.0; 3]);
        let ground = environment_diffuse(&MaterialSurface::default(), shaded, [0.55, 0.68, 0.86], [0.22, 0.16, 0.11], 0.20, true);
        let sky = environment_diffuse(&MaterialSurface::default(), geometric, [0.55, 0.68, 0.86], [0.22, 0.16, 0.11], 0.20, true);
        assert!(ground[0] > ground[2]);
        assert!(sky[2] > sky[0]);
        assert_ne!(ground, sky);
    }

    #[test]
    fn invalid_graphs_are_rejected() {
        let mut graph = vertex_color_tint("Tint", [1.0, 1.0, 1.0, 1.0]);
        graph.links_mut().clear();
        assert!(matches!(lower(&graph), Err(MaterialError::MissingOutputConnection)));

        let mut cycle = MaterialGraph::surface_unlit();
        let a = cycle.add(NodeKind::Multiply);
        let b = cycle.add(NodeKind::Multiply);
        cycle.link(a, 0, b, 0);
        cycle.link(b, 0, a, 0);
        let output = cycle.add(NodeKind::SurfaceOutput);
        cycle.link(a, 0, output, 0);
        assert!(matches!(lower(&cycle), Err(MaterialError::Cycle)));

        let mut mismatch = MaterialGraph::surface_unlit();
        let scalar = mismatch.add(NodeKind::Constant { ty: ValueType::Float, value: [1.0, 0.0, 0.0, 0.0] });
        let output = mismatch.add(NodeKind::SurfaceOutput);
        mismatch.link(scalar, 0, output, 0);
        assert!(matches!(lower(&mismatch), Err(MaterialError::TypeMismatch { .. })));

        let mut duplicate = MaterialGraph::surface_unlit();
        duplicate.add(NodeKind::VectorParameter { name: "Tint".into(), default: [1.0; 4] });
        duplicate.add(NodeKind::VectorParameter { name: "Tint".into(), default: [0.0; 4] });
        assert!(matches!(lower(&duplicate), Err(MaterialError::DuplicateParameter(_))));

        let mut dangling = MaterialGraph::surface_unlit();
        let output = dangling.add(NodeKind::SurfaceOutput);
        dangling.link(NodeId(99), 0, output, 0);
        assert!(matches!(lower(&dangling), Err(MaterialError::BadNode)));
    }
}
