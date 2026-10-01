//! Authored actor template. Not a level, not a runtime instance, and not a GPU resource.
//!
//! Schema `jarvig.prefab` format version 1. Entity records use the same components as
//! `jarvig.level`. The file keeps its uuids. Placing a prefab assigns new uuids.

use crate::json_lite::{parse_json, Json};
use crate::level::{parse_entity, spawn_entity, EntityRecord, LevelError, WorldSettingsRecord};
use crate::{EntityId, JointKind, JointLimits, JointRecord, MaterialAssetRef, MeshAssetRef, Quat, SceneWorld, Vec3};
use crate::level::{ComponentRecord, LevelDocument};

pub const PREFAB_SCHEMA: &str = "jarvig.prefab";
pub const PREFAB_FORMAT_VERSION: u32 = 1;

pub const MANNEQUIN_PREFAB_UUID: &str = "11111111-1111-4111-8111-111111111100";
pub const MANNEQUIN_LEVEL_UUID: &str = "11111111-1111-4111-8111-1111111111ee";
pub const MANNEQUIN_SETTINGS_UUID: &str = "11111111-1111-4111-8111-1111111111ff";

#[derive(Clone, Debug, PartialEq)]
pub struct PrefabDocument {
    pub format_version: u32,
    pub prefab_uuid: EntityId,
    pub name: String,
    pub entities: Vec<EntityRecord>,
}

impl PrefabDocument {
    pub fn validate(&self) -> Result<(), LevelError> {
        if self.format_version != PREFAB_FORMAT_VERSION {
            return Err(LevelError::UnsupportedVersion(self.format_version));
        }
        if self.name.is_empty() || !self.prefab_uuid.is_persistent() {
            return Err(LevelError::Corrupt("prefab name or uuid is invalid".into()));
        }
        if self.entities.is_empty() {
            return Err(LevelError::Corrupt("prefab has no entities".into()));
        }
        let mut seen = std::collections::HashSet::new();
        for entity in &self.entities {
            if !entity.uuid.is_persistent() || !seen.insert(entity.uuid) || entity.name.is_empty() {
                return Err(LevelError::Corrupt(format!("prefab entity {} is invalid", entity.name)));
            }
            if entity.components.iter().any(|component| matches!(component, ComponentRecord::WorldSettings)) {
                return Err(LevelError::Corrupt("a prefab cannot contain World Settings".into()));
            }
            if !entity.components.iter().any(|component| matches!(component, ComponentRecord::Transform { .. })) {
                return Err(LevelError::Corrupt(format!("{} has no transform", entity.name)));
            }
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
            ("schema", Json::string(PREFAB_SCHEMA)),
            ("format_version", Json::int(self.format_version as i64)),
            ("prefab_uuid", Json::string(self.prefab_uuid.to_string())),
            ("name", Json::string(&self.name)),
            ("entities", Json::array(self.entities.iter().map(EntityRecord::to_json).collect())),
        ])
        .write()
    }

    /// New uuids. Root translations shift by `origin`. A second call does not collide.
    pub fn instantiate(&self, world: &mut SceneWorld, origin: Vec3) -> Result<Vec<EntityId>, LevelError> {
        self.instantiate_with(world, origin, false)
    }

    /// Keeps the file uuids. Used when a sample level is the prefab placed once.
    pub fn instantiate_preserving_ids(&self, world: &mut SceneWorld, origin: Vec3) -> Result<Vec<EntityId>, LevelError> {
        self.instantiate_with(world, origin, true)
    }

    fn instantiate_with(&self, world: &mut SceneWorld, origin: Vec3, preserve_ids: bool) -> Result<Vec<EntityId>, LevelError> {
        self.validate()?;
        let mut remaining: Vec<&EntityRecord> = self.entities.iter().collect();
        let mut map: Vec<(EntityId, EntityId)> = Vec::new();
        let mut created = Vec::new();
        let settings = dummy_settings();
        let mut guard = remaining.len() + 1;
        while !remaining.is_empty() {
            guard -= 1;
            if guard == 0 {
                return Err(LevelError::Cycle("prefab entity order did not resolve".into()));
            }
            let ready = remaining.iter().position(|entity| match entity.parent_uuid {
                None => true,
                Some(parent) => map.iter().any(|(source, _)| *source == parent),
            });
            let Some(index) = ready else {
                return Err(LevelError::InvalidParent("a prefab parent was not created".into()));
            };
            let source = remaining.remove(index);
            let uuid = if preserve_ids { source.uuid } else { EntityId::new() };
            let parent_uuid = source.parent_uuid.map(|parent| map.iter().find(|(stored, _)| *stored == parent).map(|(_, created)| *created).expect("parent mapped"));
            let mut entity = source.clone();
            entity.uuid = uuid;
            entity.parent_uuid = parent_uuid;
            if parent_uuid.is_none() {
                for component in &mut entity.components {
                    if let ComponentRecord::Transform { translation, .. } = component {
                        *translation = Vec3::new(translation.x + origin.x, translation.y + origin.y, translation.z + origin.z);
                    }
                }
            }
            spawn_entity(world, &entity, &settings)?;
            map.push((source.uuid, uuid));
            created.push(uuid);
        }
        Ok(created)
    }
}

