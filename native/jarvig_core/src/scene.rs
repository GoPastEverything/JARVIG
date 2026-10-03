//! Authoritative bootstrap scene and the render snapshot extracted from it.
//!
//! The world is the truth. [`RenderSceneSnapshot`] is a disposable copy of the
//! poses the renderer needs. It is not a second world, not a save file, and not
//! a gameplay database. Geometry stays in [`MeshLibrary`]. An instance stores a
//! [`MeshId`], not vertices.
//!
//! Camera-relative float32 is not baked here. Each view subtracts its own origin
//! later. This module does not know the renderer, the RHI, or wgpu.

use std::collections::HashSet;

use jarvig_material::MaterialInstanceId;

use crate::level::{CameraRecord, LightRecord, MaterialAssetRef, MeshAssetRef, ProbeRecord, WorldSettingsRecord};
use crate::{
    camera_relative_f32, far_triangle_mesh, finite_color, finite_intensity, finite_range, near_triangle_mesh, rotation_emitting_toward,
    bootstrap_shared_world, Camera, EntityError, EntityHandle, EntityId, EntityOutlineInfo, EntityRegistry, AuthoringError, AuthoringResult,
    EntityInspection, EnvironmentLight, FieldInfo, FrameGraph, FrameId, GpuTransforms, HighPrecisionPose, InspectedField, InspectedSection,
    LightId, LightKind, LocalBounds, Mat4, Mesh, MeshId, MeshLibrary, ProbeId, PropertyValue, Quat, RenderLight, RenderReflectionProbe,
    ResolvedPose, SpaceError, SpotCone, TypeInfo, Vec3, BOOTSTRAP_PROBE_INTENSITY, BOOTSTRAP_PROBE_LOCAL_M, BOOTSTRAP_PROBE_PRIORITY,
    BOOTSTRAP_PROBE_RADIUS_M, FIELD_LOCAL_ROTATION, FIELD_LOCAL_TRANSLATION, FIELD_NAME, FIELD_PARENT, FIELD_UUID, TYPE_ENTITY,
    TYPE_SPATIAL_FRAME,
};

/// Bootstrap slot. Not the authoring identity. See [`EntityId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u64);

/// One extracted drawable. Not an [`ObjectId`] and not a [`MeshId`].
/// An object may later produce several of these. One of these may vanish while the object remains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderInstanceId(pub u64);

/// Identifies one extracted render frame. Not the simulation tick and not the world revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderFrameId(pub u64);

/// Runtime camera record. Not [`EntityUuid`] and not the editor camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CameraId(pub u64);

/// Runtime joint record. Not saved. The pose stays on the entity frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JointId(pub u64);

/// One joint debug segment in root space. The editor turns it into an overlay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointDebugSegment {
    pub entity: EntityId,
    pub start: Vec3,
    pub end: Vec3,
    pub color: [f32; 4],
    pub limits: bool,
}

struct WorldObject {
    id: ObjectId,
    handle: EntityHandle,
    render_id: RenderInstanceId,
    mesh: MeshId,
    frame: FrameId,
    scale: Vec3,
    visible: bool,
    cast_shadows: bool,
    receive_shadows: bool,
    /// Slot index to a logical material instance. The mesh does not own this.
    bindings: Vec<MaterialSlotBinding>,
    /// Builtin or future asset reference. Not a [`MeshId`]. Absent on a world that was never saved.
    authored_mesh: Option<MeshAssetRef>,
    /// Builtin material factors. Not a [`MaterialInstanceId`].
    authored_material: Option<MaterialAssetRef>,
}

/// One mesh slot resolved to a logical material instance. Not a shader, pipeline, or GPU buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialSlotBinding {
    pub slot: u32,
    pub instance: MaterialInstanceId,
}

struct WorldLight {
    id: LightId,
    handle: EntityHandle,
    frame: FrameId,
    kind: LightKind,
    color_linear: [f32; 3],
    intensity: f32,
    range_m: f32,
    inner_radians: f32,
    outer_radians: f32,
    enabled: bool,
    shadow: crate::LightShadowSettings,
}

/// Resolved origin used to frame an entity. Not a camera and not a selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityFocus {
    pub origin: Vec3,
    pub radius_m: f64,
    pub renderable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusError {
    Missing,
    NoSpatial,
}

/// Which authorable capabilities an entity owns. Payloads stay in subsystem records.
///
/// This is the query the outliner, the inspector, duplicate, and delete share.
/// It is not a second entity store and it is not a Win32 decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityCapabilities {
    pub transform: bool,
    pub mesh_renderer: bool,
    pub directional_light: bool,
    pub point_light: bool,
    pub spot_light: bool,
    pub reflection_probe: bool,
    pub camera: bool,
    pub joint: bool,
    pub world_settings: bool,
    pub terrain: bool,
    pub player_start: bool,
    pub block: bool,
}

impl EntityCapabilities {
    pub fn payload_count(self) -> usize {
        usize::from(self.mesh_renderer)
            + usize::from(self.directional_light)
            + usize::from(self.point_light)
            + usize::from(self.spot_light)
            + usize::from(self.reflection_probe)
            + usize::from(self.camera)
            + usize::from(self.world_settings)
    }

    pub fn authoring_class(self) -> crate::AuthoringClass {
        if self.world_settings {
            crate::AuthoringClass::WorldSettings
        } else if self.block {
            crate::AuthoringClass::Block
        } else if self.mesh_renderer {
            crate::AuthoringClass::Mesh
        } else if self.directional_light {
            crate::AuthoringClass::DirectionalLight
        } else if self.point_light {
            crate::AuthoringClass::PointLight
        } else if self.spot_light {
            crate::AuthoringClass::SpotLight
        } else if self.reflection_probe {
            crate::AuthoringClass::ReflectionProbe
        } else if self.camera {
            crate::AuthoringClass::Camera
        } else if self.terrain {
            crate::AuthoringClass::Terrain
        } else if self.player_start {
            crate::AuthoringClass::PlayerStart
        } else {
            crate::AuthoringClass::Empty
        }
    }
}

/// What the inspector may offer. Derived from [`EntityCapabilities`]. Not a second entity store.
///
/// `boolean_operand` stays false until a boolean solid exists. An imported mesh is not a parametric solid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthoringCapabilities {
    pub transformable: bool,
    pub parametric_solid: bool,
    pub face_editable: bool,
    pub boolean_operand: bool,
    pub patternable: bool,
    pub material_assignable: bool,
    pub collidable: bool,
    pub component_host: bool,
}

fn translation_matches(left: Vec3, right: Vec3) -> bool {
    (left.x - right.x).abs() < 1.0e-9 && (left.y - right.y).abs() < 1.0e-9 && (left.z - right.z).abs() < 1.0e-9
}

fn block_size_axis(field: crate::FieldId) -> Option<usize> {
    if field == crate::FIELD_BLOCK_SIZE_X {
        Some(0)
    } else if field == crate::FIELD_BLOCK_SIZE_Y {
        Some(1)
    } else if field == crate::FIELD_BLOCK_SIZE_Z {
        Some(2)
    } else {
        None
    }
}

fn block_inset_face(field: crate::FieldId) -> Option<u8> {
    if field == crate::FIELD_BLOCK_INSET_PX {
        Some(0)
    } else if field == crate::FIELD_BLOCK_INSET_NX {
        Some(1)
    } else if field == crate::FIELD_BLOCK_INSET_PY {
        Some(2)
    } else if field == crate::FIELD_BLOCK_INSET_NY {
        Some(3)
    } else if field == crate::FIELD_BLOCK_INSET_PZ {
        Some(4)
    } else if field == crate::FIELD_BLOCK_INSET_NZ {
        Some(5)
    } else {
        None
    }
}

fn block_mesh(record: &crate::BlockRecord) -> crate::Mesh {
    let size = [record.size_m[0] as f32, record.size_m[1] as f32, record.size_m[2] as f32];
    let inset = [
        record.inset_m[0] as f32,
        record.inset_m[1] as f32,
        record.inset_m[2] as f32,
        record.inset_m[3] as f32,
        record.inset_m[4] as f32,
        record.inset_m[5] as f32,
    ];
    crate::block_surface_mesh(size, inset, record.bevel_m as f32)
}

impl AuthoringCapabilities {
    pub fn from_entity(capabilities: EntityCapabilities) -> Self {
        Self {
            transformable: capabilities.transform,
            parametric_solid: capabilities.block,
            face_editable: capabilities.block,
            boolean_operand: false,
            patternable: capabilities.block,
            material_assignable: capabilities.block || capabilities.mesh_renderer,
            collidable: capabilities.block,
            component_host: !capabilities.world_settings,
        }
    }
}

/// One role on an actor's stack. Camera is named and unused. It is not the editor camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentRole {
    Transform,
    MeshRenderer,
    DirectionalLight,
    PointLight,
    SpotLight,
    ReflectionProbe,
    Camera,
    Pawn,
    FreeFly,
    Joint,
    Terrain,
    PlayerStart,
    Block,
    WorldSettings,
}

impl ComponentRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::Transform => "Transform",
            Self::MeshRenderer => "Mesh Renderer",
            Self::DirectionalLight => "Directional Light",
            Self::PointLight => "Point Light",
            Self::SpotLight => "Spot Light",
            Self::ReflectionProbe => "Reflection Probe",
            Self::Camera => "Camera",
            Self::Pawn => "Pawn",
            Self::FreeFly => "Free Fly",
            Self::Joint => "Joint",
            Self::Terrain => "Terrain",
            Self::PlayerStart => "Player Spawn",
            Self::Block => "Block",
            Self::WorldSettings => "World Settings",
        }
    }
}

/// Membership over an existing subsystem record. Not a second payload and not a save id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentBinding {
    pub role: ComponentRole,
    pub object: Option<ObjectId>,
    pub mesh: Option<MeshId>,
    pub light: Option<LightId>,
    pub probe: Option<ProbeId>,
    pub camera: Option<CameraId>,
    pub joint: Option<JointId>,
}

impl ComponentBinding {
    fn role_only(role: ComponentRole) -> Self {
        Self { role, object: None, mesh: None, light: None, probe: None, camera: None, joint: None }
    }
}

/// One entity that owns a component type. Not a render instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentQueryHit {
    pub entity: EntityId,
    pub slot: u32,
    pub binding: ComponentBinding,
}

/// One entity's membership. Subsystem ids are present only when that record owns the entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntityOwnership {
    pub capabilities: EntityCapabilities,
    pub object: Option<ObjectId>,
    pub mesh: Option<MeshId>,
    pub light: Option<LightId>,
    pub probe: Option<ProbeId>,
    pub camera: Option<CameraId>,
    pub joint: Option<JointId>,
}

const FOCUS_DEFAULT_RADIUS_M: f64 = 1.0;
const FOCUS_MIN_RADIUS_M: f64 = 0.05;

/// The bootstrap scene the engine owns. The renderer does not.
pub struct SceneWorld {
    frames: FrameGraph,
    /// Scene frame under the billion-meter root. Not an entity. Level actors hang here.
    scene: FrameId,
    meshes: MeshLibrary,
    /// Canonical imported meshes. Not GPU ids. The level stores only their asset ids.
    imported_meshes: Vec<(crate::AssetId, String, Mesh)>,
    objects: Vec<WorldObject>,
    entities: EntityRegistry,
    lights: Vec<WorldLight>,
    probes: Vec<WorldProbe>,
    game_cameras: Vec<WorldCamera>,
    joints: Vec<WorldJoint>,
    terrains: Vec<WorldTerrain>,
    /// Spawn markers. The character body is not stored here.
    player_starts: Vec<WorldPlayerStart>,
    /// Parametric solids. The mesh on the object is derived and is not the save record.
    blocks: Vec<WorldBlock>,
    /// Clusters built from a derived surface, keyed by that runtime mesh. Not saved.
    derived_meshlets: Vec<(MeshId, crate::MeshletSet)>,
    /// Chunk entities. Hidden from the outline and from the save.
    derived_visuals: HashSet<EntityHandle>,
    /// Mesh ids replaced by a sculpt. The editor evicts the GPU copy.
    retired_meshes: Vec<MeshId>,
    /// Chunks whose height samples changed and whose visual mesh may still be old.
    terrain_dirty: Vec<crate::ChunkCoord>,
    environment: EnvironmentLight,
    environment_entity: EntityHandle,
    front: Camera,
    side: Camera,
    /// Spatial entities with no mesh. Not a component store.
    anchors: Vec<(EntityHandle, FrameId)>,
    revision: u64,
    lighting_revision: u64,
    probe_policy: crate::ProbeUpdatePolicy,
    probe_recapture_serial: u64,
    simulation_tick: u64,
    /// Authored runtime-camera reference. Not the Perspective camera and not "first Camera."
    startup_camera: Option<EntityId>,
    next_object: u64,
    next_light: u64,
    next_probe: u64,
    next_camera: u64,
    next_joint: u64,
}

/// Joint limits and rest pose. The live pose stays on the entity frame.
struct WorldJoint {
    id: JointId,
    handle: EntityHandle,
    record: crate::joint::JointRecord,
}

struct WorldPlayerStart {
    handle: EntityHandle,
    record: crate::PlayerStartRecord,
}

struct WorldBlock {
    handle: EntityHandle,
    record: crate::BlockRecord,
}

struct WorldTerrain {
    handle: EntityHandle,
    frame: FrameId,
    record: crate::TerrainRecord,
    chunks: Vec<TerrainChunk>,
}

struct TerrainChunk {
    cx: u32,
    cz: u32,
    handle: EntityHandle,
    object: ObjectId,
    mesh: MeshId,
}

/// What a Land workspace draw is. Editor policy only. Not a saved class and not an outliner row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandDrawClass {
    Terrain,
    Character,
    Prop,
    Gameplay,
}

/// Heightfield numbers the surface grid needs. Not the samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainSurfaceMetrics {
    pub half_x: f32,
    pub half_z: f32,
    pub spacing_m: f32,
    pub chunk_m: f32,
    pub lod_enabled: bool,
}

/// Sphere reflection probe. A frame owns the position. Not an entity and not a mesh.
struct WorldProbe {
    id: ProbeId,
    handle: EntityHandle,
    frame: FrameId,
    radius_m: f64,
    priority: i32,
    intensity: f32,
    enabled: bool,
    resolution: u32,
}

/// Authored game camera. The pose is the actor Transform. This record does not store one.
#[derive(Clone)]
struct WorldCamera {
    id: CameraId,
    handle: EntityHandle,
    enabled: bool,
    orthographic: bool,
    vertical_fov_deg: f64,
    ortho_height_m: f64,
    near_m: f32,
    far_m: f32,
    priority: i32,
    viewport: [f32; 4],
}

impl WorldCamera {
    fn default_on(id: CameraId, handle: EntityHandle) -> Self {
        Self {
            id,
            handle,
            enabled: true,
            orthographic: false,
            vertical_fov_deg: 60.0,
            ortho_height_m: 10.0,
            near_m: 0.1,
            far_m: 1000.0,
            priority: 0,
            viewport: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

/// One coherent extraction. Owned data. Mutating the world does not change it.
#[derive(Debug, Clone)]
pub struct RenderSceneSnapshot {
    pub render_frame: RenderFrameId,
    pub simulation_tick: u64,
    pub world_revision: u64,
    pub lighting_revision: u64,
    pub probe_policy: crate::ProbeUpdatePolicy,
    pub probe_recapture_serial: u64,
    instances: Vec<RenderInstance>,
    cameras: Vec<ExtractedCamera>,
    game_cameras: Vec<ExtractedGameCamera>,
    lights: Vec<RenderLight>,
    environment: EnvironmentLight,
    probes: Vec<RenderReflectionProbe>,
}

#[derive(Debug, Clone)]
pub struct RenderInstance {
    pub id: RenderInstanceId,
    /// Authoring identity. Not [`RenderInstanceId`] and not [`ObjectId`].
    pub entity: EntityId,
    /// Bootstrap slot that produced this instance. Not the saved identity.
    pub source: ObjectId,
    pub mesh: MeshId,
    pub pose: ResolvedPose,
    pub scale: Vec3,
    pub bounds: LocalBounds,
    pub visible: bool,
    pub cast_shadows: bool,
    pub receive_shadows: bool,
    /// Logical instance ids. Not wgpu objects, buffer ids, shaders, or pipelines.
    pub material_bindings: Vec<MaterialSlotBinding>,
}

#[derive(Debug, Clone)]
pub struct ExtractedCamera {
    pub frame: FrameId,
    pub pose: ResolvedPose,
    pub vertical_fov_radians: f64,
    pub near_m: f32,
}

/// A game camera extracted from an actor. The pose is that actor's Transform.
/// Perspective uses infinite reversed-Z. `far_m` is not a perspective clip.
#[derive(Debug, Clone)]
pub struct ExtractedGameCamera {
    pub entity: EntityId,
    pub id: CameraId,
    pub pose: ResolvedPose,
    pub enabled: bool,
    pub orthographic: bool,
    pub vertical_fov_radians: f64,
    pub ortho_height_m: f64,
    pub near_m: f32,
    pub far_m: f32,
    pub priority: i32,
    pub viewport: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PayloadKind {
    WorldSettings,
    Block,
    Terrain,
    Mesh,
    Light,
    Probe,
    Camera,
    Joint,
    PlayerStart,
    Transform,
}

fn payload_kind_of(record: &crate::EntityRecord) -> PayloadKind {
    let mut kind = PayloadKind::Transform;
    for component in &record.components {
        match component {
            crate::ComponentRecord::WorldSettings => return PayloadKind::WorldSettings,
            crate::ComponentRecord::ParametricBlock(_) => return PayloadKind::Block,
            crate::ComponentRecord::Terrain(_) => return PayloadKind::Terrain,
            crate::ComponentRecord::MeshRenderer { .. } => return PayloadKind::Mesh,
            crate::ComponentRecord::DirectionalLight(_) | crate::ComponentRecord::PointLight(_) | crate::ComponentRecord::SpotLight(_) => {
                return PayloadKind::Light
            }
            crate::ComponentRecord::ReflectionProbe(_) => kind = PayloadKind::Probe,
            crate::ComponentRecord::Camera(_) => kind = PayloadKind::Camera,
            crate::ComponentRecord::Joint(_) => {
                if kind == PayloadKind::Transform {
                    kind = PayloadKind::Joint;
                }
            }
            crate::ComponentRecord::PlayerStart(_) => {
                if kind == PayloadKind::Transform {
                    kind = PayloadKind::PlayerStart;
                }
            }
            crate::ComponentRecord::Transform { .. } => {}
        }
    }
    kind
}

/// What applying a transaction did to drawables. Spawned entities need a material bind.
/// An in-place mesh rebuild keeps the bind it already had.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MementoEffect {
    pub spawned: Vec<EntityId>,
    pub rebound: Vec<EntityId>,
}

impl SceneWorld {
    /// Two meshes, two cameras, one frame tree. Insertion order is near, then far.
    pub fn bootstrap() -> Self {
        let shared = bootstrap_shared_world();
        let scene = shared.frames.parent(shared.near_object).expect("near parent").expect("scene");
        let mut meshes = MeshLibrary::default();
        let near_mesh = meshes.insert(near_triangle_mesh());
        let far_mesh = meshes.insert(far_triangle_mesh());
        let mut world = Self {
            frames: shared.frames,
            scene,
            meshes,
            imported_meshes: Vec::new(),
            objects: Vec::new(),
            entities: EntityRegistry::default(),
            lights: Vec::new(),
            probes: Vec::new(),
            game_cameras: Vec::new(),
            joints: Vec::new(),
            terrains: Vec::new(),
            player_starts: Vec::new(),
            blocks: Vec::new(),
            derived_meshlets: Vec::new(),
            derived_visuals: HashSet::new(),
            retired_meshes: Vec::new(),
            terrain_dirty: Vec::new(),
            environment: EnvironmentLight::bootstrap(),
            environment_entity: EntityHandle::INVALID,
            front: shared.front,
            side: shared.side,
            anchors: Vec::new(),
            revision: 1,
            lighting_revision: 1,
            probe_policy: crate::ProbeUpdatePolicy::Static,
            probe_recapture_serial: 0,
            simulation_tick: 0,
            startup_camera: None,
            next_object: 0,
            next_light: 0,
            next_probe: 0,
            next_camera: 0,
            next_joint: 0,
        };
        let near = world.insert_object(near_mesh, shared.near_object, Vec3::new(1.0, 1.0, 1.0));
        let far = world.insert_object(far_mesh, shared.far_object, Vec3::new(1.0, 1.0, 1.0));
        world.name_bootstrap(near, "Near Triangle");
        world.name_bootstrap(far, "Far Triangle");
        world.install_bootstrap_lights();
        world.install_bootstrap_probe();
        world.install_world_settings();
        world
    }

    pub fn meshes(&self) -> &MeshLibrary {
        &self.meshes
    }

    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    pub fn front_camera(&self) -> Camera {
        self.front
    }

    pub fn side_camera(&self) -> Camera {
        self.side
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn simulation_tick(&self) -> u64 {
        self.simulation_tick
    }

    pub fn set_simulation_tick(&mut self, tick: u64) {
        self.simulation_tick = tick;
    }

    pub fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.objects.iter().map(|object| object.id)
    }

    /// Persistent id for a bootstrap slot. Stable across edits of that object.
    pub fn entity(&self, id: ObjectId) -> Result<EntityId, EntityError> {
        let handle = self.runtime_handle(id)?;
        self.entities.uuid(handle)
    }

    /// Fast lookup for a bootstrap slot. Not saved. A retired generation fails.
    pub fn runtime_handle(&self, id: ObjectId) -> Result<EntityHandle, EntityError> {
        self.objects.iter().find(|object| object.id == id).map(|object| object.handle).ok_or(EntityError::Missing)
    }

    pub fn frame_of(&self, handle: EntityHandle) -> Result<FrameId, EntityError> {
        self.entities.uuid(handle)?;
        if let Some(object) = self.objects.iter().find(|object| object.handle == handle) {
            return Ok(object.frame);
        }
        if let Some(light) = self.lights.iter().find(|light| light.handle == handle) {
            return Ok(light.frame);
        }
        if let Some(probe) = self.probes.iter().find(|probe| probe.handle == handle) {
            return Ok(probe.frame);
        }
        if let Some((_, frame)) = self.anchors.iter().find(|(anchor, _)| *anchor == handle) {
            return Ok(*frame);
        }
        Err(EntityError::Missing)
    }

    fn class_of(&self, handle: EntityHandle) -> crate::AuthoringClass {
        self.ownership_of_handle(handle).capabilities.authoring_class()
    }

    /// Authoritative composition of one entity. Missing means the uuid is gone.
    pub fn entity_ownership(&self, id: EntityId) -> Result<EntityOwnership, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        Ok(self.ownership_of_handle(handle))
    }

    /// Ordered components this actor owns. Payloads stay in the subsystem lists.
    pub fn component_stack(&self, id: EntityId) -> Result<Vec<ComponentBinding>, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        Ok(self.stack_of_handle(handle))
    }

    fn stack_of_handle(&self, handle: EntityHandle) -> Vec<ComponentBinding> {
        let Ok(entries) = self.entities.membership(handle) else {
            return Vec::new();
        };
        let ownership = self.ownership_of_handle(handle);
        entries.iter().map(|entry| self.binding_for(entry.type_id, &ownership)).collect()
    }

    fn binding_for(&self, type_id: crate::TypeId, ownership: &EntityOwnership) -> ComponentBinding {
        if type_id == crate::TYPE_SPATIAL_FRAME {
            return ComponentBinding::role_only(ComponentRole::Transform);
        }
        if type_id == crate::TYPE_MESH_RENDERER {
            return ComponentBinding {
                role: ComponentRole::MeshRenderer,
                object: ownership.object,
                mesh: ownership.mesh,
                ..ComponentBinding::role_only(ComponentRole::MeshRenderer)
            };
        }
        if type_id == crate::TYPE_DIRECTIONAL_LIGHT {
            return ComponentBinding { role: ComponentRole::DirectionalLight, light: ownership.light, ..ComponentBinding::role_only(ComponentRole::DirectionalLight) };
        }
        if type_id == crate::TYPE_POINT_LIGHT {
            return ComponentBinding { role: ComponentRole::PointLight, light: ownership.light, ..ComponentBinding::role_only(ComponentRole::PointLight) };
        }
        if type_id == crate::TYPE_SPOT_LIGHT {
            return ComponentBinding { role: ComponentRole::SpotLight, light: ownership.light, ..ComponentBinding::role_only(ComponentRole::SpotLight) };
        }
        if type_id == crate::TYPE_REFLECTION_PROBE {
            return ComponentBinding { role: ComponentRole::ReflectionProbe, probe: ownership.probe, ..ComponentBinding::role_only(ComponentRole::ReflectionProbe) };
        }
        if type_id == crate::TYPE_CAMERA {
            return ComponentBinding { role: ComponentRole::Camera, camera: ownership.camera, ..ComponentBinding::role_only(ComponentRole::Camera) };
        }
        if type_id == crate::TYPE_PAWN {
            return ComponentBinding::role_only(ComponentRole::Pawn);
        }
        if type_id == crate::TYPE_FREE_FLY {
            return ComponentBinding::role_only(ComponentRole::FreeFly);
        }
        if type_id == crate::TYPE_JOINT {
            return ComponentBinding { role: ComponentRole::Joint, joint: ownership.joint, ..ComponentBinding::role_only(ComponentRole::Joint) };
        }
        if type_id == crate::TYPE_TERRAIN {
            return ComponentBinding::role_only(ComponentRole::Terrain);
        }
        if type_id == crate::TYPE_PLAYER_START {
            return ComponentBinding::role_only(ComponentRole::PlayerStart);
        }
        if type_id == crate::TYPE_PARAMETRIC_BLOCK {
            return ComponentBinding::role_only(ComponentRole::Block);
        }
        ComponentBinding::role_only(ComponentRole::WorldSettings)
    }

    /// Entities that own this component type. The renderer does not call this.
    pub fn query_component(&self, type_id: crate::TypeId) -> Vec<ComponentQueryHit> {
        self.entities
            .handles()
            .filter_map(|handle| {
                let entries = self.entities.membership(handle).ok()?;
                let entry = entries.iter().find(|entry| entry.type_id == type_id)?;
                let entity = self.entities.uuid(handle).ok()?;
                let ownership = self.ownership_of_handle(handle);
                Some(ComponentQueryHit { entity, slot: entry.slot, binding: self.binding_for(type_id, &ownership) })
            })
            .collect()
    }

    /// Adds a component the type registry allows. A second `One` component is [`AuthoringError::InvalidOperation`].
    pub fn add_component(&mut self, id: EntityId, type_id: crate::TypeId) -> Result<(), AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let info = crate::find_type(type_id).ok_or(AuthoringError::Unsupported)?;
        if info.multiplicity == crate::ComponentMultiplicity::NotAComponent {
            return Err(AuthoringError::Unsupported);
        }
        let owned = self.entities.membership(handle).map_err(|_| AuthoringError::NotFound)?.to_vec();
        if matches!(info.multiplicity, crate::ComponentMultiplicity::One) && owned.iter().any(|entry| entry.type_id == type_id) {
            return Err(AuthoringError::InvalidOperation);
        }
        if info.requires.iter().any(|required| owned.iter().all(|entry| entry.type_id != *required)) {
            return Err(AuthoringError::InvalidOperation);
        }
        if info.rejects.iter().any(|rejected| owned.iter().any(|entry| entry.type_id == *rejected)) {
            return Err(AuthoringError::InvalidOperation);
        }
        if owned.iter().any(|entry| crate::find_type(entry.type_id).is_some_and(|existing| existing.rejects.contains(&type_id))) {
            return Err(AuthoringError::InvalidOperation);
        }
        if type_id == crate::TYPE_ENVIRONMENT {
            return Err(AuthoringError::ProtectedEntity);
        }
        if type_id == crate::TYPE_MESH_RENDERER || type_id == crate::TYPE_TERRAIN || type_id == crate::TYPE_PARAMETRIC_BLOCK {
            return Err(AuthoringError::Unsupported);
        }
        if type_id == crate::TYPE_SPATIAL_FRAME {
            let frame = self.frames.add(Some(self.scene), crate::HighPrecisionPose::IDENTITY).map_err(|_| AuthoringError::InvalidOperation)?;
            self.anchors.push((handle, frame));
        } else if let Some(kind) = Self::light_kind_for(type_id) {
            let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
            self.next_light += 1;
            self.lights.push(WorldLight {
                id: LightId(self.next_light),
                handle,
                frame,
                kind,
                color_linear: [1.0, 1.0, 1.0],
                intensity: 1.0,
                range_m: if kind == LightKind::Directional { 0.0 } else { 8.0 },
                inner_radians: if kind == LightKind::Spot { 0.2 } else { 0.0 },
                outer_radians: if kind == LightKind::Spot { 0.6 } else { 0.0 },
                enabled: true,
                shadow: crate::LightShadowSettings::default(),
            });
        } else if type_id == crate::TYPE_CAMERA {
            let _frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
            self.next_camera += 1;
            self.game_cameras.push(WorldCamera::default_on(CameraId(self.next_camera), handle));
        } else if type_id == crate::TYPE_PAWN || type_id == crate::TYPE_FREE_FLY {
        } else if type_id == crate::TYPE_JOINT {
            let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
            let pose = self.frames.local_pose(frame).map_err(|_| AuthoringError::InvalidOperation)?;
            let record = crate::joint::JointRecord::fixed_at(pose.translation, pose.rotation);
            self.next_joint += 1;
            self.joints.push(WorldJoint { id: JointId(self.next_joint), handle, record });
            self.align_joint_frame(handle)?;
        } else if type_id == crate::TYPE_REFLECTION_PROBE {
            let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
            self.next_probe += 1;
            self.probes.push(WorldProbe {
                id: ProbeId(self.next_probe),
                handle,
                frame,
                radius_m: 8.0,
                priority: 0,
                intensity: 1.0,
                enabled: true,
                resolution: crate::REFLECTION_PROBE_RESOLUTION,
            });
        } else if type_id == crate::TYPE_PLAYER_START {
            let _frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
            self.player_starts.push(WorldPlayerStart {
                handle,
                record: crate::PlayerStartRecord { player: String::new(), preview: false },
            });
        } else {
            return Err(AuthoringError::Unsupported);
        }
        self.entities.add_membership(handle, type_id, 0).map_err(|_| AuthoringError::InvalidOperation)?;
        if Self::light_kind_for(type_id).is_some() {
            self.revise_lighting();
        } else {
            self.revise();
        }
        Ok(())
    }

