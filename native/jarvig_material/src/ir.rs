//! Canonical material instructions. Not WGSL and not a graph.

use crate::graph::{input_type, output_type, MaterialGraph, NodeId, NodeKind};
use crate::{BlendMode, CullMode, MaterialDomain, MaterialError, ShadingModel, ValueType};

use std::collections::HashSet;

/// Independently versioned. ADR-0022. Version 3 adds the standard surface outputs,
/// channel swizzle, normal decode, and lerp. Versions 1 and 2 were never serialized,
/// so there is no migrator.
pub const MATERIAL_IR_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexSemantic {
    Position,
    Color0,
    TexCoord0,
    Normal,
    Tangent,
}

/// A bound resource. Not a uniform and not a WGSL binding index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceClass {
    Texture2D,
    Sampler,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrResource {
    pub name: String,
    pub class: ResourceClass,
    pub semantic: Option<crate::TextureSemantic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrParameter {
    pub name: String,
    pub ty: ValueType,
    pub default: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrOp {
    LoadVertexColor { dest: u32 },
    LoadParameter { dest: u32, name: String },
    Constant { dest: u32, ty: ValueType, value: [f32; 4] },
    Multiply { dest: u32, left: u32, right: u32 },
    LoadTexCoord0 { dest: u32 },
    /// `uv * scale`. `scale` is a scalar parameter. Not a second shader.
    ScaleTexCoord { dest: u32, scale: u32 },
    /// Sample a 2D texture. The names are logical parameters. This is not `textureSample` text.
    SampleTexture2D { dest: u32, texture: String, sampler: String, uv: u32 },
    Swizzle { dest: u32, source: u32, channels: [u8; 4], width: u8 },
    NormalDecode { dest: u32, sample: u32, scale: u32 },
    /// `mix(from, to, t)`. Not a light and not a WGSL string.
    Lerp { dest: u32, from: u32, to: u32, factor: u32 },
    WriteSurfaceColor { value: u32 },
    WriteBaseColor { value: u32 },
    WriteMetallic { value: u32 },
    WriteRoughness { value: u32 },
    WriteNormal { value: u32 },
    WriteAmbientOcclusion { value: u32 },
    WriteEmissive { value: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialIr {
    version: u32,
    pub domain: MaterialDomain,
    pub shading: ShadingModel,
    pub blend: BlendMode,
    pub cull: CullMode,
    pub two_sided: bool,
    parameters: Vec<IrParameter>,
    resources: Vec<IrResource>,
    vertex: Vec<VertexSemantic>,
    ops: Vec<IrOp>,
}

impl MaterialIr {
    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn parameters(&self) -> &[IrParameter] {
        &self.parameters
    }

    pub fn resources(&self) -> &[IrResource] {
        &self.resources
    }

    pub fn vertex(&self) -> &[VertexSemantic] {
        &self.vertex
    }

    pub fn ops(&self) -> &[IrOp] {
        &self.ops
    }
}

/// Validate the graph, then emit a deterministic instruction list from the surface output.
pub fn lower(graph: &MaterialGraph) -> Result<MaterialIr, MaterialError> {
    validate(graph)?;
    if graph.two_sided && graph.cull != CullMode::None {
        return Err(MaterialError::TwoSidedMustNotCull);
    }
    let mut parameters = Vec::new();
    let mut resources = Vec::new();
    for node in graph.nodes() {
        match &node.kind {
            NodeKind::VectorParameter { name, default } => {
                parameters.push(IrParameter { name: name.clone(), ty: ValueType::Float4, default: *default });
            }
            NodeKind::ScalarParameter { name, default } => {
                parameters.push(IrParameter { name: name.clone(), ty: ValueType::Float, default: [*default, 0.0, 0.0, 0.0] });
            }
            NodeKind::TextureParameter { name, semantic } => {
                resources.push(IrResource { name: name.clone(), class: ResourceClass::Texture2D, semantic: Some(*semantic) });
            }
            NodeKind::SamplerParameter { name } => {
                resources.push(IrResource { name: name.clone(), class: ResourceClass::Sampler, semantic: None });
            }
            _ => {}
        }
    }
    let mut vertex = vec![VertexSemantic::Position];
    if graph.nodes().iter().any(|node| matches!(node.kind, NodeKind::VertexColor)) {
        vertex.push(VertexSemantic::Color0);
    }
    if graph.nodes().iter().any(|node| matches!(node.kind, NodeKind::TexCoord | NodeKind::ScaledTexCoord | NodeKind::TextureSample)) {
        vertex.push(VertexSemantic::TexCoord0);
    }
    if graph.shading == crate::ShadingModel::StandardMetalRough {
        vertex.push(VertexSemantic::Normal);
        vertex.push(VertexSemantic::Tangent);
    }
    enum WriteKind {
        BaseColor,
        Metallic,
        Roughness,
        Normal,
        AmbientOcclusion,
        Emissive,
    }
    let output = graph.nodes().iter().find(|node| matches!(node.kind, NodeKind::SurfaceOutput)).expect("validated");
    let mut ops = Vec::new();
    let mut memo = Vec::<(NodeId, u32)>::new();
    let mut visiting = HashSet::new();
    if graph.shading == crate::ShadingModel::Unlit {
        let color = eval_input(graph, output.id, 0, &mut ops, &mut memo, &mut visiting)?;
        ops.push(IrOp::WriteSurfaceColor { value: color });
    } else {
        let sockets = [
            (0u32, ValueType::Float4, [1.0, 1.0, 1.0, 1.0], WriteKind::BaseColor),
            (1, ValueType::Float, [0.0, 0.0, 0.0, 0.0], WriteKind::Metallic),
            (2, ValueType::Float, [0.5, 0.0, 0.0, 0.0], WriteKind::Roughness),
            (3, ValueType::Float3, [0.0, 0.0, 1.0, 0.0], WriteKind::Normal),
            (4, ValueType::Float, [1.0, 0.0, 0.0, 0.0], WriteKind::AmbientOcclusion),
            (5, ValueType::Float3, [0.0, 0.0, 0.0, 0.0], WriteKind::Emissive),
        ];
        for (socket, ty, default, kind) in sockets {
            let value = if graph.links().iter().any(|link| link.to_node == output.id && link.to_socket == socket) {
                eval_input(graph, output.id, socket, &mut ops, &mut memo, &mut visiting)?
            } else {
                let dest = ops.len() as u32;
                ops.push(IrOp::Constant { dest, ty, value: default });
                dest
            };
            ops.push(match kind {
                WriteKind::BaseColor => IrOp::WriteBaseColor { value },
                WriteKind::Metallic => IrOp::WriteMetallic { value },
                WriteKind::Roughness => IrOp::WriteRoughness { value },
                WriteKind::Normal => IrOp::WriteNormal { value },
                WriteKind::AmbientOcclusion => IrOp::WriteAmbientOcclusion { value },
                WriteKind::Emissive => IrOp::WriteEmissive { value },
            });
        }
    }
    Ok(MaterialIr {
        version: MATERIAL_IR_VERSION,
        domain: graph.domain,
        shading: graph.shading,
        blend: graph.blend,
        cull: graph.cull,
        two_sided: graph.two_sided,
        parameters,
        resources,
        vertex,
        ops,
    })
}

fn validate(graph: &MaterialGraph) -> Result<(), MaterialError> {
    let mut names = HashSet::new();
    for node in graph.nodes() {
        let name = match &node.kind {
            NodeKind::VectorParameter { name, .. }
            | NodeKind::ScalarParameter { name, .. }
            | NodeKind::TextureParameter { name, .. }
            | NodeKind::SamplerParameter { name } => Some(name),
            _ => None,
        };
        if let Some(name) = name {
            if !names.insert(name.clone()) {
                return Err(MaterialError::DuplicateParameter(name.clone()));
            }
        }
    }
    for link in graph.links() {
        let from = graph.node(link.from_node).ok_or(MaterialError::BadNode)?;
        let to = graph.node(link.to_node).ok_or(MaterialError::BadNode)?;
        if matches!(to.kind, NodeKind::Multiply | NodeKind::Lerp) {
            continue;
        }
        let from_ty = produced_type(graph, from.id, &mut HashSet::new())?;
        if matches!(to.kind, NodeKind::SurfaceOutput) && graph.shading == crate::ShadingModel::StandardMetalRough {
            let expected = standard_socket(link.to_socket).ok_or(MaterialError::BadSocket)?;
            if from_ty != expected {
                return Err(MaterialError::TypeMismatch { from: from_ty, to: expected });
            }
            continue;
        }
        let to_ty = input_type(&to.kind, link.to_socket).ok_or(MaterialError::BadSocket)?;
        if from_ty != to_ty {
            return Err(MaterialError::TypeMismatch { from: from_ty, to: to_ty });
        }
    }
    for node in graph.nodes() {
        if !matches!(node.kind, NodeKind::Multiply) {
            continue;
        }
        let mut visiting = HashSet::new();
        let left = linked_type(graph, node.id, 0, &mut visiting)?;
        let right = linked_type(graph, node.id, 1, &mut visiting)?;
        if !left.is_uniform() || left != right {
            return Err(MaterialError::TypeMismatch { from: left, to: right });
        }
    }
    for node in graph.nodes() {
        if !matches!(node.kind, NodeKind::Lerp) {
            continue;
        }
        let mut visiting = HashSet::new();
        let from = linked_type(graph, node.id, 0, &mut visiting)?;
        let to = linked_type(graph, node.id, 1, &mut visiting)?;
        let factor = linked_type(graph, node.id, 2, &mut visiting)?;
        if !from.is_uniform() || from != to || factor != ValueType::Float {
            return Err(MaterialError::TypeMismatch { from, to: if from != to { to } else { ValueType::Float } });
        }
    }
    let outputs: Vec<_> = graph.nodes().iter().filter(|node| matches!(node.kind, NodeKind::SurfaceOutput)).collect();
    if outputs.len() != 1 {
        return Err(MaterialError::MissingOutput);
    }
    if graph.shading == crate::ShadingModel::Unlit {
        let linked = graph.links().iter().any(|link| link.to_node == outputs[0].id && link.to_socket == 0);
        if !linked {
            return Err(MaterialError::MissingOutputConnection);
        }
    }
    Ok(())
}

fn standard_socket(socket: u32) -> Option<ValueType> {
    match socket {
        0 => Some(ValueType::Float4),
        1 | 2 | 4 => Some(ValueType::Float),
        3 | 5 => Some(ValueType::Float3),
        _ => None,
    }
}

fn linked_type(
    graph: &MaterialGraph,
    node: NodeId,
    socket: u32,
    visiting: &mut HashSet<NodeId>,
) -> Result<ValueType, MaterialError> {
    let link = graph
        .links()
        .iter()
        .find(|link| link.to_node == node && link.to_socket == socket)
        .ok_or(MaterialError::MissingOutputConnection)?;
    produced_type(graph, link.from_node, visiting)
}

fn produced_type(graph: &MaterialGraph, id: NodeId, visiting: &mut HashSet<NodeId>) -> Result<ValueType, MaterialError> {
    if !visiting.insert(id) {
        return Err(MaterialError::Cycle);
    }
    let node = graph.node(id).ok_or(MaterialError::BadNode)?;
    let ty = match &node.kind {
        NodeKind::Multiply | NodeKind::Lerp => linked_type(graph, id, 0, visiting)?,
        NodeKind::Swizzle(mask) => mask.value_type(),
        other => output_type(other, 0).ok_or(MaterialError::BadSocket)?,
    };
    visiting.remove(&id);
    Ok(ty)
}

fn parameter_name(graph: &MaterialGraph, node: NodeId, socket: u32) -> Result<String, MaterialError> {
    let link = graph
        .links()
        .iter()
        .find(|link| link.to_node == node && link.to_socket == socket)
        .ok_or(MaterialError::MissingOutputConnection)?;
    let source = graph.node(link.from_node).ok_or(MaterialError::BadNode)?;
    match &source.kind {
        NodeKind::TextureParameter { name, .. } | NodeKind::SamplerParameter { name } | NodeKind::VectorParameter { name, .. } => {
            Ok(name.clone())
        }
        _ => Err(MaterialError::BadNode),
    }
}

fn eval_input(
    graph: &MaterialGraph,
    node: NodeId,
    socket: u32,
    ops: &mut Vec<IrOp>,
    memo: &mut Vec<(NodeId, u32)>,
    visiting: &mut HashSet<NodeId>,
) -> Result<u32, MaterialError> {
    let link = graph
        .links()
        .iter()
        .find(|link| link.to_node == node && link.to_socket == socket)
        .ok_or(MaterialError::MissingOutputConnection)?;
    eval_node(graph, link.from_node, ops, memo, visiting)
}

fn eval_node(
    graph: &MaterialGraph,
    id: NodeId,
    ops: &mut Vec<IrOp>,
    memo: &mut Vec<(NodeId, u32)>,
    visiting: &mut HashSet<NodeId>,
) -> Result<u32, MaterialError> {
    if let Some((_, dest)) = memo.iter().find(|(node, _)| *node == id) {
        return Ok(*dest);
    }
    if !visiting.insert(id) {
        return Err(MaterialError::Cycle);
    }
    let node = graph.node(id).ok_or(MaterialError::BadNode)?;
    let dest = match &node.kind {
        NodeKind::VertexColor => {
            let dest = ops.len() as u32;
            ops.push(IrOp::LoadVertexColor { dest });
            dest
        }
        NodeKind::VectorParameter { name, .. } | NodeKind::ScalarParameter { name, .. } => {
            let dest = ops.len() as u32;
            ops.push(IrOp::LoadParameter { dest, name: name.clone() });
            dest
        }
        NodeKind::Constant { ty, value } => {
            let dest = ops.len() as u32;
            ops.push(IrOp::Constant { dest, ty: *ty, value: *value });
            dest
        }
        NodeKind::Multiply => {
            let left = eval_input(graph, id, 0, ops, memo, visiting)?;
            let right = eval_input(graph, id, 1, ops, memo, visiting)?;
            let dest = ops.len() as u32;
            ops.push(IrOp::Multiply { dest, left, right });
            dest
        }
        NodeKind::TexCoord => {
            let dest = ops.len() as u32;
            ops.push(IrOp::LoadTexCoord0 { dest });
            dest
        }
        NodeKind::ScaledTexCoord => {
            let scale = eval_input(graph, id, 0, ops, memo, visiting)?;
            let dest = ops.len() as u32;
            ops.push(IrOp::ScaleTexCoord { dest, scale });
            dest
        }
        NodeKind::Swizzle(mask) => {
            let source = eval_input(graph, id, 0, ops, memo, visiting)?;
            let dest = ops.len() as u32;
            ops.push(IrOp::Swizzle { dest, source, channels: mask.channels, width: mask.width });
            dest
        }
        NodeKind::NormalDecode => {
            let sample = eval_input(graph, id, 0, ops, memo, visiting)?;
            let scale = eval_input(graph, id, 1, ops, memo, visiting)?;
            let dest = ops.len() as u32;
            ops.push(IrOp::NormalDecode { dest, sample, scale });
            dest
        }
        NodeKind::Lerp => {
            let from = eval_input(graph, id, 0, ops, memo, visiting)?;
            let to = eval_input(graph, id, 1, ops, memo, visiting)?;
            let factor = eval_input(graph, id, 2, ops, memo, visiting)?;
            let dest = ops.len() as u32;
            ops.push(IrOp::Lerp { dest, from, to, factor });
            dest
        }
        NodeKind::TextureSample => {
            let texture = parameter_name(graph, id, 0)?;
            let sampler = parameter_name(graph, id, 1)?;
            let uv = eval_input(graph, id, 2, ops, memo, visiting)?;
            let dest = ops.len() as u32;
            ops.push(IrOp::SampleTexture2D { dest, texture, sampler, uv });
            dest
        }
        NodeKind::TextureParameter { .. } | NodeKind::SamplerParameter { .. } => return Err(MaterialError::BadNode),
        NodeKind::SurfaceOutput => return Err(MaterialError::MissingOutput),
    };
    visiting.remove(&id);
    memo.push((id, dest));
    Ok(dest)
}
