//! First-class character document. Schema `jarvig.character` version 1.
//!
//! A character stores the source mesh parts, one native Joint on each part, the
//! rest pose, and wide default limits. Collision and the animation set are absent.
//! The frame origin is the socket on that part. Mesh vertices stay in the source
//! world place. The limits are not measured from the doll.

use std::collections::VecDeque;

use crate::json_lite::{parse_json, Json};
use crate::level::{parse_entity, ComponentRecord, EntityRecord, LevelDocument, LevelError, WorldSettingsRecord};
use crate::prefab::{PrefabDocument, PREFAB_FORMAT_VERSION};
use crate::{
    AssetId, BindPart, BindPoseAssembly, EntityId, EnvironmentLight, JointKind, JointLimits, JointRecord, MaterialAssetRef, MaterialScheme, MeshAssetRef,
    ProbeUpdatePolicy, Quat, SceneWorld, Vec3, BIND_POSE_CONNECT_M, LEVEL_JOINT_VERSION,
};

pub const CHARACTER_SCHEMA: &str = "jarvig.character";
pub const CHARACTER_FORMAT_VERSION: u32 = 1;
pub const CHARACTER_LIMITS_NOTE: &str = "wide-defaults";

/// Source-space character. Placing it adds one translation to the root only.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterDocument {
    pub format_version: u32,
    pub character_uuid: EntityId,
    pub name: String,
    pub limits: String,
    pub entities: Vec<EntityRecord>,
}

/// The document plus the bind-pose report for the parts that produced it.
#[derive(Clone, Debug)]
pub struct CharacterBuild {
    pub document: CharacterDocument,
    pub report: String,
}

