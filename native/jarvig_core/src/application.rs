//! The running game. The level document is source data. [`RuntimeWorld`] is an execution copy.
//!
//! Input, script, physics, audio, gameplay, and networking are not implemented here.
//! The editor Play command is not implemented here. This module does not create a device.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{ClockAdvance, ClockError, EntityId, LevelDocument, LevelError, RenderFrameId, RenderSceneSnapshot, SceneWorld, SimulationClock, SpaceError};

static NEXT_APPLICATION: AtomicU64 = AtomicU64::new(1);

/// Resting phases are `Created`, `Ready`, `Running`, `Paused`, and `Stopped`.
/// `Loading`, `Starting`, and `Stopping` occur inside a synchronous call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationPhase {
    Created,
    Loading,
    Ready,
    Starting,
    Running,
    Paused,
    Stopping,
    Stopped,
}

#[derive(Debug)]
pub enum ApplicationError {
    InvalidPhase,
    NotRunning,
    NoRuntime,
    Level(LevelError),
    Clock(ClockError),
    Scene(SpaceError),
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPhase => write!(formatter, "application phase does not allow that transition"),
            Self::NotRunning => write!(formatter, "application is not running"),
            Self::NoRuntime => write!(formatter, "application has no runtime world"),
            Self::Level(error) => write!(formatter, "{error}"),
            Self::Clock(error) => write!(formatter, "{error:?}"),
            Self::Scene(error) => write!(formatter, "{error}"),
        }
    }
}

/// One instantiated level. Handles and subsystem ids are new. Entity UUIDs are the authored ones.
pub struct RuntimeWorld {
    world: SceneWorld,
}

impl RuntimeWorld {
    fn instantiate(document: &LevelDocument, assets: &crate::MeshAssetLibrary) -> Result<Self, LevelError> {
        Ok(Self { world: document.instantiate_with(assets)? })
    }

    pub fn world(&self) -> &SceneWorld {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut SceneWorld {
        &mut self.world
    }

    fn extract(&self, frame: RenderFrameId) -> Result<RenderSceneSnapshot, SpaceError> {
        self.world.extract(frame)
    }
}

/// Survives level replacement. Does not own the editor world, the renderer, or editor session state.
pub struct GameApplication {
    identity: u64,
    phase: ApplicationPhase,
    source: Option<LevelDocument>,
    runtime: Option<RuntimeWorld>,
    /// Copied from the document at start. Never inferred from component order.
    camera_reference: Option<EntityId>,
    clock: SimulationClock,
    runtime_generation: u64,
    level_generation: u64,
    extracts: u64,
    mesh_assets: crate::MeshAssetLibrary,
}

impl GameApplication {
    pub fn new() -> Self {
        Self {
            identity: NEXT_APPLICATION.fetch_add(1, Ordering::Relaxed),
            phase: ApplicationPhase::Created,
            source: None,
            runtime: None,
            camera_reference: None,
            clock: SimulationClock::new(60.0, 8).expect("60 Hz clock"),
            runtime_generation: 0,
            level_generation: 0,
            extracts: 0,
            mesh_assets: crate::MeshAssetLibrary::default(),
        }
    }

    pub fn set_mesh_assets(&mut self, assets: crate::MeshAssetLibrary) {
        self.mesh_assets = assets;
    }

    pub fn identity(&self) -> u64 {
        self.identity
    }

    pub fn phase(&self) -> ApplicationPhase {
        self.phase
    }

    pub fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }

    pub fn level_generation(&self) -> u64 {
        self.level_generation
    }

    pub fn clock_frame(&self) -> u64 {
        self.clock.frame
    }

