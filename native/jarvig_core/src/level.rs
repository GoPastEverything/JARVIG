//! Authored level document. This is the save file, not a render snapshot and not a GPU resource.
//!
//! Schema `jarvig.level` format version 1. A later asset database replaces `jarvig.builtin`
//! names with stable AssetIds. The loader must not invent those ids. MeshId, MaterialInstanceId,
//! EntityHandle, ObjectId, LightId, ProbeId, FrameId, and every GPU handle stay out of the file.

use std::collections::HashSet;
use std::fmt;

use crate::json_lite::{parse_json, Json, JsonError};
use crate::{
    cube_mesh, emissive_panel_mesh, far_triangle_mesh, flat_sphere_mesh, floor_mesh, near_triangle_mesh, sphere_mesh, AuthoringError,
    EntityId, EnvironmentLight, HighPrecisionPose, LightKind, LightShadowSettings, ProbeUpdatePolicy, Quat, SceneWorld, Vec3,
    BOOTSTRAP_ENVIRONMENT_INTENSITY, BOOTSTRAP_LOWER_HEMISPHERE_LINEAR, BOOTSTRAP_PROBE_INTENSITY, BOOTSTRAP_PROBE_LOCAL_M,
    BOOTSTRAP_PROBE_PRIORITY, BOOTSTRAP_PROBE_RADIUS_M, BOOTSTRAP_UPPER_HEMISPHERE_LINEAR,
};

pub const LEVEL_SCHEMA: &str = "jarvig.level";
pub const LEVEL_FORMAT_VERSION: u32 = 1;
/// Cameras are version 2. Version 1 files still load. A save writes 2 only when a camera is present.
pub const LEVEL_CAMERA_VERSION: u32 = 2;
/// Joints are version 3. Version 1 and 2 files still load. A save writes 3 only when a joint is present.
pub const LEVEL_JOINT_VERSION: u32 = 3;
/// Terrain is version 4. Versions 1–3 still load. A save writes 4 only when a terrain component is present.
pub const LEVEL_TERRAIN_VERSION: u32 = 4;
/// A Player Start is version 5. Versions 1–4 still load. A save writes 5 only when a Player Start is present.
pub const LEVEL_PLAYER_START_VERSION: u32 = 5;
/// A parametric block is version 6. Versions 1–5 still load. A save writes 6 only when a block is present.
pub const LEVEL_BLOCK_VERSION: u32 = 6;

/// Fixed identity for the regression level. Not a runtime slot.
pub const LIGHTING_LAB_LEVEL_UUID: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

