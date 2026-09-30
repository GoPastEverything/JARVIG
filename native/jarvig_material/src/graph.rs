//! Programmatic material graph. Not a node editor and not a shader.

use crate::{BlendMode, CullMode, MaterialDomain, ShadingModel, ValueType};

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    VertexColor,
    /// Runtime vector. The name is the authoring key. The compiler assigns the packed offset.
    VectorParameter { name: String, default: [f32; 4] },
    Constant { ty: ValueType, value: [f32; 4] },
    Multiply,
    /// Mesh TexCoord0. Not a WGSL location.
    TexCoord,
    /// `uv * scalar`. Socket 0 is the scale. Instance edits do not recompile.
    ScaledTexCoord,
    TextureParameter { name: String, semantic: crate::TextureSemantic },
    SamplerParameter { name: String },
    /// Sample a 2D texture. Sockets: texture, sampler, uv.
    TextureSample,
    ScalarParameter { name: String, default: f32 },
    /// Pull channels out of a Float4. Width 1 is a scalar. This is how ORM is mapped.
    Swizzle(SwizzleMask),
    /// Linear normal sample to a tangent-space vector. Sockets: sample, scale.
    NormalDecode,
    /// `mix(from, to, t)`. Sockets: from, to, t. `t` is a scalar.
    Lerp,
    SurfaceOutput,
}

/// Channel indices are 0=R, 1=G, 2=B, 3=A.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwizzleMask {
    pub channels: [u8; 4],
    pub width: u8,
}

impl SwizzleMask {
    pub fn one(channel: u8) -> Self {
        Self { channels: [channel, 0, 0, 0], width: 1 }
    }

    pub fn rgb() -> Self {
        Self { channels: [0, 1, 2, 0], width: 3 }
    }