pub fn parse_prefab(text: &str) -> Result<PrefabDocument, LevelError> {
    let json = parse_json(text)?;
    let schema = json.get("schema").and_then(Json::as_str).ok_or_else(|| LevelError::Corrupt("prefab schema is missing".into()))?;
    if schema != PREFAB_SCHEMA {
        return Err(LevelError::Corrupt(format!("schema {schema} is not {PREFAB_SCHEMA}")));
    }
    let format_version = json.get("format_version").and_then(Json::as_f64).ok_or_else(|| LevelError::Corrupt("prefab format version is missing".into()))?;
    if format_version.fract() != 0.0 || format_version <= 0.0 {
        return Err(LevelError::Corrupt("prefab format version is not an integer".into()));
    }
    let format_version = format_version as u32;
    let prefab_uuid = EntityId::parse(json.get("prefab_uuid").and_then(Json::as_str).unwrap_or("")).ok_or_else(|| LevelError::Corrupt("prefab uuid is invalid".into()))?;
    let name = json.get("name").and_then(Json::as_str).unwrap_or("").to_string();
    let entities_json = json.get("entities").and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt("prefab entities are missing".into()))?;
    let mut entities = Vec::new();
    for entity in entities_json {
        entities.push(parse_entity(entity)?);
    }
    let document = PrefabDocument { format_version, prefab_uuid, name, entities };
    document.validate()?;
    Ok(document)
}

/// JARVIG-owned primitive humanoid. Boxes, capsules, and spheres. Not an imported skeleton.
pub fn primitive_mannequin_prefab() -> PrefabDocument {
    let entities = mannequin_entities();
    PrefabDocument {
        format_version: PREFAB_FORMAT_VERSION,
        prefab_uuid: EntityId::parse(MANNEQUIN_PREFAB_UUID).expect("mannequin prefab uuid"),
        name: "Primitive Mannequin".into(),
        entities,
    }
}

/// One placement of the mannequin plus World Settings. Same bodies as the prefab, same uuids.
pub fn primitive_mannequin_level() -> LevelDocument {
    let mut world = SceneWorld::new_session();
    let settings = EntityId::parse(MANNEQUIN_SETTINGS_UUID).expect("settings uuid");
    world.spawn_saved_world_settings(settings, "World Settings", crate::EnvironmentLight::bootstrap()).expect("settings");
    primitive_mannequin_prefab().instantiate_preserving_ids(&mut world, Vec3::ZERO).expect("mannequin");
    LevelDocument::capture(&world, EntityId::parse(MANNEQUIN_LEVEL_UUID).expect("level uuid"), "Primitive Mannequin").expect("capture")
}