#[derive(Clone, Debug, PartialEq)]
pub struct LevelDocument {
    pub format_version: u32,
    pub level_uuid: EntityId,
    pub name: String,
    pub world_settings: WorldSettingsRecord,
    pub entities: Vec<EntityRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldSettingsRecord {
    pub entity: EntityId,
    pub enabled: bool,
    pub intensity: f32,
    pub upper: [f32; 3],
    pub lower: [f32; 3],
    pub probe_update_policy: ProbeUpdatePolicy,
    /// Explicit runtime camera. Absent from version-1 files. Never "the first Camera."
    pub startup_camera: Option<EntityId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityRecord {
    pub uuid: EntityId,
    pub name: String,
    pub parent_uuid: Option<EntityId>,
    pub components: Vec<ComponentRecord>,
}

/// One entity in an editor transaction. This is not a level file and not a selection.
///
/// `Absent` means the entity did not exist. `Present` is the same record a save would write.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityMemento {
    Absent(EntityId),
    Present(EntityRecord),
}

impl EntityMemento {
    pub fn id(&self) -> EntityId {
        match self {
            Self::Absent(id) => *id,
            Self::Present(record) => record.uuid,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComponentRecord {
    Transform { translation: Vec3, rotation: Quat, scale: Vec3 },
    MeshRenderer { visible: bool, cast_shadows: bool, receive_shadows: bool, mesh: MeshAssetRef, material: MaterialAssetRef },
    DirectionalLight(LightRecord),
    PointLight(LightRecord),
    SpotLight(LightRecord),
    ReflectionProbe(ProbeRecord),
    Camera(CameraRecord),
    Joint(crate::joint::JointRecord),
    /// Authoritative heightfield. Chunk meshes are rebuilt on load and are not stored.
    Terrain(crate::TerrainRecord),
    /// Where a player definition enters the level. The character body is not this component.
    PlayerStart(crate::PlayerStartRecord),
    /// Canonical solid. The triangle mesh is derived at load and is not stored.
    ParametricBlock(crate::BlockRecord),
    WorldSettings,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraRecord {
    pub enabled: bool,
    pub orthographic: bool,
    pub vertical_fov_deg: f64,
    pub ortho_height_m: f64,
    pub near_m: f32,
    pub far_m: f32,
    pub priority: i32,
    pub viewport: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightRecord {
    pub enabled: bool,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range_m: f32,
    pub inner_radians: f32,
    pub outer_radians: f32,
    pub shadow: LightShadowSettings,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProbeRecord {
    pub enabled: bool,
    pub radius_m: f64,
    pub intensity: f32,
    pub priority: i32,
    pub resolution: u32,
}

/// Authored mesh reference. A builtin is an engine mesh. `Asset` is an `AssetId`, not a path and not a [`crate::MeshId`].
#[derive(Clone, Debug, PartialEq)]
pub enum MeshAssetRef {
    NearTriangle,
    FarTriangle,
    Floor { width_m: f64, depth_m: f64 },
    Cube { size_m: f64 },
    Sphere { radius_m: f64, segments: u32, rings: u32, flat: bool },
    /// Straight section along Y, plus a hemisphere of `radius_m` at each end.
    Capsule { radius_m: f64, height_m: f64 },
    EmissivePanel { width_m: f64, height_m: f64 },
    /// A project mesh asset. The id is the reference. The name is a label.
    Asset { id: crate::AssetId, name: String },
}

/// How a level names a material. Not a GPU instance and not PNG bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MaterialScheme {
    /// `standard_white`, `bootstrap_near`, `bootstrap_far`.
    #[default]
    Builtin,
    /// A staging set name such as `Tiles101`. Resolved from the material root, not from `jarvig.asset`.
    Staged,
    /// The mesh asset's own glTF material. The name is a label. The images stay with the asset.
    Mesh,
}

/// Temporary material identity until AssetIds exist. Not a [`crate::MaterialInstanceId`].
///
/// Builtin `name` selects a texture set. Factors are the overrides. `scheme: jarvig.asset` stays
/// rejected. `jarvig.material` is a set name the editor resolves. The level does not store pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialAssetRef {
    pub scheme: MaterialScheme,
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 4],
    pub uv_scale: f32,
    pub normal_scale: f32,
    /// Present when a staged set chose a normal file. Builtin materials leave this empty.
    pub normal_convention: Option<crate::NormalConvention>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LevelError {
    Syntax(String),
    UnsupportedVersion(u32),
    DuplicateUuid(String),
    InvalidParent(String),
    Cycle(String),
    UnknownComponent(String),
    MissingAsset(String),
    MissingWorldSettings,
    Corrupt(String),
}

impl fmt::Display for LevelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(text) | Self::Corrupt(text) | Self::UnknownComponent(text) | Self::MissingAsset(text) => write!(formatter, "{text}"),
            Self::UnsupportedVersion(version) => write!(formatter, "level format version {version} is newer than {LEVEL_BLOCK_VERSION}"),
            Self::DuplicateUuid(uuid) => write!(formatter, "duplicate entity uuid {uuid}"),
            Self::InvalidParent(uuid) => write!(formatter, "parent uuid {uuid} is not in the level"),
            Self::Cycle(uuid) => write!(formatter, "parent cycle includes {uuid}"),
            Self::MissingWorldSettings => write!(formatter, "level has no World Settings entity"),
        }
    }
}

impl From<JsonError> for LevelError {
    fn from(error: JsonError) -> Self {
        Self::Syntax(error.0)
    }
}

impl LevelDocument {
    pub fn validate(&self) -> Result<(), LevelError> {
        if self.format_version == 0 {
            return Err(LevelError::Corrupt("level format version is missing".into()));
        }
        if self.format_version > LEVEL_BLOCK_VERSION {
            return Err(LevelError::UnsupportedVersion(self.format_version));
        }
        if self.name.is_empty() || !self.level_uuid.is_persistent() {
            return Err(LevelError::Corrupt("level name or uuid is invalid".into()));
        }
        let mut seen = HashSet::new();
        let mut settings = 0u32;
        for entity in &self.entities {
            let text = entity.uuid.to_string();
            if !entity.uuid.is_persistent() || !seen.insert(entity.uuid) {
                return Err(LevelError::DuplicateUuid(text));
            }
            if entity.name.is_empty() {
                return Err(LevelError::Corrupt(format!("entity {text} has no name")));
            }
            let mut kinds = HashSet::new();
            for component in &entity.components {
                let kind = component.kind_name();
                if !kinds.insert(kind) {
                    return Err(LevelError::Corrupt(format!("entity {text} repeats {kind}")));
                }
                if let ComponentRecord::WorldSettings = component {
                    settings += 1;
                    if entity.uuid != self.world_settings.entity {
                        return Err(LevelError::Corrupt("World Settings entity does not match world_settings".into()));
                    }
                }
                if let ComponentRecord::MeshRenderer { material, .. } = component {
                    material.validate()?;
                }
                if let ComponentRecord::ParametricBlock(block) = component {
                    block.validate()?;
                }
                if let ComponentRecord::ReflectionProbe(probe) = component {
                    if !crate::reflection_probe_resolution_supported(probe.resolution) {
                        return Err(LevelError::Corrupt(format!("probe resolution {} is not 32, 64, 128, or 256", probe.resolution)));
                    }
                }
            }
            let has_transform = entity.components.iter().any(|component| matches!(component, ComponentRecord::Transform { .. }));
            let needs_transform = entity.components.iter().any(|component| {
                matches!(
                    component,
                    ComponentRecord::MeshRenderer { .. }
                        | ComponentRecord::DirectionalLight(_)
                        | ComponentRecord::PointLight(_)
                        | ComponentRecord::SpotLight(_)
                        | ComponentRecord::ReflectionProbe(_)
                        | ComponentRecord::Camera(_)
                        | ComponentRecord::Joint(_)
                        | ComponentRecord::Terrain(_)
                        | ComponentRecord::PlayerStart(_)
                        | ComponentRecord::ParametricBlock(_)
                )
            });
            if needs_transform && !has_transform {
                return Err(LevelError::Corrupt(format!("entity {text} has no transform")));
            }
            if self.format_version < LEVEL_CAMERA_VERSION && entity.components.iter().any(|component| matches!(component, ComponentRecord::Camera(_))) {
                return Err(LevelError::Corrupt(format!("entity {text} has a camera, which needs level format {LEVEL_CAMERA_VERSION}")));
            }
            if let Some(ComponentRecord::Camera(camera)) = entity.components.iter().find(|component| matches!(component, ComponentRecord::Camera(_))) {
                camera.validate()?;
            }
            if let Some(ComponentRecord::Joint(joint)) = entity.components.iter().find(|component| matches!(component, ComponentRecord::Joint(_))) {
                if self.format_version < LEVEL_JOINT_VERSION {
                    return Err(LevelError::Corrupt(format!("entity {text} has a joint, which needs level format {LEVEL_JOINT_VERSION}")));
                }
                joint.validate().map_err(|error| LevelError::Corrupt(error.to_string()))?;
            }
            if let Some(ComponentRecord::Terrain(terrain)) = entity.components.iter().find(|component| matches!(component, ComponentRecord::Terrain(_))) {
                if self.format_version < LEVEL_TERRAIN_VERSION {
                    return Err(LevelError::Corrupt(format!("entity {text} has terrain, which needs level format {LEVEL_TERRAIN_VERSION}")));
                }
                terrain.validate().map_err(|error| LevelError::Corrupt(format!("terrain on {text} is not valid ({error:?})")))?;
            }
            if let Some(ComponentRecord::PlayerStart(start)) = entity.components.iter().find(|component| matches!(component, ComponentRecord::PlayerStart(_))) {
                if self.format_version < LEVEL_PLAYER_START_VERSION {
                    return Err(LevelError::Corrupt(format!("entity {text} has a player start, which needs level format {LEVEL_PLAYER_START_VERSION}")));
                }
                start.validate().map_err(|error| LevelError::Corrupt(format!("player start on {text} is not valid ({error})")))?;
            }
            if let Some(ComponentRecord::ParametricBlock(_)) = entity.components.iter().find(|component| matches!(component, ComponentRecord::ParametricBlock(_))) {
                if self.format_version < LEVEL_BLOCK_VERSION {
                    return Err(LevelError::Corrupt(format!("entity {text} has a block, which needs level format {LEVEL_BLOCK_VERSION}")));
                }
            }
        }
        if settings != 1 {
            return Err(LevelError::MissingWorldSettings);
        }
        for entity in &self.entities {
            if let Some(parent) = entity.parent_uuid {
                if !seen.contains(&parent) {
                    return Err(LevelError::InvalidParent(parent.to_string()));
                }
                if parent == entity.uuid {
                    return Err(LevelError::Cycle(entity.uuid.to_string()));
                }
            }
        }
        for entity in &self.entities {
            let mut cursor = entity.parent_uuid;
            let mut guard = 0;
            while let Some(parent) = cursor {
                guard += 1;
                if guard > self.entities.len() || parent == entity.uuid {
                    return Err(LevelError::Cycle(entity.uuid.to_string()));
                }
                cursor = self.entities.iter().find(|item| item.uuid == parent).and_then(|item| item.parent_uuid);
            }
        }
        self.world_settings.validate()?;
        if let Some(camera) = self.world_settings.startup_camera {
            if self.format_version < LEVEL_CAMERA_VERSION {
                return Err(LevelError::Corrupt(format!("startup camera {camera} needs level format {LEVEL_CAMERA_VERSION}")));
            }
            let entity = self.entities.iter().find(|entity| entity.uuid == camera).ok_or_else(|| {
                LevelError::Corrupt(format!("startup camera {camera} is not in the level"))
            })?;
            if !entity.components.iter().any(|component| matches!(component, ComponentRecord::Camera(_))) {
                return Err(LevelError::Corrupt(format!("startup camera {camera} has no Camera component")));
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> String {
        let entities = Json::array(self.entities.iter().map(EntityRecord::to_json).collect());
        Json::object(vec![
            ("schema", Json::string(LEVEL_SCHEMA)),
            ("format_version", Json::int(self.format_version as i64)),
            ("level_uuid", Json::string(self.level_uuid.to_string())),
            ("name", Json::string(&self.name)),
            ("world_settings", self.world_settings.to_json()),
            ("entities", entities),
        ])
        .write()
    }

    /// Rebuild a session world. Runtime ids are new. UUIDs, names, parents, and payloads are not.
    /// Material instances are not created here. The engine binds those when a material library exists.
    pub fn instantiate(&self) -> Result<SceneWorld, LevelError> {
        self.instantiate_with(&crate::MeshAssetLibrary::default())
    }

    /// Same as [`Self::instantiate`], resolving `scheme: jarvig.asset` from `assets`.
    pub fn instantiate_with(&self, assets: &crate::MeshAssetLibrary) -> Result<SceneWorld, LevelError> {
        self.validate()?;
        let mut world = SceneWorld::new_session();
        world.install_imported_meshes(assets);
        let mut remaining: Vec<&EntityRecord> = self.entities.iter().collect();
        let mut guard = remaining.len() + 1;
        while !remaining.is_empty() {
            guard -= 1;
            if guard == 0 {
                return Err(LevelError::Cycle("level entity order did not resolve".into()));
            }
            let ready = remaining.iter().position(|entity| match entity.parent_uuid {
                None => true,
                Some(parent) => world.entity_ownership(parent).is_ok(),
            });
            let Some(index) = ready else {
                return Err(LevelError::InvalidParent("a parent was not created".into()));
            };
            let entity = remaining.remove(index);
            spawn_entity(&mut world, entity, &self.world_settings)?;
        }
        world.set_saved_probe_policy(self.world_settings.probe_update_policy);
        if let Some(camera) = self.world_settings.startup_camera {
            world.set_startup_camera(Some(camera)).map_err(authoring)?;
        }
        Ok(world)
    }

    pub fn capture(world: &SceneWorld, level_uuid: EntityId, name: impl Into<String>) -> Result<Self, LevelError> {
        let mut entities = Vec::new();
        let mut settings = None;
        for row in world.entity_outline() {
            if let Some(record) = world.authored_world_settings(row.uuid) {
                settings = Some(record);
            }
            entities.push(capture_entity(world, row.uuid)?);
        }
        let world_settings = settings.ok_or(LevelError::MissingWorldSettings)?;
        let has_block = entities.iter().any(|entity| entity.components.iter().any(|component| matches!(component, ComponentRecord::ParametricBlock(_))));
        let has_player_start = entities.iter().any(|entity| entity.components.iter().any(|component| matches!(component, ComponentRecord::PlayerStart(_))));
        let has_terrain = entities.iter().any(|entity| entity.components.iter().any(|component| matches!(component, ComponentRecord::Terrain(_))));
        let has_joint = entities.iter().any(|entity| entity.components.iter().any(|component| matches!(component, ComponentRecord::Joint(_))));
        let has_camera = entities.iter().any(|entity| entity.components.iter().any(|component| matches!(component, ComponentRecord::Camera(_))));
        let format_version = if has_block {
            LEVEL_BLOCK_VERSION
        } else if has_player_start {
            LEVEL_PLAYER_START_VERSION
        } else if has_terrain {
            LEVEL_TERRAIN_VERSION
        } else if has_joint {
            LEVEL_JOINT_VERSION
        } else if has_camera {
            LEVEL_CAMERA_VERSION
        } else {
            LEVEL_FORMAT_VERSION
        };
        let document = Self { format_version, level_uuid, name: name.into(), world_settings, entities };
        document.validate()?;
        Ok(document)
    }
}

impl WorldSettingsRecord {
    fn validate(&self) -> Result<(), LevelError> {
        if !self.entity.is_persistent() || !self.intensity.is_finite() || self.intensity < 0.0 {
            return Err(LevelError::Corrupt("world settings are not finite".into()));
        }
        if self.upper.iter().chain(self.lower.iter()).any(|channel| !channel.is_finite()) {
            return Err(LevelError::Corrupt("world settings color is not finite".into()));
        }
        Ok(())
    }

    fn to_environment(&self) -> Result<EnvironmentLight, LevelError> {
        EnvironmentLight {
            upper_hemisphere_linear_rgb: self.upper,
            lower_hemisphere_linear_rgb: self.lower,
            intensity: self.intensity,
            enabled: self.enabled,
        }
        .validate()
        .map_err(|error| LevelError::Corrupt(error.to_string()))
    }

    fn to_json(&self) -> Json {
        let mut fields = vec![
            ("entity", Json::string(self.entity.to_string())),
            ("enabled", Json::bool(self.enabled)),
            ("intensity", Json::number(self.intensity as f64)),
            ("upper", rgb_json(self.upper)),
            ("lower", rgb_json(self.lower)),
            ("probe_update_policy", Json::string(policy_name(self.probe_update_policy))),
        ];
        if let Some(camera) = self.startup_camera {
            fields.push(("startup_camera", Json::string(camera.to_string())));
        }
        Json::object(fields)
    }
}

impl MaterialAssetRef {
    pub fn builtin(name: impl Into<String>, base_color: [f32; 4], metallic: f32, roughness: f32, emissive: [f32; 4]) -> Self {
        Self {
            scheme: MaterialScheme::Builtin,
            name: name.into(),
            base_color,
            metallic,
            roughness,
            emissive,
            uv_scale: 1.0,
            normal_scale: 1.0,
            normal_convention: None,
        }
    }

    pub fn validate(&self) -> Result<(), LevelError> {
        match self.scheme {
            MaterialScheme::Builtin => match self.name.as_str() {
                "standard_white" | "bootstrap_near" | "bootstrap_far" => {}
                other => return Err(LevelError::MissingAsset(format!("unknown builtin material {other}"))),
            },
            MaterialScheme::Staged => {
                if self.name.is_empty() || self.name.contains('/') || self.name.contains('\\') || self.name.contains('.') {
                    return Err(LevelError::MissingAsset(format!("staged material {} is not a set name", self.name)));
                }
            }
            MaterialScheme::Mesh => {
                if self.name.is_empty() {
                    return Err(LevelError::MissingAsset("imported material has no name".into()));
                }
            }
        }
        if !self.uv_scale.is_finite() || self.uv_scale <= 0.0 || !self.normal_scale.is_finite() || self.normal_scale < 0.0 {
            return Err(LevelError::Corrupt("material scale is not usable".into()));
        }
        let finite = self.base_color.iter().chain(self.emissive.iter()).all(|channel| channel.is_finite())
            && self.metallic.is_finite()
            && self.roughness.is_finite();
        if !finite {
            return Err(LevelError::Corrupt("material factors are not finite".into()));
        }
        Ok(())
    }

    fn to_json(&self) -> Json {
        let mut fields = vec![
            ("scheme", Json::string(match self.scheme {
                MaterialScheme::Builtin => "jarvig.builtin",
                MaterialScheme::Staged => "jarvig.material",
                MaterialScheme::Mesh => "jarvig.mesh",
            })),
            ("name", Json::string(&self.name)),
            ("base_color", float4_json(self.base_color)),
            ("metallic", Json::number(self.metallic as f64)),
            ("roughness", Json::number(self.roughness as f64)),
            ("emissive", float4_json(self.emissive)),
        ];
        if self.scheme != MaterialScheme::Builtin || (self.uv_scale - 1.0).abs() > 1.0e-6 {
            fields.push(("uv_scale", Json::number(self.uv_scale as f64)));
        }
        if self.scheme != MaterialScheme::Builtin || (self.normal_scale - 1.0).abs() > 1.0e-6 {
            fields.push(("normal_scale", Json::number(self.normal_scale as f64)));
        }
        if let Some(convention) = self.normal_convention {
            fields.push(("normal_convention", Json::string(convention.label())));
        }
        Json::object(fields)
    }
}

impl MeshAssetRef {
    pub fn instantiate(&self) -> crate::Mesh {
        if let Self::Asset { id, name } = self {
            panic!("mesh asset {name} ({id}) must be resolved from the project catalog");
        }
        match self.clone() {
            Self::NearTriangle => near_triangle_mesh(),
            Self::FarTriangle => far_triangle_mesh(),
            Self::Floor { width_m, depth_m } => floor_mesh(width_m as f32, depth_m as f32),
            Self::Cube { size_m } => cube_mesh(size_m as f32),
            Self::Sphere { radius_m, segments, rings, flat: false } => sphere_mesh(radius_m as f32, segments, rings),
            Self::Sphere { radius_m, segments, rings, flat: true } => flat_sphere_mesh(radius_m as f32, segments, rings),
            Self::Capsule { radius_m, height_m } => crate::capsule_mesh(radius_m as f32, height_m as f32),
            Self::EmissivePanel { width_m, height_m } => emissive_panel_mesh(width_m as f32, height_m as f32),
            Self::Asset { .. } => unreachable!("asset meshes are resolved before instantiate"),
        }
    }

    pub fn materialize(&self, assets: &crate::MeshAssetLibrary) -> Result<crate::Mesh, LevelError> {
        match self {
            Self::Asset { id, name } => assets.get(*id).cloned().ok_or_else(|| LevelError::MissingAsset(format!("mesh asset {name} ({id}) is not loaded"))),
            other => Ok(other.instantiate()),
        }
    }

    fn to_json(&self) -> Json {
        if let Self::Asset { id, name } = self {
            return Json::object(vec![
                ("scheme", Json::string("jarvig.asset")),
                ("id", Json::string(id.to_string())),
                ("name", Json::string(name)),
            ]);
        }
        match self.clone() {
            Self::NearTriangle => builtin_mesh("near_triangle", vec![]),
            Self::FarTriangle => builtin_mesh("far_triangle", vec![]),
            Self::Floor { width_m, depth_m } => builtin_mesh("floor", vec![("width_m", Json::number(width_m)), ("depth_m", Json::number(depth_m))]),
            Self::Cube { size_m } => builtin_mesh("cube", vec![("size_m", Json::number(size_m))]),
            Self::Sphere { radius_m, segments, rings, flat } => builtin_mesh(
                "sphere",
                vec![
                    ("shading", Json::string(if flat { "flat" } else { "smooth" })),
                    ("radius_m", Json::number(radius_m)),
                    ("segments", Json::int(segments as i64)),
                    ("rings", Json::int(rings as i64)),
                ],
            ),
            Self::Capsule { radius_m, height_m } => builtin_mesh("capsule", vec![("radius_m", Json::number(radius_m)), ("height_m", Json::number(height_m))]),
            Self::EmissivePanel { width_m, height_m } => {
                builtin_mesh("emissive_panel", vec![("width_m", Json::number(width_m)), ("height_m", Json::number(height_m))])
            }
            Self::Asset { .. } => unreachable!("asset meshes are written above"),
        }
    }
}

impl CameraRecord {
    pub(crate) fn validate(&self) -> Result<(), LevelError> {
        if !self.vertical_fov_deg.is_finite() || self.vertical_fov_deg <= 0.0 || self.vertical_fov_deg >= 180.0 {
            return Err(LevelError::Corrupt("camera fov is not a finite angle below 180 degrees".into()));
        }
        if !self.ortho_height_m.is_finite() || self.ortho_height_m <= 0.0 || !self.near_m.is_finite() || self.near_m <= 0.0 {
            return Err(LevelError::Corrupt("camera near plane or orthographic height is not positive".into()));
        }
        if self.orthographic && (!self.far_m.is_finite() || self.far_m <= self.near_m) {
            return Err(LevelError::Corrupt("orthographic camera far plane must be beyond the near plane".into()));
        }
        if self.viewport.iter().any(|value| !value.is_finite()) || self.viewport[2] <= 0.0 || self.viewport[3] <= 0.0 {
            return Err(LevelError::Corrupt("camera viewport is not a positive rectangle".into()));
        }
        Ok(())
    }

    fn to_json(&self) -> Json {
        Json::object(vec![
            ("type", Json::string("Camera")),
            ("version", Json::int(1)),
            ("enabled", Json::bool(self.enabled)),
            ("projection", Json::string(if self.orthographic { "Orthographic" } else { "Perspective" })),
            ("vertical_fov_deg", Json::number(self.vertical_fov_deg)),
            ("ortho_height_m", Json::number(self.ortho_height_m)),
            ("near_m", Json::number(self.near_m as f64)),
            ("far_m", Json::number(self.far_m as f64)),
            ("priority", Json::int(self.priority as i64)),
            ("viewport", Json::array(self.viewport.iter().copied().map(|value| Json::number(value as f64)).collect())),
        ])
    }
}

impl ComponentRecord {
    fn kind_name(&self) -> &'static str {
        match self {
            Self::Transform { .. } => "Transform",
            Self::MeshRenderer { .. } => "MeshRenderer",
            Self::DirectionalLight(_) => "DirectionalLight",
            Self::PointLight(_) => "PointLight",
            Self::SpotLight(_) => "SpotLight",
            Self::ReflectionProbe(_) => "ReflectionProbe",
            Self::Camera(_) => "Camera",
            Self::Joint(_) => "Joint",
            Self::Terrain(_) => "Terrain",
            Self::PlayerStart(_) => "PlayerStart",
            Self::ParametricBlock(_) => "ParametricBlock",
            Self::WorldSettings => "WorldSettings",
        }
    }
}

impl EntityRecord {
    pub(crate) fn to_json(&self) -> Json {
        let parent = match self.parent_uuid {
            Some(parent) => Json::string(parent.to_string()),
            None => Json::Null,
        };
        Json::object(vec![
            ("uuid", Json::string(self.uuid.to_string())),
            ("name", Json::string(&self.name)),
            ("parent_uuid", parent),
            ("components", Json::array(self.components.iter().map(ComponentRecord::to_json).collect())),
        ])
    }
}

impl ComponentRecord {
    fn to_json(&self) -> Json {
        match self {
            Self::Transform { translation, rotation, scale } => Json::object(vec![
                ("type", Json::string("Transform")),
                ("version", Json::int(1)),
                ("translation", vec3_json(*translation)),
                ("rotation", quat_json(*rotation)),
                ("scale", vec3_json(*scale)),
            ]),
            Self::MeshRenderer { visible, cast_shadows, receive_shadows, mesh, material } => Json::object(vec![
                ("type", Json::string("MeshRenderer")),
                ("version", Json::int(1)),
                ("visible", Json::bool(*visible)),
                ("cast_shadows", Json::bool(*cast_shadows)),
                ("receive_shadows", Json::bool(*receive_shadows)),
                ("mesh", mesh.to_json()),
                ("material", material.to_json()),
            ]),
            Self::DirectionalLight(light) => light.to_json("DirectionalLight"),
            Self::PointLight(light) => light.to_json("PointLight"),
            Self::SpotLight(light) => light.to_json("SpotLight"),
            Self::ReflectionProbe(probe) => Json::object(vec![
                ("type", Json::string("ReflectionProbe")),
                ("version", Json::int(1)),
                ("enabled", Json::bool(probe.enabled)),
                ("radius_m", Json::number(probe.radius_m)),
                ("intensity", Json::number(probe.intensity as f64)),
                ("priority", Json::int(probe.priority as i64)),
                ("resolution", Json::int(probe.resolution as i64)),
            ]),
            Self::Camera(camera) => camera.to_json(),
            Self::Joint(joint) => joint_json(joint),
            Self::Terrain(terrain) => terrain_json(terrain),
            Self::PlayerStart(start) => Json::object(vec![
                ("type", Json::string("PlayerStart")),
                ("version", Json::int(1)),
                ("player", Json::string(&start.player)),
                ("preview", Json::bool(start.preview)),
            ]),
            Self::ParametricBlock(block) => {
                let mut fields = vec![
                    ("type", Json::string("ParametricBlock")),
                    ("version", Json::int(1)),
                    ("size_m", Json::array(block.size_m.iter().copied().map(Json::number).collect())),
                    ("material", block.material.to_json()),
                ];
                if !block.is_plain() {
                    fields.push(("inset_m", Json::array(block.inset_m.iter().copied().map(Json::number).collect())));
                    fields.push(("bevel_m", Json::number(block.bevel_m)));
                    fields.push(("history", block_history_json(&block.history)));
                }
                if let Some(body) = &block.body {
                    fields.push(("body", solid_body_json(body)));
                }
                Json::object(fields)
            }
            Self::WorldSettings => Json::object(vec![("type", Json::string("WorldSettings")), ("version", Json::int(1))]),
        }
    }
}

impl LightRecord {
    fn to_json(&self, kind: &str) -> Json {
        Json::object(vec![
            ("type", Json::string(kind)),
            ("version", Json::int(1)),
            ("enabled", Json::bool(self.enabled)),
            ("color", rgb_json(self.color)),
            ("intensity", Json::number(self.intensity as f64)),
            ("range_m", Json::number(self.range_m as f64)),
            ("inner_radians", Json::number(self.inner_radians as f64)),
            ("outer_radians", Json::number(self.outer_radians as f64)),
            ("cast_shadows", Json::bool(self.shadow.cast)),
            ("shadow_resolution", Json::int(self.shadow.resolution as i64)),
            ("depth_bias_m", Json::number(self.shadow.depth_bias_m as f64)),
            ("slope_bias_m", Json::number(self.shadow.slope_bias_m as f64)),
            ("normal_bias_m", Json::number(self.shadow.normal_bias_m as f64)),
            ("filter_radius", Json::number(self.shadow.filter_radius as f64)),
            ("shadow_distance_m", Json::number(self.shadow.distance_m as f64)),
            ("cascade_count", Json::int(self.shadow.cascade_count as i64)),
            ("cascade_distribution", Json::number(self.shadow.cascade_distribution as f64)),
        ])
    }
}

pub fn parse_level(text: &str) -> Result<LevelDocument, LevelError> {
    let json = parse_json(text)?;
    let schema = required_str(&json, "schema")?;
    if schema != LEVEL_SCHEMA {
        return Err(LevelError::Corrupt(format!("schema {schema} is not {LEVEL_SCHEMA}")));
    }
    let format_version = required_u32(&json, "format_version")?;
    if format_version > LEVEL_BLOCK_VERSION {
        return Err(LevelError::UnsupportedVersion(format_version));
    }
    let level_uuid = parse_uuid(required_str(&json, "level_uuid")?)?;
    let name = required_str(&json, "name")?.to_string();
    let world_settings = parse_world_settings(json.get("world_settings").ok_or_else(|| LevelError::Corrupt("world_settings is missing".into()))?)?;
    let entities_json = json.get("entities").and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt("entities is missing".into()))?;
    let mut entities = Vec::new();
    for entity in entities_json {
        entities.push(parse_entity(entity)?);
    }
    let document = LevelDocument { format_version, level_uuid, name, world_settings, entities };
    document.validate()?;
    Ok(document)
}

/// One authored entity, in the same shape [`LevelDocument::capture`] writes.
pub fn capture_entity(world: &SceneWorld, id: EntityId) -> Result<EntityRecord, LevelError> {
    let outline = world.entity_outline();
    let row = outline.iter().find(|row| row.uuid == id).ok_or_else(|| LevelError::Corrupt("entity is not authored".into()))?;
    if world.entity_ownership(id).is_ok_and(|ownership| ownership.capabilities.payload_count() > 1) {
        return Err(LevelError::Corrupt(format!("{} has more than one payload; level version 1 cannot store it", row.name)));
    }
    let parent = world.entity_parent(id).map_err(|error| LevelError::Corrupt(error.to_string()))?;
    let mut components = Vec::new();
    if world.authored_world_settings(id).is_some() {
        components.push(ComponentRecord::WorldSettings);
    } else if world.entity_ownership(id).is_ok_and(|ownership| ownership.capabilities.mesh_renderer) {
        let Some((translation, rotation, scale, visible, cast_shadows, receive_shadows, mesh, material)) = world.authored_mesh(id) else {
            return Err(LevelError::MissingAsset(format!("{} has no builtin mesh or material reference", row.name)));
        };
        components.push(ComponentRecord::Transform { translation, rotation, scale });
        components.push(ComponentRecord::MeshRenderer { visible, cast_shadows, receive_shadows, mesh, material });
    } else if let Some((kind, light)) = world.authored_light(id) {
        let (translation, rotation) = world.authored_local_pose(id).ok_or_else(|| LevelError::Corrupt(format!("{} has no pose", row.name)))?;
        components.push(ComponentRecord::Transform { translation, rotation, scale: Vec3::new(1.0, 1.0, 1.0) });
        components.push(match kind {
            LightKind::Directional => ComponentRecord::DirectionalLight(light),
            LightKind::Point => ComponentRecord::PointLight(light),
            LightKind::Spot => ComponentRecord::SpotLight(light),
        });
    } else if let Some(probe) = world.authored_probe(id) {
        let (translation, rotation) = world.authored_local_pose(id).ok_or_else(|| LevelError::Corrupt(format!("{} has no pose", row.name)))?;
        components.push(ComponentRecord::Transform { translation, rotation, scale: Vec3::new(1.0, 1.0, 1.0) });
        components.push(ComponentRecord::ReflectionProbe(probe));
    } else if let Some(camera) = world.authored_camera(id) {
        let (translation, rotation) = world.authored_local_pose(id).ok_or_else(|| LevelError::Corrupt(format!("{} has no pose", row.name)))?;
        components.push(ComponentRecord::Transform { translation, rotation, scale: Vec3::new(1.0, 1.0, 1.0) });
        components.push(ComponentRecord::Camera(camera));
    } else if let Some(terrain) = world.authored_terrain(id) {
        let (translation, rotation) = world.authored_local_pose(id).ok_or_else(|| LevelError::Corrupt(format!("{} has no pose", row.name)))?;
        components.push(ComponentRecord::Transform { translation, rotation, scale: Vec3::new(1.0, 1.0, 1.0) });
        components.push(ComponentRecord::Terrain(terrain));
    } else if let Some((translation, rotation)) = world.authored_local_pose(id) {
        components.push(ComponentRecord::Transform { translation, rotation, scale: Vec3::new(1.0, 1.0, 1.0) });
    }
    if world.authored_world_settings(id).is_none() {
        if let Some(joint) = world.authored_joint(id) {
            if !components.iter().any(|component| matches!(component, ComponentRecord::Transform { .. })) {
                if let Some((translation, rotation)) = world.authored_local_pose(id) {
                    components.insert(0, ComponentRecord::Transform { translation, rotation, scale: Vec3::new(1.0, 1.0, 1.0) });
                }
            }
            components.push(ComponentRecord::Joint(joint));
        }
        if let Some(start) = world.authored_player_start(id) {
            if !components.iter().any(|component| matches!(component, ComponentRecord::Transform { .. })) {
                if let Some((translation, rotation)) = world.authored_local_pose(id) {
                    components.insert(0, ComponentRecord::Transform { translation, rotation, scale: Vec3::new(1.0, 1.0, 1.0) });
                }
            }
            components.push(ComponentRecord::PlayerStart(start));
        }
        if let Some(block) = world.authored_block(id) {
            if !components.iter().any(|component| matches!(component, ComponentRecord::Transform { .. })) {
                if let Some((translation, rotation)) = world.authored_local_pose(id) {
                    components.insert(0, ComponentRecord::Transform { translation, rotation, scale: Vec3::new(1.0, 1.0, 1.0) });
                }
            }
            components.push(ComponentRecord::ParametricBlock(block));
        }
    }
    Ok(EntityRecord { uuid: id, name: row.name.clone(), parent_uuid: parent, components })
}

pub(crate) fn spawn_entity(world: &mut SceneWorld, entity: &EntityRecord, settings: &WorldSettingsRecord) -> Result<(), LevelError> {
    let transform = entity.components.iter().find_map(|component| match component {
        ComponentRecord::Transform { translation, rotation, scale } => Some((*translation, *rotation, *scale)),
        _ => None,
    });
    if entity.components.iter().any(|component| matches!(component, ComponentRecord::WorldSettings)) {
        let environment = settings.to_environment()?;
        world.spawn_saved_world_settings(entity.uuid, &entity.name, environment).map_err(authoring)?;
        return Ok(());
    }
    let Some((translation, rotation, scale)) = transform else {
        return Err(LevelError::Corrupt(format!("{} has no component payload", entity.name)));
    };
    let pose = HighPrecisionPose { translation, rotation };
    for component in &entity.components {
        match component {
            ComponentRecord::Transform { .. } | ComponentRecord::WorldSettings => {}
            ComponentRecord::MeshRenderer { visible, cast_shadows, receive_shadows, mesh, material } => {
                world
                    .spawn_saved_mesh(
                        entity.uuid,
                        &entity.name,
                        entity.parent_uuid,
                        pose,
                        scale,
                        *visible,
                        *cast_shadows,
                        *receive_shadows,
                        mesh.clone(),
                        material.clone(),
                    )
                    .map_err(authoring)?;
            }
            ComponentRecord::DirectionalLight(light) => spawn_light(world, entity, pose, LightKind::Directional, light)?,
            ComponentRecord::PointLight(light) => spawn_light(world, entity, pose, LightKind::Point, light)?,
            ComponentRecord::SpotLight(light) => spawn_light(world, entity, pose, LightKind::Spot, light)?,
            ComponentRecord::ReflectionProbe(probe) => {
                world.spawn_saved_probe(entity.uuid, &entity.name, entity.parent_uuid, pose, probe.clone()).map_err(authoring)?;
            }
            ComponentRecord::Camera(camera) => {
                world.spawn_saved_camera(entity.uuid, &entity.name, entity.parent_uuid, pose, camera.clone()).map_err(authoring)?;
            }
            ComponentRecord::Joint(joint) => {
                if world.entity_ownership(entity.uuid).is_err() {
                    world.spawn_saved_transform(entity.uuid, &entity.name, entity.parent_uuid, pose).map_err(authoring)?;
                }
                world.attach_joint(entity.uuid, *joint).map_err(authoring)?;
            }
            ComponentRecord::Terrain(terrain) => {
                world.spawn_saved_terrain(entity.uuid, &entity.name, entity.parent_uuid, pose, terrain.clone()).map_err(authoring)?;
            }
            ComponentRecord::PlayerStart(start) => {
                if world.entity_ownership(entity.uuid).is_err() {
                    world.spawn_saved_transform(entity.uuid, &entity.name, entity.parent_uuid, pose).map_err(authoring)?;
                }
                world.attach_player_start(entity.uuid, start.clone()).map_err(authoring)?;
            }
            ComponentRecord::ParametricBlock(block) => {
                world.spawn_saved_block(entity.uuid, &entity.name, entity.parent_uuid, pose, block.clone()).map_err(authoring)?;
            }
        }
    }
    Ok(())
}

fn spawn_light(world: &mut SceneWorld, entity: &EntityRecord, pose: HighPrecisionPose, kind: LightKind, light: &LightRecord) -> Result<(), LevelError> {
    world.spawn_saved_light(entity.uuid, &entity.name, entity.parent_uuid, pose, kind, light.clone()).map_err(authoring)
}

fn authoring(error: AuthoringError) -> LevelError {
    LevelError::Corrupt(error.to_string())
}

pub(crate) fn parse_entity(json: &Json) -> Result<EntityRecord, LevelError> {
    let uuid = parse_uuid(required_str(json, "uuid")?)?;
    let name = required_str(json, "name")?.to_string();
    let parent_uuid = match json.get("parent_uuid") {
        Some(Json::Null) | None => None,
        Some(Json::String(text)) => Some(parse_uuid(text)?),
        _ => return Err(LevelError::Corrupt("parent_uuid is not a string or null".into())),
    };
    let components_json = json.get("components").and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt("components is missing".into()))?;
    let mut components = Vec::new();
    for component in components_json {
        components.push(parse_component(component)?);
    }
    Ok(EntityRecord { uuid, name, parent_uuid, components })
}

fn joint_json(joint: &crate::joint::JointRecord) -> Json {
    let limits = joint.limits;
    Json::object(vec![
        ("type", Json::string("Joint")),
        ("version", Json::int(1)),
        ("kind", Json::string(joint.kind.label())),
        ("rest_translation", vec3_json(joint.rest_translation)),
        ("rest_rotation", quat_json(joint.rest_rotation)),
        ("axis", vec3_json(limits.axis)),
        ("secondary_axis", vec3_json(limits.secondary_axis)),
        ("hinge_min_rad", Json::number(limits.hinge_min)),
        ("hinge_max_rad", Json::number(limits.hinge_max)),
        ("swing_rad", Json::number(limits.swing)),
        ("twist_min_rad", Json::number(limits.twist_min)),
        ("twist_max_rad", Json::number(limits.twist_max)),
        ("primary_min_rad", Json::number(limits.primary_min)),
        ("primary_max_rad", Json::number(limits.primary_max)),
        ("secondary_min_rad", Json::number(limits.secondary_min)),
        ("secondary_max_rad", Json::number(limits.secondary_max)),
        ("linear_min_m", Json::number(limits.linear_min)),
        ("linear_max_m", Json::number(limits.linear_max)),
        ("stiffness", Json::number(joint.stiffness)),
        ("damping", Json::number(joint.damping)),
    ])
}

fn parse_joint(json: &Json) -> Result<crate::joint::JointRecord, LevelError> {
    let kind = crate::joint::JointKind::parse(required_str(json, "kind")?).ok_or_else(|| LevelError::Corrupt("joint kind is unknown".into()))?;
    let joint = crate::joint::JointRecord {
        kind,
        rest_translation: required_vec3(json, "rest_translation")?,
        rest_rotation: required_quat(json, "rest_rotation")?,
        limits: crate::joint::JointLimits {
            axis: required_vec3(json, "axis")?,
            secondary_axis: required_vec3(json, "secondary_axis")?,
            hinge_min: required_f64(json, "hinge_min_rad")?,
            hinge_max: required_f64(json, "hinge_max_rad")?,
            swing: required_f64(json, "swing_rad")?,
            twist_min: required_f64(json, "twist_min_rad")?,
            twist_max: required_f64(json, "twist_max_rad")?,
            primary_min: required_f64(json, "primary_min_rad")?,
            primary_max: required_f64(json, "primary_max_rad")?,
            secondary_min: required_f64(json, "secondary_min_rad")?,
            secondary_max: required_f64(json, "secondary_max_rad")?,
            linear_min: required_f64(json, "linear_min_m")?,
            linear_max: required_f64(json, "linear_max_m")?,
        },
        stiffness: required_f64(json, "stiffness")?,
        damping: required_f64(json, "damping")?,
    };
    joint.validate().map_err(|error| LevelError::Corrupt(error.to_string()))
}

fn terrain_json(record: &crate::TerrainRecord) -> Json {
    let detail = &record.einstein;
    Json::object(vec![
        ("type", Json::string("Terrain")),
        ("version", Json::int(1)),
        ("width_m", Json::number(record.width_m as f64)),
        ("depth_m", Json::number(record.depth_m as f64)),
        ("spacing_m", Json::number(record.spacing_m as f64)),
        ("chunk_m", Json::number(record.chunk_m as f64)),
        ("height_min", Json::number(record.height_min as f64)),
        ("height_max", Json::number(record.height_max as f64)),
        ("collision", Json::bool(record.collision)),
        ("lod", Json::bool(record.lod_enabled)),
        ("debug", Json::bool(record.debug_visualization)),
        ("material", Json::string(&record.material_name)),
        ("base_color", float4_json(record.base_color)),
        ("metallic", Json::number(record.metallic as f64)),
        ("roughness", Json::number(record.roughness as f64)),
        ("heights", Json::string(crate::encode_f32_base64(&record.heights))),
        ("layers", Json::string(crate::encode_u8_base64(&record.layers))),
        ("einstein", Json::bool(detail.enabled)),
        ("einstein_seed", Json::int(detail.seed as i64)),
        ("einstein_density", Json::number(detail.density as f64)),
        ("einstein_displacement_m", Json::number(detail.max_displacement_m as f64)),
        ("einstein_error_px", Json::number(detail.error_threshold_px as f64)),
        ("einstein_distance_m", Json::number(detail.distance_m as f64)),
        ("einstein_surface_class", Json::int(detail.surface_class as i64)),
        ("cliff", Json::string(detail.cliff.label())),
        ("einstein_debug_colors", Json::bool(detail.debug_colors)),
        ("einstein_collision", Json::bool(false)),
    ])
}

fn parse_terrain(json: &Json) -> Result<crate::TerrainRecord, LevelError> {
    let seed = required_f64(json, "einstein_seed")?;
    if seed.fract() != 0.0 || !(0.0..=u32::MAX as f64).contains(&seed) {
        return Err(LevelError::Corrupt("einstein_seed is not an integer".into()));
    }
    let class = required_f64(json, "einstein_surface_class")?;
    if class.fract() != 0.0 || !(0.0..=255.0).contains(&class) {
        return Err(LevelError::Corrupt("einstein_surface_class is not a byte".into()));
    }
    let heights = crate::decode_f32_base64(required_str(json, "heights")?).map_err(|_| LevelError::Corrupt("terrain heights are not base64 f32le".into()))?;
    let layers = crate::decode_u8_base64(required_str(json, "layers")?).map_err(|_| LevelError::Corrupt("terrain layers are not base64".into()))?;
    let record = crate::TerrainRecord {
        width_m: required_f32(json, "width_m")?,
        depth_m: required_f32(json, "depth_m")?,
        spacing_m: required_f32(json, "spacing_m")?,
        chunk_m: required_f32(json, "chunk_m")?,
        height_min: required_f32(json, "height_min")?,
        height_max: required_f32(json, "height_max")?,
        collision: required_bool(json, "collision")?,
        lod_enabled: required_bool(json, "lod")?,
        debug_visualization: required_bool(json, "debug")?,
        material_name: required_str(json, "material")?.to_string(),
        base_color: required_float4(json, "base_color")?,
        metallic: required_f32(json, "metallic")?,
        roughness: required_f32(json, "roughness")?,
        heights,
        layers,
        einstein: crate::EinsteinTerrainDetail {
            enabled: required_bool(json, "einstein")?,
            seed: seed as u32,
            density: required_f32(json, "einstein_density")?,
            max_displacement_m: required_f32(json, "einstein_displacement_m")?,
            error_threshold_px: required_f32(json, "einstein_error_px")?,
            distance_m: required_f32(json, "einstein_distance_m")?,
            surface_class: class as u8,
            cliff: crate::CliffProjection::parse(required_str(json, "cliff")?).ok_or_else(|| LevelError::Corrupt("cliff projection is unknown".into()))?,
            debug_colors: required_bool(json, "einstein_debug_colors")?,
            collision: required_bool(json, "einstein_collision")?,
        },
    };
    record.validate().map_err(|error| LevelError::Corrupt(format!("terrain record is not valid ({error:?})")))?;
    Ok(record)
}

fn parse_component(json: &Json) -> Result<ComponentRecord, LevelError> {
    let kind = required_str(json, "type")?;
    let version = required_u32(json, "version")?;
    if version != 1 {
        return Err(LevelError::UnsupportedVersion(version));
    }
    match kind {
        "Transform" => Ok(ComponentRecord::Transform {
            translation: required_vec3(json, "translation")?,
            rotation: required_quat(json, "rotation")?,
            scale: required_vec3(json, "scale")?,
        }),
        "MeshRenderer" => Ok(ComponentRecord::MeshRenderer {
            visible: required_bool(json, "visible")?,
            cast_shadows: optional_bool(json, "cast_shadows", true)?,
            receive_shadows: optional_bool(json, "receive_shadows", true)?,
            mesh: parse_mesh(json.get("mesh").ok_or_else(|| LevelError::MissingAsset("mesh reference is missing".into()))?)?,
            material: parse_material(json.get("material").ok_or_else(|| LevelError::MissingAsset("material reference is missing".into()))?)?,
        }),
        "DirectionalLight" => Ok(ComponentRecord::DirectionalLight(parse_light(json)?)),
        "PointLight" => Ok(ComponentRecord::PointLight(parse_light(json)?)),
        "SpotLight" => Ok(ComponentRecord::SpotLight(parse_light(json)?)),
        "ReflectionProbe" => Ok(ComponentRecord::ReflectionProbe(ProbeRecord {
            enabled: required_bool(json, "enabled")?,
            radius_m: required_f64(json, "radius_m")?,
            intensity: required_f32(json, "intensity")?,
            priority: required_i32(json, "priority")?,
            resolution: required_u32(json, "resolution")?,
        })),
        "Joint" => Ok(ComponentRecord::Joint(parse_joint(json)?)),
        "Terrain" => Ok(ComponentRecord::Terrain(parse_terrain(json)?)),
        "PlayerStart" => {
            let start = crate::parse_player_start_fields(required_str(json, "player")?, required_bool(json, "preview")?)
                .map_err(|error| LevelError::Corrupt(error))?;
            Ok(ComponentRecord::PlayerStart(start))
        }
        "ParametricBlock" => {
            let size = required_floats(json, "size_m", 3)?;
            let material = parse_material(json.get("material").ok_or_else(|| LevelError::MissingAsset("block material is missing".into()))?)?;
            let inset = match optional_floats(json, "inset_m")? {
                None => [0.0; 6],
                Some(values) if values.len() == 6 => [values[0], values[1], values[2], values[3], values[4], values[5]],
                Some(_) => return Err(LevelError::Corrupt("inset_m has the wrong width".into())),
            };
            let bevel_m = optional_f64(json, "bevel_m", 0.0)?;
            let history = parse_block_history(json)?;
            let body = match json.get("body") {
                None => None,
                Some(value) => Some(parse_solid_body(value)?),
            };
            let block = crate::BlockRecord {
                size_m: [size[0], size[1], size[2]],
                inset_m: inset,
                bevel_m,
                material,
                history,
                body,
            };
            block.validate()?;
            Ok(ComponentRecord::ParametricBlock(block))
        }
        "Camera" => Ok(ComponentRecord::Camera(CameraRecord {
            enabled: required_bool(json, "enabled")?,
            orthographic: match required_str(json, "projection")? {
                "Perspective" => false,
                "Orthographic" => true,
                other => return Err(LevelError::Corrupt(format!("camera projection {other} is not Perspective or Orthographic"))),
            },
            vertical_fov_deg: required_f64(json, "vertical_fov_deg")?,
            ortho_height_m: required_f64(json, "ortho_height_m")?,
            near_m: required_f32(json, "near_m")?,
            far_m: required_f32(json, "far_m")?,
            priority: required_i32(json, "priority")?,
            viewport: required_viewport(json)?,
        })),
        "WorldSettings" => Ok(ComponentRecord::WorldSettings),
        other => Err(LevelError::UnknownComponent(format!("unknown component {other}"))),
    }
}

fn parse_world_settings(json: &Json) -> Result<WorldSettingsRecord, LevelError> {
    Ok(WorldSettingsRecord {
        entity: parse_uuid(required_str(json, "entity")?)?,
        enabled: required_bool(json, "enabled")?,
        intensity: required_f32(json, "intensity")?,
        upper: required_rgb(json, "upper")?,
        lower: required_rgb(json, "lower")?,
        probe_update_policy: parse_policy(required_str(json, "probe_update_policy")?)?,
        startup_camera: match optional_str(json, "startup_camera")? {
            Some(text) => Some(parse_uuid(text)?),
            None => None,
        },
    })
}

fn parse_light(json: &Json) -> Result<LightRecord, LevelError> {
    let defaults = LightShadowSettings::default();
    let shadow = LightShadowSettings {
        cast: optional_bool(json, "cast_shadows", defaults.cast)?,
        resolution: optional_u32(json, "shadow_resolution", defaults.resolution)?,
        depth_bias_m: optional_f32(json, "depth_bias_m", defaults.depth_bias_m)?,
        slope_bias_m: optional_f32(json, "slope_bias_m", defaults.slope_bias_m)?,
        normal_bias_m: optional_f32(json, "normal_bias_m", defaults.normal_bias_m)?,
        filter_radius: optional_f32(json, "filter_radius", defaults.filter_radius)?,
        distance_m: optional_f32(json, "shadow_distance_m", defaults.distance_m)?,
        cascade_count: optional_u32(json, "cascade_count", defaults.cascade_count)?,
        cascade_distribution: optional_f32(json, "cascade_distribution", defaults.cascade_distribution)?,
    };
    shadow.finite().map_err(|error| LevelError::Corrupt(error.to_string()))?;
    Ok(LightRecord {
        enabled: required_bool(json, "enabled")?,
        color: required_rgb(json, "color")?,
        intensity: required_f32(json, "intensity")?,
        range_m: required_f32(json, "range_m")?,
        inner_radians: required_f32(json, "inner_radians")?,
        outer_radians: required_f32(json, "outer_radians")?,
        shadow,
    })
}

fn parse_mesh(json: &Json) -> Result<MeshAssetRef, LevelError> {
    let scheme = required_str(json, "scheme")?;
    if scheme == "jarvig.asset" {
        let id = crate::AssetId::parse(required_str(json, "id")?).ok_or_else(|| LevelError::MissingAsset("mesh asset id is not a UUID".into()))?;
        let name = required_str(json, "name")?.to_string();
        if name.is_empty() {
            return Err(LevelError::MissingAsset("mesh asset has no name".into()));
        }
        return Ok(MeshAssetRef::Asset { id, name });
    }
    if scheme != "jarvig.builtin" {
        return Err(LevelError::MissingAsset(format!("mesh scheme {scheme} is not resolvable yet")));
    }
    match required_str(json, "name")? {
        "near_triangle" => Ok(MeshAssetRef::NearTriangle),
        "far_triangle" => Ok(MeshAssetRef::FarTriangle),
        "floor" => Ok(MeshAssetRef::Floor { width_m: required_f64(json, "width_m")?, depth_m: required_f64(json, "depth_m")? }),
        "cube" => Ok(MeshAssetRef::Cube { size_m: required_f64(json, "size_m")? }),
        "sphere" => {
            let shading = required_str(json, "shading")?;
            let flat = match shading {
                "flat" => true,
                "smooth" => false,
                other => return Err(LevelError::MissingAsset(format!("unknown sphere shading {other}"))),
            };
            Ok(MeshAssetRef::Sphere {
                radius_m: required_f64(json, "radius_m")?,
                segments: required_u32(json, "segments")?,
                rings: required_u32(json, "rings")?,
                flat,
            })
        }
        "capsule" => Ok(MeshAssetRef::Capsule { radius_m: required_f64(json, "radius_m")?, height_m: required_f64(json, "height_m")? }),
        "emissive_panel" => Ok(MeshAssetRef::EmissivePanel { width_m: required_f64(json, "width_m")?, height_m: required_f64(json, "height_m")? }),
        other => Err(LevelError::MissingAsset(format!("unknown builtin mesh {other}"))),
    }
}

fn parse_material(json: &Json) -> Result<MaterialAssetRef, LevelError> {
    let scheme = match required_str(json, "scheme")? {
        "jarvig.builtin" => MaterialScheme::Builtin,
        "jarvig.material" => MaterialScheme::Staged,
        "jarvig.mesh" => MaterialScheme::Mesh,
        other => return Err(LevelError::MissingAsset(format!("material scheme {other} is not resolvable yet"))),
    };
    let convention = match optional_str(json, "normal_convention")? {
        Some(text) => Some(crate::NormalConvention::parse(text).ok_or_else(|| LevelError::Corrupt(format!("unknown normal convention {text}")))?),
        None => None,
    };
    let material = MaterialAssetRef {
        scheme,
        name: required_str(json, "name")?.to_string(),
        base_color: required_float4(json, "base_color")?,
        metallic: required_f32(json, "metallic")?,
        roughness: required_f32(json, "roughness")?,
        emissive: required_float4(json, "emissive")?,
        uv_scale: optional_f32(json, "uv_scale", 1.0)?,
        normal_scale: optional_f32(json, "normal_scale", 1.0)?,
        normal_convention: convention,
    };
    material.validate()?;
    Ok(material)
}

fn parse_policy(text: &str) -> Result<ProbeUpdatePolicy, LevelError> {
    Ok(match text {
        "static" => ProbeUpdatePolicy::Static,
        "on-demand" => ProbeUpdatePolicy::OnDemand,
        "on-transform" => ProbeUpdatePolicy::OnTransformChange,
        "on-lighting" => ProbeUpdatePolicy::OnLightingChange,
        "time-sliced" => ProbeUpdatePolicy::TimeSliced,
        other => return Err(LevelError::Corrupt(format!("unknown probe policy {other}"))),
    })
}

fn policy_name(policy: ProbeUpdatePolicy) -> &'static str {
    match policy {
        ProbeUpdatePolicy::Static => "static",
        ProbeUpdatePolicy::OnDemand => "on-demand",
        ProbeUpdatePolicy::OnTransformChange => "on-transform",
        ProbeUpdatePolicy::OnLightingChange => "on-lighting",
        ProbeUpdatePolicy::TimeSliced => "time-sliced",
    }
}

fn parse_uuid(text: &str) -> Result<EntityId, LevelError> {
    EntityId::parse(text).ok_or_else(|| LevelError::Corrupt(format!("invalid uuid {text}")))
}

fn required_str<'a>(json: &'a Json, key: &str) -> Result<&'a str, LevelError> {
    json.get(key).and_then(Json::as_str).ok_or_else(|| LevelError::Corrupt(format!("{key} is missing")))
}

