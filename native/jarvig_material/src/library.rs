//! Masters and instances. Instances do not own graphs. Compiling is not instancing.

use crate::compile::{compile, CompiledMaterial};
use crate::graph::{standard_metal_rough, textured_unlit, MaterialGraph};
use crate::ir::{lower, MaterialIr, ResourceClass};
use crate::{BlendMode, MaterialDomain, SamplerId, ShadingModel, TextureId, ValueType};

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MasterMaterialId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialInstanceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParameterId(pub u32);

/// Runtime values can change without a new shader. Static switches are not implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterClass {
    RuntimeUniform,
    /// Bound texture. Not packed into the uniform buffer. Changing it does not recompile.
    TextureResource,
    /// Bound sampler. Not packed into the uniform buffer. Changing it does not recompile.
    SamplerResource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialParameter {
    pub id: ParameterId,
    pub name: String,
    pub ty: ValueType,
    pub class: ParameterClass,
    pub default: [f32; 4],
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterValue {
    Float(f32),
    Float4([f32; 4]),
    Texture(TextureId),
    Sampler(SamplerId),
}

impl ParameterValue {
    fn ty(self) -> ValueType {
        match self {
            Self::Float(_) => ValueType::Float,
            Self::Float4(_) => ValueType::Float4,
            Self::Texture(_) => ValueType::Texture2D,
            Self::Sampler(_) => ValueType::Sampler,
        }
    }

    fn float4(self) -> [f32; 4] {
        match self {
            Self::Float(value) => [value, 0.0, 0.0, 0.0],
            Self::Float4(value) => value,
            Self::Texture(_) | Self::Sampler(_) => [0.0; 4],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialError {
    MissingOutput,
    MissingOutputConnection,
    Cycle,
    BadNode,
    BadSocket,
    TypeMismatch { from: crate::ValueType, to: crate::ValueType },
    DuplicateParameter(String),
    UnknownParameter(String),
    ParameterType { name: String, expected: ValueType, got: ValueType },
    BadParameterName(String),
    UnknownMaster,
    UnknownInstance,
    UnsupportedIr,
    TwoSidedMustNotCull,
}

impl fmt::Display for MaterialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOutput => write!(formatter, "material graph has no surface output"),
            Self::MissingOutputConnection => write!(formatter, "surface output is not connected"),
            Self::Cycle => write!(formatter, "material graph has a cycle"),
            Self::BadNode => write!(formatter, "material link names a missing node"),
            Self::BadSocket => write!(formatter, "material link names a missing socket"),
            Self::TypeMismatch { from, to } => write!(formatter, "material types {from:?} and {to:?} are not compatible"),
            Self::DuplicateParameter(name) => write!(formatter, "duplicate material parameter {name}"),
            Self::UnknownParameter(name) => write!(formatter, "unknown material parameter {name}"),
            Self::ParameterType { name, expected, got } => {
                write!(formatter, "parameter {name} is {expected:?}, not {got:?}")
            }
            Self::BadParameterName(name) => write!(formatter, "parameter name {name} is not a WGSL field"),
            Self::UnknownMaster => write!(formatter, "unknown master material"),
            Self::UnknownInstance => write!(formatter, "unknown material instance"),
            Self::UnsupportedIr => write!(formatter, "unsupported material IR version"),
            Self::TwoSidedMustNotCull => write!(formatter, "a two-sided material cannot cull faces"),
        }
    }
}

struct Master {
    id: MasterMaterialId,
    /// Bumped when the graph changes. Not implemented as hot reload. A later edit recompiles this key.
    revision: u64,
    graph: MaterialGraph,
    ir: MaterialIr,
    compiled: CompiledMaterial,
}

struct Instance {
    id: MaterialInstanceId,
    master: MasterMaterialId,
    revision: u64,
    bytes: Vec<u8>,
    textures: Vec<(String, TextureId)>,
    samplers: Vec<(String, SamplerId)>,
}

/// In-memory masters and instances. Not the asset database.
#[derive(Default)]
pub struct MaterialLibrary {
    masters: Vec<Master>,
    instances: Vec<Instance>,
    compiles: u32,
    pbr_compiles: u32,
    next_master: u64,
    next_instance: u64,
}

impl MaterialLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_master(&mut self, graph: MaterialGraph) -> Result<MasterMaterialId, MaterialError> {
        let pbr = graph.shading == ShadingModel::StandardMetalRough;
        let ir = lower(&graph)?;
        let compiled = compile(&ir)?;
        self.compiles = self.compiles.saturating_add(1);
        if pbr {
            self.pbr_compiles = self.pbr_compiles.saturating_add(1);
        }
        self.next_master += 1;
        let id = MasterMaterialId(self.next_master);
        self.masters.push(Master { id, revision: 1, graph, ir, compiled });
        Ok(id)
    }

    /// One unlit master and two tint instances. The caller binds them to mesh slots.
    pub fn bootstrap_vertex_color_tint() -> (Self, MaterialInstanceId, MaterialInstanceId) {
        let mut library = Self::new();
        let master = library
            .add_master(crate::vertex_color_tint("Tint", [1.0, 1.0, 1.0, 1.0]))
            .expect("bootstrap material graph");
        let near = library
            .create_instance(master, &[("Tint", ParameterValue::Float4([1.0, 1.0, 1.0, 1.0]))])
            .expect("near tint");
        let far = library
            .create_instance(master, &[("Tint", ParameterValue::Float4([0.45, 0.7, 1.0, 1.0]))])
            .expect("far tint");
        (library, near, far)
    }

    /// One textured master and two instances. The caller owns the logical textures.
    pub fn bootstrap_textured(
        near_texture: TextureId,
        far_texture: TextureId,
        sampler: SamplerId,
    ) -> (Self, MaterialInstanceId, MaterialInstanceId) {
        let mut library = Self::new();
        let master = library
            .add_master(textured_unlit("Tint", [1.0, 1.0, 1.0, 1.0]))
            .expect("bootstrap textured material");
        let near = library
            .create_instance(
                master,
                &[
                    ("Tint", ParameterValue::Float4([1.0, 1.0, 1.0, 1.0])),
                    ("BaseTexture", ParameterValue::Texture(near_texture)),
                    ("BaseSampler", ParameterValue::Sampler(sampler)),
                ],
            )
            .expect("near textured instance");
        let far = library
            .create_instance(
                master,
                &[
                    ("Tint", ParameterValue::Float4([0.45, 0.7, 1.0, 1.0])),
                    ("BaseTexture", ParameterValue::Texture(far_texture)),
                    ("BaseSampler", ParameterValue::Sampler(sampler)),
                ],
            )
            .expect("far textured instance");
        (library, near, far)
    }

    /// One standard master. Near is a rough dielectric. Far is a smoother metal with a little emissive.
    pub fn bootstrap_standard(
        near_color: TextureId,
        far_color: TextureId,
        orm: TextureId,
        normal: TextureId,
        emissive: TextureId,
        sampler: SamplerId,
    ) -> (Self, MaterialInstanceId, MaterialInstanceId) {
        let mut library = Self::new();
        // Temporary. These are cards, not closed solids, so both sides stay inspectable.
        let mut graph = standard_metal_rough();
        graph.set_two_sided(true);
        let master = library.add_master(graph).expect("standard material");
        let shared = [
            ("Orm", ParameterValue::Texture(orm)),
            ("Normal", ParameterValue::Texture(normal)),
            ("Emissive", ParameterValue::Texture(emissive)),
            ("MaterialSampler", ParameterValue::Sampler(sampler)),
            ("BaseColorFactor", ParameterValue::Float4([1.0, 1.0, 1.0, 1.0])),
            ("NormalScale", ParameterValue::Float(1.0)),
            ("OcclusionStrength", ParameterValue::Float(1.0)),
        ];
        let mut near_values = shared.to_vec();
        near_values.push(("BaseColor", ParameterValue::Texture(near_color)));
        near_values.push(("MetallicFactor", ParameterValue::Float(0.0)));
        near_values.push(("RoughnessFactor", ParameterValue::Float(0.85)));
        near_values.push(("EmissiveFactor", ParameterValue::Float4([0.0, 0.0, 0.0, 0.0])));
        let near = library.create_instance(master, &near_values.iter().map(|(name, value)| (*name, *value)).collect::<Vec<_>>()).expect("near pbr");
        let mut far_values = shared.to_vec();
        far_values.push(("BaseColor", ParameterValue::Texture(far_color)));
        far_values.push(("MetallicFactor", ParameterValue::Float(1.0)));
        far_values.push(("RoughnessFactor", ParameterValue::Float(0.2)));
        far_values.push(("EmissiveFactor", ParameterValue::Float4([0.35, 0.12, 0.02, 0.0])));
        let far = library.create_instance(master, &far_values.iter().map(|(name, value)| (*name, *value)).collect::<Vec<_>>()).expect("far pbr");
        (library, near, far)
    }

    pub fn pbr_master_count(&self) -> usize {
        self.masters.iter().filter(|master| master.graph.shading == ShadingModel::StandardMetalRough).count()
    }

    pub fn pbr_instance_count(&self) -> usize {
        self.instances
            .iter()
            .filter(|instance| {
                self.masters
                    .iter()
                    .any(|master| master.id == instance.master && master.graph.shading == ShadingModel::StandardMetalRough)
            })
            .count()
    }

    pub fn pbr_compile_count(&self) -> u32 {
        self.pbr_compiles
    }

    pub fn create_instance(
        &mut self,
        master: MasterMaterialId,
        overrides: &[(&str, ParameterValue)],
    ) -> Result<MaterialInstanceId, MaterialError> {
        let compiled = &self.master(master)?.compiled;
        let mut bytes = vec![0u8; compiled.parameters.iter().map(|parameter| parameter.size as usize).sum()];
        for parameter in &compiled.parameters {
            write_float4(&mut bytes, parameter.offset, parameter.default);
        }
        let mut textures = Vec::new();
        let mut samplers = Vec::new();
        for resource in &compiled.resources {
            match resource.class {
                ResourceClass::Texture2D => textures.push((resource.name.clone(), TextureId(0))),
                ResourceClass::Sampler => samplers.push((resource.name.clone(), SamplerId(0))),
            }
        }
        for (name, value) in overrides {
            match *value {
                ParameterValue::Texture(id) => bind_texture(&compiled, &mut textures, name, id)?,
                ParameterValue::Sampler(id) => bind_sampler(&compiled, &mut samplers, name, id)?,
                numeric => apply(&compiled, &mut bytes, name, numeric)?,
            }
        }
        self.next_instance += 1;
        let id = MaterialInstanceId(self.next_instance);
        self.instances.push(Instance { id, master, revision: 1, bytes, textures, samplers });
        Ok(id)
    }

    pub fn set_parameter(&mut self, instance: MaterialInstanceId, name: &str, value: ParameterValue) -> Result<(), MaterialError> {
        let master = self.instance(instance)?.master;
        let compiled = self.master(master)?.compiled.clone();
        let slot = self.instance_mut(instance)?;
        apply(&compiled, &mut slot.bytes, name, value)?;
        slot.revision = slot.revision.saturating_add(1);
        Ok(())
    }

    pub fn set_texture(&mut self, instance: MaterialInstanceId, name: &str, texture: TextureId) -> Result<(), MaterialError> {
        let master = self.instance(instance)?.master;
        let compiled = self.master(master)?.compiled.clone();
        let slot = self.instance_mut(instance)?;
        bind_texture(&compiled, &mut slot.textures, name, texture)?;
        slot.revision = slot.revision.saturating_add(1);
        Ok(())
    }

    pub fn set_sampler(&mut self, instance: MaterialInstanceId, name: &str, sampler: SamplerId) -> Result<(), MaterialError> {
        let master = self.instance(instance)?.master;
        let compiled = self.master(master)?.compiled.clone();
        let slot = self.instance_mut(instance)?;
        bind_sampler(&compiled, &mut slot.samplers, name, sampler)?;
        slot.revision = slot.revision.saturating_add(1);
        Ok(())
    }

    pub fn texture_binding(&self, instance: MaterialInstanceId, name: &str) -> Result<TextureId, MaterialError> {
        self.instance(instance)?
            .textures
            .iter()
            .find(|(stored, _)| stored == name)
            .map(|(_, id)| *id)
            .ok_or_else(|| MaterialError::UnknownParameter(name.into()))
    }

    pub fn sampler_binding(&self, instance: MaterialInstanceId, name: &str) -> Result<SamplerId, MaterialError> {
        self.instance(instance)?
            .samplers
            .iter()
            .find(|(stored, _)| stored == name)
            .map(|(_, id)| *id)
            .ok_or_else(|| MaterialError::UnknownParameter(name.into()))
    }

    pub fn compile_count(&self) -> u32 {
        self.compiles
    }

    pub fn master_count(&self) -> usize {
        self.masters.len()
    }

    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    pub fn master_of(&self, instance: MaterialInstanceId) -> Result<MasterMaterialId, MaterialError> {
        Ok(self.instance(instance)?.master)
    }

    pub fn instance_revision(&self, instance: MaterialInstanceId) -> Result<u64, MaterialError> {
        Ok(self.instance(instance)?.revision)
    }

    pub fn parameter_bytes(&self, instance: MaterialInstanceId) -> Result<&[u8], MaterialError> {
        Ok(&self.instance(instance)?.bytes)
    }

    /// Four packed floats for one uniform. Float parameters use the first component.
    pub fn uniform_parameter(&self, instance: MaterialInstanceId, name: &str) -> Result<[f32; 4], MaterialError> {
        let master = self.instance(instance)?.master;
        let parameter = self
            .master(master)?
            .compiled
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .ok_or_else(|| MaterialError::UnknownParameter(name.into()))?;
        let bytes = &self.instance(instance)?.bytes;
        let start = parameter.offset as usize;
        let mut value = [0.0; 4];
        for (index, channel) in value.iter_mut().enumerate() {
            let offset = start + index * 4;
            let raw = bytes.get(offset..offset + 4).ok_or_else(|| MaterialError::UnknownParameter(name.into()))?;
            *channel = f32::from_le_bytes(raw.try_into().unwrap_or([0; 4]));
        }
        Ok(value)
    }

    pub fn compiled(&self, master: MasterMaterialId) -> Result<&CompiledMaterial, MaterialError> {
        Ok(&self.master(master)?.compiled)
    }

    pub fn master_revision(&self, master: MasterMaterialId) -> Result<u64, MaterialError> {
        Ok(self.master(master)?.revision)
    }

    /// Authoring schema. The renderer uses the packed bytes, not this name lookup.
    pub fn schema(&self, master: MasterMaterialId) -> Result<Vec<MaterialParameter>, MaterialError> {
        let compiled = &self.master(master)?.compiled;
        Ok(compiled
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| MaterialParameter {
                id: ParameterId(index as u32),
                name: parameter.name.clone(),
                ty: parameter.ty,
                class: ParameterClass::RuntimeUniform,
                default: parameter.default,
                offset: parameter.offset,
                size: parameter.size,
            })
            .chain(compiled.resources.iter().enumerate().map(|(index, resource)| {
                let class = match resource.class {
                    ResourceClass::Texture2D => ParameterClass::TextureResource,
                    ResourceClass::Sampler => ParameterClass::SamplerResource,
                };
                let ty = match resource.class {
                    ResourceClass::Texture2D => ValueType::Texture2D,
                    ResourceClass::Sampler => ValueType::Sampler,
                };
                MaterialParameter {
                    id: ParameterId((compiled.parameters.len() + index) as u32),
                    name: resource.name.clone(),
                    ty,
                    class,
                    default: [0.0; 4],
                    offset: 0,
                    size: 0,
                }
            }))
            .collect())
    }

    pub fn domain(&self, master: MasterMaterialId) -> Result<(MaterialDomain, ShadingModel, BlendMode), MaterialError> {
        let master = self.master(master)?;
        let _ = master.ir.version();
        Ok((master.graph.domain, master.graph.shading, master.graph.blend))
    }

    fn master(&self, id: MasterMaterialId) -> Result<&Master, MaterialError> {
        self.masters.iter().find(|master| master.id == id).ok_or(MaterialError::UnknownMaster)
    }

    fn instance(&self, id: MaterialInstanceId) -> Result<&Instance, MaterialError> {
        self.instances.iter().find(|instance| instance.id == id).ok_or(MaterialError::UnknownInstance)
    }

    fn instance_mut(&mut self, id: MaterialInstanceId) -> Result<&mut Instance, MaterialError> {
        self.instances.iter_mut().find(|instance| instance.id == id).ok_or(MaterialError::UnknownInstance)
    }
}