    /// Drops one membership entry and its subsystem record. Transform stays while a dependent component does.
    pub fn remove_component(&mut self, id: EntityId, type_id: crate::TypeId, slot: u32) -> Result<(), AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        if self.is_world_settings(handle) && type_id == crate::TYPE_ENVIRONMENT {
            return Err(AuthoringError::ProtectedEntity);
        }
        if type_id == crate::TYPE_TERRAIN || type_id == crate::TYPE_PARAMETRIC_BLOCK {
            return Err(AuthoringError::Unsupported);
        }
        let owned = self.entities.membership(handle).map_err(|_| AuthoringError::NotFound)?.to_vec();
        if !owned.iter().any(|entry| entry.type_id == type_id && entry.slot == slot) {
            return Err(AuthoringError::NotFound);
        }
        if owned.iter().any(|entry| entry.type_id != type_id && crate::find_type(entry.type_id).is_some_and(|info| info.requires.contains(&type_id))) {
            return Err(AuthoringError::InvalidOperation);
        }
        let frame = self.frame_of(handle).ok();
        if type_id == crate::TYPE_MESH_RENDERER {
            self.objects.retain(|object| object.handle != handle);
        } else if Self::light_kind_for(type_id).is_some() {
            self.lights.retain(|light| light.handle != handle);
        } else if type_id == crate::TYPE_REFLECTION_PROBE {
            self.probes.retain(|probe| probe.handle != handle);
        } else if type_id == crate::TYPE_CAMERA {
            self.game_cameras.retain(|camera| camera.handle != handle);
        } else if type_id == crate::TYPE_JOINT {
            self.release_joint_to_scene(handle)?;
            self.joints.retain(|joint| joint.handle != handle);
        } else if type_id == crate::TYPE_PLAYER_START {
            self.player_starts.retain(|start| start.handle != handle);
        } else if type_id == crate::TYPE_SPATIAL_FRAME {
            self.anchors.retain(|(anchor, _)| *anchor != handle);
        }
        if type_id != crate::TYPE_SPATIAL_FRAME {
            if let Some(frame) = frame {
                if owned.iter().any(|entry| entry.type_id == crate::TYPE_SPATIAL_FRAME) && self.frame_of(handle).is_err() {
                    self.anchors.push((handle, frame));
                }
            }
        }
        self.entities.remove_membership(handle, type_id, slot).map_err(|_| AuthoringError::NotFound)?;
        if Self::light_kind_for(type_id).is_some() {
            self.revise_lighting();
        } else {
            self.revise();
        }
        Ok(())
    }

    fn grant(&mut self, handle: EntityHandle, type_id: crate::TypeId) {
        if self.entities.membership(handle).ok().is_some_and(|entries| entries.iter().any(|entry| entry.type_id == type_id)) {
            return;
        }
        let _ = self.entities.add_membership(handle, type_id, 0);
    }

fn light_type(kind: LightKind) -> crate::TypeId {
    match kind {
        LightKind::Directional => crate::TYPE_DIRECTIONAL_LIGHT,
        LightKind::Point => crate::TYPE_POINT_LIGHT,
        LightKind::Spot => crate::TYPE_SPOT_LIGHT,
    }
}