fn optional_str<'a>(json: &'a Json, key: &str) -> Result<Option<&'a str>, LevelError> {
    match json.get(key) {
        None => Ok(None),
        Some(_) => required_str(json, key).map(Some),
    }
}

fn required_bool(json: &Json, key: &str) -> Result<bool, LevelError> {
    json.get(key).and_then(Json::as_bool).ok_or_else(|| LevelError::Corrupt(format!("{key} is missing")))
}

fn optional_bool(json: &Json, key: &str, default: bool) -> Result<bool, LevelError> {
    match json.get(key) {
        None => Ok(default),
        Some(_) => required_bool(json, key),
    }
}

fn block_history_json(history: &[crate::BlockOp]) -> Json {
    Json::array(
        history
            .iter()
            .map(|op| match op {
                crate::BlockOp::Size { size_m } => Json::object(vec![
                    ("op", Json::string("size")),
                    ("size_m", Json::array(size_m.iter().copied().map(Json::number).collect())),
                ]),
                crate::BlockOp::ExtrudeFace { face, distance_m } => Json::object(vec![
                    ("op", Json::string("extrude")),
                    ("face", Json::int(*face as i64)),
                    ("distance_m", Json::number(*distance_m)),
                ]),
                crate::BlockOp::InsetFace { face, distance_m } => Json::object(vec![
                    ("op", Json::string("inset")),
                    ("face", Json::int(*face as i64)),
                    ("distance_m", Json::number(*distance_m)),
                ]),
                crate::BlockOp::Bevel { distance_m } => Json::object(vec![
                    ("op", Json::string("bevel")),
                    ("distance_m", Json::number(*distance_m)),
                ]),
                crate::BlockOp::Mirror { axis } => Json::object(vec![
                    ("op", Json::string("mirror")),
                    ("axis", Json::int(*axis as i64)),
                ]),
                crate::BlockOp::MoveEdge { edge, delta_m } => topology_delta_json("move-edge", "edge", *edge, *delta_m),
                crate::BlockOp::ExtrudeEdge { edge, delta_m } => topology_delta_json("extrude-edge", "edge", *edge, *delta_m),
                crate::BlockOp::SplitEdge { edge } => Json::object(vec![
                    ("op", Json::string("split-edge")),
                    ("edge", Json::int(*edge as i64)),
                ]),
                crate::BlockOp::SubdivideFace { face, u, v } => Json::object(vec![
                    ("op", Json::string("subdivide-face")),
                    ("face", Json::int(*face as i64)),
                    ("u", Json::int(*u as i64)),
                    ("v", Json::int(*v as i64)),
                ]),
                crate::BlockOp::MoveVertex { vertex, delta_m } => topology_delta_json("move-vertex", "vertex", *vertex, *delta_m),
                crate::BlockOp::ExtrudeFaces { faces, delta_m } => Json::object(vec![
                    ("op", Json::string("extrude-faces")),
                    ("faces", Json::array(faces.iter().copied().map(|id| Json::int(id as i64)).collect())),
                    ("delta_m", Json::array(delta_m.iter().copied().map(Json::number).collect())),
                ]),
            })
            .collect(),
    )
}