    /// Accepts a document. Does not instantiate a runtime world.
    pub fn load(&mut self, document: LevelDocument) -> Result<(), ApplicationError> {
        if !matches!(self.phase, ApplicationPhase::Created | ApplicationPhase::Ready | ApplicationPhase::Stopped) {
            return Err(ApplicationError::InvalidPhase);
        }
        let previous = self.phase;
        self.phase = ApplicationPhase::Loading;
        if let Err(error) = document.validate() {
            self.phase = previous;
            return Err(ApplicationError::Level(error));
        }
        self.source = Some(document);
        self.runtime = None;
        self.camera_reference = None;
        self.level_generation = self.level_generation.saturating_add(1);
        self.phase = ApplicationPhase::Ready;
        Ok(())
    }

    /// Instantiates a fresh runtime world from the retained document.
    pub fn start(&mut self) -> Result<(), ApplicationError> {
        if !matches!(self.phase, ApplicationPhase::Ready | ApplicationPhase::Stopped) {
            return Err(ApplicationError::InvalidPhase);
        }
        let Some(source) = self.source.as_ref() else {
            return Err(ApplicationError::InvalidPhase);
        };
        let previous = self.phase;
        self.phase = ApplicationPhase::Starting;
        let world = match RuntimeWorld::instantiate(source, &self.mesh_assets) {
            Ok(world) => world,
            Err(error) => {
                self.phase = previous;
                return Err(ApplicationError::Level(error));
            }
        };
        self.camera_reference = source.world_settings.startup_camera;
        self.runtime = Some(world);
        self.runtime_generation = self.runtime_generation.saturating_add(1);
        self.phase = ApplicationPhase::Running;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), ApplicationError> {
        if self.phase != ApplicationPhase::Running {
            return Err(ApplicationError::InvalidPhase);
        }
        self.phase = ApplicationPhase::Paused;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), ApplicationError> {
        if self.phase != ApplicationPhase::Paused {
            return Err(ApplicationError::InvalidPhase);
        }
        self.phase = ApplicationPhase::Running;
        Ok(())
    }

    /// Drops the runtime world. The retained document and this application stay.
    pub fn stop(&mut self) -> Result<(), ApplicationError> {
        if !matches!(self.phase, ApplicationPhase::Running | ApplicationPhase::Paused) {
            return Err(ApplicationError::InvalidPhase);
        }
        self.phase = ApplicationPhase::Stopping;
        self.runtime = None;
        self.camera_reference = None;
        self.phase = ApplicationPhase::Stopped;
        Ok(())
    }

    /// Advances the application clock and the runtime world's simulation tick. Does not extract.
    pub fn tick(&mut self, delta_seconds: f64) -> Result<ClockAdvance, ApplicationError> {
        if self.phase != ApplicationPhase::Running {
            return Err(ApplicationError::NotRunning);
        }
        let advance = self.clock.advance(delta_seconds).map_err(ApplicationError::Clock)?;
        if let Some(world) = self.runtime.as_mut() {
            let tick = world.world().simulation_tick().saturating_add(u64::from(advance.fixed_steps));
            world.world_mut().set_simulation_tick(tick);
        }
        Ok(advance)
    }

    pub fn runtime_world(&self) -> Option<&SceneWorld> {
        self.runtime.as_ref().map(RuntimeWorld::world)
    }

    pub fn runtime_world_mut(&mut self) -> Result<&mut SceneWorld, ApplicationError> {
        if !matches!(self.phase, ApplicationPhase::Running | ApplicationPhase::Paused) {
            return Err(ApplicationError::InvalidPhase);
        }
        self.runtime.as_mut().map(RuntimeWorld::world_mut).ok_or(ApplicationError::NoRuntime)
    }

    /// Points the runtime view at one Camera. Does not write the authored document.
    pub fn possess_camera(&mut self, id: EntityId) -> Result<(), ApplicationError> {
        if !matches!(self.phase, ApplicationPhase::Running | ApplicationPhase::Paused) {
            return Err(ApplicationError::InvalidPhase);
        }
        let camera = self.runtime.as_ref().ok_or(ApplicationError::NoRuntime)?.world().authored_camera(id).ok_or(ApplicationError::NoRuntime)?;
        if !camera.enabled {
            return Err(ApplicationError::InvalidPhase);
        }
        self.camera_reference = Some(id);
        Ok(())
    }