fn bind_texture(compiled: &CompiledMaterial, slots: &mut [(String, TextureId)], name: &str, texture: TextureId) -> Result<(), MaterialError> {
    let resource = compiled
        .resources
        .iter()
        .find(|resource| resource.name == name)
        .ok_or_else(|| MaterialError::UnknownParameter(name.into()))?;
    if resource.class != ResourceClass::Texture2D {
        return Err(MaterialError::ParameterType { name: name.into(), expected: ValueType::Texture2D, got: ValueType::Sampler });
    }
    let slot = slots.iter_mut().find(|(stored, _)| stored == name).ok_or_else(|| MaterialError::UnknownParameter(name.into()))?;
    slot.1 = texture;
    Ok(())
}

fn bind_sampler(compiled: &CompiledMaterial, slots: &mut [(String, SamplerId)], name: &str, sampler: SamplerId) -> Result<(), MaterialError> {
    let resource = compiled
        .resources
        .iter()
        .find(|resource| resource.name == name)
        .ok_or_else(|| MaterialError::UnknownParameter(name.into()))?;
    if resource.class != ResourceClass::Sampler {
        return Err(MaterialError::ParameterType { name: name.into(), expected: ValueType::Sampler, got: ValueType::Texture2D });
    }
    let slot = slots.iter_mut().find(|(stored, _)| stored == name).ok_or_else(|| MaterialError::UnknownParameter(name.into()))?;
    slot.1 = sampler;
    Ok(())
}

fn apply(compiled: &CompiledMaterial, bytes: &mut [u8], name: &str, value: ParameterValue) -> Result<(), MaterialError> {
    let parameter = compiled
        .parameters
        .iter()
        .find(|parameter| parameter.name == name)
        .ok_or_else(|| MaterialError::UnknownParameter(name.into()))?;
    if parameter.ty != value.ty() {
        return Err(MaterialError::ParameterType { name: name.into(), expected: parameter.ty, got: value.ty() });
    }
    write_float4(bytes, parameter.offset, value.float4());
    Ok(())
}

fn write_float4(bytes: &mut [u8], offset: u32, value: [f32; 4]) {
    let start = offset as usize;
    for (index, component) in value.iter().enumerate() {
        bytes[start + index * 4..start + index * 4 + 4].copy_from_slice(&component.to_le_bytes());
    }
}