fn parse_block_history(json: &Json) -> Result<Vec<crate::BlockOp>, LevelError> {
    let Some(entries) = json.get("history") else {
        return Ok(Vec::new());
    };
    let entries = entries.as_array().ok_or_else(|| LevelError::Corrupt("block history is not a list".into()))?;
    let mut history = Vec::new();
    for entry in entries {
        let name = required_str(entry, "op")?;
        let op = match name {
            "size" => {
                let size = required_floats(entry, "size_m", 3)?;
                crate::BlockOp::Size { size_m: [size[0], size[1], size[2]] }
            }
            "extrude" => crate::BlockOp::ExtrudeFace { face: block_face_index(entry)?, distance_m: required_f64(entry, "distance_m")? },
            "inset" => crate::BlockOp::InsetFace { face: block_face_index(entry)?, distance_m: required_f64(entry, "distance_m")? },
            "bevel" => crate::BlockOp::Bevel { distance_m: required_f64(entry, "distance_m")? },
            "mirror" => crate::BlockOp::Mirror { axis: block_axis_index(entry)? },
            "move-edge" => crate::BlockOp::MoveEdge { edge: nonzero_u32(entry, "edge")?, delta_m: required_delta(entry)? },
            "extrude-edge" => crate::BlockOp::ExtrudeEdge { edge: nonzero_u32(entry, "edge")?, delta_m: required_delta(entry)? },
            "split-edge" => crate::BlockOp::SplitEdge { edge: nonzero_u32(entry, "edge")? },
            "subdivide-face" => crate::BlockOp::SubdivideFace {
                face: nonzero_u32(entry, "face")?,
                u: required_u32(entry, "u")?,
                v: required_u32(entry, "v")?,
            },
            "move-vertex" => crate::BlockOp::MoveVertex { vertex: nonzero_u32(entry, "vertex")?, delta_m: required_delta(entry)? },
            "extrude-faces" => crate::BlockOp::ExtrudeFaces { faces: required_face_ids(entry)?, delta_m: required_delta(entry)? },
            other => return Err(LevelError::Corrupt(format!("unknown block edit {other}"))),
        };
        history.push(op);
    }
    if history.len() > crate::BLOCK_HISTORY_LIMIT {
        let extra = history.len() - crate::BLOCK_HISTORY_LIMIT;
        history.drain(0..extra);
    }
    Ok(history)
}