    pub fn value_type(self) -> ValueType {
        match self.width {
            1 => ValueType::Float,
            2 => ValueType::Float2,
            3 => ValueType::Float3,
            _ => ValueType::Float4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphLink {
    pub id: u32,
    pub from_node: NodeId,
    pub from_socket: u32,
    pub to_node: NodeId,
    pub to_socket: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
}

/// One authored surface program. Connections are ids, not pointers.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialGraph {
    pub domain: MaterialDomain,
    pub shading: ShadingModel,
    pub blend: BlendMode,
    /// Opaque default is back-face culling. Two-sided forces [`CullMode::None`].
    pub cull: CullMode,
    /// When set, a visible back face flips the geometric normal and tangent handedness.
    pub two_sided: bool,
    pub(crate) nodes: Vec<Node>,
    pub(crate) links: Vec<GraphLink>,
    next_node: u32,
    next_link: u32,
}

impl MaterialGraph {
    pub fn surface_unlit() -> Self {
        Self::surface(ShadingModel::Unlit)
    }

    pub fn surface_standard() -> Self {
        Self::surface(ShadingModel::StandardMetalRough)
    }

    fn surface(shading: ShadingModel) -> Self {
        Self {
            domain: MaterialDomain::Surface,
            shading,
            blend: BlendMode::Opaque,
            cull: CullMode::Back,
            two_sided: false,
            nodes: Vec::new(),
            links: Vec::new(),
            next_node: 0,
            next_link: 0,
        }
    }

    /// Opt in to shading both sides. Does not become the default for every material.
    pub fn set_two_sided(&mut self, two_sided: bool) {
        self.two_sided = two_sided;
        if two_sided {
            self.cull = CullMode::None;
        } else if self.cull == CullMode::None {
            self.cull = CullMode::Back;
        }
    }

    pub fn add(&mut self, kind: NodeKind) -> NodeId {
        self.next_node += 1;
        let id = NodeId(self.next_node);
        self.nodes.push(Node { id, kind });
        id
    }

    pub fn link(&mut self, from_node: NodeId, from_socket: u32, to_node: NodeId, to_socket: u32) {
        self.next_link += 1;
        self.links.push(GraphLink {
            id: self.next_link,
            from_node,
            from_socket,
            to_node,
            to_socket,
        });
    }

    pub fn links_mut(&mut self) -> &mut Vec<GraphLink> {
        &mut self.links
    }

    pub(crate) fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub(crate) fn links(&self) -> &[GraphLink] {
        &self.links
    }

    pub(crate) fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }
}

/// `SurfaceOutput.color = VertexColor * Tint`. Tint is a runtime Float4.
pub fn vertex_color_tint(name: &str, default: [f32; 4]) -> MaterialGraph {
    let mut graph = MaterialGraph::surface_unlit();
    let color = graph.add(NodeKind::VertexColor);
    let tint = graph.add(NodeKind::VectorParameter { name: name.into(), default });
    let multiply = graph.add(NodeKind::Multiply);
    let output = graph.add(NodeKind::SurfaceOutput);
    graph.link(color, 0, multiply, 0);
    graph.link(tint, 0, multiply, 1);
    graph.link(multiply, 0, output, 0);
    graph
}

/// `SurfaceOutput.color = VertexColor * Tint * Sample(BaseTexture, BaseSampler, UV0)`.
pub fn textured_unlit(tint_name: &str, tint_default: [f32; 4]) -> MaterialGraph {
    let mut graph = MaterialGraph::surface_unlit();
    let color = graph.add(NodeKind::VertexColor);
    let tint = graph.add(NodeKind::VectorParameter { name: tint_name.into(), default: tint_default });
    let uv = graph.add(NodeKind::TexCoord);
    let texture = graph.add(NodeKind::TextureParameter { name: "BaseTexture".into(), semantic: crate::TextureSemantic::UnlitColor });
    let sampler = graph.add(NodeKind::SamplerParameter { name: "BaseSampler".into() });
    let sample = graph.add(NodeKind::TextureSample);
    let tinted = graph.add(NodeKind::Multiply);
    let modulated = graph.add(NodeKind::Multiply);
    let output = graph.add(NodeKind::SurfaceOutput);
    graph.link(texture, 0, sample, 0);
    graph.link(sampler, 0, sample, 1);
    graph.link(uv, 0, sample, 2);
    graph.link(color, 0, tinted, 0);
    graph.link(tint, 0, tinted, 1);
    graph.link(tinted, 0, modulated, 0);
    graph.link(sample, 0, modulated, 1);
    graph.link(modulated, 0, output, 0);
    graph
}

/// One ORM map is this graph's choice. The master does not require ORM.
/// R = AO, G = roughness, B = metallic. Factors are runtime uniforms.
pub fn standard_metal_rough() -> MaterialGraph {
    let mut graph = MaterialGraph::surface_standard();
    let uv_scale = graph.add(NodeKind::ScalarParameter { name: "UvScale".into(), default: 1.0 });
    let uv = graph.add(NodeKind::ScaledTexCoord);
    graph.link(uv_scale, 0, uv, 0);
    let sampler = graph.add(NodeKind::SamplerParameter { name: "MaterialSampler".into() });
    let base_tex = graph.add(NodeKind::TextureParameter { name: "BaseColor".into(), semantic: crate::TextureSemantic::BaseColor });
    let base_sample = graph.add(NodeKind::TextureSample);
    let base_factor = graph.add(NodeKind::VectorParameter { name: "BaseColorFactor".into(), default: [1.0, 1.0, 1.0, 1.0] });
    let base_mul = graph.add(NodeKind::Multiply);
    graph.link(base_tex, 0, base_sample, 0);
    graph.link(sampler, 0, base_sample, 1);
    graph.link(uv, 0, base_sample, 2);
    graph.link(base_sample, 0, base_mul, 0);
    graph.link(base_factor, 0, base_mul, 1);

    let orm_tex = graph.add(NodeKind::TextureParameter { name: "Orm".into(), semantic: crate::TextureSemantic::Orm });
    let orm_sample = graph.add(NodeKind::TextureSample);
    graph.link(orm_tex, 0, orm_sample, 0);
    graph.link(sampler, 0, orm_sample, 1);
    graph.link(uv, 0, orm_sample, 2);
    let ao = graph.add(NodeKind::Swizzle(SwizzleMask::one(0)));
    let rough = graph.add(NodeKind::Swizzle(SwizzleMask::one(1)));
    let metal = graph.add(NodeKind::Swizzle(SwizzleMask::one(2)));
    graph.link(orm_sample, 0, ao, 0);
    graph.link(orm_sample, 0, rough, 0);
    graph.link(orm_sample, 0, metal, 0);
    let ao_factor = graph.add(NodeKind::ScalarParameter { name: "OcclusionStrength".into(), default: 1.0 });
    let rough_factor = graph.add(NodeKind::ScalarParameter { name: "RoughnessFactor".into(), default: 0.5 });
    let metal_factor = graph.add(NodeKind::ScalarParameter { name: "MetallicFactor".into(), default: 0.0 });
    // Strength 0 keeps AO at 1. Strength 1 uses the sampled channel. Not `sample * strength`.
    let ao_one = graph.add(NodeKind::Constant { ty: ValueType::Float, value: [1.0, 0.0, 0.0, 0.0] });
    let ao_mix = graph.add(NodeKind::Lerp);
    let rough_mul = graph.add(NodeKind::Multiply);
    let metal_mul = graph.add(NodeKind::Multiply);
    graph.link(ao_one, 0, ao_mix, 0);
    graph.link(ao, 0, ao_mix, 1);
    graph.link(ao_factor, 0, ao_mix, 2);
    graph.link(rough, 0, rough_mul, 0);
    graph.link(rough_factor, 0, rough_mul, 1);
    graph.link(metal, 0, metal_mul, 0);
    graph.link(metal_factor, 0, metal_mul, 1);

    let normal_tex = graph.add(NodeKind::TextureParameter { name: "Normal".into(), semantic: crate::TextureSemantic::Normal });
    let normal_sample = graph.add(NodeKind::TextureSample);
    let normal_scale = graph.add(NodeKind::ScalarParameter { name: "NormalScale".into(), default: 1.0 });
    let normal = graph.add(NodeKind::NormalDecode);
    graph.link(normal_tex, 0, normal_sample, 0);
    graph.link(sampler, 0, normal_sample, 1);
    graph.link(uv, 0, normal_sample, 2);
    graph.link(normal_sample, 0, normal, 0);
    graph.link(normal_scale, 0, normal, 1);

    let emissive_tex = graph.add(NodeKind::TextureParameter { name: "Emissive".into(), semantic: crate::TextureSemantic::Emissive });
    let emissive_sample = graph.add(NodeKind::TextureSample);
    let emissive_rgb = graph.add(NodeKind::Swizzle(SwizzleMask::rgb()));
    let emissive_factor = graph.add(NodeKind::VectorParameter { name: "EmissiveFactor".into(), default: [0.0, 0.0, 0.0, 0.0] });
    let emissive_factor_rgb = graph.add(NodeKind::Swizzle(SwizzleMask::rgb()));
    let emissive_mul = graph.add(NodeKind::Multiply);
    graph.link(emissive_tex, 0, emissive_sample, 0);
    graph.link(sampler, 0, emissive_sample, 1);
    graph.link(uv, 0, emissive_sample, 2);
    graph.link(emissive_sample, 0, emissive_rgb, 0);
    graph.link(emissive_factor, 0, emissive_factor_rgb, 0);
    graph.link(emissive_rgb, 0, emissive_mul, 0);
    graph.link(emissive_factor_rgb, 0, emissive_mul, 1);

    let output = graph.add(NodeKind::SurfaceOutput);
    graph.link(base_mul, 0, output, 0);
    graph.link(metal_mul, 0, output, 1);
    graph.link(rough_mul, 0, output, 2);
    graph.link(normal, 0, output, 3);
    graph.link(ao_mix, 0, output, 4);
    graph.link(emissive_mul, 0, output, 5);
    graph
}

pub(crate) fn output_type(kind: &NodeKind, socket: u32) -> Option<ValueType> {
    match kind {
        NodeKind::VertexColor if socket == 0 => Some(ValueType::Float4),
        NodeKind::VectorParameter { .. } if socket == 0 => Some(ValueType::Float4),
        NodeKind::Constant { ty, .. } if socket == 0 => Some(*ty),
        NodeKind::Multiply if socket == 0 => Some(ValueType::Float4),
        NodeKind::TexCoord if socket == 0 => Some(ValueType::Float2),
        NodeKind::ScaledTexCoord if socket == 0 => Some(ValueType::Float2),
        NodeKind::TextureParameter { .. } if socket == 0 => Some(ValueType::Texture2D),
        NodeKind::SamplerParameter { .. } if socket == 0 => Some(ValueType::Sampler),
        NodeKind::TextureSample if socket == 0 => Some(ValueType::Float4),
        NodeKind::ScalarParameter { .. } if socket == 0 => Some(ValueType::Float),
        NodeKind::Swizzle(mask) if socket == 0 => Some(mask.value_type()),
        NodeKind::NormalDecode if socket == 0 => Some(ValueType::Float3),
        NodeKind::Lerp if socket == 0 => Some(ValueType::Float),
        _ => None,
    }
}

pub(crate) fn input_type(kind: &NodeKind, socket: u32) -> Option<ValueType> {
    match kind {
        NodeKind::Multiply if socket == 0 || socket == 1 => Some(ValueType::Float4),
        NodeKind::SurfaceOutput if socket == 0 => Some(ValueType::Float4),
        NodeKind::TextureSample if socket == 0 => Some(ValueType::Texture2D),
        NodeKind::TextureSample if socket == 1 => Some(ValueType::Sampler),
        NodeKind::TextureSample if socket == 2 => Some(ValueType::Float2),
        NodeKind::ScaledTexCoord if socket == 0 => Some(ValueType::Float),
        NodeKind::Swizzle(_) if socket == 0 => Some(ValueType::Float4),
        NodeKind::NormalDecode if socket == 0 => Some(ValueType::Float4),
        NodeKind::NormalDecode if socket == 1 => Some(ValueType::Float),
        _ => None,
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "node {}", self.0)
    }
}