fn light_kind_for(type_id: crate::TypeId) -> Option<LightKind> {
    if type_id == crate::TYPE_DIRECTIONAL_LIGHT {
        Some(LightKind::Directional)
    } else if type_id == crate::TYPE_POINT_LIGHT {
        Some(LightKind::Point)
    } else if type_id == crate::TYPE_SPOT_LIGHT {
        Some(LightKind::Spot)
    } else {
        None
    }
}

    /// Live parent uuid. `Ok(None)` is World. A stale stored handle is `InvalidOperation`, not a parent.
    pub fn entity_parent(&self, id: EntityId) -> Result<Option<EntityId>, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        match self.entities.parent(handle).map_err(|_| AuthoringError::NotFound)? {
            None => Ok(None),
            Some(parent) => self.entities.uuid(parent).map(Some).map_err(|_| AuthoringError::InvalidOperation),
        }
    }

    /// Direct children that are saved with the level. Derived terrain chunks are omitted.
    pub fn entity_children(&self, id: EntityId) -> Vec<EntityId> {
        let Ok(handle) = self.entities.find(id) else { return Vec::new() };
        let mut children = Vec::new();
        for child in self.entities.handles() {
            if self.derived_visuals.contains(&child) {
                continue;
            }
            if self.entities.parent(child).ok().flatten() == Some(handle) {
                if let Ok(uuid) = self.entities.uuid(child) {
                    children.push(uuid);
                }
            }
        }
        children
    }

    /// The save record for one entity. Same bytes a level capture would write for it.
    pub fn remember_entity(&self, id: EntityId) -> Result<crate::EntityRecord, crate::LevelError> {
        crate::level::capture_entity(self, id)
    }

    /// Applies one transaction set. Present entities that are missing are spawned first,
    /// existing Present entities are rewritten in place, then Absent entities are destroyed.
    /// World Settings is never destroyed. A block rewrite replaces the record, including its log.
    pub fn apply_mementos(&mut self, mementos: &[crate::EntityMemento]) -> Result<MementoEffect, AuthoringError> {
        let mut effect = MementoEffect::default();
        let mut pending: Vec<crate::EntityRecord> = mementos
            .iter()
            .filter_map(|memento| match memento {
                crate::EntityMemento::Present(record) if self.entities.find(record.uuid).is_err() => Some(record.clone()),
                _ => None,
            })
            .collect();
        let mut guard = pending.len() + 1;
        while !pending.is_empty() {
            guard -= 1;
            if guard == 0 {
                return Err(AuthoringError::InvalidOperation);
            }
            let ready = pending.iter().position(|record| match record.parent_uuid {
                None => true,
                Some(parent) => self.entities.find(parent).is_ok() || !pending.iter().any(|other| other.uuid == parent),
            });
            let Some(index) = ready else { return Err(AuthoringError::InvalidOperation) };
            let record = pending.remove(index);
            self.spawn_remembered(&record)?;
            effect.spawned.push(record.uuid);
        }
        for memento in mementos {
            let crate::EntityMemento::Present(record) = memento else { continue };
            if self.entities.find(record.uuid).is_err() {
                continue;
            }
            if self.conform_present(record)? {
                effect.rebound.push(record.uuid);
            }
        }
        for memento in mementos.iter().rev() {
            let crate::EntityMemento::Absent(id) = memento else { continue };
            if self.entities.find(*id).is_err() || self.authored_world_settings(*id).is_some() {
                continue;
            }
            self.destroy_authored(*id)?;
        }
        Ok(effect)
    }

    fn spawn_remembered(&mut self, record: &crate::EntityRecord) -> Result<(), AuthoringError> {
        let id = self.entities.uuid(self.environment_entity).map_err(|_| AuthoringError::InvalidOperation)?;
        let settings = self.authored_world_settings(id).ok_or(AuthoringError::InvalidOperation)?;
        crate::level::spawn_entity(self, record, &settings).map_err(|_| AuthoringError::InvalidOperation)
    }

    /// Rewrites an existing entity from a save record. Returns whether the material factors changed.
    fn conform_present(&mut self, record: &crate::EntityRecord) -> Result<bool, AuthoringError> {
        if self.remember_entity(record.uuid).ok().as_ref() == Some(record) {
            return Ok(false);
        }
        let id = record.uuid;
        if self.authored_world_settings(id).is_some() {
            let _ = self.set_entity_name(id, &record.name);
            return Ok(false);
        }
        let previous_material = self.authored_block(id).map(|block| block.material).or_else(|| self.authored_mesh(id).map(|mesh| mesh.7));
        let live_kind = self.payload_kind(id);
        let wanted_kind = payload_kind_of(record);
        if live_kind != wanted_kind {
            self.destroy_authored(id)?;
            self.spawn_remembered(record)?;
            return Ok(true);
        }
        let _ = self.set_entity_name(id, &record.name);
        if self.entity_parent(id).ok() != Some(record.parent_uuid) {
            self.reparent_authored(id, record.parent_uuid)?;
        }
        if let Some((translation, rotation, scale)) = record.components.iter().find_map(|component| match component {
            crate::ComponentRecord::Transform { translation, rotation, scale } => Some((*translation, *rotation, *scale)),
            _ => None,
        }) {
            let _ = self.set_entity_local_translation(id, translation);
            let _ = self.set_entity_local_rotation(id, rotation);
            if self.objects.iter().any(|object| self.entities.uuid(object.handle).ok() == Some(id)) {
                let _ = self.set_object_scale(id, scale);
            }
        }
        let mut rebound = false;
        for component in &record.components {
            match component {
                crate::ComponentRecord::Transform { .. } | crate::ComponentRecord::WorldSettings => {}
                crate::ComponentRecord::ParametricBlock(block) => {
                    self.replace_block_record(id, block.clone())?;
                    rebound = previous_material.as_ref() != Some(&block.material);
                }
                crate::ComponentRecord::Terrain(terrain) => {
                    let _ = self.set_authored_terrain(id, terrain.clone());
                }
                crate::ComponentRecord::MeshRenderer { visible, cast_shadows, receive_shadows, mesh, material } => {
                    self.write_mesh_record(id, *visible, *cast_shadows, *receive_shadows, mesh.clone(), material.clone())?;
                    rebound = previous_material.as_ref() != Some(material);
                }
                crate::ComponentRecord::DirectionalLight(light) | crate::ComponentRecord::PointLight(light) | crate::ComponentRecord::SpotLight(light) => {
                    self.write_light_record(id, light);
                }
                crate::ComponentRecord::ReflectionProbe(probe) => self.write_probe_record(id, probe),
                crate::ComponentRecord::Camera(camera) => self.write_camera_record(id, camera),
                crate::ComponentRecord::Joint(joint) => {
                    let Ok(handle) = self.entities.find(id) else { continue };
                    if let Some(slot) = self.joints.iter_mut().find(|slot| slot.handle == handle) {
                        slot.record = *joint;
                    }
                }
                crate::ComponentRecord::PlayerStart(start) => {
                    let _ = self.set_authored_player_start(id, start.clone());
                }
            }
        }
        Ok(rebound)
    }

    fn payload_kind(&self, id: EntityId) -> PayloadKind {
        if self.authored_world_settings(id).is_some() {
            PayloadKind::WorldSettings
        } else if self.authored_block(id).is_some() {
            PayloadKind::Block
        } else if self.authored_terrain(id).is_some() {
            PayloadKind::Terrain
        } else if self.authored_mesh(id).is_some() {
            PayloadKind::Mesh
        } else if self.authored_light(id).is_some() {
            PayloadKind::Light
        } else if self.authored_probe(id).is_some() {
            PayloadKind::Probe
        } else if self.authored_camera(id).is_some() {
            PayloadKind::Camera
        } else if self.authored_joint(id).is_some() {
            PayloadKind::Joint
        } else if self.authored_player_start(id).is_some() {
            PayloadKind::PlayerStart
        } else {
            PayloadKind::Transform
        }
    }

    fn write_mesh_record(
        &mut self,
        id: EntityId,
        visible: bool,
        cast_shadows: bool,
        receive_shadows: bool,
        mesh: crate::MeshAssetRef,
        material: crate::MaterialAssetRef,
    ) -> Result<(), AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let object = self.objects.iter_mut().find(|object| object.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
        object.visible = visible;
        object.cast_shadows = cast_shadows;
        object.receive_shadows = receive_shadows;
        object.authored_mesh = Some(mesh);
        object.authored_material = Some(material);
        self.revise();
        Ok(())
    }

    fn write_light_record(&mut self, id: EntityId, light: &crate::LightRecord) {
        let Ok(handle) = self.entities.find(id) else { return };
        if let Some(slot) = self.lights.iter_mut().find(|slot| slot.handle == handle) {
            slot.enabled = light.enabled;
            slot.color_linear = light.color;
            slot.intensity = light.intensity;
            slot.range_m = light.range_m;
            slot.inner_radians = light.inner_radians;
            slot.outer_radians = light.outer_radians;
            slot.shadow = light.shadow;
        }
        self.revise_lighting();
    }

    fn write_probe_record(&mut self, id: EntityId, probe: &crate::ProbeRecord) {
        let Ok(handle) = self.entities.find(id) else { return };
        if let Some(slot) = self.probes.iter_mut().find(|slot| slot.handle == handle) {
            slot.enabled = probe.enabled;
            slot.radius_m = probe.radius_m;
            slot.intensity = probe.intensity;
            slot.priority = probe.priority;
            slot.resolution = probe.resolution;
        }
        self.revise_lighting();
    }

    fn write_camera_record(&mut self, id: EntityId, camera: &crate::CameraRecord) {
        let Ok(handle) = self.entities.find(id) else { return };
        if let Some(slot) = self.game_cameras.iter_mut().find(|slot| slot.handle == handle) {
            slot.enabled = camera.enabled;
            slot.orthographic = camera.orthographic;
            slot.vertical_fov_deg = camera.vertical_fov_deg;
            slot.ortho_height_m = camera.ortho_height_m;
            slot.near_m = camera.near_m;
            slot.far_m = camera.far_m;
            slot.priority = camera.priority;
            slot.viewport = camera.viewport;
        }
        self.revise();
    }

    fn ownership_of_handle(&self, handle: EntityHandle) -> EntityOwnership {
        let object = self.objects.iter().find(|object| object.handle == handle);
        let light = self.lights.iter().find(|light| light.handle == handle);
        let probe = self.probes.iter().find(|probe| probe.handle == handle);
        let mut capabilities = EntityCapabilities::default();
        capabilities.transform = self.frame_of(handle).is_ok();
        capabilities.mesh_renderer = object.is_some();
        if let Some(light) = light {
            match light.kind {
                LightKind::Directional => capabilities.directional_light = true,
                LightKind::Point => capabilities.point_light = true,
                LightKind::Spot => capabilities.spot_light = true,
            }
        }
        let camera = self.game_cameras.iter().find(|camera| camera.handle == handle);
        let joint = self.joints.iter().find(|joint| joint.handle == handle);
        capabilities.reflection_probe = probe.is_some();
        capabilities.camera = camera.is_some();
        capabilities.joint = joint.is_some();
        capabilities.terrain = self.terrains.iter().any(|terrain| terrain.handle == handle);
        capabilities.player_start = self.player_starts.iter().any(|start| start.handle == handle);
        capabilities.block = self.blocks.iter().any(|block| block.handle == handle);
        capabilities.mesh_renderer = object.is_some() && !capabilities.block;
        capabilities.world_settings = handle == self.environment_entity;
        EntityOwnership {
            capabilities,
            object: object.map(|object| object.id),
            mesh: object.map(|object| object.mesh),
            light: light.map(|light| light.id),
            probe: probe.map(|probe| probe.id),
            camera: camera.map(|camera| camera.id),
            joint: joint.map(|joint| joint.id),
        }
    }

    fn is_world_settings(&self, handle: EntityHandle) -> bool {
        handle == self.environment_entity
    }

    pub fn entities(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.iter()
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// One hierarchy pass. Not a [`RenderSceneSnapshot`].
    pub fn entity_outline(&self) -> Vec<EntityOutlineInfo> {
        self.entities
            .outline()
            .into_iter()
            .filter(|row| self.entities.find(row.uuid).ok().is_none_or(|handle| !self.derived_visuals.contains(&handle)))
            .map(|mut row| {
                if let Ok(handle) = self.entities.find(row.uuid) {
                    row.class = self.class_of(handle);
                }
                row
            })
            .collect()
    }

    /// One uuid lookup, then slot reads. The returned snapshot is not a second world.
    pub fn inspect_entity(&self, id: EntityId) -> Result<EntityInspection, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let name = self.entities.name(handle).map_err(|_| AuthoringError::NotFound)?.to_string();
        let parent = match self.entities.parent(handle).map_err(|_| AuthoringError::NotFound)? {
            Some(parent) => match self.entities.uuid(parent) {
                Ok(parent_id) => Some((parent_id, self.entities.name(parent).map_err(|_| AuthoringError::NotFound)?.to_string())),
                Err(_) => None,
            },
            None => None,
        };
        let entity_type = crate::find_type(TYPE_ENTITY).expect("entity type");
        let mut sections = vec![InspectedSection {
            type_info: *entity_type,
            fields: vec![
                inspected(entity_type, FIELD_NAME, PropertyValue::String(name.clone()), name),
                inspected(entity_type, FIELD_UUID, PropertyValue::Entity(id), id.to_string()),
                inspected(
                    entity_type,
                    FIELD_PARENT,
                    PropertyValue::OptionalEntity(parent.as_ref().map(|(parent_id, _)| *parent_id)),
                    parent.as_ref().map(|(_, parent_name)| parent_name.clone()).unwrap_or_else(|| "World".to_string()),
                ),
            ],
        }];
        if let Ok(frame) = self.frame_of(handle) {
            let pose = self.frames.local_pose(frame).map_err(|_| AuthoringError::InvalidOperation)?;
            let frame_type = crate::find_type(TYPE_SPATIAL_FRAME).expect("frame type");
            sections.push(InspectedSection {
                type_info: *frame_type,
                fields: vec![
                    inspected(
                        frame_type,
                        FIELD_LOCAL_TRANSLATION,
                        PropertyValue::Vec3(pose.translation),
                        crate::format_vec3(pose.translation),
                    ),
                    inspected(
                        frame_type,
                        FIELD_LOCAL_ROTATION,
                        PropertyValue::Quat(pose.rotation),
                        crate::format_quat(pose.rotation),
                    ),
                ],
            });
        }
        if let Some(section) = self.component_section(handle) {
            sections.push(section);
        }
        if let Some(section) = self.joint_section(handle) {
            sections.push(section);
        }
        if let Some(section) = self.terrain_section(handle) {
            sections.push(section);
        }
        if let Some(section) = self.player_start_section(handle) {
            sections.push(section);
        }
        if let Some(section) = self.block_section(handle) {
            sections.push(section);
        }
        let stack = self.stack_of_handle(handle);
        if !stack.is_empty() {
            let info = crate::find_type(crate::TYPE_COMPONENT_STACK).expect("component stack");
            let display = stack.iter().map(|item| item.role.label()).collect::<Vec<_>>().join(", ");
            sections.push(InspectedSection {
                type_info: *info,
                fields: vec![inspected(info, crate::FIELD_COMPONENT_STACK, PropertyValue::String(display.clone()), display)],
            });
        }
        Ok(EntityInspection { entity: id, world_revision: self.revision, sections })
    }

    fn component_section(&self, handle: EntityHandle) -> Option<InspectedSection> {
        let caps = self.ownership_of_handle(handle).capabilities;
        if caps.mesh_renderer {
            let object = self.objects.iter().find(|object| object.handle == handle)?;
            let info = crate::find_type(crate::TYPE_MESH_RENDERER).expect("mesh type");
            let material_name = object.authored_material.as_ref().map(|material| material.name.clone()).unwrap_or_else(|| "Unbound".into());
            let mesh_name = object.authored_mesh.as_ref().map(mesh_choice_name).unwrap_or_else(|| "Triangle".into());
            let uv_scale = object.authored_material.as_ref().map(|material| material.uv_scale).unwrap_or(1.0) as f64;
            let roughness = object.authored_material.as_ref().map(|material| material.roughness).unwrap_or(0.5) as f64;
            let metallic = object.authored_material.as_ref().map(|material| material.metallic).unwrap_or(0.0) as f64;
            let normal_scale = object.authored_material.as_ref().map(|material| material.normal_scale).unwrap_or(1.0) as f64;
            return Some(InspectedSection {
                type_info: *info,
                fields: vec![
                    inspected(info, crate::FIELD_SURFACE, PropertyValue::String(mesh_name.clone()), mesh_name),
                    inspected(info, crate::FIELD_OBJECT_SCALE, PropertyValue::Vec3(object.scale), crate::format_vec3(object.scale)),
                    inspected(info, crate::FIELD_VISIBLE, PropertyValue::Bool(object.visible), bool_text(object.visible)),
                    inspected(info, crate::FIELD_MATERIAL_SLOT, PropertyValue::String(material_name.clone()), material_name),
                    inspected(info, crate::FIELD_UV_SCALE, PropertyValue::F64(uv_scale), crate::format_f64(uv_scale)),
                    inspected(info, crate::FIELD_ROUGHNESS_FACTOR, PropertyValue::F64(roughness), crate::format_f64(roughness)),
                    inspected(info, crate::FIELD_METALLIC_FACTOR, PropertyValue::F64(metallic), crate::format_f64(metallic)),
                    inspected(info, crate::FIELD_NORMAL_SCALE, PropertyValue::F64(normal_scale), crate::format_f64(normal_scale)),
                    inspected(info, crate::FIELD_CAST_SHADOWS, PropertyValue::Bool(object.cast_shadows), bool_text(object.cast_shadows)),
                    inspected(info, crate::FIELD_RECEIVE_SHADOWS, PropertyValue::Bool(object.receive_shadows), bool_text(object.receive_shadows)),
                ],
            });
        }
        if let Some(light) = self.lights.iter().find(|light| light.handle == handle) {
            let type_id = match light.kind {
                LightKind::Directional => crate::TYPE_DIRECTIONAL_LIGHT,
                LightKind::Point => crate::TYPE_POINT_LIGHT,
                LightKind::Spot => crate::TYPE_SPOT_LIGHT,
            };
            let info = crate::find_type(type_id).expect("light type");
            let color = Vec3::new(light.color_linear[0] as f64, light.color_linear[1] as f64, light.color_linear[2] as f64);
            let mut fields = vec![
                inspected(info, crate::FIELD_COLOR, PropertyValue::Vec3(color), crate::format_vec3(color)),
                inspected(info, crate::FIELD_INTENSITY, PropertyValue::F64(light.intensity as f64), crate::format_f64(light.intensity as f64)),
            ];
            if light.kind != LightKind::Directional {
                fields.push(inspected(info, crate::FIELD_RANGE, PropertyValue::F64(light.range_m as f64), crate::format_f64(light.range_m as f64)));
            }
            if light.kind == LightKind::Spot {
                fields.push(inspected(
                    info,
                    crate::FIELD_INNER_CONE,
                    PropertyValue::F64(light.inner_radians as f64),
                    crate::format_f64(light.inner_radians as f64),
                ));
                fields.push(inspected(
                    info,
                    crate::FIELD_OUTER_CONE,
                    PropertyValue::F64(light.outer_radians as f64),
                    crate::format_f64(light.outer_radians as f64),
                ));
            }
            fields.push(inspected(info, crate::FIELD_ENABLED, PropertyValue::Bool(light.enabled), bool_text(light.enabled)));
            push_shadow_fields(&mut fields, info, light.kind, light.shadow);
            return Some(InspectedSection { type_info: *info, fields });
        }
        if let Some(probe) = self.probes.iter().find(|probe| probe.handle == handle) {
            let info = crate::find_type(crate::TYPE_REFLECTION_PROBE).expect("probe type");
            return Some(InspectedSection {
                type_info: *info,
                fields: vec![
                    inspected(info, crate::FIELD_RADIUS, PropertyValue::F64(probe.radius_m), crate::format_f64(probe.radius_m)),
                    inspected(info, crate::FIELD_PRIORITY, PropertyValue::F64(probe.priority as f64), crate::format_f64(probe.priority as f64)),
                    inspected(info, crate::FIELD_INTENSITY, PropertyValue::F64(probe.intensity as f64), crate::format_f64(probe.intensity as f64)),
                    inspected(info, crate::FIELD_ENABLED, PropertyValue::Bool(probe.enabled), bool_text(probe.enabled)),
                    inspected(
                        info,
                        crate::FIELD_UPDATE_MODE,
                        PropertyValue::String(self.probe_policy.label().into()),
                        self.probe_policy.label().into(),
                    ),
                    inspected(
                        info,
                        crate::FIELD_CAPTURE_STATE,
                        PropertyValue::String("Static. Edits do not recapture.".into()),
                        "Static. Edits do not recapture.".into(),
                    ),
                    inspected(
                        info,
                        crate::FIELD_RESOLUTION,
                        PropertyValue::F64(probe.resolution as f64),
                        probe.resolution.to_string(),
                    ),
                ],
            });
        }
        if handle == self.environment_entity {
            let info = crate::find_type(crate::TYPE_ENVIRONMENT).expect("environment type");
            let upper = rgb_vec(self.environment.upper_hemisphere_linear_rgb);
            let lower = rgb_vec(self.environment.lower_hemisphere_linear_rgb);
            return Some(InspectedSection {
                type_info: *info,
                fields: vec![
                    inspected(info, crate::FIELD_UPPER_COLOR, PropertyValue::Vec3(upper), crate::format_vec3(upper)),
                    inspected(info, crate::FIELD_LOWER_COLOR, PropertyValue::Vec3(lower), crate::format_vec3(lower)),
                    inspected(
                        info,
                        crate::FIELD_INTENSITY,
                        PropertyValue::F64(self.environment.intensity as f64),
                        crate::format_f64(self.environment.intensity as f64),
                    ),
                    inspected(info, crate::FIELD_ENABLED, PropertyValue::Bool(self.environment.enabled), bool_text(self.environment.enabled)),
                ],
            });
        }
        if let Some(camera) = self.game_cameras.iter().find(|camera| camera.handle == handle) {
            let info = crate::find_type(crate::TYPE_CAMERA).expect("camera type");
            let projection = if camera.orthographic { "Orthographic" } else { "Perspective" };
            let mut fields = vec![
                inspected(info, crate::FIELD_ENABLED, PropertyValue::Bool(camera.enabled), bool_text(camera.enabled)),
                inspected(info, crate::FIELD_PROJECTION, PropertyValue::String(projection.into()), projection.into()),
            ];
            if camera.orthographic {
                fields.push(inspected(info, crate::FIELD_ORTHO_HEIGHT, PropertyValue::F64(camera.ortho_height_m), crate::format_f64(camera.ortho_height_m)));
                fields.push(inspected(info, crate::FIELD_NEAR_PLANE, PropertyValue::F64(camera.near_m as f64), crate::format_f64(camera.near_m as f64)));
                fields.push(inspected(info, crate::FIELD_FAR_PLANE, PropertyValue::F64(camera.far_m as f64), crate::format_f64(camera.far_m as f64)));
            } else {
                // Perspective stays infinite reversed-Z. Far is stored and is not a clip plane.
                fields.push(inspected(info, crate::FIELD_VERTICAL_FOV, PropertyValue::F64(camera.vertical_fov_deg), crate::format_f64(camera.vertical_fov_deg)));
                fields.push(inspected(info, crate::FIELD_NEAR_PLANE, PropertyValue::F64(camera.near_m as f64), crate::format_f64(camera.near_m as f64)));
            }
            fields.push(inspected(info, crate::FIELD_PRIORITY, PropertyValue::F64(camera.priority as f64), crate::format_f64(camera.priority as f64)));
            fields.push(inspected(
                info,
                crate::FIELD_VIEWPORT,
                PropertyValue::String(format!("{} {} {} {}", camera.viewport[0], camera.viewport[1], camera.viewport[2], camera.viewport[3])),
                format!(
                    "{} {} {} {}",
                    crate::format_f64(camera.viewport[0] as f64),
                    crate::format_f64(camera.viewport[1] as f64),
                    crate::format_f64(camera.viewport[2] as f64),
                    crate::format_f64(camera.viewport[3] as f64)
                ),
            ));
            fields.push(inspected(info, crate::FIELD_CLEAR_POLICY, PropertyValue::String("Sky".into()), "Sky".into()));
            return Some(InspectedSection { type_info: *info, fields });
        }
        None
    }

    fn joint_section(&self, handle: EntityHandle) -> Option<InspectedSection> {
        let joint = self.joints.iter().find(|joint| joint.handle == handle)?;
        let info = crate::find_type(crate::TYPE_JOINT).expect("joint type");
        let record = joint.record;
        let limits = record.limits;
        let kind = record.kind.label().to_string();
        let degrees = |field, radians: f64| {
            let value = radians.to_degrees();
            inspected(info, field, PropertyValue::F64(value), crate::format_f64(value))
        };
        Some(InspectedSection {
            type_info: *info,
            fields: vec![
                inspected(info, crate::FIELD_JOINT_KIND, PropertyValue::String(kind.clone()), kind),
                inspected(info, crate::FIELD_JOINT_REST_TRANSLATION, PropertyValue::Vec3(record.rest_translation), crate::format_vec3(record.rest_translation)),
                inspected(info, crate::FIELD_JOINT_REST_ROTATION, PropertyValue::Quat(record.rest_rotation), crate::format_quat(record.rest_rotation)),
                inspected(info, crate::FIELD_JOINT_AXIS, PropertyValue::Vec3(limits.axis), crate::format_vec3(limits.axis)),
                inspected(info, crate::FIELD_JOINT_SECONDARY_AXIS, PropertyValue::Vec3(limits.secondary_axis), crate::format_vec3(limits.secondary_axis)),
                degrees(crate::FIELD_JOINT_HINGE_MIN, limits.hinge_min),
                degrees(crate::FIELD_JOINT_HINGE_MAX, limits.hinge_max),
                degrees(crate::FIELD_JOINT_SWING, limits.swing),
                degrees(crate::FIELD_JOINT_TWIST_MIN, limits.twist_min),
                degrees(crate::FIELD_JOINT_TWIST_MAX, limits.twist_max),
                degrees(crate::FIELD_JOINT_PRIMARY_MIN, limits.primary_min),
                degrees(crate::FIELD_JOINT_PRIMARY_MAX, limits.primary_max),
                degrees(crate::FIELD_JOINT_SECONDARY_MIN, limits.secondary_min),
                degrees(crate::FIELD_JOINT_SECONDARY_MAX, limits.secondary_max),
                inspected(info, crate::FIELD_JOINT_LINEAR_MIN, PropertyValue::F64(limits.linear_min), crate::format_f64(limits.linear_min)),
                inspected(info, crate::FIELD_JOINT_LINEAR_MAX, PropertyValue::F64(limits.linear_max), crate::format_f64(limits.linear_max)),
                inspected(info, crate::FIELD_JOINT_STIFFNESS, PropertyValue::F64(record.stiffness), crate::format_f64(record.stiffness)),
                inspected(info, crate::FIELD_JOINT_DAMPING, PropertyValue::F64(record.damping), crate::format_f64(record.damping)),
            ],
        })
    }

    fn terrain_section(&self, handle: EntityHandle) -> Option<InspectedSection> {
        let terrain = self.terrains.iter().find(|terrain| terrain.handle == handle)?;
        let info = crate::find_type(crate::TYPE_TERRAIN).expect("terrain type");
        let record = &terrain.record;
        let detail = &record.einstein;
        let height = record.sample_height(0.0, 0.0).unwrap_or(0.0) as f64;
        let number = |field, value: f64| inspected(info, field, PropertyValue::F64(value), crate::format_f64(value));
        let flag = |field, value: bool| inspected(info, field, PropertyValue::Bool(value), bool_text(value));
        Some(InspectedSection {
            type_info: *info,
            fields: vec![
                number(crate::FIELD_TERRAIN_WIDTH, record.width_m as f64),
                number(crate::FIELD_TERRAIN_DEPTH, record.depth_m as f64),
                number(crate::FIELD_TERRAIN_SPACING, record.spacing_m as f64),
                number(crate::FIELD_TERRAIN_CHUNK, record.chunk_m as f64),
                number(crate::FIELD_TERRAIN_HEIGHT, height),
                number(crate::FIELD_TERRAIN_HEIGHT_MIN, record.height_min as f64),
                number(crate::FIELD_TERRAIN_HEIGHT_MAX, record.height_max as f64),
                flag(crate::FIELD_TERRAIN_COLLISION, record.collision),
                flag(crate::FIELD_TERRAIN_LOD, record.lod_enabled),
                inspected(info, crate::FIELD_TERRAIN_MATERIAL, PropertyValue::String(record.material_name.clone()), record.material_name.clone()),
                flag(crate::FIELD_TERRAIN_EINSTEIN, detail.enabled),
                number(crate::FIELD_TERRAIN_SEED, detail.seed as f64),
                number(crate::FIELD_TERRAIN_DENSITY, detail.density as f64),
                number(crate::FIELD_TERRAIN_DISPLACEMENT, detail.max_displacement_m as f64),
                number(crate::FIELD_TERRAIN_ERROR, detail.error_threshold_px as f64),
                number(crate::FIELD_TERRAIN_DISTANCE, detail.distance_m as f64),
                number(crate::FIELD_TERRAIN_CLASS, detail.surface_class as f64),
                inspected(info, crate::FIELD_TERRAIN_CLIFF, PropertyValue::String(detail.cliff.label().into()), detail.cliff.label().into()),
                flag(crate::FIELD_TERRAIN_DEBUG, record.debug_visualization),
                flag(crate::FIELD_TERRAIN_DEBUG_COLORS, detail.debug_colors),
                flag(crate::FIELD_TERRAIN_EINSTEIN_COLLISION, false),
            ],
        })
    }

    fn player_start_section(&self, handle: EntityHandle) -> Option<InspectedSection> {
        let start = self.player_starts.iter().find(|start| start.handle == handle)?;
        let info = crate::find_type(crate::TYPE_PLAYER_START).expect("player start type");
        let player = start.record.player.clone();
        Some(InspectedSection {
            type_info: *info,
            fields: vec![
                inspected(info, crate::FIELD_PLAYER_DEFINITION, PropertyValue::String(player.clone()), if player.is_empty() { "None".into() } else { player }),
                inspected(info, crate::FIELD_PREVIEW_CHARACTER, PropertyValue::Bool(start.record.preview), bool_text(start.record.preview)),
            ],
        })
    }

    fn block_section(&self, handle: EntityHandle) -> Option<InspectedSection> {
        let block = self.blocks.iter().find(|block| block.handle == handle)?;
        let info = crate::find_type(crate::TYPE_PARAMETRIC_BLOCK).expect("block type");
        let size = block.record.size_m;
        let inset = block.record.inset_m;
        let material = block.record.material.name.clone();
        let history = block.record.history_text();
        let number = |field, value: f64| inspected(info, field, PropertyValue::F64(value), crate::format_f64(value));
        Some(InspectedSection {
            type_info: *info,
            fields: vec![
                number(crate::FIELD_BLOCK_SIZE_X, size[0]),
                number(crate::FIELD_BLOCK_SIZE_Y, size[1]),
                number(crate::FIELD_BLOCK_SIZE_Z, size[2]),
                inspected(info, crate::FIELD_BLOCK_ORIGIN, PropertyValue::String("Center".into()), "Center".into()),
                number(crate::FIELD_BLOCK_BEVEL, block.record.bevel_m),
                number(crate::FIELD_BLOCK_INSET_PX, inset[0]),
                number(crate::FIELD_BLOCK_INSET_NX, inset[1]),
                number(crate::FIELD_BLOCK_INSET_PY, inset[2]),
                number(crate::FIELD_BLOCK_INSET_NY, inset[3]),
                number(crate::FIELD_BLOCK_INSET_PZ, inset[4]),
                number(crate::FIELD_BLOCK_INSET_NZ, inset[5]),
                inspected(info, crate::FIELD_BLOCK_HISTORY, PropertyValue::String(history.clone()), history),
                inspected(info, crate::FIELD_BLOCK_COLLISION, PropertyValue::String("Analytic box".into()), "Analytic box".into()),
                inspected(info, crate::FIELD_BLOCK_COLLISION_ENABLED, PropertyValue::String("Yes".into()), "Yes".into()),
                inspected(info, crate::FIELD_BLOCK_MATERIAL, PropertyValue::String(material.clone()), material),
            ],
        })
    }

    pub fn set_entity_name(&mut self, id: EntityId, name: &str) -> Result<AuthoringResult, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        if self.entities.name(handle).map_err(|_| AuthoringError::NotFound)? == name {
            return Ok(AuthoringResult::Unchanged);
        }
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::NotFound)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Local frame translation in meters. Binary64. Not a GPU matrix.
    /// A joint clamps this through the same limits as a rotation edit.
    pub fn set_entity_local_translation(&mut self, id: EntityId, translation: Vec3) -> Result<AuthoringResult, AuthoringError> {
        if !translation.x.is_finite() || !translation.y.is_finite() || !translation.z.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        let pose = self.frames.local_pose(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        self.write_clamped_pose(handle, frame, pose.translation, pose.rotation, translation, pose.rotation)
    }

    /// Local quaternion. Finite and normalizable. A joint clamps it before the write.
    pub fn set_entity_local_rotation(&mut self, id: EntityId, rotation: Quat) -> Result<AuthoringResult, AuthoringError> {
        let rotation = unit_quaternion(rotation)?;
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        let pose = self.frames.local_pose(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        self.write_clamped_pose(handle, frame, pose.translation, pose.rotation, pose.translation, rotation)
    }

    fn write_clamped_pose(
        &mut self,
        handle: EntityHandle,
        frame: FrameId,
        current_translation: Vec3,
        current_rotation: Quat,
        translation: Vec3,
        rotation: Quat,
    ) -> Result<AuthoringResult, AuthoringError> {
        let record = self.joints.iter().find(|joint| joint.handle == handle).map(|joint| joint.record);
        let (translation, rotation) = if let Some(record) = record {
            crate::joint::clamp_joint_pose(&record, translation, rotation)
        } else {
            (translation, rotation)
        };
        let translation_same = current_translation == translation;
        let rotation_same = quaternion_matches(current_rotation, rotation);
        if translation_same && rotation_same {
            return Ok(AuthoringResult::Unchanged);
        }
        if !translation_same {
            self.frames.set_local_translation(frame, translation).map_err(|_| AuthoringError::InvalidOperation)?;
        }
        if !rotation_same {
            self.frames.set_local_rotation(frame, rotation).map_err(|_| AuthoringError::InvalidOperation)?;
        }
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Parent-relative pose. Read-only. A block under the scene uses this as scene-local meters.
    pub fn entity_local_pose(&self, id: EntityId) -> Result<HighPrecisionPose, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.frames.local_pose(frame).map_err(|_| AuthoringError::InvalidOperation)
    }

    /// Resolved root pose. Read-only. Does not revise the world.
    pub fn entity_world_pose(&self, id: EntityId) -> Result<ResolvedPose, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.frames.resolve(frame).map_err(|_| AuthoringError::InvalidOperation)
    }

    /// `local_baseline` plus a world-space delta, rotated into the parent frame.
    /// The delta stays small. It is not added to the billion-meter origin first.
    pub fn local_translation_plus_world_delta(&self, id: EntityId, local_baseline: Vec3, world_delta: Vec3) -> Result<Vec3, AuthoringError> {
        if !local_baseline.x.is_finite() || !local_baseline.y.is_finite() || !local_baseline.z.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        if !world_delta.x.is_finite() || !world_delta.y.is_finite() || !world_delta.z.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let parent_rotation = self.parent_world_rotation(id)?;
        let local_delta = parent_rotation.conjugate().rotate(world_delta);
        Ok(Vec3::new(
            local_baseline.x + local_delta.x,
            local_baseline.y + local_delta.y,
            local_baseline.z + local_delta.z,
        ))
    }

    /// World point expressed as this entity's local translation. Parent rotation is included.
    /// Subtracting two absolute positions at the billion-meter root is only accurate to the f64 ulp there.
    /// A drag uses [`Self::local_translation_plus_world_delta`] so a centimeter is not added to that origin.
    pub fn local_translation_for_world_point(&self, id: EntityId, world_point: Vec3) -> Result<Vec3, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        let parent = self.frames.parent(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        let parent_pose = match parent {
            Some(parent) => self.frames.resolve(parent).map_err(|_| AuthoringError::InvalidOperation)?,
            None => ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
        };
        let delta = Vec3::new(
            world_point.x - parent_pose.translation.x,
            world_point.y - parent_pose.translation.y,
            world_point.z - parent_pose.translation.z,
        );
        Ok(parent_pose.rotation.conjugate().rotate(delta))
    }

    /// World orientation written back as the local quaternion. `world = parent * local`.
    pub fn local_rotation_for_world_rotation(&self, id: EntityId, world_rotation: Quat) -> Result<Quat, AuthoringError> {
        let world_rotation = unit_quaternion(world_rotation)?;
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        let parent = self.frames.parent(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        let parent_rotation = match parent {
            Some(parent) => self.frames.resolve(parent).map_err(|_| AuthoringError::InvalidOperation)?.rotation,
            None => Quat::IDENTITY,
        };
        unit_quaternion(parent_rotation.conjugate().mul(world_rotation))
    }

    fn parent_world_rotation(&self, id: EntityId) -> Result<Quat, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        let parent = self.frames.parent(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        match parent {
            Some(parent) => Ok(self.frames.resolve(parent).map_err(|_| AuthoringError::InvalidOperation)?.rotation),
            None => Ok(Quat::IDENTITY),
        }
    }

    /// Cold path. Scans uuids. Not a per-frame lookup.
    pub fn find_entity(&self, id: EntityId) -> Result<EntityHandle, EntityError> {
        self.entities.find(id)
    }

    pub fn resolve(&self, handle: EntityHandle) -> Result<EntityId, EntityError> {
        self.entities.uuid(handle)
    }

    /// Read-only framing query. Does not revise the world and does not select anything.
    pub fn entity_focus(&self, id: EntityId) -> Result<EntityFocus, FocusError> {
        let handle = self.entities.find(id).map_err(|_| FocusError::Missing)?;
        if let Some(object) = self.objects.iter().find(|object| object.handle == handle) {
            let pose = self.frames.resolve(object.frame).map_err(|_| FocusError::NoSpatial)?;
            let mesh = self.meshes.get(object.mesh).ok_or(FocusError::NoSpatial)?;
            let sphere = mesh.bounds().sphere;
            let center = Vec3::new(
                sphere.center[0] as f64 * object.scale.x,
                sphere.center[1] as f64 * object.scale.y,
                sphere.center[2] as f64 * object.scale.z,
            );
            let max_scale = object.scale.x.abs().max(object.scale.y.abs()).max(object.scale.z.abs());
            let radius = (sphere.radius as f64 * max_scale).max(FOCUS_MIN_RADIUS_M);
            return Ok(EntityFocus {
                origin: pose.translation + pose.rotation.rotate(center),
                radius_m: radius,
                renderable: true,
            });
        }
        if let Some((_, frame)) = self.anchors.iter().find(|(anchor, _)| *anchor == handle) {
            let pose = self.frames.resolve(*frame).map_err(|_| FocusError::NoSpatial)?;
            return Ok(EntityFocus { origin: pose.translation, radius_m: FOCUS_DEFAULT_RADIUS_M, renderable: false });
        }
        if let Some(light) = self.lights.iter().find(|light| light.handle == handle) {
            let pose = self.frames.resolve(light.frame).map_err(|_| FocusError::NoSpatial)?;
            let radius = if light.range_m > 0.0 { light.range_m as f64 } else { FOCUS_DEFAULT_RADIUS_M };
            return Ok(EntityFocus { origin: pose.translation, radius_m: radius.max(FOCUS_MIN_RADIUS_M), renderable: false });
        }
        if let Some(probe) = self.probes.iter().find(|probe| probe.handle == handle) {
            let pose = self.frames.resolve(probe.frame).map_err(|_| FocusError::NoSpatial)?;
            return Ok(EntityFocus { origin: pose.translation, radius_m: probe.radius_m.max(FOCUS_MIN_RADIUS_M), renderable: false });
        }
        Err(FocusError::NoSpatial)
    }

    /// A named entity with a frame and no mesh. Authoring. This is not an editor camera.
    pub fn create_frame_anchor(&mut self, name: &str, local: HighPrecisionPose) -> Result<EntityId, SpaceError> {
        let parent = match self.objects.first() {
            Some(object) => self.frames.parent(object.frame)?,
            None => None,
        };
        let frame = self.frames.add(parent, local)?;
        let handle = self.entities.create();
        self.entities.set_name(handle, name).expect("fresh anchor");
        self.anchors.push((handle, frame));
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        let id = self.entities.uuid(handle).map_err(|_| SpaceError::MissingFrame)?;
        self.revise();
        Ok(id)
    }

    /// An entity with no mesh and no [`ObjectId`]. The outliner still lists it.
    pub fn create_entity(&mut self, name: &str) -> EntityHandle {
        let handle = self.entities.create();
        self.entities.set_name(handle, name).expect("fresh entity");
        self.revise();
        handle
    }

    pub fn rename_entity(&mut self, handle: EntityHandle, name: &str) -> Result<(), EntityError> {
        self.entities.set_name(handle, name)?;
        self.revise();
        Ok(())
    }

    pub fn reparent_entity(&mut self, handle: EntityHandle, parent: Option<EntityHandle>) -> Result<(), AuthoringError> {
        self.entities.uuid(handle).map_err(|_| AuthoringError::NotFound)?;
        if self.is_world_settings(handle) || parent.is_some_and(|parent| self.is_world_settings(parent)) {
            return Err(AuthoringError::ProtectedEntity);
        }
        self.entities.set_parent(handle, parent).map_err(|error| match error {
            EntityError::Cycle => AuthoringError::InvalidOperation,
            EntityError::Missing | EntityError::Stale => AuthoringError::NotFound,
            EntityError::Duplicate => AuthoringError::InvalidOperation,
        })?;
        self.align_joint_frame(handle)?;
        self.revise();
        Ok(())
    }

    /// Removes the entity and the subsystem record that owns it. Children are reparented to World.
    ///
    /// The mesh asset and material instances stay. Frames are left in the graph and are not extracted.
    /// World Settings is refused. The editor does not destroy a GPU object here.
    pub fn retire_entity(&mut self, handle: EntityHandle) -> Result<EntityId, AuthoringError> {
        self.destroy_authored_handle(handle)
    }

    pub fn destroy_authored(&mut self, id: EntityId) -> Result<EntityId, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        self.destroy_authored_handle(handle)
    }

    fn destroy_authored_handle(&mut self, handle: EntityHandle) -> Result<EntityId, AuthoringError> {
        self.entities.uuid(handle).map_err(|_| AuthoringError::NotFound)?;
        if self.is_world_settings(handle) {
            return Err(AuthoringError::ProtectedEntity);
        }
        if self.derived_visuals.contains(&handle) {
            return Err(AuthoringError::Unsupported);
        }
        if let Some(index) = self.terrains.iter().position(|terrain| terrain.handle == handle) {
            self.retire_terrain_visuals(index);
            self.terrains.remove(index);
        }
        let children: Vec<EntityHandle> = self
            .entities
            .handles()
            .filter(|child| self.entities.parent(*child).ok().flatten() == Some(handle))
            .collect();
        for child in children {
            self.entities.set_parent(child, None).map_err(|_| AuthoringError::InvalidOperation)?;
        }
        let block_mesh = self.blocks.iter().find(|block| block.handle == handle).and_then(|_| {
            self.objects.iter().find(|object| object.handle == handle).map(|object| object.mesh)
        });
        self.blocks.retain(|block| block.handle != handle);
        self.objects.retain(|object| object.handle != handle);
        if let Some(mesh) = block_mesh {
            self.retire_unused_mesh(mesh);
        }
        self.lights.retain(|light| light.handle != handle);
        self.probes.retain(|probe| probe.handle != handle);
        self.game_cameras.retain(|camera| camera.handle != handle);
        self.joints.retain(|joint| joint.handle != handle);
        self.player_starts.retain(|start| start.handle != handle);
        self.anchors.retain(|(anchor, _)| *anchor != handle);
        let uuid = self.entities.retire(handle).map_err(|_| AuthoringError::NotFound)?;
        self.revise();
        Ok(uuid)
    }

    /// New uuid and new handle, plus a copied subsystem record when the source has one.
    ///
    /// A mesh shares `MeshId` and material-instance ids. A light gets a new `LightId`.
    /// A probe gets a new `ProbeId` and no renderer capture. World Settings is refused.
    pub fn duplicate_entity(&mut self, handle: EntityHandle) -> Result<EntityHandle, AuthoringError> {
        let created = self.duplicate_authored_handle(handle)?;
        self.entities.find(created).map_err(|_| AuthoringError::NotFound)
    }

    pub fn duplicate_authored(&mut self, id: EntityId) -> Result<EntityId, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        self.duplicate_authored_handle(handle)
    }

    fn duplicate_authored_handle(&mut self, handle: EntityHandle) -> Result<EntityId, AuthoringError> {
        if self.derived_visuals.contains(&handle) || self.terrains.iter().any(|terrain| terrain.handle == handle) {
            return Err(AuthoringError::Unsupported);
        }
        let ownership = self.ownership_of_handle(handle);
        if ownership.capabilities.world_settings {
            return Err(AuthoringError::ProtectedEntity);
        }
        if ownership.capabilities.payload_count() > 1 {
            return Err(AuthoringError::Unsupported);
        }
        let mesh = ownership.object.and_then(|id| {
            self.objects.iter().find(|object| object.id == id).map(|object| {
                (
                    object.mesh,
                    object.frame,
                    object.scale,
                    object.visible,
                    object.cast_shadows,
                    object.receive_shadows,
                    object.bindings.clone(),
                    object.authored_mesh.clone(),
                    object.authored_material.clone(),
                )
            })
        });
        let light = ownership.light.and_then(|id| self.lights.iter().find(|light| light.id == id).map(|light| {
            (
                light.frame,
                light.kind,
                light.color_linear,
                light.intensity,
                light.range_m,
                light.inner_radians,
                light.outer_radians,
                light.enabled,
                light.shadow,
            )
        }));
        let probe = ownership.probe.and_then(|id| self.probes.iter().find(|probe| probe.id == id).map(|probe| {
            (probe.frame, probe.radius_m, probe.priority, probe.intensity, probe.enabled)
        }));
        let game_camera = ownership.camera.and_then(|id| self.game_cameras.iter().find(|camera| camera.id == id).cloned());
        let joint_record = self.joints.iter().find(|joint| joint.handle == handle).map(|joint| joint.record);
        let player_start = self.player_starts.iter().find(|start| start.handle == handle).map(|start| start.record.clone());
        let block_record = self.blocks.iter().find(|block| block.handle == handle).map(|block| block.record.clone());
        let anchor = if ownership.capabilities.transform && ownership.capabilities.payload_count() == 0 {
            self.anchors.iter().find(|(anchor, _)| *anchor == handle).map(|(_, frame)| *frame)
        } else {
            None
        };
        let copy = self.entities.duplicate(handle).map_err(|_| AuthoringError::NotFound)?;
        if let Some((mesh, frame, scale, visible, cast_shadows, receive_shadows, bindings, authored_mesh, authored_material)) = mesh {
            self.copy_frame_record(frame, |world, copied| {
                let created = world.insert_object_with_handle(mesh, copied, scale, copy);
                if let Some(object) = world.objects.iter_mut().find(|object| object.id == created) {
                    object.visible = visible;
                    object.cast_shadows = cast_shadows;
                    object.receive_shadows = receive_shadows;
                    object.bindings = bindings;
                    object.authored_mesh = authored_mesh;
                    object.authored_material = authored_material;
                }
            })?;
        } else if let Some((frame, kind, color, intensity, range_m, inner, outer, enabled, shadow)) = light {
            self.copy_frame_record(frame, |world, copied| {
                world.next_light += 1;
                world.lights.push(WorldLight {
                    id: LightId(world.next_light),
                    handle: copy,
                    frame: copied,
                    kind,
                    color_linear: color,
                    intensity,
                    range_m,
                    inner_radians: inner,
                    outer_radians: outer,
                    enabled,
                    shadow,
                });
            })?;
        } else if let Some((frame, radius_m, priority, intensity, enabled)) = probe {
            self.copy_frame_record(frame, |world, copied| {
                world.next_probe += 1;
                world.probes.push(WorldProbe {
                    id: ProbeId(world.next_probe),
                    handle: copy,
                    frame: copied,
                    radius_m,
                    priority,
                    intensity,
                    enabled,
                    resolution: crate::REFLECTION_PROBE_RESOLUTION,
                });
            })?;
        } else if let Some(camera) = game_camera {
            let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
            self.copy_frame_record(frame, |world, copied| {
                world.anchors.push((copy, copied));
                world.next_camera += 1;
                let mut created = camera;
                created.id = CameraId(world.next_camera);
                created.handle = copy;
                world.game_cameras.push(created);
            })?;
        } else if let Some(frame) = anchor {
            self.copy_frame_record(frame, |world, copied| {
                world.anchors.push((copy, copied));
            })?;
        }
        if let Some(record) = joint_record {
            self.next_joint += 1;
            self.joints.push(WorldJoint { id: JointId(self.next_joint), handle: copy, record });
        }
        if let Some(record) = player_start {
            self.player_starts.push(WorldPlayerStart { handle: copy, record });
        }
        if let Some(record) = block_record {
            let _ = self.entities.remove_membership(copy, crate::TYPE_MESH_RENDERER, 0);
            self.grant(copy, crate::TYPE_PARAMETRIC_BLOCK);
            self.blocks.push(WorldBlock { handle: copy, record });
        }
        self.revise();
        self.entities.uuid(copy).map_err(|_| AuthoringError::NotFound)
    }

    fn copy_frame_record(&mut self, frame: FrameId, write: impl FnOnce(&mut Self, FrameId)) -> Result<(), AuthoringError> {
        let local = self.frames.local_pose(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        let parent = self.frames.parent(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        let copied = self.frames.add(parent, local).map_err(|_| AuthoringError::InvalidOperation)?;
        write(self, copied);
        Ok(())
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Full extraction. Two objects do not justify a dirty set. A later revision can skip unchanged objects.
    pub fn extract(&self, render_frame: RenderFrameId) -> Result<RenderSceneSnapshot, SpaceError> {
        let mut instances = Vec::with_capacity(self.objects.len());
        for object in &self.objects {
            let mesh = self.meshes.get(object.mesh).ok_or(SpaceError::MissingFrame)?;
            instances.push(RenderInstance {
                id: object.render_id,
                entity: self.entities.uuid(object.handle).map_err(|_| SpaceError::MissingFrame)?,
                source: object.id,
                mesh: object.mesh,
                pose: self.frames.resolve(object.frame)?,
                scale: object.scale,
                bounds: mesh.bounds(),
                visible: object.visible,
                cast_shadows: object.cast_shadows,
                receive_shadows: object.receive_shadows,
                material_bindings: object.bindings.clone(),
            });
        }
        let mut cameras = Vec::with_capacity(2);
        for camera in [self.front, self.side] {
            cameras.push(ExtractedCamera {
                frame: camera.frame,
                pose: self.frames.resolve(camera.frame)?,
                vertical_fov_radians: camera.vertical_fov_radians,
                near_m: camera.near_m,
            });
        }
        let mut lights = Vec::new();
        for light in &self.lights {
            if !light.enabled {
                continue;
            }
            let (cos_inner, cos_outer) = if light.kind == LightKind::Spot {
                SpotCone { inner_radians: light.inner_radians, outer_radians: light.outer_radians }.cosines()
            } else {
                (1.0, 1.0)
            };
            lights.push(RenderLight {
                id: light.id,
                kind: light.kind,
                pose: self.frames.resolve(light.frame)?,
                color_linear: light.color_linear,
                intensity: light.intensity,
                range_m: light.range_m,
                cos_inner,
                cos_outer,
                shadow: light.shadow,
            });
        }
        let mut probes = Vec::new();
        for probe in &self.probes {
            probes.push(RenderReflectionProbe {
                id: probe.id,
                translation: self.frames.resolve(probe.frame)?.translation,
                radius_m: probe.radius_m,
                priority: probe.priority,
                intensity: probe.intensity,
                enabled: probe.enabled,
                resolution: probe.resolution,
                mip_count: crate::reflection_probe_mip_count_for(probe.resolution),
            });
        }
        let game_cameras = self.extract_game_cameras()?;
        Ok(RenderSceneSnapshot {
            render_frame,
            simulation_tick: self.simulation_tick,
            world_revision: self.revision,
            lighting_revision: self.lighting_revision,
            probe_policy: self.probe_policy,
            probe_recapture_serial: self.probe_recapture_serial,
            instances,
            cameras,
            game_cameras,
            lights,
            environment: self.environment,
            probes,
        })
    }

    fn extract_game_cameras(&self) -> Result<Vec<ExtractedGameCamera>, SpaceError> {
        let mut cameras = Vec::new();
        for camera in &self.game_cameras {
            let frame = self.frame_of(camera.handle).map_err(|_| SpaceError::MissingFrame)?;
            cameras.push(ExtractedGameCamera {
                entity: self.entities.uuid(camera.handle).map_err(|_| SpaceError::MissingFrame)?,
                id: camera.id,
                pose: self.frames.resolve(frame)?,
                enabled: camera.enabled,
                orthographic: camera.orthographic,
                vertical_fov_radians: camera.vertical_fov_deg.to_radians(),
                ortho_height_m: camera.ortho_height_m,
                near_m: camera.near_m,
                far_m: camera.far_m,
                priority: camera.priority,
                viewport: camera.viewport,
            });
        }
        Ok(cameras)
    }

    /// The frame must already exist. This does not compile a shader or upload a buffer.
    pub fn add_light(
        &mut self,
        frame: FrameId,
        kind: LightKind,
        color_linear: [f32; 3],
        intensity: f32,
    ) -> Result<LightId, SpaceError> {
        self.frames.resolve(frame)?;
        let id = self.insert_light(frame, kind, finite_color(color_linear)?, finite_intensity(intensity)?)?;
        self.revise_lighting();
        Ok(id)
    }

    pub fn set_light_color(&mut self, id: LightId, color_linear: [f32; 3]) -> Result<(), SpaceError> {
        self.light_mut(id)?.color_linear = finite_color(color_linear)?;
        self.revise_lighting();
        Ok(())
    }

    pub fn set_light_intensity(&mut self, id: LightId, intensity: f32) -> Result<(), SpaceError> {
        self.light_mut(id)?.intensity = finite_intensity(intensity)?;
        self.revise_lighting();
        Ok(())
    }

    pub fn set_light_enabled(&mut self, id: LightId, enabled: bool) -> Result<(), SpaceError> {
        self.light_mut(id)?.enabled = enabled;
        self.revise_lighting();
        Ok(())
    }

    /// `0` means no cutoff. Directional lights have no range.
    pub fn set_light_range(&mut self, id: LightId, range_m: f32) -> Result<(), SpaceError> {
        let light = self.light_mut(id)?;
        if light.kind == LightKind::Directional {
            return Err(SpaceError::BadLight);
        }
        light.range_m = finite_range(range_m)?;
        self.revise_lighting();
        Ok(())
    }

    pub fn set_spot_cone(&mut self, id: LightId, cone: SpotCone) -> Result<(), SpaceError> {
        cone.validate()?;
        let light = self.light_mut(id)?;
        if light.kind != LightKind::Spot {
            return Err(SpaceError::BadLight);
        }
        light.inner_radians = cone.inner_radians;
        light.outer_radians = cone.outer_radians;
        self.revise_lighting();
        Ok(())
    }

    pub fn set_light_translation(&mut self, id: LightId, translation: Vec3) -> Result<(), SpaceError> {
        let frame = self.light(id)?.frame;
        self.frames.set_local_translation(frame, translation)?;
        self.revise_lighting();
        Ok(())
    }

    pub fn set_light_rotation(&mut self, id: LightId, rotation: Quat) -> Result<(), SpaceError> {
        let frame = self.light(id)?.frame;
        self.frames.set_local_rotation(frame, rotation)?;
        self.revise_lighting();
        Ok(())
    }

    pub fn light_count(&self) -> usize {
        self.lights.len()
    }

    pub fn light_count_of(&self, kind: LightKind) -> usize {
        self.lights.iter().filter(|light| light.kind == kind).count()
    }

    pub fn light_ids(&self) -> impl Iterator<Item = LightId> + '_ {
        self.lights.iter().map(|light| light.id)
    }

    /// The one environment light. Editing it revises the world. It is not a direct light.
    pub fn environment(&self) -> EnvironmentLight {
        self.environment
    }

    pub fn environment_light_count(&self) -> usize {
        1
    }

    pub fn set_environment_enabled(&mut self, enabled: bool) -> Result<(), SpaceError> {
        self.environment.enabled = enabled;
        self.revise_lighting();
        Ok(())
    }

    pub fn set_environment_intensity(&mut self, intensity: f32) -> Result<(), SpaceError> {
        self.environment.intensity = finite_intensity(intensity)?;
        self.revise_lighting();
        Ok(())
    }

    pub fn set_environment_upper(&mut self, color_linear: [f32; 3]) -> Result<(), SpaceError> {
        self.environment.upper_hemisphere_linear_rgb = finite_color(color_linear)?;
        self.revise_lighting();
        Ok(())
    }

    pub fn set_environment_lower(&mut self, color_linear: [f32; 3]) -> Result<(), SpaceError> {
        self.environment.lower_hemisphere_linear_rgb = finite_color(color_linear)?;
        self.revise_lighting();
        Ok(())
    }

    pub fn probe_update_policy(&self) -> crate::ProbeUpdatePolicy {
        self.probe_policy
    }

    /// Does not itself recapture. The renderer reads this on the next extract.
    pub fn set_probe_update_policy(&mut self, policy: crate::ProbeUpdatePolicy) {
        self.probe_policy = policy;
    }

    /// Manual refresh. Honored in every policy, including Static. Does not move the camera.
    pub fn request_probe_recapture(&mut self) {
        self.probe_recapture_serial = self.probe_recapture_serial.saturating_add(1);
    }

    pub fn probe_recapture_serial(&self) -> u64 {
        self.probe_recapture_serial
    }

    /// 32, 64, 128, or 256. Marks lighting dirty. The renderer keeps the old cube until a budgeted rebuild finishes.
    pub fn set_reflection_probe_resolution(&mut self, resolution: u32) -> Result<(), SpaceError> {
        if !crate::reflection_probe_resolution_supported(resolution) {
            return Err(SpaceError::BadLight);
        }
        for probe in &mut self.probes {
            probe.resolution = resolution;
        }
        self.revise_lighting();
        Ok(())
    }

    /// Emissive and other material light edits are not a transform. The editor calls this.
    pub fn note_lighting_edit(&mut self) {
        self.revise_lighting();
    }

    pub fn reflection_probe_count(&self) -> usize {
        self.probes.len()
    }

    pub fn reflection_probe_enabled(&self) -> bool {
        self.probes.first().is_some_and(|probe| probe.enabled)
    }

    pub fn reflection_probe_ids(&self) -> impl Iterator<Item = ProbeId> + '_ {
        self.probes.iter().map(|probe| probe.id)
    }

    pub fn set_reflection_probe_enabled(&mut self, id: ProbeId, enabled: bool) -> Result<(), SpaceError> {
        let probe = self.probes.iter_mut().find(|probe| probe.id == id).ok_or(SpaceError::BadLight)?;
        probe.enabled = enabled;
        self.revise();
        Ok(())
    }

    /// Scene-local meters. The frame parent is the bootstrap scene, not the billion-meter root.
    pub fn set_authored_light_color(&mut self, id: EntityId, color: Vec3) -> Result<AuthoringResult, AuthoringError> {
        let color = rgb_from_vec(color)?;
        let light = self.light_for_entity(id)?;
        if light.color_linear == color {
            return Ok(AuthoringResult::Unchanged);
        }
        let light_id = light.id;
        self.set_light_color(light_id, color).map_err(|_| AuthoringError::InvalidValue)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_light_intensity(&mut self, id: EntityId, intensity: f64) -> Result<AuthoringResult, AuthoringError> {
        let intensity = f32_from_f64(intensity)?;
        let light = self.light_for_entity(id)?;
        if (light.intensity - intensity).abs() < 1.0e-6 {
            return Ok(AuthoringResult::Unchanged);
        }
        let light_id = light.id;
        self.set_light_intensity(light_id, intensity).map_err(|_| AuthoringError::InvalidValue)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_light_range(&mut self, id: EntityId, range_m: f64) -> Result<AuthoringResult, AuthoringError> {
        let range_m = f32_from_f64(range_m)?;
        let light = self.light_for_entity(id)?;
        if light.kind == LightKind::Directional {
            return Err(AuthoringError::InvalidOperation);
        }
        if (light.range_m - range_m).abs() < 1.0e-6 {
            return Ok(AuthoringResult::Unchanged);
        }
        let light_id = light.id;
        self.set_light_range(light_id, range_m).map_err(|_| AuthoringError::InvalidValue)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_light_enabled(&mut self, id: EntityId, enabled: bool) -> Result<AuthoringResult, AuthoringError> {
        let light = self.light_for_entity(id)?;
        if light.enabled == enabled {
            return Ok(AuthoringResult::Unchanged);
        }
        let light_id = light.id;
        self.set_light_enabled(light_id, enabled).map_err(|_| AuthoringError::InvalidOperation)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_light_shadow(&mut self, id: EntityId, field: crate::FieldId, value: PropertyValue) -> Result<AuthoringResult, AuthoringError> {
        let light = self.light_for_entity(id)?;
        let mut shadow = light.shadow;
        let directional = light.kind == LightKind::Directional;
        match field {
            crate::FIELD_CAST_SHADOWS => {
                let PropertyValue::Bool(cast) = value else { return Err(AuthoringError::WrongType) };
                if shadow.cast == cast {
                    return Ok(AuthoringResult::Unchanged);
                }
                shadow.cast = cast;
            }
            crate::FIELD_SHADOW_RESOLUTION => shadow.resolution = shadow_u32(value)?,
            crate::FIELD_SHADOW_BIAS => shadow.depth_bias_m = shadow_meters(value)?,
            crate::FIELD_SHADOW_SLOPE_BIAS => shadow.slope_bias_m = shadow_meters(value)?,
            crate::FIELD_SHADOW_NORMAL_BIAS => shadow.normal_bias_m = shadow_meters(value)?,
            crate::FIELD_SHADOW_FILTER => shadow.filter_radius = shadow_meters(value)?,
            crate::FIELD_SHADOW_DISTANCE => shadow.distance_m = shadow_meters(value)?,
            crate::FIELD_CASCADE_COUNT if directional => shadow.cascade_count = shadow_u32(value)?.min(crate::MAX_SHADOW_CASCADES as u32),
            crate::FIELD_CASCADE_DISTRIBUTION if directional => {
                let lambda = f32_from_f64(match value {
                    PropertyValue::F64(lambda) => lambda,
                    _ => return Err(AuthoringError::WrongType),
                })?;
                if !(0.0..=1.0).contains(&lambda) {
                    return Err(AuthoringError::InvalidValue);
                }
                shadow.cascade_distribution = lambda;
            }
            crate::FIELD_CASCADE_COUNT | crate::FIELD_CASCADE_DISTRIBUTION => return Err(AuthoringError::InvalidOperation),
            _ => return Err(AuthoringError::ReadOnly),
        }
        let shadow = shadow.finite().map_err(|_| AuthoringError::InvalidValue)?;
        let light = self.light_for_entity(id)?;
        if light.shadow == shadow {
            return Ok(AuthoringResult::Unchanged);
        }
        let light_id = light.id;
        let light = self.light_mut(light_id).map_err(|_| AuthoringError::NotFound)?;
        light.shadow = shadow;
        self.revise_lighting();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_visible(&mut self, id: EntityId, visible: bool) -> Result<AuthoringResult, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let object = self.objects.iter_mut().find(|object| object.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
        if object.visible == visible {
            return Ok(AuthoringResult::Unchanged);
        }
        object.visible = visible;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_probe_resolution(&mut self, id: EntityId, resolution: u32) -> Result<AuthoringResult, AuthoringError> {
        if !crate::reflection_probe_resolution_supported(resolution) {
            return Err(AuthoringError::InvalidValue);
        }
        {
            let probe = self.probe_for_entity_mut(id)?;
            if probe.resolution == resolution {
                return Ok(AuthoringResult::Unchanged);
            }
            probe.resolution = resolution;
        }
        self.revise_lighting();
        Ok(AuthoringResult::Applied)
    }

    /// World policy, stored with world settings. A revision bump dirties the level. It does not recapture by itself.
    pub fn set_authored_probe_policy(&mut self, policy: crate::ProbeUpdatePolicy) -> AuthoringResult {
        if self.probe_policy == policy {
            return AuthoringResult::Unchanged;
        }
        self.probe_policy = policy;
        self.revise();
        AuthoringResult::Applied
    }

    pub fn set_object_scale(&mut self, id: EntityId, scale: Vec3) -> Result<AuthoringResult, AuthoringError> {
        if !scale.x.is_finite() || !scale.y.is_finite() || !scale.z.is_finite() || scale.x == 0.0 || scale.y == 0.0 || scale.z == 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let object = self.objects.iter_mut().find(|object| object.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
        if object.scale == scale {
            return Ok(AuthoringResult::Unchanged);
        }
        object.scale = scale;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn mesh_material_slots(&self, object: ObjectId) -> Vec<u32> {
        let Some(object) = self.objects.iter().find(|candidate| candidate.id == object) else { return Vec::new() };
        let Some(mesh) = self.meshes.get(object.mesh) else { return Vec::new() };
        let mut slots = Vec::new();
        for submesh in mesh.submeshes() {
            if !slots.contains(&submesh.material_slot) {
                slots.push(submesh.material_slot);
            }
        }
        slots
    }

    pub fn place_imported_mesh(&mut self, id: crate::AssetId, name: &str, material: crate::MaterialAssetRef) -> Result<EntityId, AuthoringError> {
        let entity = EntityId::new();
        self.spawn_saved_mesh(
            entity,
            name,
            None,
            HighPrecisionPose::at(0.0, 0.4, -2.0),
            Vec3::new(1.0, 1.0, 1.0),
            true,
            true,
            true,
            crate::MeshAssetRef::Asset { id, name: name.to_string() },
            material,
        )?;
        Ok(entity)
    }

    pub fn place_imported_mesh_at(&mut self, id: crate::AssetId, name: &str, material: crate::MaterialAssetRef, x: f64, y: f64, z: f64) -> Result<EntityId, AuthoringError> {
        let entity = EntityId::new();
        self.spawn_saved_mesh(
            entity,
            name,
            None,
            HighPrecisionPose::at(x, y, z),
            Vec3::new(1.0, 1.0, 1.0),
            true,
            true,
            true,
            crate::MeshAssetRef::Asset { id, name: name.to_string() },
            material,
        )?;
        Ok(entity)
    }

    pub fn install_imported_meshes(&mut self, assets: &crate::MeshAssetLibrary) {
        self.imported_meshes = assets.records().iter().filter_map(|record| assets.get(record.id).map(|mesh| (record.id, record.name.clone(), mesh.clone()))).collect();
    }

    pub fn imported_mesh_names(&self) -> Vec<String> {
        self.imported_meshes.iter().map(|(_, name, _)| name.clone()).collect()
    }

    /// One draw mesh per imported asset. A second actor reuses that `MeshId` instead of copying the vertices.
    fn draw_mesh_id(&mut self, mesh_ref: &crate::MeshAssetRef) -> Result<MeshId, AuthoringError> {
        if let crate::MeshAssetRef::Asset { id, .. } = mesh_ref {
            if let Some(mesh) = self.objects.iter().find_map(|object| match &object.authored_mesh {
                Some(crate::MeshAssetRef::Asset { id: stored, .. }) if stored == id => Some(object.mesh),
                _ => None,
            }) {
                return Ok(mesh);
            }
        }
        Ok(self.meshes.insert(self.materialize_mesh(mesh_ref)?))
    }

    fn materialize_mesh(&self, mesh_ref: &crate::MeshAssetRef) -> Result<Mesh, AuthoringError> {
        match mesh_ref {
            crate::MeshAssetRef::Asset { id, .. } => self.imported_meshes.iter().find(|(stored, _, _)| stored == id).map(|(_, _, mesh)| mesh.clone()).ok_or(AuthoringError::InvalidOperation),
            other => Ok(other.instantiate()),
        }
    }

    pub fn set_authored_mesh_choice(&mut self, id: EntityId, choice: &str) -> Result<AuthoringResult, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let current = self.objects.iter().find(|object| object.handle == handle).and_then(|object| object.authored_mesh.clone()).ok_or(AuthoringError::InvalidOperation)?;
        let chosen = self.imported_meshes.iter().find(|(_, name, _)| name == choice).map(|(id, name, _)| (*id, name.clone()));
        if let Some((asset_id, name)) = chosen {
            let next = crate::MeshAssetRef::Asset { id: asset_id, name };
            if next == current {
                return Ok(AuthoringResult::Unchanged);
            }
            let mesh = self.draw_mesh_id(&next)?;
            let object = self.objects.iter_mut().find(|object| object.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
            object.mesh = mesh;
            object.authored_mesh = Some(next);
            self.revise();
            return Ok(AuthoringResult::Applied);
        }
        let switchable = matches!(mesh_choice_name(&current).as_str(), "Cube" | "Sphere" | "Plane") || matches!(current, crate::MeshAssetRef::Asset { .. });
        if !switchable {
            return Err(AuthoringError::ReadOnly);
        }
        let next = mesh_choice_apply(&current, choice).ok_or(AuthoringError::InvalidValue)?;
        if next == current {
            return Ok(AuthoringResult::Unchanged);
        }
        let mesh = self.meshes.insert(next.instantiate());
        let object = self.objects.iter_mut().find(|object| object.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
        object.mesh = mesh;
        object.authored_mesh = Some(next);
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_mesh_shadow(&mut self, id: EntityId, field: crate::FieldId, value: PropertyValue) -> Result<AuthoringResult, AuthoringError> {
        let PropertyValue::Bool(enabled) = value else { return Err(AuthoringError::WrongType) };
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let object = self.objects.iter_mut().find(|object| object.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
        let current = match field {
            crate::FIELD_CAST_SHADOWS => &mut object.cast_shadows,
            crate::FIELD_RECEIVE_SHADOWS => &mut object.receive_shadows,
            _ => return Err(AuthoringError::ReadOnly),
        };
        if *current == enabled {
            return Ok(AuthoringResult::Unchanged);
        }
        *current = enabled;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_spot_inner(&mut self, id: EntityId, radians: f64) -> Result<AuthoringResult, AuthoringError> {
        let radians = f32_from_f64(radians)?;
        let light = self.light_for_entity(id)?;
        if light.kind != LightKind::Spot {
            return Err(AuthoringError::InvalidOperation);
        }
        let light_id = light.id;
        let outer = light.outer_radians;
        if (light.inner_radians - radians).abs() < 1.0e-6 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.set_spot_cone(light_id, SpotCone { inner_radians: radians, outer_radians: outer }).map_err(|_| AuthoringError::InvalidValue)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_spot_outer(&mut self, id: EntityId, radians: f64) -> Result<AuthoringResult, AuthoringError> {
        let radians = f32_from_f64(radians)?;
        let light = self.light_for_entity(id)?;
        if light.kind != LightKind::Spot {
            return Err(AuthoringError::InvalidOperation);
        }
        let light_id = light.id;
        let inner = light.inner_radians;
        if (light.outer_radians - radians).abs() < 1.0e-6 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.set_spot_cone(light_id, SpotCone { inner_radians: inner, outer_radians: radians }).map_err(|_| AuthoringError::InvalidValue)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_probe_radius(&mut self, id: EntityId, radius_m: f64) -> Result<AuthoringResult, AuthoringError> {
        if !radius_m.is_finite() || radius_m <= 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        {
            let probe = self.probe_for_entity_mut(id)?;
            if (probe.radius_m - radius_m).abs() < 1.0e-9 {
                return Ok(AuthoringResult::Unchanged);
            }
            probe.radius_m = radius_m;
        }
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_probe_priority(&mut self, id: EntityId, priority: f64) -> Result<AuthoringResult, AuthoringError> {
        if !priority.is_finite() || priority.fract() != 0.0 || priority < i32::MIN as f64 || priority > i32::MAX as f64 {
            return Err(AuthoringError::InvalidValue);
        }
        let priority = priority as i32;
        {
            let probe = self.probe_for_entity_mut(id)?;
            if probe.priority == priority {
                return Ok(AuthoringResult::Unchanged);
            }
            probe.priority = priority;
        }
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_probe_intensity(&mut self, id: EntityId, intensity: f64) -> Result<AuthoringResult, AuthoringError> {
        let intensity = f32_from_f64(intensity)?;
        if intensity < 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        {
            let probe = self.probe_for_entity_mut(id)?;
            if (probe.intensity - intensity).abs() < 1.0e-6 {
                return Ok(AuthoringResult::Unchanged);
            }
            probe.intensity = intensity;
        }
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_probe_enabled(&mut self, id: EntityId, enabled: bool) -> Result<AuthoringResult, AuthoringError> {
        {
            let probe = self.probe_for_entity_mut(id)?;
            if probe.enabled == enabled {
                return Ok(AuthoringResult::Unchanged);
            }
            probe.enabled = enabled;
        }
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_environment_upper(&mut self, id: EntityId, color: Vec3) -> Result<AuthoringResult, AuthoringError> {
        self.require_world_settings(id)?;
        let color = rgb_from_vec(color)?;
        if self.environment.upper_hemisphere_linear_rgb == color {
            return Ok(AuthoringResult::Unchanged);
        }
        self.set_environment_upper(color).map_err(|_| AuthoringError::InvalidValue)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_environment_lower(&mut self, id: EntityId, color: Vec3) -> Result<AuthoringResult, AuthoringError> {
        self.require_world_settings(id)?;
        let color = rgb_from_vec(color)?;
        if self.environment.lower_hemisphere_linear_rgb == color {
            return Ok(AuthoringResult::Unchanged);
        }
        self.set_environment_lower(color).map_err(|_| AuthoringError::InvalidValue)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_environment_intensity(&mut self, id: EntityId, intensity: f64) -> Result<AuthoringResult, AuthoringError> {
        self.require_world_settings(id)?;
        let intensity = f32_from_f64(intensity)?;
        if (self.environment.intensity - intensity).abs() < 1.0e-6 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.set_environment_intensity(intensity).map_err(|_| AuthoringError::InvalidValue)?;
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_environment_enabled(&mut self, id: EntityId, enabled: bool) -> Result<AuthoringResult, AuthoringError> {
        self.require_world_settings(id)?;
        if self.environment.enabled == enabled {
            return Ok(AuthoringResult::Unchanged);
        }
        self.set_environment_enabled(enabled).map_err(|_| AuthoringError::InvalidOperation)?;
        Ok(AuthoringResult::Applied)
    }

    fn require_world_settings(&self, id: EntityId) -> Result<(), AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        if handle == self.environment_entity {
            Ok(())
        } else {
            Err(AuthoringError::InvalidOperation)
        }
    }

    fn light_for_entity(&self, id: EntityId) -> Result<&WorldLight, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        self.lights.iter().find(|light| light.handle == handle).ok_or(AuthoringError::InvalidOperation)
    }

    fn probe_for_entity_mut(&mut self, id: EntityId) -> Result<&mut WorldProbe, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        self.probes.iter_mut().find(|probe| probe.handle == handle).ok_or(AuthoringError::InvalidOperation)
    }

    pub fn set_reflection_probe_translation(&mut self, id: ProbeId, translation: Vec3) -> Result<(), SpaceError> {
        let frame = self.probes.iter().find(|probe| probe.id == id).ok_or(SpaceError::BadLight)?.frame;
        self.frames.set_local_translation(frame, translation)?;
        self.revise();
        Ok(())
    }

    pub fn light_intensity(&self, id: LightId) -> Result<f32, SpaceError> {
        Ok(self.light(id)?.intensity)
    }

    pub fn light_enabled(&self, id: LightId) -> Result<bool, SpaceError> {
        Ok(self.light(id)?.enabled)
    }

    /// Bind a mesh slot to a logical material instance. Does not compile or upload.
    pub fn bind_material(&mut self, id: ObjectId, slot: u32, instance: MaterialInstanceId) -> Result<(), SpaceError> {
        let object = self.object_mut(id)?;
        if let Some(binding) = object.bindings.iter_mut().find(|binding| binding.slot == slot) {
            binding.instance = instance;
        } else {
            object.bindings.push(MaterialSlotBinding { slot, instance });
        }
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn clear_material_slot(&mut self, id: ObjectId, slot: u32) -> Result<(), SpaceError> {
        let object = self.object_mut(id)?;
        object.bindings.retain(|binding| binding.slot != slot);
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn add_mesh(&mut self, mesh: Mesh) -> MeshId {
        self.meshes.insert(mesh)
    }

    pub fn set_object_mesh(&mut self, id: ObjectId, mesh: MeshId) -> Result<(), SpaceError> {
        self.object_mut(id)?.mesh = mesh;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn set_visible(&mut self, id: ObjectId, visible: bool) -> Result<(), SpaceError> {
        self.object_mut(id)?.visible = visible;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn set_object_local_translation(&mut self, id: ObjectId, translation: Vec3) -> Result<(), SpaceError> {
        let frame = self.object(id)?.frame;
        self.frames.set_local_translation(frame, translation)?;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    fn name_bootstrap(&mut self, id: ObjectId, name: &str) {
        let handle = self.runtime_handle(id).expect("bootstrap object");
        self.entities.set_name(handle, name).expect("bootstrap name");
        if let Some(object) = self.objects.iter_mut().find(|object| object.id == id) {
            object.authored_mesh = Some(match name {
                "Near Triangle" => MeshAssetRef::NearTriangle,
                "Far Triangle" => MeshAssetRef::FarTriangle,
                _ => return,
            });
        }
    }

    fn revise(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    fn revise_lighting(&mut self) {
        self.revise();
        self.lighting_revision = self.lighting_revision.saturating_add(1);
    }

    /// Scene frame under the billion-meter root. Empty worlds have no objects and still have this frame.
    pub fn scene_frame(&self) -> Result<FrameId, SpaceError> {
        Ok(self.scene)
    }

    /// Engine-owned validation object. New frame, new entity, shared mesh id supplied by the caller.
    pub fn spawn_named_object(&mut self, name: &str, mesh: MeshId, parent: FrameId, local: Vec3, scale: Vec3) -> Result<ObjectId, SpaceError> {
        let frame = self.frames.add(Some(parent), crate::HighPrecisionPose::at(local.x, local.y, local.z))?;
        let id = self.insert_object(mesh, frame, scale);
        let handle = self.runtime_handle(id).map_err(|_| SpaceError::MissingFrame)?;
        self.entities.set_name(handle, name).map_err(|_| SpaceError::MissingFrame)?;
        self.revise();
        Ok(id)
    }

    fn insert_object(&mut self, mesh: MeshId, frame: FrameId, scale: Vec3) -> ObjectId {
        let handle = self.entities.create();
        self.insert_object_with_handle(mesh, frame, scale, handle)
    }

    fn insert_object_with_handle(&mut self, mesh: MeshId, frame: FrameId, scale: Vec3, handle: EntityHandle) -> ObjectId {
        self.next_object += 1;
        let id = ObjectId(self.next_object);
        self.objects.push(WorldObject {
            id,
            handle,
            render_id: RenderInstanceId(self.next_object),
            mesh,
            frame,
            scale,
            visible: true,
            cast_shadows: true,
            receive_shadows: true,
            bindings: Vec::new(),
            authored_mesh: None,
            authored_material: None,
        });
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, crate::TYPE_MESH_RENDERER);
        id
    }

    /// Cameras and the scene frame only. No triangles, lights, probe, or world settings.
    pub fn new_session() -> Self {
        let mut frames = FrameGraph::new();
        let root = frames.add(None, HighPrecisionPose::at(crate::BOOTSTRAP_ROOT_M, 0.0, 0.0)).expect("root");
        let scene = frames.add(Some(root), HighPrecisionPose::IDENTITY).expect("scene");
        let front = frames.add(Some(scene), HighPrecisionPose::IDENTITY).expect("front camera");
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), -0.45).expect("yaw");
        let side = frames
            .add(Some(scene), HighPrecisionPose { translation: Vec3::new(1.6, 0.45, 0.35), rotation: yaw })
            .expect("side camera");
        let camera = |frame| Camera { frame, vertical_fov_radians: 60.0_f64.to_radians(), near_m: 0.1 };
        Self {
            frames,
            scene,
            meshes: MeshLibrary::default(),
            imported_meshes: Vec::new(),
            objects: Vec::new(),
            entities: EntityRegistry::default(),
            lights: Vec::new(),
            probes: Vec::new(),
            game_cameras: Vec::new(),
            joints: Vec::new(),
            terrains: Vec::new(),
            player_starts: Vec::new(),
            blocks: Vec::new(),
            derived_meshlets: Vec::new(),
            derived_visuals: HashSet::new(),
            retired_meshes: Vec::new(),
            terrain_dirty: Vec::new(),
            environment: EnvironmentLight::bootstrap(),
            environment_entity: EntityHandle::INVALID,
            front: camera(front),
            side: camera(side),
            anchors: Vec::new(),
            revision: 1,
            lighting_revision: 1,
            probe_policy: crate::ProbeUpdatePolicy::Static,
            probe_recapture_serial: 0,
            simulation_tick: 0,
            startup_camera: None,
            next_object: 0,
            next_light: 0,
            next_probe: 0,
            next_camera: 0,
            next_joint: 0,
        }
    }

    /// Makes this execution copy draw with the source world's mesh, material, light, and probe ids.
    ///
    /// The records stay here. The source world is not written. The renderer keys GPU resources by
    /// those ids, so a fresh instantiate would otherwise miss the resident meshes and drop the probe.
    pub fn alias_draw_keys_from(&mut self, source: &SceneWorld) {
        let entities: Vec<EntityId> = self.entity_outline().iter().map(|row| row.uuid).collect();
        for entity in entities {
            let Ok(source_handle) = source.entities.find(entity) else { continue };
            let Ok(handle) = self.entities.find(entity) else { continue };
            if let Some(source_object) = source.objects.iter().find(|object| object.handle == source_handle) {
                let mesh = source.meshes.get(source_object.mesh).cloned();
                if let Some(object) = self.objects.iter_mut().find(|object| object.handle == handle) {
                    object.mesh = source_object.mesh;
                    object.render_id = source_object.render_id;
                    object.bindings = source_object.bindings.clone();
                }
                if let Some(mesh) = mesh {
                    self.meshes.insert_exact(source_object.mesh, mesh);
                }
                if let Some(set) = source.derived_meshlets(source_object.mesh).cloned() {
                    self.store_derived_meshlets(source_object.mesh, set);
                }
            }
            if let Some(source_light) = source.lights.iter().find(|light| light.handle == source_handle) {
                if let Some(light) = self.lights.iter_mut().find(|light| light.handle == handle) {
                    light.id = source_light.id;
                    if source_light.id.0 > self.next_light {
                        self.next_light = source_light.id.0;
                    }
                }
            }
            if let Some(source_probe) = source.probes.iter().find(|probe| probe.handle == source_handle) {
                if let Some(probe) = self.probes.iter_mut().find(|probe| probe.handle == handle) {
                    probe.id = source_probe.id;
                    if source_probe.id.0 > self.next_probe {
                        self.next_probe = source_probe.id.0;
                    }
                }
            }
            self.alias_terrain_chunks(source, entity);
        }
        self.revision = source.revision;
        self.lighting_revision = source.lighting_revision;
        self.probe_recapture_serial = source.probe_recapture_serial;
        self.probe_policy = source.probe_policy;
    }

    pub fn startup_camera(&self) -> Option<EntityId> {
        self.startup_camera
    }

    /// Names the runtime camera. `None` clears it. The entity must already own Camera.
    /// This does not scan for another camera and does not touch lighting or probes.
    pub fn set_startup_camera(&mut self, camera: Option<EntityId>) -> Result<AuthoringResult, AuthoringError> {
        if self.startup_camera == camera {
            return Ok(AuthoringResult::Unchanged);
        }
        if let Some(id) = camera {
            if !id.is_persistent() || self.authored_camera(id).is_none() {
                return Err(AuthoringError::InvalidOperation);
            }
        }
        self.startup_camera = camera;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_saved_probe_policy(&mut self, policy: crate::ProbeUpdatePolicy) {
        self.probe_policy = policy;
    }

    pub fn material_instance(&self, id: ObjectId, slot: u32) -> Option<MaterialInstanceId> {
        self.objects.iter().find(|object| object.id == id).and_then(|object| object.bindings.iter().find(|binding| binding.slot == slot).map(|binding| binding.instance))
    }

    pub fn replace_authored_material(&mut self, id: ObjectId, material: MaterialAssetRef) -> Result<(), SpaceError> {
        let object = self.object_mut(id)?;
        object.authored_material = Some(material);
        self.revise();
        Ok(())
    }

    pub fn set_authored_assets(&mut self, id: ObjectId, mesh: MeshAssetRef, material: MaterialAssetRef) -> Result<(), SpaceError> {
        let object = self.object_mut(id)?;
        object.authored_mesh = Some(mesh);
        object.authored_material = Some(material);
        Ok(())
    }

    pub fn authored_local_pose(&self, id: EntityId) -> Option<(Vec3, Quat)> {
        let handle = self.entities.find(id).ok()?;
        let frame = self.frame_of(handle).ok()?;
        let pose = self.frames.local_pose(frame).ok()?;
        Some((pose.translation, pose.rotation))
    }

    pub fn authored_mesh(&self, id: EntityId) -> Option<(Vec3, Quat, Vec3, bool, bool, bool, MeshAssetRef, MaterialAssetRef)> {
        let handle = self.entities.find(id).ok()?;
        let object = self.objects.iter().find(|object| object.handle == handle)?;
        let pose = self.frames.local_pose(object.frame).ok()?;
        Some((
            pose.translation,
            pose.rotation,
            object.scale,
            object.visible,
            object.cast_shadows,
            object.receive_shadows,
            object.authored_mesh.clone()?,
            object.authored_material.clone()?,
        ))
    }

    pub fn authored_light(&self, id: EntityId) -> Option<(LightKind, LightRecord)> {
        let handle = self.entities.find(id).ok()?;
        let light = self.lights.iter().find(|light| light.handle == handle)?;
        Some((
            light.kind,
            LightRecord {
                enabled: light.enabled,
                color: light.color_linear,
                intensity: light.intensity,
                range_m: light.range_m,
                inner_radians: light.inner_radians,
                outer_radians: light.outer_radians,
                shadow: light.shadow,
            },
        ))
    }

    pub fn authored_camera(&self, id: EntityId) -> Option<CameraRecord> {
        let handle = self.entities.find(id).ok()?;
        let camera = self.game_cameras.iter().find(|camera| camera.handle == handle)?;
        Some(CameraRecord {
            enabled: camera.enabled,
            orthographic: camera.orthographic,
            vertical_fov_deg: camera.vertical_fov_deg,
            ortho_height_m: camera.ortho_height_m,
            near_m: camera.near_m,
            far_m: camera.far_m,
            priority: camera.priority,
            viewport: camera.viewport,
        })
    }

    pub fn terrain_count(&self) -> usize {
        self.terrains.len()
    }

    pub fn terrain_entity(&self) -> Option<EntityId> {
        let terrain = self.terrains.first()?;
        self.entities.uuid(terrain.handle).ok()
    }

    /// The terrain actor that owns `id`, including when `id` is a derived chunk.
    pub fn terrain_owner(&self, id: EntityId) -> Option<EntityId> {
        let handle = self.entities.find(id).ok()?;
        if let Some(terrain) = self.terrains.iter().find(|terrain| terrain.handle == handle || terrain.chunks.iter().any(|chunk| chunk.handle == handle)) {
            return self.entities.uuid(terrain.handle).ok();
        }
        None
    }

    pub fn terrain_chunk_objects(&self) -> Vec<ObjectId> {
        self.terrains.iter().flat_map(|terrain| terrain.chunks.iter().map(|chunk| chunk.object)).collect()
    }

    pub fn take_retired_meshes(&mut self) -> Vec<MeshId> {
        std::mem::take(&mut self.retired_meshes)
    }

    fn retire_unused_mesh(&mut self, mesh: MeshId) {
        if self.objects.iter().any(|object| object.mesh == mesh) {
            return;
        }
        if self.terrains.iter().any(|terrain| terrain.chunks.iter().any(|chunk| chunk.mesh == mesh)) {
            return;
        }
        self.retired_meshes.push(mesh);
    }

    pub fn authored_terrain(&self, id: EntityId) -> Option<crate::TerrainRecord> {
        let index = self.terrain_index(id).ok()?;
        Some(self.terrains[index].record.clone())
    }

    pub fn set_authored_terrain(&mut self, id: EntityId, record: crate::TerrainRecord) -> Result<AuthoringResult, AuthoringError> {
        record.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let index = self.terrain_index(id)?;
        if self.terrains[index].record == record {
            return Ok(AuthoringResult::Unchanged);
        }
        let geometry = {
            let current = &self.terrains[index].record;
            current.heights != record.heights
                || current.width_m.to_bits() != record.width_m.to_bits()
                || current.depth_m.to_bits() != record.depth_m.to_bits()
                || current.spacing_m.to_bits() != record.spacing_m.to_bits()
                || current.chunk_m.to_bits() != record.chunk_m.to_bits()
        };
        self.terrains[index].record = record;
        if geometry {
            self.rebuild_terrain_chunks(index)?;
        }
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Runtime mesh of one entity. A block's mesh is derived from its size.
    pub fn object_mesh(&self, id: EntityId) -> Option<MeshId> {
        let handle = self.entities.find(id).ok()?;
        self.objects.iter().find(|object| object.handle == handle).map(|object| object.mesh)
    }

    /// Clusters for one derived mesh. Empty when the mesh is not a procedural surface.
    pub fn derived_meshlets(&self, id: MeshId) -> Option<&crate::MeshletSet> {
        self.derived_meshlets.iter().find(|(stored, _)| *stored == id).map(|(_, set)| set)
    }

    fn store_derived_meshlets(&mut self, id: MeshId, set: crate::MeshletSet) {
        if let Some(slot) = self.derived_meshlets.iter_mut().find(|(stored, _)| *stored == id) {
            slot.1 = set;
        } else {
            self.derived_meshlets.push((id, set));
        }
    }

    fn attach_derived_meshlets(&mut self, id: MeshId) {
        let Some(set) = self.meshes.get(id).map(crate::build_meshlets) else { return };
        self.store_derived_meshlets(id, set);
    }

    fn forget_derived_meshlets(&mut self, id: MeshId) {
        self.derived_meshlets.retain(|(stored, _)| *stored != id);
    }

    pub fn authored_block(&self, id: EntityId) -> Option<crate::BlockRecord> {
        let handle = self.entities.find(id).ok()?;
        self.blocks.iter().find(|block| block.handle == handle).map(|block| block.record.clone())
    }

    /// Replaces the saved solid, including its log. Does not append an operation.
    pub fn replace_block_record(&mut self, id: EntityId, record: crate::BlockRecord) -> Result<AuthoringResult, AuthoringError> {
        record.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let index = self.block_index(id)?;
        let current = &self.blocks[index].record;
        if current == &record {
            return Ok(AuthoringResult::Unchanged);
        }
        let mesh_changed = current.size_m != record.size_m || current.inset_m != record.inset_m || current.bevel_m != record.bevel_m;
        self.blocks[index].record = record;
        if mesh_changed {
            self.rebuild_block_mesh(index)?;
        }
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Scene-local analytic boxes. The derived triangle mesh is not the solid.
    pub fn block_solids(&self) -> Vec<crate::BlockSolid> {
        let Ok(scene_pose) = self.frames.resolve(self.scene) else { return Vec::new() };
        let mut solids = Vec::new();
        for block in &self.blocks {
            let Ok(frame) = self.frame_of(block.handle) else { continue };
            let Ok(resolved) = self.frames.resolve(frame) else { continue };
            let delta = Vec3::new(
                resolved.translation.x - scene_pose.translation.x,
                resolved.translation.y - scene_pose.translation.y,
                resolved.translation.z - scene_pose.translation.z,
            );
            solids.push(crate::BlockSolid {
                translation: scene_pose.rotation.conjugate().rotate(delta),
                rotation: scene_pose.rotation.conjugate().mul(resolved.rotation),
                size_m: block.record.size_m,
            });
        }
        solids
    }

    pub fn separate_from_blocks(&self, point: Vec3) -> Vec3 {
        crate::keep_outside_blocks(point, &self.block_solids(), crate::FLY_COLLISION_RADIUS_M)
    }

    /// One parametric block. `local` is scene-local meters. Size is the solid, not entity scale.
    pub fn create_block(&mut self, local: Vec3, record: crate::BlockRecord) -> Result<EntityId, AuthoringError> {
        if !local.x.is_finite() || !local.y.is_finite() || !local.z.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        record.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let handle = self.entities.create();
        let name = if self.blocks.is_empty() { "Block".to_string() } else { format!("Block {}", self.blocks.len() + 1) };
        self.entities.set_name(handle, &name).map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), HighPrecisionPose::at(local.x, local.y, local.z)).map_err(|_| AuthoringError::InvalidOperation)?;
        self.install_block(handle, frame, record)?;
        self.revise();
        self.entities.uuid(handle).map_err(|_| AuthoringError::NotFound)
    }

    pub fn spawn_saved_block(
        &mut self,
        id: EntityId,
        name: &str,
        parent: Option<EntityId>,
        local: HighPrecisionPose,
        record: crate::BlockRecord,
    ) -> Result<(), AuthoringError> {
        record.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let parent_handle = self.parent_handle(parent)?;
        let handle = self.entities.insert(id, parent_handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), local).map_err(|_| AuthoringError::InvalidOperation)?;
        self.install_block(handle, frame, record)?;
        self.revise();
        Ok(())
    }

    pub fn set_block_field(&mut self, id: EntityId, field: crate::FieldId, value: PropertyValue) -> Result<AuthoringResult, AuthoringError> {
        if let Some(axis) = block_size_axis(field) {
            let PropertyValue::F64(meters) = value else { return Err(AuthoringError::WrongType) };
            return self.set_block_extent(id, axis, meters);
        }
        if field == crate::FIELD_BLOCK_BEVEL {
            let PropertyValue::F64(meters) = value else { return Err(AuthoringError::WrongType) };
            return self.set_block_bevel(id, meters);
        }
        if let Some(face) = block_inset_face(field) {
            let PropertyValue::F64(meters) = value else { return Err(AuthoringError::WrongType) };
            return self.set_block_inset(id, face, meters);
        }
        Err(AuthoringError::ReadOnly)
    }

    pub fn set_block_extent(&mut self, id: EntityId, axis: usize, meters: f64) -> Result<AuthoringResult, AuthoringError> {
        if axis > 2 || !meters.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let meters = meters.clamp(crate::BLOCK_MIN_EXTENT_M, crate::BLOCK_MAX_EXTENT_M);
        let index = self.block_index(id)?;
        if (self.blocks[index].record.size_m[axis] - meters).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.size_m[axis] = meters;
        self.blocks[index].record.clamp_features();
        let size = self.blocks[index].record.size_m;
        self.blocks[index].record.push_op(crate::BlockOp::Size { size_m: size });
        self.rebuild_block_mesh(index)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Absolute face push from a drag baseline. The opposite face stays. History is written by [`Self::commit_block_face`].
    pub fn push_block_face(
        &mut self,
        id: EntityId,
        face: u8,
        baseline_size: [f64; 3],
        baseline_local: Vec3,
        outward_m: f64,
    ) -> Result<AuthoringResult, AuthoringError> {
        let index = self.block_index(id)?;
        let local = self.entity_local_pose(id)?;
        let pushed = crate::push_face(baseline_size, baseline_local, local.rotation, face, outward_m).ok_or(AuthoringError::InvalidValue)?;
        let same_size = (0..3).all(|axis| (self.blocks[index].record.size_m[axis] - pushed.size_m[axis]).abs() < 1.0e-9);
        let same_place = translation_matches(local.translation, pushed.translation);
        if same_size && same_place {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.size_m = pushed.size_m;
        self.blocks[index].record.clamp_features();
        self.rebuild_block_mesh(index)?;
        self.write_block_translation(id, pushed.translation)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Records one face push after the drag releases. The mesh is already at the new size.
    pub fn commit_block_face(&mut self, id: EntityId, face: u8, baseline_size: [f64; 3]) -> Result<AuthoringResult, AuthoringError> {
        let index = self.block_index(id)?;
        let axis = (face / 2) as usize;
        if face > 5 || axis > 2 {
            return Err(AuthoringError::InvalidValue);
        }
        let distance = self.blocks[index].record.size_m[axis] - baseline_size[axis];
        if distance.abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.push_op(crate::BlockOp::ExtrudeFace { face, distance_m: distance });
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_block_inset(&mut self, id: EntityId, face: u8, meters: f64) -> Result<AuthoringResult, AuthoringError> {
        if face > 5 || !meters.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let index = self.block_index(id)?;
        let limit = crate::feature_limit(self.blocks[index].record.size_m);
        let meters = meters.clamp(0.0, limit);
        if (self.blocks[index].record.inset_m[face as usize] - meters).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.inset_m[face as usize] = meters;
        self.blocks[index].record.push_op(crate::BlockOp::InsetFace { face, distance_m: meters });
        self.rebuild_block_mesh(index)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_block_bevel(&mut self, id: EntityId, meters: f64) -> Result<AuthoringResult, AuthoringError> {
        if !meters.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let index = self.block_index(id)?;
        let limit = crate::feature_limit(self.blocks[index].record.size_m);
        let meters = meters.clamp(0.0, limit);
        if (self.blocks[index].record.bevel_m - meters).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.bevel_m = meters;
        self.blocks[index].record.push_op(crate::BlockOp::Bevel { distance_m: meters });
        self.rebuild_block_mesh(index)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Live bevel. The mesh updates. History waits for [`Self::commit_block_bevel`].
    pub fn preview_block_bevel(&mut self, id: EntityId, meters: f64) -> Result<AuthoringResult, AuthoringError> {
        if !meters.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let index = self.block_index(id)?;
        let limit = crate::feature_limit(self.blocks[index].record.size_m);
        let meters = meters.clamp(0.0, limit);
        if (self.blocks[index].record.bevel_m - meters).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.bevel_m = meters;
        self.rebuild_block_mesh(index)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Live inset on one face. History waits for [`Self::commit_block_inset`].
    pub fn preview_block_inset(&mut self, id: EntityId, face: u8, meters: f64) -> Result<AuthoringResult, AuthoringError> {
        if face > 5 || !meters.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let index = self.block_index(id)?;
        let limit = crate::feature_limit(self.blocks[index].record.size_m);
        let meters = meters.clamp(0.0, limit);
        if (self.blocks[index].record.inset_m[face as usize] - meters).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.inset_m[face as usize] = meters;
        self.rebuild_block_mesh(index)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// One bevel entry when the preview differs from the value the session started with.
    pub fn commit_block_bevel(&mut self, id: EntityId, baseline_m: f64) -> Result<AuthoringResult, AuthoringError> {
        let index = self.block_index(id)?;
        let current = self.blocks[index].record.bevel_m;
        if (current - baseline_m).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.push_op(crate::BlockOp::Bevel { distance_m: current });
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// One inset entry when the preview differs from the value the session started with.
    pub fn commit_block_inset(&mut self, id: EntityId, face: u8, baseline_m: f64) -> Result<AuthoringResult, AuthoringError> {
        if face > 5 {
            return Err(AuthoringError::InvalidValue);
        }
        let index = self.block_index(id)?;
        let current = self.blocks[index].record.inset_m[face as usize];
        if (current - baseline_m).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        self.blocks[index].record.push_op(crate::BlockOp::InsetFace { face, distance_m: current });
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Size returns to 2 m and the feature parameters clear. The entity stays where it is.
    pub fn reset_block_shape(&mut self, id: EntityId) -> Result<AuthoringResult, AuthoringError> {
        let index = self.block_index(id)?;
        let material = self.blocks[index].record.material.clone();
        let plain = self.blocks[index].record.size_m == [2.0, 2.0, 2.0] && self.blocks[index].record.is_plain();
        if plain {
            return Ok(AuthoringResult::Unchanged);
        }
        let mut record = crate::BlockRecord::standard([2.0, 2.0, 2.0]).map_err(|_| AuthoringError::InvalidValue)?;
        record.material = material;
        self.blocks[index].record = record;
        self.rebuild_block_mesh(index)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Independent copy sharing the positive face. Insets on that axis are exchanged on the copy only.
    pub fn mirror_block(&mut self, id: EntityId, axis: usize) -> Result<EntityId, AuthoringError> {
        if axis > 2 {
            return Err(AuthoringError::InvalidValue);
        }
        let source = self.authored_block(id).ok_or(AuthoringError::InvalidOperation)?;
        let local = self.entity_local_pose(id)?;
        let created = self.duplicate_authored(id)?;
        let index = self.block_index(created)?;
        let insets = crate::swap_insets(source.inset_m, axis).ok_or(AuthoringError::InvalidValue)?;
        self.blocks[index].record.inset_m = insets;
        self.blocks[index].record.push_op(crate::BlockOp::Mirror { axis: axis as u8 });
        let translation = crate::mirrored_translation(local.translation, local.rotation, source.size_m, axis).ok_or(AuthoringError::InvalidValue)?;
        self.rebuild_block_mesh(index)?;
        self.write_block_translation(created, translation)?;
        self.revise();
        Ok(created)
    }

    /// Lowest corner onto scene Y = 0. The rotation stays.
    pub fn align_block_to_ground(&mut self, id: EntityId) -> Result<AuthoringResult, AuthoringError> {
        let index = self.block_index(id)?;
        let local = self.entity_local_pose(id)?;
        let translation = crate::align_translation_to_ground(local.translation, local.rotation, self.blocks[index].record.size_m);
        if translation_matches(local.translation, translation) {
            return Ok(AuthoringResult::Unchanged);
        }
        self.write_block_translation(id, translation)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Puts the drag baseline back. Does not append a history entry.
    pub fn restore_block_drag(
        &mut self,
        id: EntityId,
        size_m: [f64; 3],
        inset_m: [f64; 6],
        bevel_m: f64,
        translation: Vec3,
    ) -> Result<AuthoringResult, AuthoringError> {
        if size_m.iter().any(|axis| !axis.is_finite()) || inset_m.iter().any(|value| !value.is_finite()) || !bevel_m.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let index = self.block_index(id)?;
        let local = self.entity_local_pose(id)?;
        let record = &self.blocks[index].record;
        let same_size = (0..3).all(|axis| (record.size_m[axis] - size_m[axis]).abs() < 1.0e-9);
        let same_inset = (0..6).all(|face| (record.inset_m[face] - inset_m[face]).abs() < 1.0e-9);
        let same_bevel = (record.bevel_m - bevel_m).abs() < 1.0e-9;
        if same_size && same_inset && same_bevel && translation_matches(local.translation, translation) {
            return Ok(AuthoringResult::Unchanged);
        }
        let size = [
            size_m[0].clamp(crate::BLOCK_MIN_EXTENT_M, crate::BLOCK_MAX_EXTENT_M),
            size_m[1].clamp(crate::BLOCK_MIN_EXTENT_M, crate::BLOCK_MAX_EXTENT_M),
            size_m[2].clamp(crate::BLOCK_MIN_EXTENT_M, crate::BLOCK_MAX_EXTENT_M),
        ];
        self.blocks[index].record.size_m = size;
        self.blocks[index].record.inset_m = inset_m;
        self.blocks[index].record.bevel_m = bevel_m;
        self.blocks[index].record.clamp_features();
        self.rebuild_block_mesh(index)?;
        self.write_block_translation(id, translation)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Rounds the local translation. Does not change a gizmo drag.
    pub fn snap_block_translation(&mut self, id: EntityId, step_m: f64) -> Result<AuthoringResult, AuthoringError> {
        let _ = self.block_index(id)?;
        let local = self.entity_local_pose(id)?;
        let translation = crate::snap_translation(local.translation, step_m);
        if translation_matches(local.translation, translation) {
            return Ok(AuthoringResult::Unchanged);
        }
        self.write_block_translation(id, translation)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    fn block_index(&self, id: EntityId) -> Result<usize, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        self.blocks.iter().position(|block| block.handle == handle).ok_or(AuthoringError::InvalidOperation)
    }

    fn write_block_translation(&mut self, id: EntityId, translation: Vec3) -> Result<(), AuthoringError> {
        if !translation.x.is_finite() || !translation.y.is_finite() || !translation.z.is_finite() {
            return Err(AuthoringError::InvalidValue);
        }
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.frames.set_local_translation(frame, translation).map_err(|_| AuthoringError::InvalidOperation)
    }

    fn install_block(&mut self, handle: EntityHandle, frame: FrameId, record: crate::BlockRecord) -> Result<(), AuthoringError> {
        let mesh = self.meshes.insert(block_mesh(&record));
        self.attach_derived_meshlets(mesh);
        let object = self.insert_object_with_handle(mesh, frame, Vec3::new(1.0, 1.0, 1.0), handle);
        if let Some(slot) = self.objects.iter_mut().find(|slot| slot.id == object) {
            slot.authored_mesh = None;
            slot.authored_material = Some(record.material.clone());
            slot.scale = Vec3::new(1.0, 1.0, 1.0);
        }
        let _ = self.entities.remove_membership(handle, crate::TYPE_MESH_RENDERER, 0);
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, crate::TYPE_PARAMETRIC_BLOCK);
        self.blocks.push(WorldBlock { handle, record });
        Ok(())
    }

    fn rebuild_block_mesh(&mut self, index: usize) -> Result<(), AuthoringError> {
        let handle = self.blocks[index].handle;
        let mesh = self.meshes.insert(block_mesh(&self.blocks[index].record));
        self.attach_derived_meshlets(mesh);
        let object = self.objects.iter_mut().find(|object| object.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
        let previous = object.mesh;
        object.mesh = mesh;
        object.scale = Vec3::new(1.0, 1.0, 1.0);
        if !self.objects.iter().any(|object| object.mesh == previous) {
            self.retired_meshes.push(previous);
            self.forget_derived_meshlets(previous);
        }
        Ok(())
    }

    /// One terrain actor in this foundation. The origin is scene-local meters, Y up.
    pub fn create_terrain(&mut self, local: Vec3, record: crate::TerrainRecord) -> Result<EntityId, AuthoringError> {
        if !self.terrains.is_empty() {
            return Err(AuthoringError::InvalidOperation);
        }
        record.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let handle = self.entities.create();
        self.entities.set_name(handle, "Terrain").map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), HighPrecisionPose::at(local.x, local.y, local.z)).map_err(|_| AuthoringError::InvalidOperation)?;
        self.anchors.push((handle, frame));
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, crate::TYPE_TERRAIN);
        self.terrains.push(WorldTerrain { handle, frame, record, chunks: Vec::new() });
        self.rebuild_terrain_chunks(self.terrains.len() - 1)?;
        self.revise();
        self.entities.uuid(handle).map_err(|_| AuthoringError::NotFound)
    }

    pub fn spawn_saved_terrain(
        &mut self,
        id: EntityId,
        name: &str,
        parent: Option<EntityId>,
        local: HighPrecisionPose,
        record: crate::TerrainRecord,
    ) -> Result<(), AuthoringError> {
        if !self.terrains.is_empty() {
            return Err(AuthoringError::InvalidOperation);
        }
        record.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let parent_handle = self.parent_handle(parent)?;
        let handle = self.entities.insert(id, parent_handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), local).map_err(|_| AuthoringError::InvalidOperation)?;
        self.anchors.push((handle, frame));
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, crate::TYPE_TERRAIN);
        self.terrains.push(WorldTerrain { handle, frame, record, chunks: Vec::new() });
        self.rebuild_terrain_chunks(self.terrains.len() - 1)?;
        self.revise();
        Ok(())
    }

    pub fn stamp_terrain(
        &mut self,
        id: EntityId,
        brush: crate::TerrainBrush,
        local_x: f32,
        local_z: f32,
        radius_m: f32,
        delta_m: f32,
        layer: u8,
        falloff: crate::TerrainFalloff,
        flatten_to: Option<f32>,
        rebuild_meshes: bool,
    ) -> Result<crate::StampResult, AuthoringError> {
        let index = self.terrain_index(id)?;
        let result = self.terrains[index]
            .record
            .stamp(brush, local_x, local_z, radius_m, delta_m, layer, falloff, flatten_to)
            .map_err(|_| AuthoringError::InvalidValue)?;
        self.terrain_dirty = result.dirty.clone();
        if result.height_changed {
            if rebuild_meshes {
                let dirty = result.dirty.clone();
                for coord in dirty {
                    self.sync_chunk(index, coord.x, coord.z)?;
                }
            }
            self.revise();
        }
        Ok(result)
    }

    pub fn take_terrain_dirty(&mut self) -> Vec<crate::ChunkCoord> {
        std::mem::take(&mut self.terrain_dirty)
    }

    pub fn terrain_chunk_mesh(&self, cx: u32, cz: u32) -> Option<MeshId> {
        self.terrains.first()?.chunks.iter().find(|chunk| chunk.cx == cx && chunk.cz == cz).map(|chunk| chunk.mesh)
    }

    /// Replace derived chunk visuals. The heightfield is already authoritative. These meshes are not saved.
    pub fn install_terrain_chunks(&mut self, id: EntityId, built: Vec<(u32, u32, Mesh)>) -> Result<(), AuthoringError> {
        let index = self.terrain_index(id)?;
        if built.is_empty() {
            return Ok(());
        }
        for (cx, cz, mesh) in built {
            let Some(existing) = self.terrains[index].chunks.iter().position(|chunk| chunk.cx == cx && chunk.cz == cz) else {
                continue;
            };
            self.replace_chunk_mesh(index, existing, mesh);
        }
        self.revise();
        Ok(())
    }

    pub fn terrain_overlay_lines(
        &self,
        mask: crate::TerrainOverlayMask,
        minor_m: f32,
        major_m: f32,
        follow: bool,
        focus_x: f32,
        focus_z: f32,
    ) -> Vec<crate::TerrainOverlayLine> {
        let Some(terrain) = self.terrains.first() else { return Vec::new() };
        crate::terrain_overlay_lines(&terrain.record, mask, minor_m, major_m, follow, focus_x, focus_z)
    }

    pub fn terrain_brush_ring(&self, local_x: f32, local_z: f32, radius_m: f32) -> Vec<[f32; 3]> {
        let Some(terrain) = self.terrains.first() else { return Vec::new() };
        crate::brush_ring(&terrain.record, local_x, local_z, radius_m, 48)
    }

    /// Chunk meshes the surface grid redraws. Not a second copy of the heightfield.
    pub fn terrain_surface_meshes(&self) -> Vec<MeshId> {
        self.terrains.iter().flat_map(|terrain| terrain.chunks.iter().map(|chunk| chunk.mesh)).collect()
    }

    /// Spacing and extents for the surface grid. Does not clone the samples.
    pub fn terrain_surface_metrics(&self) -> Option<TerrainSurfaceMetrics> {
        let record = &self.terrains.first()?.record;
        Some(TerrainSurfaceMetrics {
            half_x: record.width_m * 0.5,
            half_z: record.depth_m * 0.5,
            spacing_m: record.spacing_m,
            chunk_m: record.chunk_m,
            lod_enabled: record.lod_enabled,
        })
    }

    /// Meshes the Land workspace can show or hide. Derived chunks are terrain. A joint mesh is a character.
    pub fn land_draw_actors(&self) -> Vec<(EntityId, LandDrawClass)> {
        let mut actors = Vec::new();
        for object in &self.objects {
            let Ok(id) = self.entities.uuid(object.handle) else { continue };
            let class = if self.derived_visuals.contains(&object.handle) {
                LandDrawClass::Terrain
            } else if self.joints.iter().any(|joint| joint.handle == object.handle) {
                LandDrawClass::Character
            } else if self.membership_has(object.handle, crate::TYPE_PAWN) || self.membership_has(object.handle, crate::TYPE_FREE_FLY) {
                LandDrawClass::Gameplay
            } else {
                LandDrawClass::Prop
            };
            actors.push((id, class));
        }
        actors
    }

    fn membership_has(&self, handle: EntityHandle, type_id: crate::TypeId) -> bool {
        self.entities.membership(handle).ok().is_some_and(|entries| entries.iter().any(|entry| entry.type_id == type_id))
    }

    /// Collision height in terrain-local meters. `None` when base collision is off or the point is outside.
    pub fn terrain_height_at(&self, local_x: f32, local_z: f32) -> Option<f32> {
        self.terrains.first()?.record.height_at(local_x, local_z)
    }

    /// World ray minus the terrain pose in binary64, then narrowed. The absolute root is never stored in the heightfield.
    pub fn terrain_ray_local(&self, id: EntityId, origin: Vec3, direction: Vec3) -> Option<crate::TerrainHit> {
        let index = self.terrain_index(id).ok()?;
        let pose = self.frames.resolve(self.terrains[index].frame).ok()?;
        let delta = Vec3::new(origin.x - pose.translation.x, origin.y - pose.translation.y, origin.z - pose.translation.z);
        let local = pose.rotation.conjugate().rotate(delta);
        let dir = pose.rotation.conjugate().rotate(direction);
        self.terrains[index].record.ray_heightfield([local.x as f32, local.y as f32, local.z as f32], [dir.x as f32, dir.y as f32, dir.z as f32])
    }

    fn terrain_index(&self, id: EntityId) -> Result<usize, AuthoringError> {
        self.terrains.iter().position(|terrain| self.entities.uuid(terrain.handle).ok() == Some(id)).ok_or(AuthoringError::NotFound)
    }

    fn rebuild_terrain_chunks(&mut self, index: usize) -> Result<(), AuthoringError> {
        let (chunks_x, chunks_z) = {
            let record = &self.terrains[index].record;
            (record.chunks_x(), record.chunks_z())
        };
        let existing = std::mem::take(&mut self.terrains[index].chunks);
        let mut kept = Vec::new();
        for chunk in existing {
            if chunk.cx < chunks_x && chunk.cz < chunks_z {
                kept.push(chunk);
            } else {
                self.drop_chunk_visual(chunk);
            }
        }
        self.terrains[index].chunks = kept;
        for cz in 0..chunks_z {
            for cx in 0..chunks_x {
                self.sync_chunk(index, cx, cz)?;
            }
        }
        Ok(())
    }

    fn replace_chunk_mesh(&mut self, index: usize, existing: usize, mesh: Mesh) {
        let old_mesh = self.terrains[index].chunks[existing].mesh;
        let object = self.terrains[index].chunks[existing].object;
        let id = self.meshes.insert(mesh);
        self.retired_meshes.push(old_mesh);
        self.meshes.remove(old_mesh);
        if let Some(slot) = self.objects.iter_mut().find(|candidate| candidate.id == object) {
            slot.mesh = id;
            slot.cast_shadows = false;
        }
        self.terrains[index].chunks[existing].mesh = id;
    }

    fn sync_chunk(&mut self, index: usize, cx: u32, cz: u32) -> Result<(), AuthoringError> {
        let mesh = self.terrains[index].record.chunk_mesh(cx, cz).map_err(|_| AuthoringError::InvalidValue)?;
        let material_name = self.terrains[index].record.material_name.clone();
        let base_color = self.terrains[index].record.base_color;
        let metallic = self.terrains[index].record.metallic;
        let roughness = self.terrains[index].record.roughness;
        let chunk_m = self.terrains[index].record.chunk_m;
        let owner = self.terrains[index].handle;
        let frame = self.terrains[index].frame;
        let material = MaterialAssetRef::builtin(material_name, base_color, metallic, roughness, [0.0, 0.0, 0.0, 1.0]);
        let mesh_ref = MeshAssetRef::Floor { width_m: chunk_m as f64, depth_m: chunk_m as f64 };
        if let Some(existing) = self.terrains[index].chunks.iter().position(|chunk| chunk.cx == cx && chunk.cz == cz) {
            self.replace_chunk_mesh(index, existing, mesh);
            let object = self.terrains[index].chunks[existing].object;
            if let Some(slot) = self.objects.iter_mut().find(|candidate| candidate.id == object) {
                slot.authored_mesh = Some(mesh_ref);
                slot.authored_material = Some(material);
            }
        } else {
            let id = self.meshes.insert(mesh);
            let handle = self.entities.create();
            self.entities.set_name(handle, &format!("Terrain {cx} {cz}")).map_err(|_| AuthoringError::InvalidOperation)?;
            self.entities.set_parent(handle, Some(owner)).map_err(|_| AuthoringError::InvalidOperation)?;
            let object = self.insert_object_with_handle(id, frame, Vec3::new(1.0, 1.0, 1.0), handle);
            if let Some(slot) = self.objects.iter_mut().find(|candidate| candidate.id == object) {
                slot.cast_shadows = false;
                slot.authored_mesh = Some(mesh_ref);
                slot.authored_material = Some(material);
            }
            self.derived_visuals.insert(handle);
            self.terrains[index].chunks.push(TerrainChunk { cx, cz, handle, object, mesh: id });
        }
        Ok(())
    }

    fn drop_chunk_visual(&mut self, chunk: TerrainChunk) {
        self.derived_visuals.remove(&chunk.handle);
        self.objects.retain(|object| object.id != chunk.object);
        self.meshes.remove(chunk.mesh);
        self.retired_meshes.push(chunk.mesh);
        let _ = self.entities.retire(chunk.handle);
    }

    fn retire_terrain_visuals(&mut self, index: usize) {
        let chunks = std::mem::take(&mut self.terrains[index].chunks);
        for chunk in chunks {
            self.drop_chunk_visual(chunk);
        }
    }

    fn alias_terrain_chunks(&mut self, source: &SceneWorld, entity: EntityId) {
        let Some(source_index) = source.terrains.iter().position(|terrain| source.entities.uuid(terrain.handle).ok() == Some(entity)) else {
            return;
        };
        let Some(index) = self.terrains.iter().position(|terrain| self.entities.uuid(terrain.handle).ok() == Some(entity)) else {
            return;
        };
        let plan: Vec<(u32, u32, MeshId, RenderInstanceId, Mesh)> = source.terrains[source_index]
            .chunks
            .iter()
            .filter_map(|chunk| {
                let mesh = source.meshes.get(chunk.mesh)?.clone();
                let render = source.objects.iter().find(|object| object.id == chunk.object)?.render_id;
                Some((chunk.cx, chunk.cz, chunk.mesh, render, mesh))
            })
            .collect();
        for (cx, cz, mesh_id, render_id, mesh) in plan {
            let Some(chunk_index) = self.terrains[index].chunks.iter().position(|chunk| chunk.cx == cx && chunk.cz == cz) else {
                continue;
            };
            let runtime_mesh = self.terrains[index].chunks[chunk_index].mesh;
            let object = self.terrains[index].chunks[chunk_index].object;
            if let Some(slot) = self.objects.iter_mut().find(|candidate| candidate.id == object) {
                slot.mesh = mesh_id;
                slot.render_id = render_id;
            }
            self.terrains[index].chunks[chunk_index].mesh = mesh_id;
            self.meshes.insert_exact(mesh_id, mesh);
            if runtime_mesh != mesh_id {
                self.meshes.remove(runtime_mesh);
                self.retired_meshes.push(runtime_mesh);
            }
        }
    }

    pub fn authored_joint(&self, id: EntityId) -> Option<crate::joint::JointRecord> {
        let handle = self.entities.find(id).ok()?;
        self.joints.iter().find(|joint| joint.handle == handle).map(|joint| joint.record)
    }

    pub fn set_authored_joint(&mut self, id: EntityId, record: crate::joint::JointRecord) -> Result<AuthoringResult, AuthoringError> {
        let record = record.validate()?;
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let unchanged = {
            let slot = self.joints.iter_mut().find(|joint| joint.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
            if slot.record == record {
                true
            } else {
                slot.record = record;
                false
            }
        };
        if unchanged {
            return Ok(AuthoringResult::Unchanged);
        }
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        let pose = self.frames.local_pose(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        let (translation, rotation) = crate::joint::clamp_joint_pose(&record, pose.translation, pose.rotation);
        self.frames.set_local_translation(frame, translation).map_err(|_| AuthoringError::InvalidOperation)?;
        self.frames.set_local_rotation(frame, rotation).map_err(|_| AuthoringError::InvalidOperation)?;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    /// Parents a joint frame to the parent entity's frame and keeps the stored local pose.
    pub fn authored_player_start(&self, id: EntityId) -> Option<crate::PlayerStartRecord> {
        let handle = self.entities.find(id).ok()?;
        self.player_starts.iter().find(|start| start.handle == handle).map(|start| start.record.clone())
    }

    pub fn set_authored_player_start(&mut self, id: EntityId, record: crate::PlayerStartRecord) -> Result<AuthoringResult, AuthoringError> {
        record.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let slot = self.player_starts.iter_mut().find(|start| start.handle == handle).ok_or(AuthoringError::InvalidOperation)?;
        if slot.record == record {
            return Ok(AuthoringResult::Unchanged);
        }
        slot.record = record;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn attach_player_start(&mut self, id: EntityId, record: crate::PlayerStartRecord) -> Result<(), AuthoringError> {
        record.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        if self.player_starts.iter().any(|start| start.handle == handle) {
            return Err(AuthoringError::InvalidOperation);
        }
        let _frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.player_starts.push(WorldPlayerStart { handle, record });
        self.grant(handle, crate::TYPE_PLAYER_START);
        self.revise();
        Ok(())
    }

    /// Hides an entity from the outline and from level capture. The caller revises once after a batch.
    pub fn mark_derived(&mut self, id: EntityId) -> Result<(), AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        self.derived_visuals.insert(handle);
        Ok(())
    }

    pub fn is_derived(&self, id: EntityId) -> bool {
        self.entities.find(id).ok().is_some_and(|handle| self.derived_visuals.contains(&handle))
    }

    /// Drops a derived visual and its derived children. Children go before the parent.
    pub fn retire_derived(&mut self, id: EntityId) -> Result<(), AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        self.destroy_derived_tree(handle)
    }

    fn destroy_derived_tree(&mut self, handle: EntityHandle) -> Result<(), AuthoringError> {
        let children: Vec<EntityHandle> = self
            .entities
            .handles()
            .filter(|child| self.entities.parent(*child).ok().flatten() == Some(handle))
            .collect();
        for child in children {
            self.destroy_derived_tree(child)?;
        }
        self.derived_visuals.remove(&handle);
        self.destroy_authored_handle(handle)?;
        Ok(())
    }

    /// Preview meshes must not enter the shadow pass. Does not revise.
    pub fn mute_cast_shadows(&mut self, id: EntityId) -> Result<(), AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        if let Some(object) = self.objects.iter_mut().find(|object| object.handle == handle) {
            object.cast_shadows = false;
        }
        Ok(())
    }

    pub fn finish_visual_edit(&mut self) {
        self.revise();
    }

    /// Parents an authored entity. Used to hang a preview character on a Player Start.
    pub fn reparent_authored(&mut self, id: EntityId, parent: Option<EntityId>) -> Result<(), AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        let parent_handle = match parent {
            Some(parent) => Some(self.entities.find(parent).map_err(|_| AuthoringError::NotFound)?),
            None => None,
        };
        self.reparent_entity(handle, parent_handle)
    }

    /// Spawn gizmo in world space. Forward is local -Z. Not a mesh.
    pub fn player_start_segments(&self) -> Vec<JointDebugSegment> {
        let mut segments = Vec::new();
        let color = [0.95, 0.92, 0.82, 1.0];
        for start in &self.player_starts {
            let Ok(frame) = self.frame_of(start.handle) else { continue };
            let Ok(pose) = self.frames.resolve(frame) else { continue };
            let Ok(entity) = self.entities.uuid(start.handle) else { continue };
            for (from, to) in crate::player_start_lines() {
                segments.push(JointDebugSegment {
                    entity,
                    start: pose.translation + pose.rotation.rotate(from),
                    end: pose.translation + pose.rotation.rotate(to),
                    color,
                    limits: false,
                });
            }
        }
        segments
    }

    pub fn attach_joint(&mut self, id: EntityId, record: crate::joint::JointRecord) -> Result<(), AuthoringError> {
        let record = record.validate()?;
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        if self.joints.iter().any(|joint| joint.handle == handle) {
            return Err(AuthoringError::InvalidOperation);
        }
        let _frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.next_joint += 1;
        self.joints.push(WorldJoint { id: JointId(self.next_joint), handle, record });
        self.grant(handle, crate::TYPE_JOINT);
        self.align_joint_frame(handle)?;
        self.revise();
        Ok(())
    }

    /// A frame with no mesh. Used when a saved joint has no other payload.
    pub fn spawn_saved_transform(&mut self, id: EntityId, name: &str, parent: Option<EntityId>, local: HighPrecisionPose) -> Result<(), AuthoringError> {
        let parent_handle = self.parent_handle(parent)?;
        let handle = self.entities.insert(id, parent_handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), local).map_err(|_| AuthoringError::InvalidOperation)?;
        self.anchors.push((handle, frame));
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.revise();
        Ok(())
    }

    /// World-space overlay segments. Limit marks are drawn for `selected` only.
    pub fn joint_debug_segments(&self, selected: Option<EntityId>) -> Vec<JointDebugSegment> {
        let mut segments = Vec::new();
        for joint in &self.joints {
            let Ok(frame) = self.frame_of(joint.handle) else { continue };
            let Ok(pose) = self.frames.resolve(frame) else { continue };
            let Ok(entity) = self.entities.uuid(joint.handle) else { continue };
            let selected_this = selected.is_some_and(|id| self.entities.find(id).ok() == Some(joint.handle));
            for line in crate::joint::joint_debug_lines(&joint.record, selected_this) {
                segments.push(JointDebugSegment {
                    entity,
                    start: pose.translation + pose.rotation.rotate(line.from),
                    end: pose.translation + pose.rotation.rotate(line.to),
                    color: line.color,
                    limits: line.limits,
                });
            }
        }
        segments
    }

    fn align_joint_frame(&mut self, handle: EntityHandle) -> Result<(), AuthoringError> {
        if !self.joints.iter().any(|joint| joint.handle == handle) {
            return Ok(());
        }
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        let parent_entity = self.entities.parent(handle).map_err(|_| AuthoringError::NotFound)?;
        let parent_frame = match parent_entity {
            Some(parent) => self.frame_of(parent).unwrap_or(self.scene),
            None => self.scene,
        };
        if parent_frame == frame {
            return Err(AuthoringError::InvalidOperation);
        }
        self.frames.set_parent(frame, Some(parent_frame)).map_err(|_| AuthoringError::InvalidOperation)?;
        Ok(())
    }

    /// Puts the frame back under the scene frame and keeps the world pose.
    fn release_joint_to_scene(&mut self, handle: EntityHandle) -> Result<(), AuthoringError> {
        let frame = self.frame_of(handle).map_err(|_| AuthoringError::InvalidOperation)?;
        let world = self.frames.resolve(frame).map_err(|_| AuthoringError::InvalidOperation)?;
        let scene = self.frames.resolve(self.scene).map_err(|_| AuthoringError::InvalidOperation)?;
        let delta = Vec3::new(world.translation.x - scene.translation.x, world.translation.y - scene.translation.y, world.translation.z - scene.translation.z);
        let local_translation = scene.rotation.conjugate().rotate(delta);
        let local_rotation = unit_quaternion(scene.rotation.conjugate().mul(world.rotation))?;
        self.frames.set_parent(frame, Some(self.scene)).map_err(|_| AuthoringError::InvalidOperation)?;
        self.frames.set_local_translation(frame, local_translation).map_err(|_| AuthoringError::InvalidOperation)?;
        self.frames.set_local_rotation(frame, local_rotation).map_err(|_| AuthoringError::InvalidOperation)?;
        Ok(())
    }

    pub fn spawn_saved_camera(
        &mut self,
        id: EntityId,
        name: &str,
        parent: Option<EntityId>,
        local: HighPrecisionPose,
        camera: CameraRecord,
    ) -> Result<(), AuthoringError> {
        camera.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let parent_handle = self.parent_handle(parent)?;
        let handle = self.entities.insert(id, parent_handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), local).map_err(|_| AuthoringError::InvalidOperation)?;
        self.anchors.push((handle, frame));
        self.next_camera += 1;
        self.game_cameras.push(WorldCamera {
            id: CameraId(self.next_camera),
            handle,
            enabled: camera.enabled,
            orthographic: camera.orthographic,
            vertical_fov_deg: camera.vertical_fov_deg,
            ortho_height_m: camera.ortho_height_m,
            near_m: camera.near_m,
            far_m: camera.far_m,
            priority: camera.priority,
            viewport: camera.viewport,
        });
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, crate::TYPE_CAMERA);
        self.revise();
        Ok(())
    }

    pub fn set_authored_camera_enabled(&mut self, id: EntityId, enabled: bool) -> Result<AuthoringResult, AuthoringError> {
        let camera = self.camera_mut(id)?;
        if camera.enabled == enabled {
            return Ok(AuthoringResult::Unchanged);
        }
        camera.enabled = enabled;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_camera_projection(&mut self, id: EntityId, orthographic: bool) -> Result<AuthoringResult, AuthoringError> {
        let camera = self.camera_mut(id)?;
        if camera.orthographic == orthographic {
            return Ok(AuthoringResult::Unchanged);
        }
        camera.orthographic = orthographic;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_camera_fov_deg(&mut self, id: EntityId, degrees: f64) -> Result<AuthoringResult, AuthoringError> {
        if !degrees.is_finite() || degrees <= 0.0 || degrees >= 180.0 {
            return Err(AuthoringError::InvalidValue);
        }
        let camera = self.camera_mut(id)?;
        if (camera.vertical_fov_deg - degrees).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        camera.vertical_fov_deg = degrees;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_camera_ortho_height(&mut self, id: EntityId, meters: f64) -> Result<AuthoringResult, AuthoringError> {
        if !meters.is_finite() || meters <= 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        let camera = self.camera_mut(id)?;
        if (camera.ortho_height_m - meters).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        camera.ortho_height_m = meters;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_camera_near(&mut self, id: EntityId, near_m: f64) -> Result<AuthoringResult, AuthoringError> {
        if !near_m.is_finite() || near_m <= 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        let camera = self.camera_mut(id)?;
        if (camera.near_m as f64 - near_m).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        camera.near_m = near_m as f32;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_camera_far(&mut self, id: EntityId, far_m: f64) -> Result<AuthoringResult, AuthoringError> {
        if !far_m.is_finite() || far_m <= 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        let camera = self.camera_mut(id)?;
        if camera.orthographic && far_m <= camera.near_m as f64 {
            return Err(AuthoringError::InvalidValue);
        }
        if (camera.far_m as f64 - far_m).abs() < 1.0e-9 {
            return Ok(AuthoringResult::Unchanged);
        }
        camera.far_m = far_m as f32;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_camera_priority(&mut self, id: EntityId, priority: f64) -> Result<AuthoringResult, AuthoringError> {
        if priority.fract() != 0.0 || !(i32::MIN as f64..=i32::MAX as f64).contains(&priority) {
            return Err(AuthoringError::InvalidValue);
        }
        let camera = self.camera_mut(id)?;
        if camera.priority as f64 == priority {
            return Ok(AuthoringResult::Unchanged);
        }
        camera.priority = priority as i32;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    pub fn set_authored_camera_viewport(&mut self, id: EntityId, text: &str) -> Result<AuthoringResult, AuthoringError> {
        let mut values = [0.0f32; 4];
        let parts: Vec<_> = text.split_whitespace().collect();
        if parts.len() != 4 {
            return Err(AuthoringError::InvalidValue);
        }
        for (index, part) in parts.iter().enumerate() {
            let value = part.parse::<f32>().map_err(|_| AuthoringError::InvalidValue)?;
            if !value.is_finite() {
                return Err(AuthoringError::InvalidValue);
            }
            values[index] = value;
        }
        if values[2] <= 0.0 || values[3] <= 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        let camera = self.camera_mut(id)?;
        if camera.viewport == values {
            return Ok(AuthoringResult::Unchanged);
        }
        camera.viewport = values;
        self.revise();
        Ok(AuthoringResult::Applied)
    }

    fn camera_mut(&mut self, id: EntityId) -> Result<&mut WorldCamera, AuthoringError> {
        let handle = self.entities.find(id).map_err(|_| AuthoringError::NotFound)?;
        self.game_cameras.iter_mut().find(|camera| camera.handle == handle).ok_or(AuthoringError::InvalidOperation)
    }

    pub fn authored_probe(&self, id: EntityId) -> Option<ProbeRecord> {
        let handle = self.entities.find(id).ok()?;
        let probe = self.probes.iter().find(|probe| probe.handle == handle)?;
        Some(ProbeRecord {
            enabled: probe.enabled,
            radius_m: probe.radius_m,
            intensity: probe.intensity,
            priority: probe.priority,
            resolution: probe.resolution,
        })
    }

    pub fn authored_world_settings(&self, id: EntityId) -> Option<WorldSettingsRecord> {
        let handle = self.entities.find(id).ok()?;
        if handle != self.environment_entity {
            return None;
        }
        Some(WorldSettingsRecord {
            entity: id,
            enabled: self.environment.enabled,
            intensity: self.environment.intensity,
            upper: self.environment.upper_hemisphere_linear_rgb,
            lower: self.environment.lower_hemisphere_linear_rgb,
            probe_update_policy: self.probe_policy,
            startup_camera: self.startup_camera,
        })
    }

    pub fn spawn_saved_world_settings(&mut self, id: EntityId, name: &str, environment: EnvironmentLight) -> Result<(), AuthoringError> {
        let environment = environment.validate().map_err(|_| AuthoringError::InvalidValue)?;
        let handle = self.entities.insert(id, None).map_err(|_| AuthoringError::InvalidOperation)?;
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::InvalidOperation)?;
        self.environment_entity = handle;
        self.environment = environment;
        self.grant(handle, crate::TYPE_ENVIRONMENT);
        self.revise();
        Ok(())
    }

    pub fn spawn_saved_mesh(
        &mut self,
        id: EntityId,
        name: &str,
        parent: Option<EntityId>,
        local: HighPrecisionPose,
        scale: Vec3,
        visible: bool,
        cast_shadows: bool,
        receive_shadows: bool,
        mesh_ref: MeshAssetRef,
        material_ref: MaterialAssetRef,
    ) -> Result<(), AuthoringError> {
        let parent_handle = self.parent_handle(parent)?;
        let handle = self.entities.insert(id, parent_handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), local).map_err(|_| AuthoringError::InvalidOperation)?;
        let mesh = self.draw_mesh_id(&mesh_ref)?;
        let object = self.insert_object_with_handle(mesh, frame, scale, handle);
        if let Some(slot) = self.objects.iter_mut().find(|object_slot| object_slot.id == object) {
            slot.visible = visible;
            slot.cast_shadows = cast_shadows;
            slot.receive_shadows = receive_shadows;
            slot.authored_mesh = Some(mesh_ref);
            slot.authored_material = Some(material_ref);
        }
        self.revise();
        Ok(())
    }

    pub fn spawn_saved_light(
        &mut self,
        id: EntityId,
        name: &str,
        parent: Option<EntityId>,
        local: HighPrecisionPose,
        kind: LightKind,
        light: LightRecord,
    ) -> Result<(), AuthoringError> {
        let parent_handle = self.parent_handle(parent)?;
        let handle = self.entities.insert(id, parent_handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), local).map_err(|_| AuthoringError::InvalidOperation)?;
        self.next_light += 1;
        self.lights.push(WorldLight {
            id: LightId(self.next_light),
            handle,
            frame,
            kind,
            color_linear: light.color,
            intensity: light.intensity,
            range_m: light.range_m,
            inner_radians: light.inner_radians,
            outer_radians: light.outer_radians,
            enabled: light.enabled,
            shadow: light.shadow,
        });
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, Self::light_type(kind));
        self.revise_lighting();
        Ok(())
    }

    pub fn spawn_saved_probe(
        &mut self,
        id: EntityId,
        name: &str,
        parent: Option<EntityId>,
        local: HighPrecisionPose,
        probe: ProbeRecord,
    ) -> Result<(), AuthoringError> {
        let parent_handle = self.parent_handle(parent)?;
        let handle = self.entities.insert(id, parent_handle).map_err(|_| AuthoringError::InvalidOperation)?;
        self.entities.set_name(handle, name).map_err(|_| AuthoringError::InvalidOperation)?;
        let frame = self.frames.add(Some(self.scene), local).map_err(|_| AuthoringError::InvalidOperation)?;
        self.next_probe += 1;
        self.probes.push(WorldProbe {
            id: ProbeId(self.next_probe),
            handle,
            frame,
            radius_m: probe.radius_m,
            priority: probe.priority,
            intensity: probe.intensity,
            enabled: probe.enabled,
            resolution: probe.resolution,
        });
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, crate::TYPE_REFLECTION_PROBE);
        self.revise();
        Ok(())
    }

    fn parent_handle(&self, parent: Option<EntityId>) -> Result<Option<EntityHandle>, AuthoringError> {
        match parent {
            None => Ok(None),
            Some(parent) => self.entities.find(parent).map(Some).map_err(|_| AuthoringError::NotFound),
        }
    }

    /// New authoring id, new slot, same mesh and a copied local pose. Not the same entity.
    pub fn duplicate_object(&mut self, id: ObjectId) -> Result<ObjectId, EntityError> {
        let handle = self.runtime_handle(id)?;
        let created = self.duplicate_authored_handle(handle).map_err(|_| EntityError::Missing)?;
        let handle = self.entities.find(created).map_err(|_| EntityError::Missing)?;
        self.objects.iter().find(|object| object.handle == handle).map(|object| object.id).ok_or(EntityError::Missing)
    }

    fn object(&self, id: ObjectId) -> Result<&WorldObject, SpaceError> {
        self.objects.iter().find(|object| object.id == id).ok_or(SpaceError::MissingFrame)
    }

    fn object_mut(&mut self, id: ObjectId) -> Result<&mut WorldObject, SpaceError> {
        self.objects.iter_mut().find(|object| object.id == id).ok_or(SpaceError::MissingFrame)
    }

    fn insert_light(
        &mut self,
        frame: FrameId,
        kind: LightKind,
        color_linear: [f32; 3],
        intensity: f32,
    ) -> Result<LightId, SpaceError> {
        self.next_light += 1;
        let id = LightId(self.next_light);
        let handle = self.entities.create();
        let name = match kind {
            LightKind::Directional => "Directional Light",
            LightKind::Point => "Point Light",
            LightKind::Spot => "Spot Light",
        };
        self.entities.set_name(handle, name).expect("fresh light entity");
        self.lights.push(WorldLight {
            id,
            handle,
            frame,
            kind,
            color_linear,
            intensity,
            range_m: 0.0,
            inner_radians: 0.0,
            outer_radians: 0.0,
            enabled: true,
            shadow: crate::LightShadowSettings::default(),
        });
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, Self::light_type(kind));
        Ok(id)
    }

    /// Chosen so the untonemapped sRGB target stays inspectable. Not a calibrated exposure.
    fn install_bootstrap_lights(&mut self) {
        let scene = self.frames.parent(self.objects[0].frame).expect("near frame").expect("scene frame");
        let directional = self.frames.add(Some(scene), HighPrecisionPose::IDENTITY).expect("directional frame");
        self.insert_light(directional, LightKind::Directional, [1.0, 0.96, 0.90], 0.35).expect("directional");
        let point = self.frames.add(Some(scene), HighPrecisionPose::at(1.25, 0.7, -0.8)).expect("point frame");
        let point_id = self.insert_light(point, LightKind::Point, [0.30, 0.48, 1.0], 14.0).expect("point");
        let point_handle = self.light(point_id).expect("point").handle;
        self.entities.set_name(point_handle, "Blue Point Light").expect("point name");
        let spot_pose = HighPrecisionPose {
            translation: Vec3::new(0.15, 1.35, -3.1),
            rotation: rotation_emitting_toward(Vec3::new(-0.15, -1.15, -1.9)).expect("spot aim"),
        };
        let spot_frame = self.frames.add(Some(scene), spot_pose).expect("spot frame");
        let spot = self.insert_light(spot_frame, LightKind::Spot, [1.0, 0.40, 0.12], 22.0).expect("spot");
        let light = self.light_mut(spot).expect("spot");
        light.inner_radians = 0.20;
        light.outer_radians = 0.75;
        let spot_handle = light.handle;
        self.entities.set_name(spot_handle, "Warm Spot Light").expect("spot name");
    }

    /// One sphere between the two cards. The entity is the authoring identity. ProbeId stays on the record.
    /// These installs do not revise. The first extract still sees revision 1.
    fn install_bootstrap_probe(&mut self) {
        let scene = self.frames.parent(self.objects[0].frame).expect("near frame").expect("scene frame");
        let (x, y, z) = BOOTSTRAP_PROBE_LOCAL_M;
        let frame = self.frames.add(Some(scene), HighPrecisionPose::at(x, y, z)).expect("probe frame");
        self.next_probe += 1;
        let handle = self.entities.create();
        self.entities.set_name(handle, "Reflection Probe").expect("probe name");
        self.probes.push(WorldProbe {
            id: ProbeId(self.next_probe),
            handle,
            frame,
            radius_m: BOOTSTRAP_PROBE_RADIUS_M,
            priority: BOOTSTRAP_PROBE_PRIORITY,
            intensity: BOOTSTRAP_PROBE_INTENSITY,
            enabled: true,
            resolution: crate::REFLECTION_PROBE_RESOLUTION,
        });
        self.grant(handle, crate::TYPE_SPATIAL_FRAME);
        self.grant(handle, crate::TYPE_REFLECTION_PROBE);
    }

    fn install_world_settings(&mut self) {
        let handle = self.entities.create();
        self.entities.set_name(handle, "World Settings").expect("world settings name");
        self.environment_entity = handle;
        self.grant(handle, crate::TYPE_ENVIRONMENT);
    }

    fn light(&self, id: LightId) -> Result<&WorldLight, SpaceError> {
        self.lights.iter().find(|light| light.id == id).ok_or(SpaceError::BadLight)
    }

    fn light_mut(&mut self, id: LightId) -> Result<&mut WorldLight, SpaceError> {
        self.lights.iter_mut().find(|light| light.id == id).ok_or(SpaceError::BadLight)
    }
}

impl RenderInstance {
    pub fn material_for_slot(&self, slot: u32) -> Option<MaterialInstanceId> {
        self.material_bindings.iter().find(|binding| binding.slot == slot).map(|binding| binding.instance)
    }
}

impl RenderSceneSnapshot {
    pub fn instances(&self) -> &[RenderInstance] {
        &self.instances
    }

    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    pub fn visible_count(&self) -> usize {
        self.instances.iter().filter(|instance| instance.visible).count()
    }

    pub fn camera(&self, frame: FrameId) -> Option<&ExtractedCamera> {
        self.cameras.iter().find(|camera| camera.frame == frame)
    }

    pub fn game_cameras(&self) -> &[ExtractedGameCamera] {
        &self.game_cameras
    }

    pub fn lights(&self) -> &[RenderLight] {
        &self.lights
    }

    pub fn light_count(&self) -> usize {
        self.lights.len()
    }

    pub fn environment(&self) -> EnvironmentLight {
        self.environment
    }

    pub fn reflection_probes(&self) -> &[RenderReflectionProbe] {
        &self.probes
    }
}

/// Per-view upload. The snapshot pose stays in binary64. This view's origin is subtracted here.
pub fn instance_gpu_transforms(
    instance: &RenderInstance,
    camera_pose: &ResolvedPose,
    vertical_fov_radians: f64,
    near_m: f32,
    aspect: f32,
) -> Result<GpuTransforms, SpaceError> {
    let relative = camera_relative_f32(instance.pose.translation, camera_pose.translation);
    let scale = [instance.scale.x as f32, instance.scale.y as f32, instance.scale.z as f32];
    Ok(GpuTransforms {
        model: Mat4::from_rotation_translation_scale(instance.pose.rotation, relative, scale),
        view: Mat4::from_rotation(camera_pose.rotation.conjugate()),
        projection: Mat4::perspective_infinite_reverse_z(vertical_fov_radians as f32, aspect, near_m)?,
    })
}

fn mesh_choice_name(mesh: &crate::MeshAssetRef) -> String {
    match mesh {
        crate::MeshAssetRef::Cube { .. } => "Cube".into(),
        crate::MeshAssetRef::Sphere { .. } => "Sphere".into(),
        crate::MeshAssetRef::Capsule { .. } => "Capsule".into(),
        crate::MeshAssetRef::Floor { .. } => "Plane".into(),
        crate::MeshAssetRef::NearTriangle | crate::MeshAssetRef::FarTriangle => "Triangle".into(),
        crate::MeshAssetRef::EmissivePanel { .. } => "Panel".into(),
        crate::MeshAssetRef::Asset { name, .. } => name.clone(),
    }
}

fn mesh_choice_apply(current: &crate::MeshAssetRef, choice: &str) -> Option<crate::MeshAssetRef> {
    Some(match choice {
        "Cube" => crate::MeshAssetRef::Cube { size_m: match current { crate::MeshAssetRef::Cube { size_m } => *size_m, _ => 1.0 } },
        "Sphere" => match current {
            crate::MeshAssetRef::Sphere { radius_m, segments, rings, flat } => crate::MeshAssetRef::Sphere { radius_m: *radius_m, segments: *segments, rings: *rings, flat: *flat },
            _ => crate::MeshAssetRef::Sphere { radius_m: 0.5, segments: 32, rings: 24, flat: false },
        },
        "Plane" => match current {
            crate::MeshAssetRef::Floor { width_m, depth_m } => crate::MeshAssetRef::Floor { width_m: *width_m, depth_m: *depth_m },
            _ => crate::MeshAssetRef::Floor { width_m: 2.0, depth_m: 2.0 },
        },
        _ => return None,
    })
}

fn unit_quaternion(rotation: Quat) -> Result<Quat, AuthoringError> {
    if !rotation.x.is_finite() || !rotation.y.is_finite() || !rotation.z.is_finite() || !rotation.w.is_finite() {
        return Err(AuthoringError::InvalidValue);
    }
    let length = (rotation.x * rotation.x + rotation.y * rotation.y + rotation.z * rotation.z + rotation.w * rotation.w).sqrt();
    if length < 1.0e-8 {
        return Err(AuthoringError::InvalidValue);
    }
    Ok(Quat { x: rotation.x / length, y: rotation.y / length, z: rotation.z / length, w: rotation.w / length })
}

fn quaternion_matches(left: Quat, right: Quat) -> bool {
    let same = (left.x - right.x).abs() + (left.y - right.y).abs() + (left.z - right.z).abs() + (left.w - right.w).abs();
    let opposite = (left.x + right.x).abs() + (left.y + right.y).abs() + (left.z + right.z).abs() + (left.w + right.w).abs();
    same < 1.0e-8 || opposite < 1.0e-8
}

fn push_shadow_fields(fields: &mut Vec<InspectedField>, info: &TypeInfo, kind: LightKind, shadow: crate::LightShadowSettings) {
    fields.push(inspected(info, crate::FIELD_CAST_SHADOWS, PropertyValue::Bool(shadow.cast), bool_text(shadow.cast)));
    fields.push(inspected(
        info,
        crate::FIELD_SHADOW_RESOLUTION,
        PropertyValue::F64(shadow.resolution as f64),
        crate::format_f64(shadow.resolution as f64),
    ));
    fields.push(inspected(info, crate::FIELD_SHADOW_BIAS, PropertyValue::F64(shadow.depth_bias_m as f64), crate::format_f64(shadow.depth_bias_m as f64)));
    fields.push(inspected(
        info,
        crate::FIELD_SHADOW_SLOPE_BIAS,
        PropertyValue::F64(shadow.slope_bias_m as f64),
        crate::format_f64(shadow.slope_bias_m as f64),
    ));
    fields.push(inspected(
        info,
        crate::FIELD_SHADOW_NORMAL_BIAS,
        PropertyValue::F64(shadow.normal_bias_m as f64),
        crate::format_f64(shadow.normal_bias_m as f64),
    ));
    fields.push(inspected(info, crate::FIELD_SHADOW_FILTER, PropertyValue::F64(shadow.filter_radius as f64), crate::format_f64(shadow.filter_radius as f64)));
    fields.push(inspected(info, crate::FIELD_SHADOW_DISTANCE, PropertyValue::F64(shadow.distance_m as f64), crate::format_f64(shadow.distance_m as f64)));
    if kind == LightKind::Directional {
        fields.push(inspected(info, crate::FIELD_CASCADE_COUNT, PropertyValue::F64(shadow.cascade_count as f64), crate::format_f64(shadow.cascade_count as f64)));
        fields.push(inspected(
            info,
            crate::FIELD_CASCADE_DISTRIBUTION,
            PropertyValue::F64(shadow.cascade_distribution as f64),
            crate::format_f64(shadow.cascade_distribution as f64),
        ));
    }
}

fn shadow_meters(value: PropertyValue) -> Result<f32, AuthoringError> {
    let PropertyValue::F64(meters) = value else { return Err(AuthoringError::WrongType) };
    let meters = f32_from_f64(meters)?;
    if meters < 0.0 { Err(AuthoringError::InvalidValue) } else { Ok(meters) }
}

fn shadow_u32(value: PropertyValue) -> Result<u32, AuthoringError> {
    let PropertyValue::F64(number) = value else { return Err(AuthoringError::WrongType) };
    if number < 0.0 || number > 4096.0 || number.fract() != 0.0 {
        return Err(AuthoringError::InvalidValue);
    }
    Ok(number as u32)
}

fn bool_text(value: bool) -> String {
    if value { "true".into() } else { "false".into() }
}

fn rgb_vec(color: [f32; 3]) -> Vec3 {
    Vec3::new(color[0] as f64, color[1] as f64, color[2] as f64)
}

fn rgb_from_vec(value: Vec3) -> Result<[f32; 3], AuthoringError> {
    finite_color([value.x as f32, value.y as f32, value.z as f32]).map_err(|_| AuthoringError::InvalidValue)
}

fn f32_from_f64(value: f64) -> Result<f32, AuthoringError> {
    if !value.is_finite() {
        return Err(AuthoringError::InvalidValue);
    }
    let narrowed = value as f32;
    if !narrowed.is_finite() {
        return Err(AuthoringError::InvalidValue);
    }
    Ok(narrowed)
}

fn inspected(type_info: &TypeInfo, field: crate::FieldId, value: PropertyValue, display: String) -> InspectedField {
    let info: FieldInfo = *type_info.fields.iter().find(|info| info.id == field).expect("registered field");
    InspectedField { info, value, display }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvig_material::MaterialInstanceId;

    fn camera_inspector_fields(world: &SceneWorld, id: EntityId) -> Vec<crate::FieldId> {
        let inspection = world.inspect_entity(id).unwrap();
        let camera = inspection.sections.iter().find(|section| section.type_info.id == crate::TYPE_CAMERA).unwrap();
        camera.fields.iter().map(|field| field.info.id).collect()
    }

    fn translation_field(world: &SceneWorld, id: EntityId) -> Vec3 {
        let inspection = world.inspect_entity(id).unwrap();
        let frame = inspection.sections.iter().find(|section| section.type_info.id == crate::TYPE_SPATIAL_FRAME).unwrap();
        match frame.fields[0].value {
            PropertyValue::Vec3(value) => value,
            _ => panic!("translation"),
        }
    }

    #[test]
    fn component_stack_wraps_subsystem_ids_and_skips_the_editor_camera() {
        let mut world = SceneWorld::bootstrap();
        let outline = world.entity_outline();
        let near = outline[0].uuid;
        let near_stack = world.component_stack(near).unwrap();
        let near_owned = world.entity_ownership(near).unwrap();
        assert_eq!(near_stack.len(), 2);
        assert_eq!(near_stack[0].role, ComponentRole::Transform);
        assert!(near_stack[0].object.is_none());
        assert_eq!(near_stack[1].role, ComponentRole::MeshRenderer);
        assert_eq!(near_stack[1].object, near_owned.object);
        assert_eq!(near_stack[1].mesh, near_owned.mesh);
        assert!(near_stack[1].light.is_none() && near_stack[1].probe.is_none());
        let point = outline.iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let point_stack = world.component_stack(point).unwrap();
        assert_eq!(point_stack[1].role, ComponentRole::PointLight);
        assert_eq!(point_stack[1].light, world.entity_ownership(point).unwrap().light);
        let spot = outline.iter().find(|row| row.name == "Warm Spot Light").unwrap().uuid;
        assert_eq!(
            world.component_stack(spot).unwrap().iter().map(|item| item.role.label()).collect::<Vec<_>>(),
            vec!["Transform", "Spot Light"]
        );
        let probe = outline.iter().find(|row| row.name == "Reflection Probe").unwrap().uuid;
        let probe_stack = world.component_stack(probe).unwrap();
        assert_eq!(probe_stack.iter().map(|item| item.role.label()).collect::<Vec<_>>(), vec!["Transform", "Reflection Probe"]);
        assert_eq!(probe_stack[1].probe, world.entity_ownership(probe).unwrap().probe);
        let settings = outline.iter().find(|row| row.name == "World Settings").unwrap().uuid;
        let settings_stack = world.component_stack(settings).unwrap();
        assert_eq!(settings_stack, vec![ComponentBinding::role_only(ComponentRole::WorldSettings)]);
        let revision = world.revision();
        let before = world.extract(RenderFrameId(1)).unwrap();
        for row in &outline {
            let stack = world.component_stack(row.uuid).unwrap();
            assert!(stack.iter().all(|item| item.role != ComponentRole::Camera));
            let inspection = world.inspect_entity(row.uuid).unwrap();
            let shown = inspection
                .sections
                .iter()
                .find(|section| section.type_info.id == crate::TYPE_COMPONENT_STACK)
                .map(|section| section.fields[0].display.as_str())
                .unwrap_or("");
            let expected = stack.iter().map(|item| item.role.label()).collect::<Vec<_>>().join(", ");
            assert_eq!(shown, expected);
            assert_ne!(row.name, "Transform");
            assert_ne!(row.name, "Components");
        }
        assert_eq!(world.revision(), revision);
        let after = world.extract(RenderFrameId(2)).unwrap();
        assert_eq!(after.instance_count(), before.instance_count());
        assert_eq!(after.light_count(), before.light_count());
        assert_eq!(after.world_revision, before.world_revision);
        let light_id = point_stack[1].light.unwrap();
        assert!(after.lights().iter().any(|light| light.id == light_id));
        let hits = world.query_component(crate::TYPE_POINT_LIGHT);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entity, point);
        assert_eq!(hits[0].slot, 0);
        assert_eq!(hits[0].binding.light, Some(light_id));
        assert!(matches!(world.add_component(near, crate::TYPE_MESH_RENDERER), Err(AuthoringError::InvalidOperation)));
        assert!(matches!(world.add_component(settings, crate::TYPE_SPATIAL_FRAME), Err(AuthoringError::InvalidOperation)));
        assert!(matches!(world.remove_component(near, crate::TYPE_SPATIAL_FRAME, 0), Err(AuthoringError::InvalidOperation)));
        assert_eq!(world.entity_ownership(point).unwrap().light, Some(light_id));
    }

    #[test]
    fn a_camera_actor_uses_its_transform_and_round_trips_without_touching_version_1() {
        let mut world = SceneWorld::bootstrap();
        let marker = world.create_entity("View");
        let id = world.resolve(marker).unwrap();
        let before = world.extract(RenderFrameId(1)).unwrap();
        let lighting = before.lighting_revision;
        let probes = before.probe_recapture_serial;
        assert!(matches!(world.add_component(id, crate::TYPE_CAMERA), Err(AuthoringError::InvalidOperation)));
        world.add_component(id, crate::TYPE_SPATIAL_FRAME).unwrap();
        world.add_component(id, crate::TYPE_CAMERA).unwrap();
        assert!(matches!(world.add_component(id, crate::TYPE_CAMERA), Err(AuthoringError::InvalidOperation)));
        assert!(matches!(world.remove_component(id, crate::TYPE_SPATIAL_FRAME, 0), Err(AuthoringError::InvalidOperation)));
        let after_add = world.extract(RenderFrameId(2)).unwrap();
        assert_eq!(after_add.lighting_revision, lighting);
        assert_eq!(after_add.probe_recapture_serial, probes);
        world.set_authored_camera_fov_deg(id, 42.0).unwrap();
        let after_edit = world.extract(RenderFrameId(3)).unwrap();
        assert_eq!(after_edit.lighting_revision, lighting);
        assert_eq!(after_edit.probe_recapture_serial, probes);
        let labels: Vec<_> = world.component_stack(id).unwrap().iter().map(|item| item.role.label()).collect();
        assert_eq!(labels, vec!["Transform", "Camera"]);
        let perspective_fields = camera_inspector_fields(&world, id);
        assert!(perspective_fields.contains(&crate::FIELD_VERTICAL_FOV));
        assert!(perspective_fields.contains(&crate::FIELD_NEAR_PLANE));
        assert!(!perspective_fields.contains(&crate::FIELD_FAR_PLANE));
        assert!(!perspective_fields.contains(&crate::FIELD_ORTHO_HEIGHT));
        world.set_authored_camera_projection(id, true).unwrap();
        let ortho_fields = camera_inspector_fields(&world, id);
        assert!(ortho_fields.contains(&crate::FIELD_ORTHO_HEIGHT));
        assert!(ortho_fields.contains(&crate::FIELD_NEAR_PLANE));
        assert!(ortho_fields.contains(&crate::FIELD_FAR_PLANE));
        assert!(!ortho_fields.contains(&crate::FIELD_VERTICAL_FOV));
        world.set_authored_camera_projection(id, false).unwrap();
        let camera_id = world.entity_ownership(id).unwrap().camera.unwrap();
        let extracted = world.extract(RenderFrameId(4)).unwrap();
        assert_eq!(extracted.camera(world.front_camera().frame).unwrap().vertical_fov_radians, world.front_camera().vertical_fov_radians);
        let game = extracted.game_cameras().iter().find(|camera| camera.entity == id).unwrap();
        assert_eq!(game.id, camera_id);
        assert!((game.vertical_fov_radians - 42.0_f64.to_radians()).abs() < 1.0e-9);
        assert!(!game.orthographic);
        let pose = world.entity_world_pose(id).unwrap();
        assert_eq!(game.pose.translation, pose.translation);
        let copy = world.duplicate_authored(id).unwrap();
        assert_ne!(copy, id);
        assert_eq!(world.authored_camera(copy).unwrap().vertical_fov_deg, 42.0);
        assert_ne!(world.entity_ownership(copy).unwrap().camera, Some(camera_id));
        world.destroy_authored(id).unwrap();
        assert!(world.component_stack(id).is_err());
        assert!(world.authored_camera(id).is_none());
        let mut saved_world = SceneWorld::new_session();
        let settings_id = EntityId::parse("11111111-1111-4111-8111-111111111111").unwrap();
        saved_world.spawn_saved_world_settings(settings_id, "World Settings", crate::EnvironmentLight::bootstrap()).unwrap();
        let camera_uuid = EntityId::parse("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
        saved_world
            .spawn_saved_camera(
                camera_uuid,
                "Shot",
                None,
                crate::HighPrecisionPose::at(1.0, 2.0, 3.0),
                crate::CameraRecord {
                    enabled: true,
                    orthographic: false,
                    vertical_fov_deg: 42.0,
                    ortho_height_m: 10.0,
                    near_m: 0.1,
                    far_m: 1000.0,
                    priority: 3,
                    viewport: [0.0, 0.0, 1.0, 1.0],
                },
            )
            .unwrap();
        let level_id = EntityId::parse("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap();
        let saved = crate::LevelDocument::capture(&saved_world, level_id, "Cameras").unwrap();
        assert_eq!(saved.format_version, crate::LEVEL_CAMERA_VERSION);
        let loaded = crate::parse_level(&saved.to_json()).unwrap();
        assert_eq!(loaded.to_json(), saved.to_json());
        let again = loaded.instantiate().unwrap();
        assert_eq!(again.resolve(again.find_entity(camera_uuid).unwrap()).unwrap(), camera_uuid);
        assert_eq!(again.authored_camera(camera_uuid).unwrap().vertical_fov_deg, 42.0);
        assert_eq!(again.authored_camera(camera_uuid).unwrap().priority, 3);
        let pose = again.entity_world_pose(camera_uuid).unwrap();
        assert!((pose.translation.x - (crate::BOOTSTRAP_ROOT_M + 1.0)).abs() < 1.0e-6);
        assert_eq!(again.component_stack(camera_uuid).unwrap().iter().map(|item| item.role.label()).collect::<Vec<_>>(), vec!["Transform", "Camera"]);
    }

    #[test]
    fn a_later_edit_does_not_change_the_snapshot_already_extracted() {
        let mut world = SceneWorld::bootstrap();
        let first_id = world.objects().next().unwrap();
        world.set_simulation_tick(4);
        let first = world.extract(RenderFrameId(1)).unwrap();
        let original = first.instances()[0].pose.translation;
        assert_eq!(first.simulation_tick, 4);
        assert_eq!(first.world_revision, 1);
        assert_eq!(first.instance_count(), 2);
        assert_eq!(first.visible_count(), 2);
        world.set_object_local_translation(first_id, Vec3::new(0.0, 0.0, -4.0)).unwrap();
        assert_eq!(first.instances()[0].pose.translation, original);
        assert_eq!(first.world_revision, 1);
        let second = world.extract(RenderFrameId(2)).unwrap();
        assert_ne!(second.instances()[0].pose.translation, original);
        assert_eq!(second.world_revision, 2);
        assert_eq!(first.instances()[0].pose.translation, original);
        assert!((second.instances()[0].pose.translation.x - crate::BOOTSTRAP_ROOT_M).abs() < 1.0e-6);
        assert_eq!(first.light_count(), 3);
        assert_eq!(second.light_count(), 3);
        assert_eq!(world.entity(first_id).unwrap(), first.instances()[0].entity);
        assert_eq!(first.reflection_probes().len(), 1);
        assert_eq!(world.entity_count(), 7);
    }

    #[test]
    fn authoring_ids_survive_an_edit_and_a_duplicate_is_new() {
        let mut world = SceneWorld::bootstrap();
        let ids: Vec<_> = world.objects().collect();
        let first = world.entity(ids[0]).unwrap();
        let second = world.entity(ids[1]).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.to_string().len(), 36);
        world.set_visible(ids[0], false).unwrap();
        assert_eq!(world.entity(ids[0]).unwrap(), first);
        let handle = world.runtime_handle(ids[0]).unwrap();
        assert_eq!(world.entity(ids[0]).unwrap(), first);
        assert_ne!(first.to_string(), ids[0].0.to_string());
        assert!(world.frame_of(handle).is_ok());
        let copy = world.duplicate_object(ids[1]).unwrap();
        let copied = world.entity(copy).unwrap();
        assert_ne!(copied, second);
        assert_ne!(copied, first);
        assert_ne!(world.runtime_handle(copy).unwrap(), world.runtime_handle(ids[1]).unwrap());
        assert_eq!(world.object_count(), 3);
        assert_eq!(world.entities().count(), 8);
        let extracted = world.extract(RenderFrameId(3)).unwrap();
        assert_eq!(extracted.instances()[1].entity, second);
        assert_eq!(extracted.instances()[2].entity, copied);
        assert_eq!(extracted.instances()[2].source, copy);
    }

    #[test]
    fn lights_extract_once_and_each_view_gets_its_own_meter_scale_position() {
        use crate::{render_light_record, LightKind, BOOTSTRAP_ROOT_M};
        let mut world = SceneWorld::bootstrap();
        assert_eq!(world.light_count(), 3);
        assert_eq!(world.light_count_of(LightKind::Directional), 1);
        assert_eq!(world.light_count_of(LightKind::Point), 1);
        assert_eq!(world.light_count_of(LightKind::Spot), 1);
        let directional = world.light_ids().next().unwrap();
        assert!(world.set_spot_cone(directional, crate::SpotCone { inner_radians: 0.4, outer_radians: 0.1 }).is_err());
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        assert_eq!(snapshot.light_count(), 3);
        let point = snapshot.lights().iter().find(|light| light.kind == LightKind::Point).unwrap();
        assert!(point.pose.translation.x > BOOTSTRAP_ROOT_M);
        assert_eq!(snapshot.reflection_probes().len(), 1);
        let probe = &snapshot.reflection_probes()[0];
        assert!((probe.translation.x - BOOTSTRAP_ROOT_M).abs() < 1.0e-3);
        assert!((probe.translation.z + 3.5).abs() < 1.0e-6);
        let far = &snapshot.instances()[1].pose.translation;
        let inside = crate::reflection_probe_weight(probe, *far);
        assert!(inside > 0.5, "{inside}");
        let front = snapshot.camera(world.front_camera().frame).unwrap();
        let center = crate::reflection_probe_center(probe, front.pose.translation);
        assert!(center.iter().all(|channel| channel.is_finite() && channel.abs() < 20.0), "{center:?}");
        let id = probe.id;
        let far_id = world.objects().nth(1).unwrap();
        world.set_object_local_translation(far_id, Vec3::new(0.0, 0.0, -30.0)).unwrap();
        let moved = world.extract(RenderFrameId(2)).unwrap();
        assert_eq!(crate::reflection_probe_influence(moved.reflection_probes(), moved.instances()[1].pose.translation), 0.0);
        world.set_reflection_probe_enabled(id, false).unwrap();
        let disabled = world.extract(RenderFrameId(3)).unwrap();
        assert_eq!(crate::reflection_probe_influence(disabled.reflection_probes(), disabled.instances()[0].pose.translation), 0.0);
        assert_eq!(world.entity_count(), 7);
        let side = snapshot.camera(world.side_camera().frame).unwrap();
        let front_gpu = render_light_record(point, &front.pose);
        let side_gpu = render_light_record(point, &side.pose);
        assert_ne!(front_gpu.position, side_gpu.position);
        assert!(front_gpu.position.iter().all(|value| value.is_finite() && value.abs() < 30.0));
        assert!(side_gpu.position.iter().all(|value| value.is_finite() && value.abs() < 30.0));
        assert!(front_gpu.position.iter().all(|value| value.abs() < 1.0e6));
        let directional = snapshot.lights().iter().find(|light| light.kind == LightKind::Directional).unwrap();
        let forward = render_light_record(directional, &front.pose).direction;
        assert!(forward[2] < -0.9);
        let spot = world.light_ids().last().unwrap();
        world.set_light_enabled(spot, false).unwrap();
        assert_eq!(world.light_count(), 3);
        assert!(world.light_enabled(spot).unwrap() == false);
        assert_eq!(world.extract(RenderFrameId(2)).unwrap().light_count(), 2);
        assert_eq!(snapshot.light_count(), 3);
        assert!(snapshot.environment().enabled);
        assert_eq!(world.environment_light_count(), 1);
    }

    #[test]
    fn a_downward_face_loses_every_direct_light_and_keeps_the_lower_hemisphere() {
        use crate::{emission_forward, environment_diffuse_for, plus_z_normal, BOOTSTRAP_ROOT_M};
        let mut world = SceneWorld::bootstrap();
        let near = world.entity_outline()[0].uuid;
        let before = world.extract(RenderFrameId(1)).unwrap();
        assert_eq!(before.environment().intensity, crate::BOOTSTRAP_ENVIRONMENT_INTENSITY);
        let down = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        world.set_entity_local_rotation(near, down).unwrap();
        let posed = world.extract(RenderFrameId(2)).unwrap();
        assert_eq!(before.environment(), world.environment());
        assert_ne!(before.world_revision, posed.world_revision);
        let normal = plus_z_normal(posed.instances()[0].pose.rotation);
        assert!(normal[1] < -0.99, "{normal:?}");
        let origin = posed.instances()[0].pose.translation;
        assert!(origin.x > BOOTSTRAP_ROOT_M - 1.0);
        for light in posed.lights() {
            let to_light = match light.kind {
                LightKind::Directional => emission_forward(light.pose.rotation).scale(-1.0),
                _ => Vec3::new(
                    light.pose.translation.x - origin.x,
                    light.pose.translation.y - origin.y,
                    light.pose.translation.z - origin.z,
                ),
            };
            let dot = normal[0] as f64 * to_light.x + normal[1] as f64 * to_light.y + normal[2] as f64 * to_light.z;
            assert!(dot <= 1.0e-4, "{:?} still lights the downward face ({dot})", light.kind);
        }
        let fill = environment_diffuse_for(&posed.environment(), normal, [1.0, 1.0, 1.0], 0.0, 1.0);
        assert!(fill[0] > fill[2] && fill[0] > 0.02, "{fill:?}");
        let packet = crate::GpuEnvironmentPacket::from_light(&posed.environment()).to_bytes();
        for chunk in packet.chunks(4) {
            let value = f32::from_le_bytes(chunk.try_into().unwrap());
            assert!(value.abs() < 4.0, "environment packet picked up a world position {value}");
        }
        world.set_environment_enabled(false).unwrap();
        let disabled = world.extract(RenderFrameId(3)).unwrap();
        assert!(!disabled.environment().enabled);
        assert_eq!(disabled.light_count(), 3);
        assert_eq!(environment_diffuse_for(&disabled.environment(), normal, [1.0, 1.0, 1.0], 0.0, 1.0), [0.0; 3]);
        assert!(world.set_environment_upper([-1.0, 0.0, 0.0]).is_err());
        assert_eq!(posed.environment().upper_hemisphere_linear_rgb, crate::BOOTSTRAP_UPPER_HEMISPHERE_LINEAR);
    }

    #[test]
    fn one_snapshot_feeds_two_camera_origins() {
        let world = SceneWorld::bootstrap();
        let snapshot = world.extract(RenderFrameId(7)).unwrap();
        let instance = &snapshot.instances()[0];
        let front = snapshot.camera(world.front_camera().frame).unwrap();
        let side = snapshot.camera(world.side_camera().frame).unwrap();
        let front_packet = instance_gpu_transforms(instance, &front.pose, front.vertical_fov_radians, front.near_m, 1.0).unwrap();
        let side_packet = instance_gpu_transforms(instance, &side.pose, side.vertical_fov_radians, side.near_m, 0.5).unwrap();
        assert_ne!(front_packet.model.cols[3], side_packet.model.cols[3]);
        assert!(front_packet.model.cols[3][0].abs() < 20.0);
        assert!(side_packet.model.cols[3][0].abs() < 20.0);
        assert!(instance.pose.translation.x > 1.0e8);
        let mesh = world.meshes().get(instance.mesh).unwrap();
        assert_eq!(instance.bounds, mesh.bounds());
        assert!(!mesh.streams()[0].bytes.is_empty());
    }

    #[test]
    fn hiding_an_object_keeps_it_in_the_snapshot_but_not_in_the_visible_set() {
        let mut world = SceneWorld::bootstrap();
        let far = world.objects().nth(1).unwrap();
        world.set_visible(far, false).unwrap();
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        assert_eq!(snapshot.instance_count(), 2);
        assert_eq!(snapshot.visible_count(), 1);
        assert!(!snapshot.instances()[1].visible);
        assert_eq!(world.mesh_count(), 2);
    }

    #[test]
    fn a_material_slot_binding_is_copied_and_then_isolated() {
        let mut world = SceneWorld::bootstrap();
        let near = world.objects().next().unwrap();
        let far = world.objects().nth(1).unwrap();
        world.bind_material(near, 0, MaterialInstanceId(7)).unwrap();
        world.bind_material(far, 0, MaterialInstanceId(8)).unwrap();
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        assert_eq!(snapshot.instances()[0].material_for_slot(0), Some(MaterialInstanceId(7)));
        assert_eq!(snapshot.instances()[1].material_for_slot(0), Some(MaterialInstanceId(8)));
        assert!(snapshot.instances()[0].material_for_slot(1).is_none());
        world.bind_material(near, 0, MaterialInstanceId(9)).unwrap();
        assert_eq!(snapshot.instances()[0].material_for_slot(0), Some(MaterialInstanceId(7)));
        let next = world.extract(RenderFrameId(2)).unwrap();
        assert_eq!(next.instances()[0].material_for_slot(0), Some(MaterialInstanceId(9)));
    }

    #[test]
    fn outline_lists_a_non_renderable_and_keeps_identity_across_rename() {
        let mut world = SceneWorld::bootstrap();
        let outline = world.entity_outline();
        assert_eq!(outline.len(), 7);
        assert_eq!(outline[0].name, "Near Triangle");
        assert_eq!(outline[1].name, "Far Triangle");
        assert_ne!(outline[0].uuid, outline[1].uuid);
        let marker = world.create_entity("Gameplay Marker");
        let marker_id = world.resolve(marker).unwrap();
        assert_eq!(world.entity_count(), 8);
        assert_eq!(world.object_count(), 2);
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        assert_eq!(snapshot.instance_count(), 2);
        assert!(snapshot.instances().iter().all(|instance| instance.entity != marker_id));
        world.rename_entity(marker, "Empty Entity").unwrap();
        assert_eq!(world.resolve(marker).unwrap(), marker_id);
        assert_eq!(marker, world.find_entity(marker_id).unwrap());
        let row = world.entity_outline().into_iter().find(|row| row.uuid == marker_id).unwrap();
        assert_eq!(row.name, "Empty Entity");
        assert_eq!(row.parent, None);
        let copy = world.duplicate_entity(marker).unwrap();
        let copy_id = world.resolve(copy).unwrap();
        assert_ne!(copy_id, marker_id);
        world.reparent_entity(copy, Some(marker)).unwrap();
        let rows = world.entity_outline();
        assert_eq!(rows.iter().find(|row| row.uuid == copy_id).unwrap().parent, Some(marker_id));
        assert_eq!(rows.iter().find(|row| row.uuid == copy_id).unwrap().name, "Empty Entity");
        assert!(world.reparent_entity(marker, Some(copy)).is_err());
        assert_eq!(world.entity_outline().iter().find(|row| row.uuid == marker_id).unwrap().parent, None);
        let retired = world.retire_entity(copy).unwrap();
        assert_eq!(retired, copy_id);
        assert!(world.resolve(copy).is_err());
        assert!(world.entity_outline().iter().all(|row| row.uuid != copy_id));
        let reused = world.create_entity("Other");
        assert_eq!(reused.index, copy.index);
        assert_ne!(reused.generation, copy.generation);
        assert!(world.resolve(copy).is_err());
        assert!(world.entity_outline().iter().all(|row| row.uuid != copy_id));
        assert_eq!(world.extract(RenderFrameId(2)).unwrap().instance_count(), 2);
    }

    #[test]
    fn inspection_reads_without_mutation_and_translation_stays_binary64() {
        let mut world = SceneWorld::bootstrap();
        let revision = world.revision();
        let outline = world.entity_outline();
        let near = outline[0].uuid;
        let missing = EntityId::new();
        assert!(matches!(world.inspect_entity(missing), Err(AuthoringError::NotFound)));
        let inspection = world.inspect_entity(near).unwrap();
        assert_eq!(world.revision(), revision);
        assert_eq!(inspection.entity, near);
        assert_eq!(inspection.sections.len(), 4);
        assert_eq!(inspection.sections[0].type_info.id, TYPE_ENTITY);
        assert_eq!(inspection.sections[1].type_info.id, TYPE_SPATIAL_FRAME);
        assert_eq!(inspection.sections[3].type_info.id, crate::TYPE_COMPONENT_STACK);
        assert_eq!(inspection.sections[3].fields[0].display, "Transform, Mesh Renderer");
        assert_eq!(inspection.sections[0].fields[0].value, PropertyValue::String("Near Triangle".into()));
        assert_eq!(inspection.sections[0].fields[1].value, PropertyValue::Entity(near));
        assert_eq!(inspection.sections[0].fields[2].display, "World");
        assert_eq!(inspection.sections[0].fields[2].value, PropertyValue::OptionalEntity(None));
        let PropertyValue::Vec3(local) = inspection.sections[1].fields[0].value else { panic!("translation") };
        let PropertyValue::Quat(rotation) = inspection.sections[1].fields[1].value else { panic!("rotation") };
        assert_eq!(local, Vec3::new(0.0, 0.0, -2.0));
        assert_eq!(rotation, Quat::IDENTITY);
        let handle = world.find_entity(near).unwrap();
        assert_eq!(world.set_entity_name(near, "Near Triangle").unwrap(), AuthoringResult::Unchanged);
        assert_eq!(world.revision(), revision);
        assert_eq!(world.set_entity_name(near, "Player Start").unwrap(), AuthoringResult::Applied);
        assert_eq!(world.revision(), revision + 1);
        assert_eq!(world.find_entity(near).unwrap(), handle);
        assert_eq!(world.inspect_entity(near).unwrap().sections[0].fields[0].display, "Player Start");
        let moved = Vec3::new(local.x + 0.5, local.y, local.z);
        assert_eq!(world.set_entity_local_translation(near, moved).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.set_entity_local_translation(near, moved).unwrap(), AuthoringResult::Unchanged);
        assert!(matches!(world.set_entity_local_translation(near, Vec3::new(f64::NAN, 0.0, 0.0)), Err(AuthoringError::InvalidValue)));
        let snapshot = world.extract(RenderFrameId(4)).unwrap();
        let instance = snapshot.instances().iter().find(|instance| instance.entity == near).unwrap();
        assert!((instance.pose.translation.x - (crate::BOOTSTRAP_ROOT_M + 0.5)).abs() < 1.0e-6);
        let camera = snapshot.camera(world.front_camera().frame).unwrap();
        let relative = crate::camera_relative_f32(instance.pose.translation, camera.pose.translation);
        assert!(relative.iter().all(|component| component.is_finite() && component.abs() < 20.0));
        let marker = world.create_entity("Marker");
        let marker_id = world.resolve(marker).unwrap();
        let marker_view = world.inspect_entity(marker_id).unwrap();
        assert_eq!(marker_view.sections.len(), 1);
        assert!(matches!(world.set_entity_local_translation(marker_id, Vec3::ZERO), Err(AuthoringError::InvalidOperation)));
    }

    #[test]
    fn authorable_rows_are_entities_and_subsystem_ids_stay_internal() {
        let mut world = SceneWorld::bootstrap();
        let outline = world.entity_outline();
        let names: Vec<_> = outline.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Near Triangle",
                "Far Triangle",
                "Directional Light",
                "Blue Point Light",
                "Warm Spot Light",
                "Reflection Probe",
                "World Settings",
            ]
        );
        assert!(outline.iter().all(|row| row.parent.is_none()));
        assert!(outline.iter().all(|row| !row.name.contains("Camera") && !row.name.contains("Perspective")));
        let classes: Vec<_> = outline.iter().map(|row| row.class).collect();
        assert_eq!(
            classes,
            [
                crate::AuthoringClass::Mesh,
                crate::AuthoringClass::Mesh,
                crate::AuthoringClass::DirectionalLight,
                crate::AuthoringClass::PointLight,
                crate::AuthoringClass::SpotLight,
                crate::AuthoringClass::ReflectionProbe,
                crate::AuthoringClass::WorldSettings,
            ]
        );
        assert_eq!(world.object_count(), 2);
        assert_eq!(world.light_count(), 3);
        assert_eq!(world.reflection_probe_count(), 1);
        let point = outline[3].uuid;
        let directional = outline[2].uuid;
        let probe = outline[5].uuid;
        let settings = outline[6].uuid;
        let before = world.extract(RenderFrameId(1)).unwrap();
        assert_eq!(before.instance_count(), 2);
        assert_eq!(before.light_count(), 3);
        assert_eq!(before.reflection_probes().len(), 1);
        assert!((before.reflection_probes()[0].radius_m - crate::BOOTSTRAP_PROBE_RADIUS_M).abs() < 1.0e-9);
        assert_eq!(before.lights()[1].kind, LightKind::Point);
        assert!((before.lights()[1].intensity - 14.0).abs() < 1.0e-5);
        let probe_id = before.reflection_probes()[0].id;
        let point_light_id = before.lights()[1].id;
        let near_material = before.instances()[0].material_for_slot(0);
        assert_eq!(world.set_authored_light_intensity(point, 14.0).unwrap(), AuthoringResult::Unchanged);
        assert_eq!(world.set_authored_light_intensity(point, 15.0).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.set_authored_light_intensity(point, 15.0).unwrap(), AuthoringResult::Unchanged);
        assert_eq!(world.set_authored_probe_radius(probe, 8.0).unwrap(), AuthoringResult::Unchanged);
        assert_eq!(world.set_authored_probe_radius(probe, 9.0).unwrap(), AuthoringResult::Applied);
        assert!(matches!(world.set_authored_probe_radius(probe, 0.0), Err(AuthoringError::InvalidValue)));
        assert_eq!(
            world.set_authored_environment_intensity(settings, crate::BOOTSTRAP_ENVIRONMENT_INTENSITY as f64).unwrap(),
            AuthoringResult::Unchanged
        );
        assert!(matches!(world.set_authored_environment_intensity(point, 0.2), Err(AuthoringError::InvalidOperation)));
        assert!(matches!(world.set_authored_light_range(directional, 10.0), Err(AuthoringError::InvalidOperation)));
        assert!(matches!(world.entity_focus(settings), Err(FocusError::NoSpatial)));
        let settings_view = world.inspect_entity(settings).unwrap();
        assert_eq!(settings_view.sections.len(), 3);
        assert_eq!(settings_view.sections[1].type_info.id, crate::TYPE_ENVIRONMENT);
        assert_eq!(settings_view.sections[2].fields[0].display, "World Settings");
        let point_view = world.inspect_entity(point).unwrap();
        assert!(point_view.sections.iter().any(|section| section.type_info.id == crate::TYPE_POINT_LIGHT));
        assert_eq!(world.object_count(), 2);
        assert_eq!(world.light_count(), 3);
        let after = world.extract(RenderFrameId(2)).unwrap();
        assert_eq!(after.instance_count(), 2);
        assert_eq!(after.light_count(), 3);
        assert_eq!(after.reflection_probes().len(), 1);
        assert_eq!(after.reflection_probes()[0].id, probe_id);
        assert!((after.reflection_probes()[0].radius_m - 9.0).abs() < 1.0e-9);
        assert_eq!(after.lights()[1].id, point_light_id);
        assert!((after.lights()[1].intensity - 15.0).abs() < 1.0e-5);
        assert_eq!(after.instances()[0].material_for_slot(0), near_material);
        assert_eq!(after.environment().intensity, crate::BOOTSTRAP_ENVIRONMENT_INTENSITY);
        let marker = world.create_entity("Loose");
        let marker_id = world.resolve(marker).unwrap();
        assert_eq!(world.entity_outline().iter().find(|row| row.uuid == marker_id).unwrap().class, crate::AuthoringClass::Empty);
    }

    #[test]
    fn duplicate_and_delete_keep_subsystem_records_with_the_entity() {
        let mut world = SceneWorld::bootstrap();
        let outline = world.entity_outline();
        let near = outline[0].uuid;
        let point = outline.iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let probe = outline.iter().find(|row| row.name == "Reflection Probe").unwrap().uuid;
        let settings = outline.iter().find(|row| row.name == "World Settings").unwrap().uuid;
        let near_mesh = world.entity_ownership(near).unwrap().mesh.unwrap();
        let near_material = world.extract(RenderFrameId(1)).unwrap().instances()[0].material_for_slot(0);
        let point_light = world.entity_ownership(point).unwrap().light.unwrap();
        let probe_id = world.entity_ownership(probe).unwrap().probe.unwrap();
        let point_pose = world.entity_world_pose(point).unwrap();

        let copy_id = world.duplicate_authored(near).unwrap();
        assert_ne!(copy_id, near);
        assert_ne!(world.find_entity(copy_id).unwrap(), world.find_entity(near).unwrap());
        let copied = world.entity_ownership(copy_id).unwrap();
        assert!(copied.capabilities.mesh_renderer);
        assert_eq!(copied.mesh, Some(near_mesh));
        assert_ne!(copied.object, world.entity_ownership(near).unwrap().object);
        world.set_entity_local_translation(copy_id, Vec3::new(3.0, 0.0, -2.0)).unwrap();
        assert_eq!(world.object_count(), 3);
        assert_eq!(world.mesh_count(), 2);
        let extracted = world.extract(RenderFrameId(2)).unwrap();
        assert_eq!(extracted.instance_count(), 3);
        let copy_instance = extracted.instances().iter().find(|instance| instance.entity == copy_id).unwrap();
        let near_instance = extracted.instances().iter().find(|instance| instance.entity == near).unwrap();
        assert_eq!(copy_instance.mesh, near_instance.mesh);
        assert_eq!(copy_instance.material_for_slot(0), near_material);
        assert_ne!(copy_instance.pose.translation, near_instance.pose.translation);
        world.destroy_authored(copy_id).unwrap();
        assert!(world.entity_ownership(copy_id).is_err());
        assert_eq!(world.object_count(), 2);
        assert_eq!(world.mesh_count(), 2);
        let restored = world.extract(RenderFrameId(3)).unwrap();
        assert_eq!(restored.instance_count(), 2);
        assert!(restored.instances().iter().all(|instance| instance.entity != copy_id));
        assert!(restored.instances().iter().any(|instance| instance.entity == near));

        let light_copy = world.duplicate_authored(point).unwrap();
        let light_ownership = world.entity_ownership(light_copy).unwrap();
        assert!(light_ownership.capabilities.point_light);
        let new_light = light_ownership.light.unwrap();
        assert_ne!(new_light, point_light);
        assert_eq!(world.light_count(), 4);
        let both = world.extract(RenderFrameId(4)).unwrap();
        let lights: Vec<_> = both.lights().iter().filter(|light| light.kind == LightKind::Point).collect();
        assert_eq!(lights.len(), 2);
        assert!((lights[0].intensity - lights[1].intensity).abs() < 1.0e-5);
        assert_eq!(lights[0].color_linear, lights[1].color_linear);
        world.set_entity_local_translation(light_copy, Vec3::new(4.0, 1.0, -1.0)).unwrap();
        assert_eq!(world.entity_world_pose(point).unwrap().translation, point_pose.translation);
        world.destroy_authored(point).unwrap();
        assert_eq!(world.light_count(), 3);
        let after_light = world.extract(RenderFrameId(5)).unwrap();
        assert!(after_light.lights().iter().all(|light| light.id != point_light));
        assert!(after_light.lights().iter().any(|light| light.id == new_light));
        world.destroy_authored(light_copy).unwrap();
        assert_eq!(world.light_count(), 2);
        assert!(world.extract(RenderFrameId(6)).unwrap().lights().iter().all(|light| light.id != new_light));

        let probe_copy = world.duplicate_authored(probe).unwrap();
        let probe_ownership = world.entity_ownership(probe_copy).unwrap();
        assert!(probe_ownership.capabilities.reflection_probe);
        let new_probe = probe_ownership.probe.unwrap();
        assert_ne!(new_probe, probe_id);
        assert_eq!(world.reflection_probe_count(), 2);
        let probes = world.extract(RenderFrameId(7)).unwrap();
        assert_eq!(probes.reflection_probes().len(), 2);
        assert_eq!(probes.reflection_probes()[0].radius_m, probes.reflection_probes()[1].radius_m);
        world.destroy_authored(probe).unwrap();
        assert_eq!(world.reflection_probe_count(), 1);
        assert_eq!(world.extract(RenderFrameId(8)).unwrap().reflection_probes()[0].id, new_probe);
        world.destroy_authored(probe_copy).unwrap();
        assert_eq!(world.reflection_probe_count(), 0);
        assert!(world.extract(RenderFrameId(9)).unwrap().reflection_probes().is_empty());

        let settings_ownership = world.entity_ownership(settings).unwrap();
        assert!(settings_ownership.capabilities.world_settings);
        assert!(!settings_ownership.capabilities.transform);
        assert!(matches!(world.duplicate_authored(settings), Err(AuthoringError::ProtectedEntity)));
        assert!(matches!(world.destroy_authored(settings), Err(AuthoringError::ProtectedEntity)));
        assert!(matches!(world.reparent_entity(world.find_entity(settings).unwrap(), Some(world.find_entity(near).unwrap())), Err(AuthoringError::ProtectedEntity)));
        assert!(matches!(world.entity_focus(settings), Err(FocusError::NoSpatial)));
        assert!(world.entity_ownership(settings).is_ok());
        assert_eq!(world.object_count(), 2);

        let parent = world.create_entity("Parent");
        let child = world.create_entity("Child");
        let parent_id = world.resolve(parent).unwrap();
        let child_id = world.resolve(child).unwrap();
        world.reparent_entity(child, Some(parent)).unwrap();
        world.destroy_authored(parent_id).unwrap();
        assert_eq!(world.entity_parent(child_id).unwrap(), None);
        assert!(world.entity_outline().iter().any(|row| row.uuid == child_id && row.parent.is_none()));
        let reused = world.create_entity("Reuse");
        assert_eq!(reused.index, parent.index);
        assert_ne!(world.entity_parent(child_id).unwrap(), Some(world.resolve(reused).unwrap()));
    }

    #[test]
    fn focus_reads_bounds_without_revising_or_selecting() {
        let mut world = SceneWorld::bootstrap();
        let revision = world.revision();
        let near = world.entity_outline()[0].uuid;
        let framed = world.entity_focus(near).unwrap();
        assert!(framed.renderable);
        assert!(framed.radius_m >= 0.05);
        assert!((framed.origin.x - crate::BOOTSTRAP_ROOT_M).abs() < 2.0);
        assert_eq!(world.revision(), revision);
        let loose = world.create_entity("Loose");
        let loose_id = world.resolve(loose).unwrap();
        let revision = world.revision();
        assert!(matches!(world.entity_focus(loose_id), Err(FocusError::NoSpatial)));
        assert_eq!(world.revision(), revision);
        let anchor = world.create_frame_anchor("Marker", crate::HighPrecisionPose::at(4.0, 1.0, -2.0)).unwrap();
        let revision = world.revision();
        let marker = world.entity_focus(anchor).unwrap();
        assert!(!marker.renderable);
        assert!((marker.radius_m - 1.0).abs() < 1.0e-9);
        assert!((marker.origin.x - (crate::BOOTSTRAP_ROOT_M + 4.0)).abs() < 1.0e-6);
        assert!((marker.origin.y - 1.0).abs() < 1.0e-6);
        assert_eq!(world.revision(), revision);
        assert!(matches!(world.entity_focus(EntityId::new()), Err(FocusError::Missing)));
        assert_eq!(world.extract(RenderFrameId(9)).unwrap().instance_count(), 2);
    }

    #[test]
    fn a_world_drag_converts_through_the_parent_and_rotation_normalizes() {
        let mut world = SceneWorld::bootstrap();
        let near = world.entity_outline()[0].uuid;
        let pose = world.entity_world_pose(near).unwrap();
        let round_trip = world.local_translation_for_world_point(near, pose.translation).unwrap();
        let local = translation_field(&world, near);
        assert!((round_trip.x - local.x).abs() < 1.0e-6);
        assert!((round_trip.z - local.z).abs() < 1.0e-6);
        let moved = Vec3::new(pose.translation.x + 0.25, pose.translation.y, pose.translation.z);
        let shifted = world.local_translation_for_world_point(near, moved).unwrap();
        assert!((shifted.x - (local.x + 0.25)).abs() < 1.0e-6);
        assert!((shifted.y - local.y).abs() < 1.0e-6);
        let tiny = Vec3::new(pose.translation.x + 0.01, pose.translation.y, pose.translation.z);
        let precise = world.local_translation_for_world_point(near, tiny).unwrap();
        assert!(
            (precise.x - (local.x + 0.01)).abs() < 1.0e-6,
            "absolute round trip at 1e9 is only good to the f64 ulp, got {}",
            precise.x - local.x
        );
        let exact = world.local_translation_plus_world_delta(near, local, Vec3::new(0.01, 0.0, 0.0)).unwrap();
        assert!((exact.x - (local.x + 0.01)).abs() < 1.0e-12);
        assert!((exact.y - local.y).abs() < 1.0e-12);
        assert!((exact.z - local.z).abs() < 1.0e-12);
        let handle = world.find_entity(near).unwrap();
        let frame = world.frame_of(handle).unwrap();
        let parent = world.frames.parent(frame).unwrap().unwrap();
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        world.frames.set_local_rotation(parent, yaw).unwrap();
        let turned = world.entity_world_pose(near).unwrap();
        let plus_x = Vec3::new(turned.translation.x + 1.0, turned.translation.y, turned.translation.z);
        let converted = world.local_translation_for_world_point(near, plus_x).unwrap();
        let current = world.frames.local_pose(frame).unwrap().translation;
        assert!((converted.x - current.x).abs() < 1.0e-4, "world +X must not land on local X when the parent is yawed");
        assert!((converted.z - current.z).abs() > 0.5);
        let exact_parent = world.local_translation_plus_world_delta(near, current, Vec3::new(1.0, 0.0, 0.0)).unwrap();
        assert!((exact_parent.x - current.x).abs() < 1.0e-6);
        assert!((exact_parent.z - current.z).abs() > 0.5);
        assert_eq!(world.set_entity_local_rotation(near, Quat::IDENTITY).unwrap(), AuthoringResult::Unchanged);
        let turned_q = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.2).unwrap();
        assert_eq!(world.set_entity_local_rotation(near, turned_q).unwrap(), AuthoringResult::Applied);
        let stored = world.frames.local_pose(frame).unwrap().rotation;
        let length = (stored.x * stored.x + stored.y * stored.y + stored.z * stored.z + stored.w * stored.w).sqrt();
        assert!((length - 1.0).abs() < 1.0e-6);
        assert!(world.set_entity_local_rotation(near, Quat { x: f64::NAN, y: 0.0, z: 0.0, w: 1.0 }).is_err());
    }

    /// The triangle's geometric normal is object +Z. The bootstrap directional shines along -Z,
    /// so that face starts in the light. Each authoring rotation is a new extract. Nothing here
    /// keeps the previous normal matrix.
    #[test]
    fn a_rotation_changes_the_shaded_normal_and_the_original_lighting_returns() {
        let mut world = SceneWorld::bootstrap();
        let near = world.entity_outline()[0].uuid;
        let home = shade_near(&world);
        assert!(home.directional_ndotl > 0.95, "the face should start toward the directional light, got {}", home.directional_ndotl);
        assert!(home.ndotv > 0.95, "the face should start toward the camera");
        assert!(home.normal[2] > 0.95, "view-space normal should be +Z");
        assert!(home.tangent[0] > 0.95, "tangent should start as +X");
        assert!(dot3(home.normal, home.tangent).abs() < 1.0e-4);
        assert!((len3(home.bitangent) - 1.0).abs() < 1.0e-4);

        let mut previous = home.directional_ndotl;
        for degrees in [15.0_f64, 30.0, 45.0] {
            rotate_x(&mut world, near, degrees);
            let now = shade_near(&world);
            assert!(now.directional_ndotl < previous - 0.02, "{degrees}° did not darken the directional term ({previous} -> {})", now.directional_ndotl);
            assert!(now.directional_ndotl > 0.2, "{degrees}° should still face the light somewhat");
            assert!(now.ndotv > 0.2);
            assert!((len3(now.normal) - 1.0).abs() < 1.0e-4);
            assert!(dot3(now.normal, now.tangent).abs() < 1.0e-3);
            previous = now.directional_ndotl;
        }
        let forty_five = shade_near(&world);
        assert!((forty_five.directional_ndotl - 0.707).abs() < 0.05, "{}", forty_five.directional_ndotl);

        rotate_x(&mut world, near, 90.0);
        let edge = shade_near(&world);
        assert!(edge.directional_ndotl.abs() < 0.05, "90° should put the directional light on the horizon, got {}", edge.directional_ndotl);
        assert!(edge.point_ndotl < 0.05, "the point light is on the camera side of the triangle, got {}", edge.point_ndotl);
        assert!(edge.spot_ndotl < 0.05, "the spot is also on that side, got {}", edge.spot_ndotl);
        assert!(edge.tangent[0] > 0.95, "rotation about X must not invent a new tangent axis");

        rotate_x(&mut world, near, 180.0);
        let away = shade_near(&world);
        assert!(away.ndotv < -0.9, "the normal must face away from the camera, got {}", away.ndotv);
        assert!(away.directional_ndotl < -0.9);
        let lights = world.extract(RenderFrameId(9)).unwrap();
        let light_pose = lights.lights()[0].pose;
        assert_eq!(light_pose.rotation, Quat::IDENTITY, "rotating the triangle must not rewrite the light");

        world.set_entity_local_rotation(near, Quat::IDENTITY).unwrap();
        let restored = shade_near(&world);
        assert!(same3(restored.normal, home.normal));
        assert!(same3(restored.tangent, home.tangent));
        assert!(same3(restored.bitangent, home.bitangent));
        assert!((restored.directional_ndotl - home.directional_ndotl).abs() < 1.0e-4);
        assert!((restored.point_ndotl - home.point_ndotl).abs() < 1.0e-4);
        assert!((restored.point_illuminance - home.point_illuminance).abs() < 1.0e-3);
        assert!((restored.ndotv - home.ndotv).abs() < 1.0e-4);

        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        world.set_entity_local_rotation(near, yaw).unwrap();
        let turned = shade_near(&world);
        assert!(turned.tangent[0].abs() < 0.05, "yaw must rotate the tangent off +X, got {:?}", turned.tangent);
        assert!(turned.normal[2].abs() < 0.05, "yaw must rotate the normal off +Z");
        assert!(dot3(turned.normal, turned.tangent).abs() < 1.0e-3);
        assert!((dot3(turned.bitangent, cross3(turned.normal, turned.tangent)) - len3(turned.bitangent)).abs() < 1.0e-3);
    }

    #[test]
    fn moving_toward_the_point_light_follows_inverse_square_and_returns() {
        let mut world = SceneWorld::bootstrap();
        let near = world.entity_outline()[0].uuid;
        let home = shade_near(&world);
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        let object = snapshot.instances().iter().find(|instance| instance.entity == near).unwrap().pose.translation;
        let point = snapshot.lights().iter().find(|light| light.kind == LightKind::Point).unwrap().pose.translation;
        let toward = Vec3::new(point.x - object.x, point.y - object.y, point.z - object.z);
        let distance = (toward.x * toward.x + toward.y * toward.y + toward.z * toward.z).sqrt();
        let step = toward.scale(0.4 / distance);
        let local = translation_field(&world, near);
        let nearer = Vec3::new(local.x + step.x, local.y + step.y, local.z + step.z);
        world.set_entity_local_translation(near, nearer).unwrap();
        let close = shade_near(&world);
        let ratio = close.point_illuminance / home.point_illuminance;
        let expected = (home.point_distance / close.point_distance).powi(2);
        assert!((ratio - expected).abs() < 1.0e-3, "illuminance {ratio} vs distance rule {expected}");
        assert!(close.point_illuminance > home.point_illuminance * 1.3);
        assert!((close.directional_ndotl - home.directional_ndotl).abs() < 1.0e-4, "translation must not change the directional normal");
        let farther = Vec3::new(local.x - step.x, local.y - step.y, local.z - step.z);
        world.set_entity_local_translation(near, farther).unwrap();
        let far = shade_near(&world);
        assert!(far.point_illuminance < home.point_illuminance);
        world.set_entity_local_translation(near, local).unwrap();
        let back = shade_near(&world);
        assert!((back.point_illuminance - home.point_illuminance).abs() < 1.0e-3);
        assert!((back.point_distance - home.point_distance).abs() < 1.0e-4);
        assert!(same3(back.normal, home.normal));
    }

    #[test]
    fn the_shader_normal_matrix_matches_the_inverse_transpose() {
        let stretched = jarvig_material::Mat3::scale(2.0, 1.0, 1.0);
        let geometric = [1.0_f32, 1.0, 0.0];
        let reference = jarvig_material::shade_normal(stretched, geometric, [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        let shader = normalize3(shader_normal_matrix(stretched).mul_vec(geometric));
        assert!(same3(reference, shader), "shader {shader:?} reference {reference:?}");
        let naive = normalize3(stretched.mul_vec(geometric));
        assert!((shader[0] - naive[0]).abs() > 0.05, "a non-uniform scale must not use the raw matrix");
    }

    struct Shade {
        normal: [f32; 3],
        tangent: [f32; 3],
        bitangent: [f32; 3],
        directional_ndotl: f32,
        point_ndotl: f32,
        spot_ndotl: f32,
        point_illuminance: f32,
        point_distance: f32,
        ndotv: f32,
    }

    fn rotate_x(world: &mut SceneWorld, entity: crate::EntityId, degrees: f64) {
        let rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), degrees.to_radians()).unwrap();
        world.set_entity_local_rotation(entity, rotation).unwrap();
    }

    fn shade_near(world: &SceneWorld) -> Shade {
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        let near = world.entity_outline()[0].uuid;
        let instance = snapshot.instances().iter().find(|instance| instance.entity == near).unwrap();
        let camera = snapshot.camera(world.front_camera().frame).unwrap();
        let packet = instance_gpu_transforms(instance, &camera.pose, camera.vertical_fov_radians, camera.near_m, 1.0).unwrap();
        let model = mat3_from(&packet.model);
        let view = mat3_from(&packet.view);
        let linear = mul3(view, model);
        let geometric = [0.0_f32, 0.0, 1.0];
        let tangent = [1.0_f32, 0.0, 0.0, 1.0];
        let normal = jarvig_material::shade_normal(linear, geometric, tangent, [0.0, 0.0, 1.0]);
        let by_quaternion = {
            let world_normal = instance.pose.rotation.rotate(Vec3::new(0.0, 0.0, 1.0));
            let viewed = camera.pose.rotation.conjugate().rotate(world_normal);
            [viewed.x as f32, viewed.y as f32, viewed.z as f32]
        };
        assert!(same3(normal, by_quaternion), "uploaded basis {normal:?} quaternion {by_quaternion:?}");
        let tangent_v = normalize3(linear.mul_vec([tangent[0], tangent[1], tangent[2]]));
        let bitangent = scale3(cross3(normal, tangent_v), tangent[3]);
        let frag = view.mul_vec([packet.model.cols[3][0], packet.model.cols[3][1], packet.model.cols[3][2]]);
        let ndotv = dot3(normal, normalize3(scale3(frag, -1.0)));
        let directional = snapshot.lights().iter().find(|light| light.kind == LightKind::Directional).unwrap();
        let point = snapshot.lights().iter().find(|light| light.kind == LightKind::Point).unwrap();
        let spot = snapshot.lights().iter().find(|light| light.kind == LightKind::Spot).unwrap();
        let drec = crate::render_light_record(directional, &camera.pose);
        let prec = crate::render_light_record(point, &camera.pose);
        let srec = crate::render_light_record(spot, &camera.pose);
        let ldir = normalize3([-drec.direction[0], -drec.direction[1], -drec.direction[2]]);
        let to_point = [prec.position[0] - frag[0], prec.position[1] - frag[1], prec.position[2] - frag[2]];
        let to_spot = [srec.position[0] - frag[0], srec.position[1] - frag[1], srec.position[2] - frag[2]];
        let point_distance = len3(to_point);
        Shade {
            normal,
            tangent: tangent_v,
            bitangent: normalize3(bitangent),
            directional_ndotl: dot3(normal, ldir),
            point_ndotl: dot3(normal, normalize3(to_point)),
            spot_ndotl: dot3(normal, normalize3(to_spot)),
            point_illuminance: prec.intensity / point_distance.max(0.01).powi(2),
            point_distance,
            ndotv,
        }
    }

    fn mat3_from(matrix: &Mat4) -> jarvig_material::Mat3 {
        jarvig_material::Mat3 {
            cols: [
                [matrix.cols[0][0], matrix.cols[0][1], matrix.cols[0][2]],
                [matrix.cols[1][0], matrix.cols[1][1], matrix.cols[1][2]],
                [matrix.cols[2][0], matrix.cols[2][1], matrix.cols[2][2]],
            ],
        }
    }

    fn mul3(left: jarvig_material::Mat3, right: jarvig_material::Mat3) -> jarvig_material::Mat3 {
        jarvig_material::Mat3 { cols: [left.mul_vec(right.cols[0]), left.mul_vec(right.cols[1]), left.mul_vec(right.cols[2])] }
    }

    /// The WGSL `jarvig_normal_matrix`: columns are the pairwise crosses, which is inverse-transpose.
    fn shader_normal_matrix(m: jarvig_material::Mat3) -> jarvig_material::Mat3 {
        let c0 = m.cols[0];
        let c1 = m.cols[1];
        let c2 = m.cols[2];
        let r0 = cross3(c1, c2);
        let inv_det = 1.0 / dot3(c0, r0).max(1.0e-8);
        jarvig_material::Mat3 { cols: [scale3(r0, inv_det), scale3(cross3(c2, c0), inv_det), scale3(cross3(c0, c1), inv_det)] }
    }

    fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    }

    fn scale3(v: [f32; 3], s: f32) -> [f32; 3] {
        [v[0] * s, v[1] * s, v[2] * s]
    }

    fn len3(v: [f32; 3]) -> f32 {
        dot3(v, v).sqrt()
    }

    fn normalize3(v: [f32; 3]) -> [f32; 3] {
        scale3(v, 1.0 / len3(v).max(1.0e-8))
    }

    fn same3(a: [f32; 3], b: [f32; 3]) -> bool {
        (a[0] - b[0]).abs() < 1.0e-4 && (a[1] - b[1]).abs() < 1.0e-4 && (a[2] - b[2]).abs() < 1.0e-4
    }

    #[test]
    fn block_memento_restores_the_log_instead_of_appending() {
        let mut world = SceneWorld::bootstrap();
        let id = world.create_block(crate::Vec3::new(0.0, 1.0, -4.0), crate::BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let before = world.remember_entity(id).unwrap();
        assert!(world.set_block_bevel(id, 0.2).is_ok());
        assert_eq!(world.authored_block(id).unwrap().history.len(), 1);
        world.apply_mementos(&[crate::EntityMemento::Present(before)]).unwrap();
        let restored = world.authored_block(id).unwrap();
        assert!(restored.bevel_m.abs() < 1.0e-9);
        assert!(restored.history.is_empty());
        let present = world.remember_entity(id).unwrap();
        world.apply_mementos(&[crate::EntityMemento::Absent(id)]).unwrap();
        assert!(world.authored_block(id).is_none());
        world.apply_mementos(&[crate::EntityMemento::Present(present)]).unwrap();
        assert_eq!(world.remember_entity(id).unwrap().uuid, id);
        assert!(world.authored_block(id).unwrap().history.is_empty());
    }
}