    /// The explicit startup camera when it still has an enabled Camera component. Never a scan.
    pub fn active_camera(&self) -> Option<EntityId> {
        let id = self.camera_reference?;
        let camera = self.runtime.as_ref()?.world().authored_camera(id)?;
        if camera.enabled { Some(id) } else { None }
    }

    /// CPU snapshot of the runtime world. Does not create a device and does not read the editor world.
    pub fn extract_snapshot(&mut self) -> Result<RenderSceneSnapshot, ApplicationError> {
        if !matches!(self.phase, ApplicationPhase::Running | ApplicationPhase::Paused) {
            return Err(ApplicationError::InvalidPhase);
        }
        self.extracts = self.extracts.saturating_add(1);
        let frame = RenderFrameId(self.extracts);
        self.runtime.as_ref().ok_or(ApplicationError::NoRuntime)?.extract(frame).map_err(ApplicationError::Scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lighting_lab_level, CameraRecord, EntityId, HighPrecisionPose, LevelDocument, LEVEL_CAMERA_VERSION, LEVEL_FORMAT_VERSION};

    fn uuid(text: &str) -> EntityId {
        EntityId::parse(text).expect(text)
    }

    #[test]
    fn a_runtime_world_is_a_copy_and_stop_discards_it() {
        let document = lighting_lab_level();
        let mut authored = document.instantiate().unwrap();
        let level_id = document.level_uuid;
        let before = LevelDocument::capture(&authored, level_id, "Lighting Lab").unwrap().to_json();
        let revision = authored.revision();
        let count = authored.entity_count();
        let near = uuid("22222222-2222-4222-8222-222222222222");
        let floor = uuid("88888888-8888-4888-8888-888888888888");
        let mut app = GameApplication::new();
        let identity = app.identity();
        assert_eq!(app.phase(), ApplicationPhase::Created);
        assert!(app.start().is_err());
        assert!(app.stop().is_err());
        app.load(document).unwrap();
        assert_eq!(app.phase(), ApplicationPhase::Ready);
        assert!(app.runtime_world().is_none());
        app.load(lighting_lab_level()).unwrap();
        app.start().unwrap();
        assert_eq!(app.phase(), ApplicationPhase::Running);
        assert_eq!(app.identity(), identity);
        assert_eq!(app.runtime_generation(), 1);
        assert!(!std::ptr::eq(app.runtime_world().unwrap(), &authored));
        assert_eq!(app.runtime_world().unwrap().entity_count(), count);
        assert!(app.runtime_world().unwrap().entity_ownership(near).is_ok());
        assert!(app.active_camera().is_none());
        let _editor_view = app.runtime_world().unwrap().front_camera();
        assert!(_editor_view.near_m > 0.0);

        app.runtime_world_mut().unwrap().destroy_authored(near).unwrap();
        app.runtime_world_mut().unwrap().create_entity("Runtime Crate");
        assert!(app.runtime_world().unwrap().entity_ownership(near).is_err());
        assert!(app.runtime_world().unwrap().entity_outline().iter().any(|row| row.name == "Runtime Crate"));
        assert_eq!(authored.revision(), revision);
        assert_eq!(authored.entity_count(), count);
        assert!(authored.entity_ownership(near).is_ok());
        assert!(!authored.entity_outline().iter().any(|row| row.name == "Runtime Crate"));
        assert_eq!(LevelDocument::capture(&authored, level_id, "Lighting Lab").unwrap().to_json(), before);

        authored.set_entity_name(floor, "Floor Edited").unwrap();
        let runtime_floor = app.runtime_world().unwrap().entity_outline().into_iter().find(|row| row.uuid == floor).unwrap();
        assert_eq!(runtime_floor.name, "Floor");
        authored.set_entity_name(floor, "Floor").unwrap();
        assert_eq!(LevelDocument::capture(&authored, level_id, "Lighting Lab").unwrap().to_json(), before);

        let advance = app.tick(1.0 / 60.0).unwrap();
        assert!(advance.fixed_steps >= 1);
        assert_eq!(app.clock_frame(), 1);
        assert_eq!(app.runtime_world().unwrap().simulation_tick(), u64::from(advance.fixed_steps));
        app.pause().unwrap();
        assert!(app.tick(1.0 / 60.0).is_err());
        assert!(app.load(lighting_lab_level()).is_err());
        app.resume().unwrap();

        let snapshot = app.extract_snapshot().unwrap();
        assert!(snapshot.instance_count() > 0);
        assert!(snapshot.game_cameras().is_empty());

        app.stop().unwrap();
        assert_eq!(app.phase(), ApplicationPhase::Stopped);
        assert!(app.runtime_world().is_none());
        assert!(app.extract_snapshot().is_err());
        assert_eq!(LevelDocument::capture(&authored, level_id, "Lighting Lab").unwrap().to_json(), before);

        app.start().unwrap();
        assert_eq!(app.runtime_generation(), 2);
        assert_eq!(app.identity(), identity);
        assert_eq!(app.clock_frame(), 1);
        assert_eq!(app.runtime_world().unwrap().simulation_tick(), 0);
        assert!(app.runtime_world().unwrap().entity_ownership(near).is_ok());
        assert!(!app.runtime_world().unwrap().entity_outline().iter().any(|row| row.name == "Runtime Crate"));
        app.tick(1.0 / 60.0).unwrap();
        assert_eq!(app.clock_frame(), 2);
        app.stop().unwrap();
        assert_eq!(LevelDocument::capture(&authored, level_id, "Lighting Lab").unwrap().to_json(), before);
    }

    #[test]
    fn the_startup_camera_is_explicit_and_a_level_change_keeps_the_application() {
        let mut source = SceneWorld::new_session();
        let settings = uuid("11111111-1111-4111-8111-111111111111");
        source.spawn_saved_world_settings(settings, "World Settings", crate::EnvironmentLight::bootstrap()).unwrap();
        let primary = uuid("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
        let other = uuid("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        let camera = CameraRecord {
            enabled: true,
            orthographic: false,
            vertical_fov_deg: 42.0,
            ortho_height_m: 10.0,
            near_m: 0.1,
            far_m: 1000.0,
            priority: 0,
            viewport: [0.0, 0.0, 1.0, 1.0],
        };
        source.spawn_saved_camera(other, "Other", None, HighPrecisionPose::at(0.0, 1.0, 4.0), camera.clone()).unwrap();
        source.spawn_saved_camera(primary, "Primary", None, HighPrecisionPose::at(1.0, 1.6, 8.0), camera).unwrap();
        let level_id = uuid("dddddddd-dddd-4ddd-8ddd-dddddddddddd");
        let mut unlabeled = LevelDocument::capture(&source, level_id, "Unlabeled").unwrap();
        assert_eq!(unlabeled.format_version, LEVEL_CAMERA_VERSION);
        assert!(unlabeled.world_settings.startup_camera.is_none());
        assert!(!unlabeled.to_json().contains("startup_camera"));

        let mut app = GameApplication::new();
        let identity = app.identity();
        app.load(unlabeled.clone()).unwrap();
        app.start().unwrap();
        assert!(app.active_camera().is_none());
        let snapshot = app.extract_snapshot().unwrap();
        assert_eq!(snapshot.game_cameras().len(), 2);
        assert!(snapshot.game_cameras().iter().any(|camera| camera.entity == other));
        assert!(snapshot.game_cameras().iter().any(|camera| camera.entity == primary));
        app.stop().unwrap();

        source.set_startup_camera(Some(primary)).unwrap();
        assert!(matches!(source.set_startup_camera(Some(settings)), Err(crate::AuthoringError::InvalidOperation)));
        let mut labeled = LevelDocument::capture(&source, level_id, "Labeled").unwrap();
        assert_eq!(labeled.world_settings.startup_camera, Some(primary));
        assert!(labeled.to_json().contains("startup_camera"));
        let round_trip = crate::parse_level(&labeled.to_json()).unwrap();
        assert_eq!(round_trip.world_settings.startup_camera, Some(primary));
        app.load(labeled.clone()).unwrap();
        assert_eq!(app.identity(), identity);
        assert_eq!(app.phase(), ApplicationPhase::Ready);
        app.start().unwrap();
        assert_eq!(app.runtime_generation(), 2);
        assert_eq!(app.active_camera(), Some(primary));
        let pose = app.runtime_world().unwrap().entity_world_pose(primary).unwrap();
        let snapshot = app.extract_snapshot().unwrap();
        let selected = snapshot.game_cameras().iter().find(|camera| camera.entity == primary).unwrap();
        assert!((selected.vertical_fov_radians - 42.0_f64.to_radians()).abs() < 1.0e-9);
        assert_eq!(selected.pose.translation, pose.translation);
        app.stop().unwrap();

        labeled.entities.iter_mut().find(|entity| entity.uuid == primary).unwrap().components.iter_mut().for_each(|component| {
            if let crate::ComponentRecord::Camera(camera) = component {
                camera.enabled = false;
            }
        });
        app.load(labeled).unwrap();
        app.start().unwrap();
        assert!(app.active_camera().is_none());
        assert!(app.runtime_world().unwrap().authored_camera(other).unwrap().enabled);
        app.stop().unwrap();

        unlabeled.world_settings.startup_camera = Some(floor_id());
        unlabeled.format_version = LEVEL_FORMAT_VERSION;
        assert!(unlabeled.validate().is_err());
        assert_eq!(app.identity(), identity);
    }

    #[test]
    fn an_execution_copy_can_draw_with_the_source_ids_without_writing_the_source() {
        let document = lighting_lab_level();
        let mut authored = document.instantiate().unwrap();
        let floor = floor_id();
        let duplicate = authored.duplicate_authored(floor).unwrap();
        let authored_mesh = authored.entity_ownership(duplicate).unwrap().mesh;
        let probe = uuid("77777777-7777-4777-8777-777777777777");
        let authored_probe = authored.entity_ownership(probe).unwrap().probe;
        let revision = authored.revision();
        let pose = authored.entity_world_pose(floor).unwrap();
        let captured = LevelDocument::capture(&authored, document.level_uuid, "Lighting Lab").unwrap();
        let mut app = GameApplication::new();
        app.load(captured).unwrap();
        app.start().unwrap();
        assert_ne!(app.runtime_world().unwrap().entity_ownership(duplicate).unwrap().mesh, authored_mesh);
        app.runtime_world_mut().unwrap().alias_draw_keys_from(&authored);
        assert_eq!(app.runtime_world().unwrap().entity_ownership(duplicate).unwrap().mesh, authored_mesh);
        assert_eq!(app.runtime_world().unwrap().entity_ownership(probe).unwrap().probe, authored_probe);
        assert_eq!(authored.revision(), revision);
        app.runtime_world_mut().unwrap().set_entity_local_translation(floor, crate::Vec3::new(4.0, 4.0, 4.0)).unwrap();
        assert_eq!(authored.entity_world_pose(floor).unwrap().translation, pose.translation);
        assert_eq!(authored.revision(), revision);
        let generation = app.runtime_generation();
        app.stop().unwrap();
        app.start().unwrap();
        assert_ne!(app.runtime_generation(), generation);
        assert_ne!(app.runtime_world().unwrap().entity_ownership(duplicate).unwrap().mesh, authored_mesh);
    }

    fn floor_id() -> EntityId {
        uuid("88888888-8888-4888-8888-888888888888")
    }
}