impl CharacterDocument {
    pub fn validate(&self) -> Result<(), LevelError> {
        if self.format_version != CHARACTER_FORMAT_VERSION {
            return Err(LevelError::UnsupportedVersion(self.format_version));
        }
        if self.name.is_empty() || !self.character_uuid.is_persistent() {
            return Err(LevelError::Corrupt("character name or uuid is invalid".into()));
        }
        if self.limits != CHARACTER_LIMITS_NOTE {
            return Err(LevelError::Corrupt("character limits are not wide-defaults".into()));
        }
        if self.entities.is_empty() {
            return Err(LevelError::Corrupt("character has no parts".into()));
        }
        let mut seen = std::collections::HashSet::new();
        let mut roots = 0u32;
        for entity in &self.entities {
            if !entity.uuid.is_persistent() || !seen.insert(entity.uuid) || entity.name.is_empty() {
                return Err(LevelError::Corrupt(format!("character part {} is invalid", entity.name)));
            }
            if entity.components.iter().any(|component| matches!(component, ComponentRecord::WorldSettings)) {
                return Err(LevelError::Corrupt("a character cannot contain World Settings".into()));
            }
            let has_transform = entity.components.iter().any(|component| matches!(component, ComponentRecord::Transform { .. }));
            let has_mesh = entity.components.iter().any(|component| matches!(component, ComponentRecord::MeshRenderer { .. }));
            let has_joint = entity.components.iter().any(|component| matches!(component, ComponentRecord::Joint(_)));
            if !has_transform || !has_mesh || !has_joint {
                return Err(LevelError::Corrupt(format!("{} needs a transform, a mesh, and a joint", entity.name)));
            }
            if entity.parent_uuid.is_none() {
                roots += 1;
            }
        }
        if roots != 1 {
            return Err(LevelError::Corrupt("a character needs one root joint".into()));
        }
        for entity in &self.entities {
            if let Some(parent) = entity.parent_uuid {
                if !seen.contains(&parent) {
                    return Err(LevelError::InvalidParent(parent.to_string()));
                }
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> String {
        Json::object(vec![
            ("schema", Json::string(CHARACTER_SCHEMA)),
            ("format_version", Json::int(self.format_version as i64)),
            ("character_uuid", Json::string(self.character_uuid.to_string())),
            ("name", Json::string(&self.name)),
            ("limits", Json::string(&self.limits)),
            ("collision", Json::Null),
            ("animation_set", Json::Null),
            ("entities", Json::array(self.entities.iter().map(EntityRecord::to_json).collect())),
        ])
        .write()
    }

    /// New uuids. The root translation shifts by `origin`. Children keep their rest locals.
    pub fn instantiate(&self, world: &mut SceneWorld, origin: Vec3) -> Result<Vec<EntityId>, LevelError> {
        self.as_prefab()?.instantiate(world, origin)
    }

    /// Keeps the file uuids. Used when a sample level places the character once.
    pub fn instantiate_preserving_ids(&self, world: &mut SceneWorld, origin: Vec3) -> Result<Vec<EntityId>, LevelError> {
        self.as_prefab()?.instantiate_preserving_ids(world, origin)
    }

    /// A level that is only this character, in source space. The caller loads the mesh assets first.
    pub fn open_as_level(&self, level_uuid: EntityId, settings_uuid: EntityId) -> Result<LevelDocument, LevelError> {
        self.validate()?;
        if settings_uuid == level_uuid || self.entities.iter().any(|entity| entity.uuid == settings_uuid || entity.uuid == level_uuid) {
            return Err(LevelError::Corrupt("character level ids collide with a part".into()));
        }
        let light = EnvironmentLight::bootstrap();
        let mut entities = vec![EntityRecord {
            uuid: settings_uuid,
            name: "World Settings".into(),
            parent_uuid: None,
            components: vec![ComponentRecord::WorldSettings],
        }];
        entities.extend(self.entities.iter().cloned());
        let document = LevelDocument {
            format_version: LEVEL_JOINT_VERSION,
            level_uuid,
            name: self.name.clone(),
            world_settings: WorldSettingsRecord {
                entity: settings_uuid,
                enabled: light.enabled,
                intensity: light.intensity,
                upper: light.upper_hemisphere_linear_rgb,
                lower: light.lower_hemisphere_linear_rgb,
                probe_update_policy: ProbeUpdatePolicy::Static,
                startup_camera: None,
            },
            entities,
            organization: crate::SceneOrganization::default(),
        };
        document.validate()?;
        Ok(document)
    }

    fn as_prefab(&self) -> Result<PrefabDocument, LevelError> {
        self.validate()?;
        let prefab = PrefabDocument {
            format_version: PREFAB_FORMAT_VERSION,
            prefab_uuid: self.character_uuid,
            name: self.name.clone(),
            entities: self.entities.clone(),
        };
        prefab.validate()?;
        Ok(prefab)
    }
}

pub fn parse_character(text: &str) -> Result<CharacterDocument, LevelError> {
    let json = parse_json(text)?;
    let schema = json.get("schema").and_then(Json::as_str).ok_or_else(|| LevelError::Corrupt("character schema is missing".into()))?;
    if schema != CHARACTER_SCHEMA {
        return Err(LevelError::Corrupt(format!("schema {schema} is not {CHARACTER_SCHEMA}")));
    }
    let format_version = json.get("format_version").and_then(Json::as_f64).ok_or_else(|| LevelError::Corrupt("character format version is missing".into()))?;
    if format_version.fract() != 0.0 || format_version <= 0.0 {
        return Err(LevelError::Corrupt("character format version is not an integer".into()));
    }
    let character_uuid = EntityId::parse(json.get("character_uuid").and_then(Json::as_str).unwrap_or("")).ok_or_else(|| LevelError::Corrupt("character uuid is invalid".into()))?;
    let name = json.get("name").and_then(Json::as_str).unwrap_or("").to_string();
    let limits = json.get("limits").and_then(Json::as_str).unwrap_or("").to_string();
    if !matches!(json.get("collision"), Some(Json::Null)) || !matches!(json.get("animation_set"), Some(Json::Null)) {
        return Err(LevelError::Corrupt("collision and animation_set stay null until a later ticket".into()));
    }
    let entities_json = json.get("entities").and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt("character entities are missing".into()))?;
    let mut entities = Vec::new();
    for entity in entities_json {
        entities.push(parse_entity(entity)?);
    }
    let document = CharacterDocument { format_version: format_version as u32, character_uuid, name, limits, entities };
    document.validate()?;
    Ok(document)
}

/// Map one standing bind pose onto Joint and SpatialFrame.
///
/// The root is Fixed. Every other part is a Ball whose rest is the change of basis
/// from its contact parent, so the world pose matches the socket. Limits are the
/// wide defaults. The frame origin is the ball that joins the part to its parent.
pub fn character_from_bind_pose(name: &str, character_uuid: EntityId, body_code: u8, assembly: &BindPoseAssembly, assets: &[AssetId]) -> Result<CharacterBuild, LevelError> {
    if name.is_empty() || !character_uuid.is_persistent() {
        return Err(LevelError::Corrupt("character name or uuid is invalid".into()));
    }
    if assembly.parts.is_empty() || assets.len() != assembly.parts.len() {
        return Err(LevelError::Corrupt("character parts and mesh assets do not match".into()));
    }
    let pelvis = assembly.parts.iter().position(|part| part.name == assembly.pelvis_name).ok_or_else(|| LevelError::Corrupt("pelvis part is missing".into()))?;
    let span = span_from_pelvis(&assembly.parts, pelvis)?;
    let mut locals = Vec::with_capacity(assembly.parts.len());
    for index in 0..assembly.parts.len() {
        locals.push(rest_local(&assembly.parts, &span.parent, index)?);
    }
    verify_world_pose(&assembly.parts, &span.parent, &locals)?;
    let mut entities = Vec::with_capacity(span.order.len());
    for index in &span.order {
        let index = *index;
        let part = &assembly.parts[index];
        let parent = span.parent[index].map(|parent| part_uuid(body_code, parent)).transpose()?;
        let (translation, rotation) = locals[index];
        let translation = durable_vec(translation);
        let rotation = durable_quat(rotation);
        let scale = durable_vec(Vec3::new(part.local.scale[0], part.local.scale[1], part.local.scale[2]));
        let kind = if parent.is_none() { JointKind::Fixed } else { JointKind::Ball };
        let joint = durable_joint(JointRecord {
            kind,
            rest_translation: translation,
            rest_rotation: rotation,
            limits: JointLimits::unlocked(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
            stiffness: 0.0,
            damping: 0.0,
        });
        joint.validate().map_err(|error| LevelError::Corrupt(error.to_string()))?;
        let entity_name = if index == pelvis { name.to_string() } else { part.name.clone() };
        entities.push(EntityRecord {
            uuid: part_uuid(body_code, index)?,
            name: entity_name,
            parent_uuid: parent,
            components: vec![
                ComponentRecord::Transform { translation, rotation, scale },
                ComponentRecord::MeshRenderer {
                    visible: true,
                    cast_shadows: true,
                    receive_shadows: true,
                    mesh: MeshAssetRef::Asset { id: assets[index], name: part.name.clone() },
                    material: part_material(assembly),
                },
                ComponentRecord::Joint(joint),
            ],
        });
    }
    let document = CharacterDocument {
        format_version: CHARACTER_FORMAT_VERSION,
        character_uuid,
        name: name.to_string(),
        limits: CHARACTER_LIMITS_NOTE.into(),
        entities,
    };
    document.validate()?;
    let report = bind_report(name, assembly, &span, &locals);
    Ok(CharacterBuild { document, report })
}

struct Span {
    parent: Vec<Option<usize>>,
    order: Vec<usize>,
    depth: usize,
}

fn span_from_pelvis(parts: &[BindPart], pelvis: usize) -> Result<Span, LevelError> {
    let mut parent = vec![None; parts.len()];
    let mut seen = vec![false; parts.len()];
    let mut order = Vec::new();
    let mut queue = VecDeque::new();
    seen[pelvis] = true;
    queue.push_back(pelvis);
    while let Some(current) = queue.pop_front() {
        order.push(current);
        for other in 0..parts.len() {
            if seen[other] {
                continue;
            }
            if boxes_touch(parts[current].bounds_min, parts[current].bounds_max, parts[other].bounds_min, parts[other].bounds_max, BIND_POSE_CONNECT_M as f32) {
                seen[other] = true;
                parent[other] = Some(current);
                queue.push_back(other);
            }
        }
    }
    if seen.iter().any(|flag| !flag) {
        return Err(LevelError::Corrupt("a kept part is not connected to the pelvis within 0.02 m".into()));
    }
    let mut depth = 1usize;
    for index in 0..parts.len() {
        let mut cursor = Some(index);
        let mut chain = 0usize;
        while let Some(current) = cursor {
            chain += 1;
            if chain >= 28 {
                return Err(LevelError::Corrupt(format!("{} is too deep for the frame graph", parts[index].name)));
            }
            cursor = parent[current];
        }
        depth = depth.max(chain);
    }
    Ok(Span { parent, order, depth })
}

fn rest_local(parts: &[BindPart], parent: &[Option<usize>], index: usize) -> Result<(Vec3, Quat), LevelError> {
    let world_t = vec3(parts[index].socket_world);
    let world_r = unit_quat(parts[index].world.rotation).ok_or_else(|| LevelError::Corrupt(format!("{} rotation is not usable", parts[index].name)))?;
    let Some(parent_index) = parent[index] else {
        return Ok((world_t, world_r));
    };
    let parent_t = vec3(parts[parent_index].socket_world);
    let parent_r = unit_quat(parts[parent_index].world.rotation).ok_or_else(|| LevelError::Corrupt(format!("{} rotation is not usable", parts[parent_index].name)))?;
    let delta = Vec3::new(world_t.x - parent_t.x, world_t.y - parent_t.y, world_t.z - parent_t.z);
    Ok((parent_r.conjugate().rotate(delta), unit_quat_value(parent_r.conjugate().mul(world_r))))
}

fn verify_world_pose(parts: &[BindPart], parent: &[Option<usize>], locals: &[(Vec3, Quat)]) -> Result<(), LevelError> {
    for index in 0..parts.len() {
        let (translation, rotation) = composed_world(parent, locals, index);
        let source_t = vec3(parts[index].socket_world);
        let source_r = unit_quat(parts[index].world.rotation).ok_or_else(|| LevelError::Corrupt(format!("{} rotation is not usable", parts[index].name)))?;
        let delta = (translation.x - source_t.x).abs().max((translation.y - source_t.y).abs()).max((translation.z - source_t.z).abs());
        let dot = (rotation.x * source_r.x + rotation.y * source_r.y + rotation.z * source_r.z + rotation.w * source_r.w).abs();
        if delta > 1.0e-4 || dot < 1.0 - 1.0e-6 {
            return Err(LevelError::Corrupt(format!("{} world pose does not match the source", parts[index].name)));
        }
        if parts[index].local.scale.iter().any(|axis| !axis.is_finite() || *axis == 0.0) {
            return Err(LevelError::Corrupt(format!("{} scale is not usable", parts[index].name)));
        }
    }
    Ok(())
}

fn composed_world(parent: &[Option<usize>], locals: &[(Vec3, Quat)], index: usize) -> (Vec3, Quat) {
    let mut chain = Vec::new();
    let mut cursor = Some(index);
    while let Some(current) = cursor {
        chain.push(current);
        cursor = parent[current];
    }
    let mut translation = Vec3::ZERO;
    let mut rotation = Quat::IDENTITY;
    for current in chain.into_iter().rev() {
        let (local_t, local_r) = locals[current];
        translation = Vec3::new(
            translation.x + rotation.rotate(local_t).x,
            translation.y + rotation.rotate(local_t).y,
            translation.z + rotation.rotate(local_t).z,
        );
        rotation = rotation.mul(local_r);
    }
    (translation, rotation)
}

fn part_uuid(body: u8, index: usize) -> Result<EntityId, LevelError> {
    if index > 0xff {
        return Err(LevelError::Corrupt("a character has more than 256 parts".into()));
    }
    EntityId::parse(&format!("33333333-3333-4333-8333-33333333{body:02x}{index:02x}")).ok_or_else(|| LevelError::Corrupt("part uuid is invalid".into()))
}

fn part_material(assembly: &BindPoseAssembly) -> MaterialAssetRef {
    MaterialAssetRef {
        scheme: MaterialScheme::Mesh,
        name: "check".into(),
        base_color: assembly.base_color,
        metallic: assembly.metallic,
        roughness: assembly.roughness,
        emissive: [0.0, 0.0, 0.0, 0.0],
        uv_scale: 1.0,
        normal_scale: 1.0,
        normal_convention: None,
    }
}

fn bind_report(name: &str, assembly: &BindPoseAssembly, span: &Span, locals: &[(Vec3, Quat)]) -> String {
    let ratio = if assembly.height_m.abs() > 1.0e-8 { (assembly.pelvis_y - assembly.feet_y) / assembly.height_m } else { 0.0 };
    let mut lines = Vec::new();
    lines.push(format!("character: {name}"));
    lines.push(format!("schema: {CHARACTER_SCHEMA} {CHARACTER_FORMAT_VERSION}"));
    lines.push(format!("parts: {}", assembly.parts.len()));
    lines.push(format!("outliers: {}", assembly.outliers.len()));
    lines.push(format!("outline_triangles_excluded: {}", assembly.outline_triangles));
    lines.push(format!("feet_y: {:.6}", assembly.feet_y));
    lines.push(format!("pelvis: {} center_y={:.6} height_m={:.6} ratio={:.6}", assembly.pelvis_name, assembly.pelvis_y, assembly.height_m, ratio));
    lines.push(format!("bounds_m: width={:.6} height={:.6} depth={:.6}", assembly.width_m, assembly.height_m, assembly.depth_m));
    lines.push(format!("symmetry_max_m: {:.6}", assembly.symmetry_max_m));
    lines.push(format!("contact_tree_depth: {}", span.depth));
    lines.push("limits: wide-defaults. Not measured. Axis is +Y. Secondary axis is +X. Root is Fixed. Other parts are Ball.".into());
    lines.push("pivots: ball centers. The frame origin is that socket. Mesh vertices stay in the source world place. L is the side toward world -X.".into());
    lines.push("joint_parent: spanning tree of parts whose world boxes meet within 0.02 m, rooted at the hip. The source parent stays the glTF parent.".into());
    lines.push("document_space: source. A level may add one translation to the root. Part-to-part deltas stay the source deltas.".into());
    lines.push("collision: null".into());
    lines.push("animation_set: null".into());
    for (ordinal, index) in span.order.iter().enumerate() {
        let part = &assembly.parts[*index];
        let parent_name = span.parent[*index].map(|parent| assembly.parts[parent].name.as_str()).unwrap_or("(none)");
        let entity_name = if *index == span.order[0] { name } else { part.name.as_str() };
        let (nearest, gap) = nearest_part(&assembly.parts, *index);
        let (local_t, local_r) = locals[*index];
        lines.push(format!("part {ordinal}"));
        lines.push(format!("  source_name: {}", part.name));
        lines.push(format!("  entity: {entity_name}"));
        lines.push(format!("  source_parent: {}", part.source_parent));
        lines.push(format!("  joint_parent: {parent_name}"));
        lines.push(format!("  local: {}", transform_text(&part.local)));
        lines.push(format!("  world: {}", transform_text(&part.world)));
        lines.push(format!("  socket_world: ({:.8}, {:.8}, {:.8})", part.socket_world[0], part.socket_world[1], part.socket_world[2]));
        lines.push(format!("  joint_local: t=({:.8}, {:.8}, {:.8}) r=({:.8}, {:.8}, {:.8}, {:.8})", local_t.x, local_t.y, local_t.z, local_r.x, local_r.y, local_r.z, local_r.w));
        lines.push(format!(
            "  bounds: ({:.6}, {:.6}, {:.6}) ({:.6}, {:.6}, {:.6})",
            part.bounds_min[0], part.bounds_min[1], part.bounds_min[2], part.bounds_max[0], part.bounds_max[1], part.bounds_max[2]
        ));
        lines.push(format!("  triangles: {}", part.triangles));
        lines.push(format!("  neighbor: {nearest} gap_m={gap:.6}"));
        lines.push(format!("  outlier: {}", part.outlier));
    }
    for part in &assembly.outliers {
        let (nearest, gap) = nearest_named(&assembly.parts, part);
        lines.push(format!("outlier {}", part.name));
        lines.push(format!("  source_parent: {}", part.source_parent));
        lines.push(format!("  joint_parent: (none)"));
        lines.push(format!("  local: {}", transform_text(&part.local)));
        lines.push(format!("  world: {}", transform_text(&part.world)));
        lines.push(format!(
            "  bounds: ({:.6}, {:.6}, {:.6}) ({:.6}, {:.6}, {:.6})",
            part.bounds_min[0], part.bounds_min[1], part.bounds_min[2], part.bounds_max[0], part.bounds_max[1], part.bounds_max[2]
        ));
        lines.push(format!("  triangles: {}", part.triangles));
        lines.push(format!("  neighbor: {nearest} gap_m={gap:.6}"));
        lines.push("  outlier: true".into());
    }
    lines.join("\n")
}

fn nearest_part(parts: &[BindPart], index: usize) -> (String, f64) {
    let mut best_name = "(none)".to_string();
    let mut best = f64::MAX;
    for (other, part) in parts.iter().enumerate() {
        if other == index {
            continue;
        }
        let gap = box_gap(&parts[index], part);
        if gap < best {
            best = gap;
            best_name = part.name.clone();
        }
    }
    (best_name, best)
}

fn nearest_named(parts: &[BindPart], outlier: &BindPart) -> (String, f64) {
    let mut best_name = "(none)".to_string();
    let mut best = f64::MAX;
    for part in parts {
        let gap = box_gap(outlier, part);
        if gap < best {
            best = gap;
            best_name = part.name.clone();
        }
    }
    (best_name, best)
}

fn box_gap(left: &BindPart, right: &BindPart) -> f64 {
    let mut squared = 0.0;
    for axis in 0..3 {
        let separation = (left.bounds_min[axis] as f64 - right.bounds_max[axis] as f64).max(right.bounds_min[axis] as f64 - left.bounds_max[axis] as f64).max(0.0);
        squared += separation * separation;
    }
    squared.sqrt()
}

fn transform_text(value: &crate::RigidTransform) -> String {
    format!(
        "t=({:.8}, {:.8}, {:.8}) r=({:.8}, {:.8}, {:.8}, {:.8}) s=({:.8}, {:.8}, {:.8})",
        value.translation[0], value.translation[1], value.translation[2], value.rotation[0], value.rotation[1], value.rotation[2], value.rotation[3], value.scale[0], value.scale[1],
        value.scale[2]
    )
}

fn vec3(value: [f64; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], value[2])
}

fn durable_f64(value: f64) -> f64 {
    crate::json_lite::round_trip_number(value)
}

fn durable_vec(value: Vec3) -> Vec3 {
    Vec3::new(durable_f64(value.x), durable_f64(value.y), durable_f64(value.z))
}

fn durable_quat(value: Quat) -> Quat {
    Quat { x: durable_f64(value.x), y: durable_f64(value.y), z: durable_f64(value.z), w: durable_f64(value.w) }
}

fn durable_joint(mut joint: JointRecord) -> JointRecord {
    joint.rest_translation = durable_vec(joint.rest_translation);
    joint.rest_rotation = durable_quat(joint.rest_rotation);
    joint.limits.axis = durable_vec(joint.limits.axis);
    joint.limits.secondary_axis = durable_vec(joint.limits.secondary_axis);
    joint.limits.hinge_min = durable_f64(joint.limits.hinge_min);
    joint.limits.hinge_max = durable_f64(joint.limits.hinge_max);
    joint.limits.swing = durable_f64(joint.limits.swing);
    joint.limits.twist_min = durable_f64(joint.limits.twist_min);
    joint.limits.twist_max = durable_f64(joint.limits.twist_max);
    joint.limits.primary_min = durable_f64(joint.limits.primary_min);
    joint.limits.primary_max = durable_f64(joint.limits.primary_max);
    joint.limits.secondary_min = durable_f64(joint.limits.secondary_min);
    joint.limits.secondary_max = durable_f64(joint.limits.secondary_max);
    joint.limits.linear_min = durable_f64(joint.limits.linear_min);
    joint.limits.linear_max = durable_f64(joint.limits.linear_max);
    joint.stiffness = durable_f64(joint.stiffness);
    joint.damping = durable_f64(joint.damping);
    joint
}

fn unit_quat(value: [f64; 4]) -> Option<Quat> {
    let scale = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2] + value[3] * value[3]).sqrt();
    if !scale.is_finite() || scale < 1.0e-12 {
        None
    } else {
        Some(Quat { x: value[0] / scale, y: value[1] / scale, z: value[2] / scale, w: value[3] / scale })
    }
}