fn topology_delta_json(op: &str, key: &str, id: u32, delta_m: [f64; 3]) -> Json {
    Json::object(vec![
        ("op", Json::string(op)),
        (key, Json::int(id as i64)),
        ("delta_m", Json::array(delta_m.iter().copied().map(Json::number).collect())),
    ])
}

fn solid_body_json(body: &crate::topology::SolidBody) -> Json {
    Json::object(vec![
        ("next_id", Json::int(body.next_id as i64)),
        (
            "vertices",
            Json::array(
                body.vertices
                    .iter()
                    .map(|vertex| {
                        Json::array(vec![
                            Json::int(vertex.id as i64),
                            Json::number(vertex.position[0]),
                            Json::number(vertex.position[1]),
                            Json::number(vertex.position[2]),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "edges",
            Json::array(
                body.edges
                    .iter()
                    .map(|edge| Json::array(vec![Json::int(edge.id as i64), Json::int(edge.a as i64), Json::int(edge.b as i64)]))
                    .collect(),
            ),
        ),
        (
            "faces",
            Json::array(
                body.faces
                    .iter()
                    .map(|face| {
                        let mut values = vec![Json::int(face.id as i64)];
                        values.extend(face.vertices.iter().copied().map(|id| Json::int(id as i64)));
                        Json::array(values)
                    })
                    .collect(),
            ),
        ),
    ])
}

fn parse_solid_body(json: &Json) -> Result<crate::topology::SolidBody, LevelError> {
    let next_id = required_u32(json, "next_id")?;
    let mut vertices = Vec::new();
    for entry in json.get("vertices").and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt("block body vertices are missing".into()))? {
        let values = entry.as_array().ok_or_else(|| LevelError::Corrupt("block body vertex is not a list".into()))?;
        if values.len() != 4 {
            return Err(LevelError::Corrupt("block body vertex has the wrong width".into()));
        }
        vertices.push(crate::topology::SolidVertex {
            id: json_u32(&values[0], "vertex")?,
            position: [json_f64(&values[1], "vertex")?, json_f64(&values[2], "vertex")?, json_f64(&values[3], "vertex")?],
        });
    }
    let mut edges = Vec::new();
    for entry in json.get("edges").and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt("block body edges are missing".into()))? {
        let values = entry.as_array().ok_or_else(|| LevelError::Corrupt("block body edge is not a list".into()))?;
        if values.len() != 3 {
            return Err(LevelError::Corrupt("block body edge has the wrong width".into()));
        }
        edges.push(crate::topology::SolidEdge { id: json_u32(&values[0], "edge")?, a: json_u32(&values[1], "edge")?, b: json_u32(&values[2], "edge")? });
    }
    let mut faces = Vec::new();
    for entry in json.get("faces").and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt("block body faces are missing".into()))? {
        let values = entry.as_array().ok_or_else(|| LevelError::Corrupt("block body face is not a list".into()))?;
        if values.len() < 4 {
            return Err(LevelError::Corrupt("block body face has the wrong width".into()));
        }
        let mut loop_ = Vec::new();
        for value in values.iter().skip(1) {
            loop_.push(json_u32(value, "face")?);
        }
        faces.push(crate::topology::SolidFace { id: json_u32(&values[0], "face")?, vertices: loop_ });
    }
    Ok(crate::topology::SolidBody { next_id, vertices, edges, faces })
}

fn json_f64(json: &Json, label: &str) -> Result<f64, LevelError> {
    json.as_f64().ok_or_else(|| LevelError::Corrupt(format!("{label} is not a number")))
}

fn json_u32(json: &Json, label: &str) -> Result<u32, LevelError> {
    let value = json_f64(json, label)?;
    if value.fract() != 0.0 || !(0.0..u32::MAX as f64).contains(&value) {
        return Err(LevelError::Corrupt(format!("{label} is not an integer")));
    }
    Ok(value as u32)
}

fn nonzero_u32(json: &Json, key: &str) -> Result<u32, LevelError> {
    let value = required_u32(json, key)?;
    if value == 0 {
        return Err(LevelError::Corrupt("block element id is zero".into()));
    }
    Ok(value)
}

fn required_face_ids(json: &Json) -> Result<Vec<u32>, LevelError> {
    let values = json.get("faces").and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt("block faces are missing".into()))?;
    if values.is_empty() {
        return Err(LevelError::Corrupt("block faces are empty".into()));
    }
    let mut faces = Vec::new();
    for value in values {
        let id = json_u32(value, "face")?;
        if id == 0 || faces.contains(&id) {
            return Err(LevelError::Corrupt("block face id is repeated or zero".into()));
        }
        faces.push(id);
    }
    Ok(faces)
}

fn required_delta(json: &Json) -> Result<[f64; 3], LevelError> {
    let values = required_floats(json, "delta_m", 3)?;
    Ok([values[0], values[1], values[2]])
}

fn block_face_index(json: &Json) -> Result<u8, LevelError> {
    let face = required_u32(json, "face")?;
    if face > 5 {
        return Err(LevelError::Corrupt("block face is outside 0..5".into()));
    }
    Ok(face as u8)
}

fn block_axis_index(json: &Json) -> Result<u8, LevelError> {
    let axis = required_u32(json, "axis")?;
    if axis > 2 {
        return Err(LevelError::Corrupt("block axis is outside 0..2".into()));
    }
    Ok(axis as u8)
}

fn optional_floats(json: &Json, key: &str) -> Result<Option<Vec<f64>>, LevelError> {
    match json.get(key) {
        None => Ok(None),
        Some(_) => required_floats(json, key, json.get(key).and_then(Json::as_array).map(|values| values.len()).unwrap_or(0)).map(Some),
    }
}

fn optional_f64(json: &Json, key: &str, default: f64) -> Result<f64, LevelError> {
    match json.get(key) {
        None => Ok(default),
        Some(_) => required_f64(json, key),
    }
}

fn optional_f32(json: &Json, key: &str, default: f32) -> Result<f32, LevelError> {
    match json.get(key) {
        None => Ok(default),
        Some(_) => required_f32(json, key),
    }
}

fn optional_u32(json: &Json, key: &str, default: u32) -> Result<u32, LevelError> {
    match json.get(key) {
        None => Ok(default),
        Some(_) => required_u32(json, key),
    }
}

fn required_f64(json: &Json, key: &str) -> Result<f64, LevelError> {
    json.get(key).and_then(Json::as_f64).ok_or_else(|| LevelError::Corrupt(format!("{key} is missing")))
}

fn required_f32(json: &Json, key: &str) -> Result<f32, LevelError> {
    Ok(required_f64(json, key)? as f32)
}

fn required_u32(json: &Json, key: &str) -> Result<u32, LevelError> {
    let value = required_f64(json, key)?;
    if value.fract() != 0.0 || !(0.0..u32::MAX as f64).contains(&value) {
        return Err(LevelError::Corrupt(format!("{key} is not an integer")));
    }
    Ok(value as u32)
}

fn required_i32(json: &Json, key: &str) -> Result<i32, LevelError> {
    let value = required_f64(json, key)?;
    if value.fract() != 0.0 || !(i32::MIN as f64..=i32::MAX as f64).contains(&value) {
        return Err(LevelError::Corrupt(format!("{key} is not an integer")));
    }
    Ok(value as i32)
}

fn required_viewport(json: &Json) -> Result<[f32; 4], LevelError> {
    let values = required_floats(json, "viewport", 4)?;
    Ok([values[0] as f32, values[1] as f32, values[2] as f32, values[3] as f32])
}

fn required_vec3(json: &Json, key: &str) -> Result<Vec3, LevelError> {
    let values = required_floats(json, key, 3)?;
    Ok(Vec3::new(values[0], values[1], values[2]))
}

fn required_quat(json: &Json, key: &str) -> Result<Quat, LevelError> {
    let values = required_floats(json, key, 4)?;
    Ok(Quat { x: values[0], y: values[1], z: values[2], w: values[3] })
}

fn required_rgb(json: &Json, key: &str) -> Result<[f32; 3], LevelError> {
    let values = required_floats(json, key, 3)?;
    Ok([values[0] as f32, values[1] as f32, values[2] as f32])
}

fn required_float4(json: &Json, key: &str) -> Result<[f32; 4], LevelError> {
    let values = required_floats(json, key, 4)?;
    Ok([values[0] as f32, values[1] as f32, values[2] as f32, values[3] as f32])
}

fn required_floats(json: &Json, key: &str, count: usize) -> Result<Vec<f64>, LevelError> {
    let values = json.get(key).and_then(Json::as_array).ok_or_else(|| LevelError::Corrupt(format!("{key} is missing")))?;
    if values.len() != count {
        return Err(LevelError::Corrupt(format!("{key} has the wrong width")));
    }
    values.iter().map(|value| value.as_f64().ok_or_else(|| LevelError::Corrupt(format!("{key} is not a number")))).collect()
}

fn vec3_json(value: Vec3) -> Json {
    Json::array(vec![Json::number(value.x), Json::number(value.y), Json::number(value.z)])
}

fn quat_json(value: Quat) -> Json {
    Json::array(vec![Json::number(value.x), Json::number(value.y), Json::number(value.z), Json::number(value.w)])
}

fn rgb_json(value: [f32; 3]) -> Json {
    Json::array(vec![Json::number(value[0] as f64), Json::number(value[1] as f64), Json::number(value[2] as f64)])
}

fn float4_json(value: [f32; 4]) -> Json {
    Json::array(value.iter().copied().map(|channel| Json::number(channel as f64)).collect())
}

fn builtin_mesh(name: &str, extra: Vec<(&str, Json)>) -> Json {
    let mut fields = vec![("scheme", Json::string("jarvig.builtin")), ("name", Json::string(name))];
    fields.extend(extra);
    Json::object(fields)
}

fn uuid(text: &str) -> EntityId {
    EntityId::parse(text).expect("stable level uuid")
}

fn transform(x: f64, y: f64, z: f64) -> ComponentRecord {
    ComponentRecord::Transform { translation: Vec3::new(x, y, z), rotation: Quat::IDENTITY, scale: Vec3::new(1.0, 1.0, 1.0) }
}

fn solid(name: &str, color: [f32; 4], metallic: f32, roughness: f32, emissive: [f32; 4]) -> MaterialAssetRef {
    MaterialAssetRef::builtin(name, color, metallic, roughness, emissive)
}

fn mesh_entity(id: &str, name: &str, pose: ComponentRecord, mesh: MeshAssetRef, material: MaterialAssetRef) -> EntityRecord {
    EntityRecord {
        uuid: uuid(id),
        name: name.into(),
        parent_uuid: None,
        components: vec![pose, ComponentRecord::MeshRenderer { visible: true, cast_shadows: true, receive_shadows: true, mesh, material }],
    }
}

/// One World Settings entity and the bootstrap environment. No meshes, lights, probes, or cameras.
pub fn empty_world_level() -> LevelDocument {
    let settings = EntityId::new();
    LevelDocument {
        format_version: LEVEL_FORMAT_VERSION,
        level_uuid: EntityId::new(),
        name: "Empty World".into(),
        world_settings: WorldSettingsRecord {
            entity: settings,
            enabled: true,
            intensity: BOOTSTRAP_ENVIRONMENT_INTENSITY,
            upper: BOOTSTRAP_UPPER_HEMISPHERE_LINEAR,
            lower: BOOTSTRAP_LOWER_HEMISPHERE_LINEAR,
            probe_update_policy: ProbeUpdatePolicy::Static,
            startup_camera: None,
        },
        entities: vec![EntityRecord {
            uuid: settings,
            name: "World Settings".into(),
            parent_uuid: None,
            components: vec![ComponentRecord::WorldSettings],
        }],
    }
}

/// The permanent lighting and material validation level. Builtin references, not GPU ids.
pub fn lighting_lab_level() -> LevelDocument {
    let settings_id = uuid("11111111-1111-4111-8111-111111111111");
    let spot_aim = crate::rotation_emitting_toward(Vec3::new(-0.15, -1.15, -1.9)).expect("spot aim");
    let (probe_x, probe_y, probe_z) = BOOTSTRAP_PROBE_LOCAL_M;
    let white = [1.0, 1.0, 1.0, 1.0];
    let none = [0.0, 0.0, 0.0, 0.0];
    LevelDocument {
        format_version: LEVEL_FORMAT_VERSION,
        level_uuid: uuid(LIGHTING_LAB_LEVEL_UUID),
        name: "Lighting Lab".into(),
        world_settings: WorldSettingsRecord {
            entity: settings_id,
            enabled: true,
            intensity: BOOTSTRAP_ENVIRONMENT_INTENSITY,
            upper: BOOTSTRAP_UPPER_HEMISPHERE_LINEAR,
            lower: BOOTSTRAP_LOWER_HEMISPHERE_LINEAR,
            probe_update_policy: ProbeUpdatePolicy::Static,
            startup_camera: None,
        },
        entities: vec![
            EntityRecord {
                uuid: settings_id,
                name: "World Settings".into(),
                parent_uuid: None,
                components: vec![ComponentRecord::WorldSettings],
            },
            mesh_entity(
                "22222222-2222-4222-8222-222222222222",
                "Near Triangle",
                transform(0.0, 0.0, -2.0),
                MeshAssetRef::NearTriangle,
                solid("bootstrap_near", white, 0.0, 0.85, none),
            ),
            mesh_entity(
                "33333333-3333-4333-8333-333333333333",
                "Far Triangle",
                transform(0.0, 0.0, -5.0),
                MeshAssetRef::FarTriangle,
                solid("bootstrap_far", white, 1.0, 0.2, [0.35, 0.12, 0.02, 0.0]),
            ),
            EntityRecord {
                uuid: uuid("44444444-4444-4444-8444-444444444444"),
                name: "Directional Light".into(),
                parent_uuid: None,
                components: vec![
                    transform(0.0, 0.0, 0.0),
                    ComponentRecord::DirectionalLight(LightRecord {
                        enabled: true,
                        color: [1.0, 0.96, 0.90],
                        intensity: 0.35,
                        range_m: 0.0,
                        inner_radians: 0.0,
                        outer_radians: 0.0,
                        shadow: LightShadowSettings::default(),
                    }),
                ],
            },
            EntityRecord {
                uuid: uuid("55555555-5555-4555-8555-555555555555"),
                name: "Blue Point Light".into(),
                parent_uuid: None,
                components: vec![
                    transform(1.25, 0.7, -0.8),
                    ComponentRecord::PointLight(LightRecord {
                        enabled: true,
                        color: [0.30, 0.48, 1.0],
                        intensity: 14.0,
                        range_m: 0.0,
                        inner_radians: 0.0,
                        outer_radians: 0.0,
                        shadow: LightShadowSettings::default(),
                    }),
                ],
            },
            EntityRecord {
                uuid: uuid("66666666-6666-4666-8666-666666666666"),
                name: "Warm Spot Light".into(),
                parent_uuid: None,
                components: vec![
                    ComponentRecord::Transform { translation: Vec3::new(0.15, 1.35, -3.1), rotation: spot_aim, scale: Vec3::new(1.0, 1.0, 1.0) },
                    ComponentRecord::SpotLight(LightRecord {
                        enabled: true,
                        color: [1.0, 0.40, 0.12],
                        intensity: 22.0,
                        range_m: 0.0,
                        inner_radians: 0.20,
                        outer_radians: 0.75,
                        shadow: LightShadowSettings::default(),
                    }),
                ],
            },
            EntityRecord {
                uuid: uuid("77777777-7777-4777-8777-777777777777"),
                name: "Reflection Probe".into(),
                parent_uuid: None,
                components: vec![
                    transform(probe_x, probe_y, probe_z),
                    ComponentRecord::ReflectionProbe(ProbeRecord {
                        enabled: true,
                        radius_m: BOOTSTRAP_PROBE_RADIUS_M,
                        intensity: BOOTSTRAP_PROBE_INTENSITY,
                        priority: BOOTSTRAP_PROBE_PRIORITY,
                        resolution: 64,
                    }),
                ],
            },
            mesh_entity(
                "88888888-8888-4888-8888-888888888888",
                "Floor",
                transform(0.0, -1.15, -4.0),
                MeshAssetRef::Floor { width_m: 6.0, depth_m: 6.0 },
                solid("standard_white", [0.62, 0.62, 0.60, 1.0], 0.0, 0.92, none),
            ),
            mesh_entity(
                "99999999-9999-4999-8999-999999999999",
                "White Cube",
                transform(-1.55, -0.65, -4.2),
                MeshAssetRef::Cube { size_m: 1.0 },
                solid("standard_white", [0.82, 0.82, 0.80, 1.0], 0.0, 0.78, none),
            ),
            mesh_entity(
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "Metal Sphere",
                transform(1.25, -0.65, -3.7),
                MeshAssetRef::Sphere { radius_m: 0.5, segments: 32, rings: 24, flat: false },
                solid("standard_white", [0.92, 0.92, 0.94, 1.0], 1.0, 0.045, none),
            ),
            mesh_entity(
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                "Flat Sphere",
                transform(0.35, -0.83, -5.15),
                MeshAssetRef::Sphere { radius_m: 0.32, segments: 32, rings: 24, flat: true },
                solid("standard_white", [0.92, 0.92, 0.94, 1.0], 1.0, 0.045, none),
            ),
            mesh_entity(
                "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
                "Emissive Panel",
                transform(2.35, -0.2, -4.0),
                MeshAssetRef::EmissivePanel { width_m: 1.4, height_m: 0.9 },
                solid("standard_white", [0.02, 0.02, 0.02, 1.0], 0.0, 0.5, [8.0, 0.15, 0.08, 1.0]),
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_text(document: &LevelDocument) -> LevelDocument {
        parse_level(&document.to_json()).expect("level json")
    }

    #[test]
    fn lighting_lab_round_trips_without_runtime_ids() {
        let document = lighting_lab_level();
        document.validate().unwrap();
        let text = document.to_json();
        for forbidden in ["ObjectId", "EntityHandle", "LightId", "ProbeId", "MeshId", "MaterialInstanceId", "FrameId", "RenderInstanceId", "wgpu"] {
            assert!(!text.contains(forbidden), "{forbidden} leaked into the level");
        }
        let parsed = round_trip_text(&document);
        let world = parsed.instantiate().unwrap();
        let captured = LevelDocument::capture(&world, parsed.level_uuid, parsed.name.clone()).unwrap();
        assert_eq!(captured.to_json(), parsed.to_json());
        assert_eq!(world.entity_count(), 12);
        assert_eq!(world.object_count(), 7);
        assert_eq!(world.light_count(), 3);
        let cube = uuid("99999999-9999-4999-8999-999999999999");
        let (translation, _, scale, visible, cast_shadows, receive_shadows, mesh, material) = world.authored_mesh(cube).unwrap();
        assert!((translation.x + 1.55).abs() < 1.0e-9);
        assert_eq!(scale, Vec3::new(1.0, 1.0, 1.0));
        assert!(visible && cast_shadows && receive_shadows);
        assert_eq!(mesh, MeshAssetRef::Cube { size_m: 1.0 });
        assert!((material.roughness - 0.78).abs() < 1.0e-6);
        assert!(world.entity_parent(cube).unwrap().is_none());
        let flat = world.authored_mesh(uuid("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")).unwrap();
        assert!(matches!(flat.6, MeshAssetRef::Sphere { flat: true, .. }));
        let smooth = world.authored_mesh(uuid("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")).unwrap();
        assert!(matches!(smooth.6, MeshAssetRef::Sphere { flat: false, .. }));
    }

    #[test]
    fn bad_levels_do_not_instantiate() {
        let mut document = lighting_lab_level();
        document.entities.push(document.entities[1].clone());
        assert!(matches!(document.validate(), Err(LevelError::DuplicateUuid(_))));
        document = lighting_lab_level();
        document.entities[1].parent_uuid = Some(uuid("12345678-1234-4234-8234-123456789abc"));
        assert!(matches!(document.validate(), Err(LevelError::InvalidParent(_))));
        document = lighting_lab_level();
        document.entities[1].parent_uuid = Some(document.entities[2].uuid);
        document.entities[2].parent_uuid = Some(document.entities[1].uuid);
        assert!(matches!(document.validate(), Err(LevelError::Cycle(_))));
        document = lighting_lab_level();
        document.format_version = LEVEL_BLOCK_VERSION + 1;
        assert!(matches!(document.validate(), Err(LevelError::UnsupportedVersion(7))));
        let mut text = lighting_lab_level().to_json();
        text.truncate(24);
        assert!(matches!(parse_level(&text), Err(LevelError::Syntax(_))));
        text = lighting_lab_level().to_json().replace("\"Near Triangle\"", "\"Near Triangle\"").replace("near_triangle", "imported_hero");
        assert!(matches!(parse_level(&text), Err(LevelError::MissingAsset(_))));
        text = lighting_lab_level().to_json().replace("\"MeshRenderer\"", "\"Nanite\"");
        assert!(matches!(parse_level(&text), Err(LevelError::UnknownComponent(_))));
        text = lighting_lab_level().to_json().replace("\"jarvig.builtin\"", "\"jarvig.asset\"");
        // A builtin mesh rewritten to jarvig.asset has no asset id. That is a corrupt reference.
        assert!(matches!(parse_level(&text), Err(LevelError::Corrupt(_))));
    }

    #[test]
    fn empty_world_is_only_world_settings() {
        let document = empty_world_level();
        assert_eq!(document.format_version, LEVEL_FORMAT_VERSION);
        assert_eq!(document.entities.len(), 1);
        assert_eq!(document.entities[0].name, "World Settings");
        assert!(document.entities[0].components.iter().all(|component| matches!(component, ComponentRecord::WorldSettings)));
        let world = document.instantiate().unwrap();
        assert_eq!(world.entity_count(), 1);
        assert_eq!(world.object_count(), 0);
        assert_eq!(world.light_count(), 0);
        assert_eq!(world.entity_outline()[0].class, crate::AuthoringClass::WorldSettings);
    }

    #[test]
    fn sculpted_terrain_heights_round_trip_bit_exact_and_chunks_do_not_cast_shadows() {
        let mut document = empty_world_level();
        let mut record = crate::TerrainRecord::flat(4.0, 4.0, 1.0, 2.0, 0.0).unwrap();
        record.heights[3] = 1.25;
        record.heights[7] = -0.5;
        let flat: Vec<u32> = record.heights.iter().map(|height| height.to_bits()).collect();
        let stamped = record.stamp(crate::TerrainBrush::Sculpt, 0.0, 0.0, 1.2, 0.4, 2, crate::TerrainFalloff::Smooth, None).unwrap();
        assert!(stamped.height_changed);
        let before: Vec<u32> = record.heights.iter().map(|height| height.to_bits()).collect();
        assert_ne!(before, flat);
        record.einstein.enabled = true;
        assert!(record.detail_displacement(0.0, 0.0, 1).is_none());
        assert_eq!(record.heights.iter().map(|height| height.to_bits()).collect::<Vec<_>>(), before);
        record.einstein.collision = true;
        assert!(record.validate().is_err());
        record.einstein.collision = false;
        document.format_version = LEVEL_TERRAIN_VERSION;
        document.entities.push(EntityRecord {
            uuid: EntityId::new(),
            name: "Terrain".into(),
            parent_uuid: None,
            components: vec![
                ComponentRecord::Transform { translation: Vec3::new(1.0, 0.0, 2.0), rotation: Quat::IDENTITY, scale: Vec3::new(1.0, 1.0, 1.0) },
                ComponentRecord::Terrain(record),
            ],
        });
        let text = document.to_json();
        assert!(text.contains("\"heights\""));
        let parsed = parse_level(&text).unwrap();
        assert_eq!(parsed.format_version, LEVEL_TERRAIN_VERSION);
        let loaded = parsed.entities.iter().find_map(|entity| {
            entity.components.iter().find_map(|component| match component {
                ComponentRecord::Terrain(terrain) => Some(terrain),
                _ => None,
            })
        }).unwrap();
        assert_eq!(loaded.heights.iter().map(|height| height.to_bits()).collect::<Vec<_>>(), before);
        assert!(loaded.einstein.enabled);
        let mut world = parsed.instantiate().unwrap();
        assert_eq!(world.entity_outline().len(), 2);
        assert_eq!(world.terrain_count(), 1);
        assert_eq!(world.object_count(), 4);
        let snapshot = world.extract(crate::RenderFrameId(1)).unwrap();
        assert!(snapshot.instances().iter().all(|instance| !instance.cast_shadows));
        assert!(snapshot.instances().iter().all(|instance| world.meshes().get(instance.mesh).unwrap().index_count() / 3 == 8));
        let captured = LevelDocument::capture(&world, parsed.level_uuid, parsed.name.clone()).unwrap();
        let again = parse_level(&captured.to_json()).unwrap();
        let round = again.entities.iter().find_map(|entity| {
            entity.components.iter().find_map(|component| match component {
                ComponentRecord::Terrain(terrain) => Some(terrain.heights.iter().map(|height| height.to_bits()).collect::<Vec<_>>()),
                _ => None,
            })
        }).unwrap();
        assert_eq!(round, before);
        assert!(world.create_terrain(Vec3::ZERO, crate::TerrainRecord::flat(4.0, 4.0, 1.0, 2.0, 0.0).unwrap()).is_err());
    }

    #[test]
    fn a_deferred_brush_rebuilds_only_the_dirty_chunk() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_terrain(Vec3::ZERO, crate::TerrainRecord::flat(8.0, 8.0, 1.0, 4.0, 0.0).unwrap()).unwrap();
        let mut before = Vec::new();
        for z in 0..2 {
            for x in 0..2 {
                before.push((x, z, world.terrain_chunk_mesh(x, z).unwrap()));
            }
        }
        let stamped = world
            .stamp_terrain(id, crate::TerrainBrush::Sculpt, -3.0, -3.0, 1.2, 0.4, 0, crate::TerrainFalloff::Smooth, None, false)
            .unwrap();
        assert!(stamped.height_changed);
        assert!(stamped.dirty.iter().all(|coord| coord.x == 0 && coord.z == 0));
        for (x, z, mesh) in &before {
            assert_eq!(world.terrain_chunk_mesh(*x, *z), Some(*mesh));
        }
        let record = world.authored_terrain(id).unwrap();
        let built = crate::build_dirty_chunks(&record, &stamped.dirty).unwrap();
        assert_eq!(built.len(), 1);
        world.install_terrain_chunks(id, built.into_iter().map(|chunk| (chunk.coord.x, chunk.coord.z, chunk.mesh)).collect()).unwrap();
        assert_ne!(world.terrain_chunk_mesh(0, 0), before.iter().find(|item| item.0 == 0 && item.1 == 0).map(|item| item.2));
        for (x, z, mesh) in before {
            if x == 0 && z == 0 {
                continue;
            }
            assert_eq!(world.terrain_chunk_mesh(x, z), Some(mesh));
        }
        assert!(world.authored_terrain(id).unwrap().sample_height(-3.0, -3.0).unwrap() > 0.0);
        let again = world.extract(crate::RenderFrameId(2)).unwrap();
        assert!(again.instances().iter().all(|instance| !instance.cast_shadows));
    }

    #[test]
    fn land_draw_classes_keep_chunks_terrain_and_a_jointed_mesh_a_character() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_terrain(Vec3::ZERO, crate::TerrainRecord::flat(8.0, 8.0, 1.0, 4.0, 0.0).unwrap()).unwrap();
        let height_bits: Vec<u32> = world.authored_terrain(id).unwrap().heights.iter().map(|height| height.to_bits()).collect();
        let scene = world.scene_frame().unwrap();
        let mesh = world.add_mesh(cube_mesh(1.0));
        world.spawn_named_object("Crate", mesh, scene, Vec3::new(2.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 1.0)).unwrap();
        let pawn_mesh = world.add_mesh(cube_mesh(0.5));
        world.spawn_named_object("Pawn", pawn_mesh, scene, Vec3::new(-2.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 1.0)).unwrap();
        let crate_id = world.entity_outline().iter().find(|row| row.name == "Crate").unwrap().uuid;
        let pawn_id = world.entity_outline().iter().find(|row| row.name == "Pawn").unwrap().uuid;
        let actors = world.land_draw_actors();
        assert_eq!(actors.iter().filter(|(_, class)| *class == crate::LandDrawClass::Terrain).count(), 4);
        assert_eq!(actors.iter().find(|(actor, _)| *actor == crate_id).unwrap().1, crate::LandDrawClass::Prop);
        assert_eq!(world.terrain_surface_meshes().len(), 4);
        world.add_component(pawn_id, crate::TYPE_PAWN).unwrap();
        assert_eq!(world.land_draw_actors().iter().find(|(actor, _)| *actor == pawn_id).unwrap().1, crate::LandDrawClass::Gameplay);
        world.add_component(crate_id, crate::TYPE_JOINT).unwrap();
        assert_eq!(world.land_draw_actors().iter().find(|(actor, _)| *actor == crate_id).unwrap().1, crate::LandDrawClass::Character);
        assert_eq!(world.authored_terrain(id).unwrap().heights.iter().map(|height| height.to_bits()).collect::<Vec<_>>(), height_bits);
        let drawn = world.extract(crate::RenderFrameId(3)).unwrap();
        assert!(drawn.instances().iter().all(|instance| instance.visible));
    }
}