fn mannequin_entities() -> Vec<EntityRecord> {
    let bones = mannequin_bones();
    bones
        .iter()
        .enumerate()
        .map(|(index, bone)| {
            let parent_uuid = bone.parent.map(|parent| bone_id(parent));
            let translation = Vec3::new(bone.local[0], bone.local[1], bone.local[2]);
            let scale = Vec3::new(bone.scale[0], bone.scale[1], bone.scale[2]);
            let mut limits = JointLimits::unlocked(Vec3::new(bone.axis[0], bone.axis[1], bone.axis[2]), Vec3::new(bone.secondary[0], bone.secondary[1], bone.secondary[2]));
            limits.hinge_min = bone.hinge[0].to_radians();
            limits.hinge_max = bone.hinge[1].to_radians();
            limits.swing = bone.swing_deg.to_radians();
            limits.twist_min = bone.twist[0].to_radians();
            limits.twist_max = bone.twist[1].to_radians();
            limits.primary_min = bone.primary[0].to_radians();
            limits.primary_max = bone.primary[1].to_radians();
            limits.secondary_min = bone.secondary_limit[0].to_radians();
            limits.secondary_max = bone.secondary_limit[1].to_radians();
            let joint = JointRecord {
                kind: bone.kind,
                rest_translation: translation,
                rest_rotation: Quat::IDENTITY,
                limits,
                stiffness: 0.0,
                damping: 0.0,
            };
            EntityRecord {
                uuid: bone_id(index),
                name: bone.name.into(),
                parent_uuid,
                components: vec![
                    ComponentRecord::Transform { translation, rotation: Quat::IDENTITY, scale },
                    ComponentRecord::MeshRenderer {
                        visible: true,
                        cast_shadows: true,
                        receive_shadows: true,
                        mesh: bone.mesh.clone(),
                        material: skin_material(),
                    },
                    ComponentRecord::Joint(joint),
                ],
            }
        })
        .collect()
}

struct Bone {
    name: &'static str,
    parent: Option<usize>,
    local: [f64; 3],
    scale: [f64; 3],
    kind: JointKind,
    axis: [f64; 3],
    secondary: [f64; 3],
    hinge: [f64; 2],
    swing_deg: f64,
    twist: [f64; 2],
    primary: [f64; 2],
    secondary_limit: [f64; 2],
    mesh: MeshAssetRef,
}