fn unit_quat_value(value: Quat) -> Quat {
    unit_quat([value.x, value.y, value.z, value.w]).unwrap_or(Quat::IDENTITY)
}

fn boxes_touch(a_min: [f32; 3], a_max: [f32; 3], b_min: [f32; 3], b_max: [f32; 3], gap: f32) -> bool {
    (0..3).all(|axis| a_min[axis] <= b_max[axis] + gap && b_min[axis] <= a_max[axis] + gap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RigidTransform, BIND_POSE_CONNECT_M};

    fn transform(translation: [f64; 3], rotation: [f64; 4], scale: [f64; 3]) -> RigidTransform {
        RigidTransform { translation, rotation, scale }
    }

    fn part(name: &str, translation: [f64; 3], rotation: [f64; 4], min: [f32; 3], max: [f32; 3]) -> BindPart {
        BindPart {
            name: name.into(),
            source_parent: "RootNode".into(),
            local: transform(translation, rotation, [2.0, 2.0, 2.0]),
            world: transform(translation, rotation, [2.0, 2.0, 2.0]),
            socket_world: translation,
            bounds_min: min,
            bounds_max: max,
            triangles: 12,
            neighbor_gap_m: 0.0,
            outlier: false,
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
        }
    }

    #[test]
    fn a_rotated_child_keeps_the_source_world_pose() {
        let yaw = std::f64::consts::FRAC_PI_2;
        let half = yaw * 0.5;
        let rotation = [0.0, half.sin(), 0.0, half.cos()];
        let hip = part("hip", [0.0, 0.9, 0.0], [0.0, 0.0, 0.0, 1.0], [-0.2, 0.5, -0.15], [0.2, 1.2, 0.15]);
        let arm = part("arm", [0.3, 0.9, 0.0], rotation, [0.15, 0.8, -0.1], [0.45, 1.1, 0.1]);
        let stray = part("stray", [4.0, 0.9, 0.0], [0.0, 0.0, 0.0, 1.0], [3.8, 0.7, -0.1], [4.2, 1.1, 0.1]);
        let mut stray = stray;
        stray.outlier = true;
        stray.neighbor_gap_m = 3.0;
        let assembly = BindPoseAssembly {
            parts: vec![arm, hip],
            outliers: vec![stray],
            outline_triangles: 9,
            feet_y: 0.5,
            pelvis_name: "hip".into(),
            pelvis_y: 0.85,
            height_m: 1.6,
            width_m: 0.65,
            depth_m: 0.3,
            symmetry_max_m: 0.0,
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            roughness: 1.0,
        };
        let assets = [
            AssetId(EntityId::parse("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1").unwrap()),
            AssetId(EntityId::parse("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2").unwrap()),
        ];
        let character_uuid = EntityId::parse("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaa10").unwrap();
        let build = character_from_bind_pose("Fixture", character_uuid, 0x11, &assembly, &assets).expect("character");
        let parsed = parse_character(&build.document.to_json()).expect("parse");
        assert_eq!(parsed.entities.len(), 2);
        assert_eq!(parsed, build.document);
        let root = parsed.entities.iter().find(|entity| entity.parent_uuid.is_none()).unwrap();
        assert_eq!(root.name, "Fixture");
        let child = parsed.entities.iter().find(|entity| entity.name == "arm").unwrap();
        let ComponentRecord::Transform { translation, rotation: child_rotation, .. } = child.components[0] else { panic!("transform") };
        assert!((translation.x - 0.3).abs() < 1.0e-6, "{}", translation.x);
        assert!(translation.y.abs() < 1.0e-6 && translation.z.abs() < 1.0e-6);
        let dot = (child_rotation.y * rotation[1] + child_rotation.w * rotation[3]).abs();
        assert!(dot > 1.0 - 1.0e-6, "{dot}");
        assert!(matches!(root.components.last(), Some(ComponentRecord::Joint(joint)) if joint.kind == JointKind::Fixed));
        assert!(matches!(child.components.last(), Some(ComponentRecord::Joint(joint)) if joint.kind == JointKind::Ball));
        assert!(build.report.contains("outlier: false"));
        assert!(build.report.contains("outlier stray"));
        assert!(build.report.contains("outlier: true"));
        assert!(build.report.contains("wide-defaults"));
        let colliding = build.document.entities[0].uuid;
        assert!(parsed.open_as_level(EntityId::parse("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb1").unwrap(), colliding).is_err());
        let level = parsed
            .open_as_level(EntityId::parse("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb1").unwrap(), EntityId::parse("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbf1").unwrap())
            .expect("level");
        assert_eq!(level.format_version, LEVEL_JOINT_VERSION);
        assert_eq!(level.entities.len(), 3);
        let _ = BIND_POSE_CONNECT_M;
    }
}