fn mannequin_bones() -> Vec<Bone> {
    let y = [0.0, 1.0, 0.0];
    let x = [1.0, 0.0, 0.0];
    let z = [0.0, 0.0, 1.0];
    let down = [0.0, -1.0, 0.0];
    let left = [1.0, 0.0, 0.0];
    let right = [-1.0, 0.0, 0.0];
    vec![
        bone("Pelvis", None, [0.0, 0.95, 0.0], [0.26, 0.14, 0.16], JointKind::Fixed, y, x, cube()),
        bone("Spine", Some(0), [0.0, 0.12, 0.0], [1.0, 1.0, 1.0], JointKind::Ball, y, x, capsule(0.07, 0.16)).limited_ball(12.0, 8.0),
        bone("Chest", Some(1), [0.0, 0.22, 0.0], [0.32, 0.20, 0.16], JointKind::Ball, y, x, cube()).limited_ball(18.0, 12.0),
        bone("Neck", Some(2), [0.0, 0.18, 0.0], [1.0, 1.0, 1.0], JointKind::Ball, y, x, capsule(0.045, 0.06)).limited_ball(35.0, 25.0),
        bone("Head", Some(3), [0.0, 0.12, 0.0], [1.0, 1.0, 1.0], JointKind::Fixed, y, x, sphere(0.11)),
        bone("Shoulder.L", Some(2), [0.16, 0.10, 0.0], [1.0, 1.0, 1.0], JointKind::Ball, left, y, sphere(0.055)).limited_ball(80.0, 70.0),
        bone("UpperArm.L", Some(5), [0.12, 0.0, 0.0], [1.0, 1.0, 1.0], JointKind::Fixed, left, y, capsule(0.045, 0.18)),
        bone("Forearm.L", Some(6), [0.24, 0.0, 0.0], [1.0, 1.0, 1.0], JointKind::Hinge, z, y, capsule(0.04, 0.18)).hinge(0.0, 140.0),
        bone("Hand.L", Some(7), [0.22, 0.0, 0.0], [1.0, 1.0, 1.0], JointKind::Universal, z, y, sphere(0.04)).universal(35.0, 25.0),
        bone("Shoulder.R", Some(2), [-0.16, 0.10, 0.0], [1.0, 1.0, 1.0], JointKind::Ball, right, y, sphere(0.055)).limited_ball(80.0, 70.0),
        bone("UpperArm.R", Some(9), [-0.12, 0.0, 0.0], [1.0, 1.0, 1.0], JointKind::Fixed, right, y, capsule(0.045, 0.18)),
        bone("Forearm.R", Some(10), [-0.24, 0.0, 0.0], [1.0, 1.0, 1.0], JointKind::Hinge, z, y, capsule(0.04, 0.18)).hinge(0.0, 140.0),
        bone("Hand.R", Some(11), [-0.22, 0.0, 0.0], [1.0, 1.0, 1.0], JointKind::Universal, z, y, sphere(0.04)).universal(35.0, 25.0),
        bone("Hip.L", Some(0), [0.08, -0.06, 0.0], [1.0, 1.0, 1.0], JointKind::Ball, down, x, sphere(0.06)).limited_ball(70.0, 40.0),
        bone("Thigh.L", Some(13), [0.0, -0.08, 0.0], [1.0, 1.0, 1.0], JointKind::Fixed, down, x, capsule(0.06, 0.32)),
        bone("Shin.L", Some(14), [0.0, -0.40, 0.0], [1.0, 1.0, 1.0], JointKind::Hinge, x, z, capsule(0.05, 0.32)).hinge(0.0, 140.0),
        bone("Foot.L", Some(15), [0.0, -0.38, 0.04], [0.08, 0.05, 0.18], JointKind::Universal, x, z, cube()).universal(30.0, 20.0),
        bone("Hip.R", Some(0), [-0.08, -0.06, 0.0], [1.0, 1.0, 1.0], JointKind::Ball, down, x, sphere(0.06)).limited_ball(70.0, 40.0),
        bone("Thigh.R", Some(17), [0.0, -0.08, 0.0], [1.0, 1.0, 1.0], JointKind::Fixed, down, x, capsule(0.06, 0.32)),
        bone("Shin.R", Some(18), [0.0, -0.40, 0.0], [1.0, 1.0, 1.0], JointKind::Hinge, x, z, capsule(0.05, 0.32)).hinge(0.0, 140.0),
        bone("Foot.R", Some(19), [0.0, -0.38, 0.04], [0.08, 0.05, 0.18], JointKind::Universal, x, z, cube()).universal(30.0, 20.0),
    ]
}

fn bone(name: &'static str, parent: Option<usize>, local: [f64; 3], scale: [f64; 3], kind: JointKind, axis: [f64; 3], secondary: [f64; 3], mesh: MeshAssetRef) -> Bone {
    Bone {
        name,
        parent,
        local,
        scale,
        kind,
        axis,
        secondary,
        hinge: [-180.0, 180.0],
        swing_deg: 180.0,
        twist: [-180.0, 180.0],
        primary: [-180.0, 180.0],
        secondary_limit: [-180.0, 180.0],
        mesh,
    }
}

impl Bone {
    fn limited_ball(mut self, swing_deg: f64, twist_deg: f64) -> Self {
        self.swing_deg = swing_deg;
        self.twist = [-twist_deg, twist_deg];
        self
    }

    fn hinge(mut self, min_deg: f64, max_deg: f64) -> Self {
        self.hinge = [min_deg, max_deg];
        self
    }

    fn universal(mut self, primary_deg: f64, secondary_deg: f64) -> Self {
        self.primary = [-primary_deg, primary_deg];
        self.secondary_limit = [-secondary_deg, secondary_deg];
        self
    }
}

fn cube() -> MeshAssetRef {
    MeshAssetRef::Cube { size_m: 1.0 }
}

fn sphere(radius_m: f64) -> MeshAssetRef {
    MeshAssetRef::Sphere { radius_m, segments: 12, rings: 8, flat: false }
}

fn capsule(radius_m: f64, height_m: f64) -> MeshAssetRef {
    MeshAssetRef::Capsule { radius_m, height_m }
}

fn skin_material() -> MaterialAssetRef {
    MaterialAssetRef::builtin("standard_white", [0.72, 0.67, 0.60, 1.0], 0.0, 0.65, [0.0, 0.0, 0.0, 0.0])
}

fn bone_id(index: usize) -> EntityId {
    EntityId::parse(&format!("11111111-1111-4111-8111-1111111111{index:02x}")).expect("bone uuid")
}

fn dummy_settings() -> WorldSettingsRecord {
    WorldSettingsRecord {
        entity: EntityId::parse(MANNEQUIN_SETTINGS_UUID).expect("settings"),
        enabled: true,
        intensity: 0.2,
        upper: [1.0, 1.0, 1.0],
        lower: [0.0, 0.0, 0.0],
        probe_update_policy: crate::ProbeUpdatePolicy::Static,
        startup_camera: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthoringError, HighPrecisionPose, Quat, Vec3, LEVEL_JOINT_VERSION};
    use std::fs;
    use std::path::PathBuf;

    fn sample_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/primitive-mannequin")
    }

    fn mannequin_primitive_meshes_build() {
        let _ = crate::cube_mesh(1.0);
        let _ = crate::sphere_mesh(0.1, 8, 6);
        let _ = crate::capsule_mesh(0.05, 0.2);
    }

    #[test]
    fn mannequin_prefab_round_trips_and_places_twice() {
        let prefab = primitive_mannequin_prefab();
        let parsed = parse_prefab(&prefab.to_json()).unwrap();
        assert_eq!(parsed, prefab);
        assert_eq!(prefab.entities.len(), 21);
        let names: Vec<_> = prefab.entities.iter().map(|entity| entity.name.as_str()).collect();
        for name in ["Pelvis", "Spine", "Chest", "Neck", "Head", "Shoulder.L", "Forearm.L", "Hand.L", "Hip.R", "Shin.R", "Foot.R"] {
            assert!(names.contains(&name), "{name}");
        }
        assert!(names.iter().all(|name| !name.to_ascii_lowercase().contains("finger")));
        let mut world = SceneWorld::new_session();
        let settings = EntityId::parse(MANNEQUIN_SETTINGS_UUID).unwrap();
        world.spawn_saved_world_settings(settings, "World Settings", crate::EnvironmentLight::bootstrap()).unwrap();
        let stranger = world.spawn_saved_mesh(
            EntityId::new(),
            "Stranger",
            None,
            HighPrecisionPose::at(3.0, 0.0, -2.0),
            Vec3::new(1.0, 1.0, 1.0),
            true,
            true,
            true,
            MeshAssetRef::Cube { size_m: 0.4 },
            skin_material(),
        );
        assert!(stranger.is_ok());
        let stranger_id = world.entity_outline().iter().find(|row| row.name == "Stranger").unwrap().uuid;
        let before = world.authored_local_pose(stranger_id).unwrap();
        let first = prefab.instantiate(&mut world, Vec3::ZERO).unwrap();
        let second = prefab.instantiate(&mut world, Vec3::new(2.0, 0.0, 0.0)).unwrap();
        assert_eq!(first.len(), 21);
        assert_eq!(second.len(), 21);
        assert!(first.iter().all(|id| !second.contains(id)));
        let elbow = world.entity_outline().iter().find(|row| row.name == "Forearm.L" && first.contains(&row.uuid)).unwrap().uuid;
        let hand = world.entity_outline().iter().find(|row| row.name == "Hand.L" && row.parent == Some(elbow)).unwrap().uuid;
        let pelvis = world.entity_outline().iter().find(|row| row.name == "Pelvis" && first.contains(&row.uuid)).unwrap().uuid;
        assert_eq!(world.entity_parent(elbow).unwrap().is_some(), true);
        let hand_before = world.entity_world_pose(hand).unwrap();
        let bent = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), 200.0_f64.to_radians()).unwrap();
        world.set_entity_local_rotation(elbow, bent).unwrap();
        let elbow_pose = world.authored_local_pose(elbow).unwrap();
        let clamped = elbow_pose.1;
        let again = world.set_entity_local_rotation(elbow, clamped);
        assert_eq!(again, Ok(crate::AuthoringResult::Unchanged));
        let relative = clamped;
        let axis = Vec3::new(0.0, 0.0, 1.0);
        let angle = {
            let along = relative.x * axis.x + relative.y * axis.y + relative.z * axis.z;
            2.0 * along.atan2(relative.w)
        };
        assert!((angle.to_degrees() - 140.0).abs() < 1.0e-4, "{}", angle.to_degrees());
        let hand_after = world.entity_world_pose(hand).unwrap();
        let moved = (hand_after.translation.x - hand_before.translation.x).abs()
            + (hand_after.translation.y - hand_before.translation.y).abs()
            + (hand_after.translation.z - hand_before.translation.z).abs();
        assert!(moved > 0.01, "{moved}");
        let shoulder = world.entity_outline().iter().find(|row| row.name == "Shoulder.L" && first.contains(&row.uuid)).unwrap().uuid;
        let upper_before = world.entity_world_pose(world.entity_outline().iter().find(|row| row.name == "UpperArm.L" && row.parent == Some(shoulder)).unwrap().uuid).unwrap();
        world.set_entity_local_rotation(shoulder, Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), 20.0_f64.to_radians()).unwrap()).unwrap();
        let upper_after = world.entity_world_pose(world.entity_outline().iter().find(|row| row.name == "UpperArm.L" && row.parent == Some(shoulder)).unwrap().uuid).unwrap();
        assert!(
            (upper_after.translation.x - upper_before.translation.x).abs()
                + (upper_after.translation.y - upper_before.translation.y).abs()
                + (upper_after.translation.z - upper_before.translation.z).abs()
                > 0.005
        );
        assert_eq!(world.authored_local_pose(stranger_id).unwrap(), before);
        assert_eq!(world.entity_parent(pelvis).unwrap(), None);
        let level = LevelDocument::capture(&world, EntityId::parse(MANNEQUIN_LEVEL_UUID).unwrap(), "Placed").unwrap();
        assert_eq!(level.format_version, LEVEL_JOINT_VERSION);
        let loaded = level.instantiate().unwrap();
        let loaded_elbow = loaded.entity_outline().into_iter().find(|row| row.uuid == elbow).unwrap();
        assert_eq!(loaded_elbow.name, "Forearm.L");
        let loaded_pose = loaded.authored_local_pose(elbow).unwrap();
        assert_eq!(loaded_pose.1, clamped);
        assert_eq!(loaded.authored_local_pose(stranger_id).unwrap(), before);
        let knee = world.entity_outline().iter().find(|row| row.name == "Shin.L" && first.contains(&row.uuid)).unwrap().uuid;
        world.set_entity_local_rotation(knee, Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 3.0).unwrap()).unwrap();
        let knee_pose = world.authored_local_pose(knee).unwrap().1;
        let knee_angle = {
            let axis = Vec3::new(1.0, 0.0, 0.0);
            let along = knee_pose.x * axis.x + knee_pose.y * axis.y + knee_pose.z * axis.z;
            2.0 * along.atan2(knee_pose.w)
        };
        assert!(knee_angle <= 140.0_f64.to_radians() + 1.0e-6);
        let _ = AuthoringError::InvalidValue;
    }

    #[test]
    fn mannequin_files_match_the_builder() {
        let root = sample_root();
        let prefab_path = root.join("Content/Prefabs/Mannequin.jarvigprefab");
        let level_path = root.join("Content/Levels/PrimitiveMannequin.jarviglevel");
        let project_path = root.join("PrimitiveMannequin.jarvigproject");
        let settings_path = root.join("Config/Project.jarvigsettings");
        let prefab = primitive_mannequin_prefab().to_json();
        let level = primitive_mannequin_level().to_json();
        if std::env::var("JARVIG_WRITE_MANNEQUIN").ok().as_deref() == Some("1") || !prefab_path.exists() {
            fs::create_dir_all(prefab_path.parent().unwrap()).unwrap();
            fs::create_dir_all(level_path.parent().unwrap()).unwrap();
            fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
            fs::write(&prefab_path, &prefab).unwrap();
            fs::write(&level_path, &level).unwrap();
            fs::write(
                &project_path,
                "{\n  \"schema\": \"jarvig.project\",\n  \"format_version\": 1,\n  \"project_uuid\": \"11111111-1111-4111-8111-1111111111dd\",\n  \"display_name\": \"Primitive Mannequin\",\n  \"engine_version\": \"0.0.1\",\n  \"startup_level\": \"Content/Levels/PrimitiveMannequin.jarviglevel\",\n  \"content_directory\": \"Content\",\n  \"saved_directory\": \"Saved\",\n  \"config_directory\": \"Config\",\n  \"intermediate_directory\": \"Intermediate\",\n  \"settings\": \"Config/Project.jarvigsettings\"\n}\n",
            )
            .unwrap();
            fs::write(
                &settings_path,
                "{\n  \"schema\": \"jarvig.settings\",\n  \"format_version\": 1,\n  \"default_player_controller\": \"JARVIG.PlayerController\",\n  \"default_pawn\": \"JARVIG.DefaultFreeFlyPawn\",\n  \"default_mapping_context\": \"JARVIG.Default\",\n  \"startup_camera\": \"pawn\"\n}\n",
            )
            .unwrap();
        }
        assert_eq!(fs::read_to_string(&prefab_path).unwrap(), prefab);
        assert_eq!(fs::read_to_string(&level_path).unwrap(), level);
        mannequin_primitive_meshes_build();
    }

    #[test]
    fn editing_one_joint_does_not_change_another() {
        let mut world = SceneWorld::new_session();
        world.spawn_saved_world_settings(EntityId::parse(MANNEQUIN_SETTINGS_UUID).unwrap(), "World Settings", crate::EnvironmentLight::bootstrap()).unwrap();
        primitive_mannequin_prefab().instantiate_preserving_ids(&mut world, Vec3::ZERO).unwrap();
        let head = world.entity_outline().iter().find(|row| row.name == "Head").unwrap().uuid;
        let knee = world.entity_outline().iter().find(|row| row.name == "Shin.L").unwrap().uuid;
        let head_before = world.authored_local_pose(head).unwrap();
        world.set_entity_local_rotation(knee, Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 1.0).unwrap()).unwrap();
        assert_eq!(world.authored_local_pose(head).unwrap(), head_before);
        let lines = world.joint_debug_segments(Some(knee));
        assert!(lines.iter().any(|line| line.limits));
        assert!(world.joint_debug_segments(None).iter().all(|line| !line.limits));
    }
}
