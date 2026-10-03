//! Local player, player controller, and the builtin free-fly pawn.
//!
//! This runs on a runtime world. It does not read the editor camera and it does not
//! write the authored world. A project that does not select a pawn gets none of it.
//! [`PlayControl::attach`] spawns the free-fly pawn at local (0, 1.6, 8), looking
//! toward −Z. Play In Editor may then call [`PlayControl::place_free_fly_view`] so
//! that pawn starts at the view the user already has. Standalone play leaves the
//! default spawn. The runtime camera is still the pawn's Camera.

use std::collections::BTreeSet;

use crate::{
    ActionState, EntityId, GameSettings, InputDeviceState, InputMappingContext, PawnSelection, Quat, SceneWorld, StartupCameraPolicy, Vec3,
    TYPE_CAMERA, TYPE_FREE_FLY, TYPE_PAWN, TYPE_SPATIAL_FRAME,
};

const FLY_SPEED_M_S: f64 = 5.0;
const SPRINT_MULTIPLIER: f64 = 4.0;
const PITCH_LIMIT: f64 = 89.0 * std::f64::consts::PI / 180.0;

/// One possessed pawn for this play session. Dropped with the runtime world.
#[derive(Clone, Debug)]
pub struct PlayControl {
    settings: GameSettings,
    context: Option<InputMappingContext>,
    device: InputDeviceState,
    previously_down: BTreeSet<String>,
    pawn: Option<EntityId>,
    yaw: f64,
    pitch: f64,
    captured: bool,
    last: ActionState,
}

impl PlayControl {
    pub fn inactive() -> Self {
        Self {
            settings: GameSettings::inactive(),
            context: None,
            device: InputDeviceState::default(),
            previously_down: BTreeSet::new(),
            pawn: None,
            yaw: 0.0,
            pitch: 0.0,
            captured: false,
            last: empty_actions(),
        }
    }

    /// Spawns into `world` only when the project asked for a controller and a pawn.
    pub fn attach(world: &mut SceneWorld, settings: &GameSettings) -> Result<Self, String> {
        let mut control = Self::inactive();
        control.settings = settings.clone();
        if !settings.drives_a_pawn() {
            return Ok(control);
        }
        if settings.mapping_context.as_deref() == Some("JARVIG.Default") {
            control.context = Some(InputMappingContext::jarvig_default());
        }
        let pawn = match settings.pawn {
            PawnSelection::None => None,
            PawnSelection::DefaultFreeFly => Some(spawn_free_fly(world)?),
            PawnSelection::Entity(id) => {
                if world.authored_camera(id).is_none() || world.authored_local_pose(id).is_none() {
                    return Err(format!("authored pawn {id} needs Transform and Camera"));
                }
                Some(id)
            }
        };
        control.pawn = pawn;
        control.captured = pawn.is_some();
        Ok(control)
    }

    /// Moves the builtin free-fly pawn before the first tick. An authored pawn is left where the level put it.
    ///
    /// `local_translation` is the pawn's parent frame, which is the scene. Yaw is about +Y and pitch is about
    /// local +X, the same composition [`Self::fly`] writes every tick. The default spawn from [`Self::attach`]
    /// stays local (0, 1.6, 8) until this is called.
    pub fn place_free_fly_view(&mut self, world: &mut SceneWorld, local_translation: Vec3, yaw: f64, pitch: f64) -> Result<(), String> {
        if self.settings.pawn != PawnSelection::DefaultFreeFly {
            return Ok(());
        }
        let Some(pawn) = self.pawn else {
            return Ok(());
        };
        if !local_translation.x.is_finite() || !local_translation.y.is_finite() || !local_translation.z.is_finite() || !yaw.is_finite() || !pitch.is_finite()
        {
            return Err("free-fly start is not finite".into());
        }
        let pitch = pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
        let yaw_rotation = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), yaw).map_err(|error| error.to_string())?;
        let pitch_rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), pitch).map_err(|error| error.to_string())?;
        world.set_entity_local_translation(pawn, local_translation).map_err(|error| error.to_string())?;
        world.set_entity_local_rotation(pawn, yaw_rotation.mul(pitch_rotation)).map_err(|error| error.to_string())?;
        self.yaw = yaw;
        self.pitch = pitch;
        Ok(())
    }

    pub fn pawn(&self) -> Option<EntityId> {
        self.pawn
    }

    pub fn pawn_camera(&self) -> Option<EntityId> {
        self.pawn.filter(|_| self.settings.startup_camera == StartupCameraPolicy::Pawn)
    }

    pub fn wants_capture(&self) -> bool {
        self.captured && self.pawn.is_some()
    }

    pub fn device_mut(&mut self) -> &mut InputDeviceState {
        &mut self.device
    }

    pub fn actions(&self) -> &ActionState {
        &self.last
    }

    /// Applies one sample. Mouse deltas on the device are consumed.
    pub fn tick(&mut self, world: &mut SceneWorld, dt: f64) -> Result<(), String> {
        let Some(context) = self.context.clone() else {
            self.device.mouse_dx = 0.0;
            self.device.mouse_dy = 0.0;
            return Ok(());
        };
        let state = context.evaluate(&self.device, dt, &self.previously_down);
        self.previously_down = state.remember_down();
        self.device.mouse_dx = 0.0;
        self.device.mouse_dy = 0.0;
        if state.just_pressed("Pause") {
            self.captured = false;
        }
        if state.just_pressed("Engage") && !self.captured && self.pawn.is_some() {
            self.captured = true;
        }
        if let Some(pawn) = self.pawn {
            self.fly(world, pawn, &state, dt)?;
        }
        self.last = state;
        Ok(())
    }

    fn fly(&mut self, world: &mut SceneWorld, pawn: EntityId, state: &ActionState, dt: f64) -> Result<(), String> {
        let dt = if dt.is_finite() && dt > 0.0 { dt.min(0.1) } else { 0.0 };
        if self.captured {
            let (look_x, look_y) = state.vec2("Look");
            self.yaw -= look_x;
            self.pitch = (self.pitch - look_y).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), self.yaw).map_err(|error| error.to_string())?;
        let pitch = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), self.pitch).map_err(|error| error.to_string())?;
        let rotation = yaw.mul(pitch);
        let forward = rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
        let right = rotation.rotate(Vec3::new(1.0, 0.0, 0.0));
        let (move_x, move_y) = state.vec2("Move");
        let up = state.float("MoveUp");
        let mut direction = right.scale(move_x) + forward.scale(move_y) + Vec3::new(0.0, up, 0.0);
        let length = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        if length > 1.0 {
            direction = direction.scale(1.0 / length);
        }
        let speed = if state.pressed("Sprint") { FLY_SPEED_M_S * SPRINT_MULTIPLIER } else { FLY_SPEED_M_S };
        let (mut translation, _) = world.authored_local_pose(pawn).ok_or("pawn has no transform")?;
        if length > 1.0e-8 {
            translation = translation + direction.scale(speed * dt);
        }
        // Analytic oriented box. A zero move still ejects a pawn that started inside. Not JRV-0090.
        translation = world.separate_from_blocks(translation);
        world.set_entity_local_translation(pawn, translation).map_err(|error| error.to_string())?;
        world.set_entity_local_rotation(pawn, rotation).map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn spawn_free_fly(world: &mut SceneWorld) -> Result<EntityId, String> {
    let handle = world.create_entity("Player");
    let id = world.resolve(handle).map_err(|error| error.to_string())?;
    world.add_component(id, TYPE_SPATIAL_FRAME).map_err(|error| error.to_string())?;
    world.add_component(id, TYPE_CAMERA).map_err(|error| error.to_string())?;
    world.add_component(id, TYPE_PAWN).map_err(|error| error.to_string())?;
    world.add_component(id, TYPE_FREE_FLY).map_err(|error| error.to_string())?;
    world.set_entity_local_translation(id, Vec3::new(0.0, 1.6, 8.0)).map_err(|error| error.to_string())?;
    Ok(id)
}

fn empty_actions() -> ActionState {
    InputMappingContext { name: String::new(), actions: Vec::new(), bindings: Vec::new() }.evaluate(&InputDeviceState::default(), 0.0, &BTreeSet::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lighting_lab_level, GameSettings, PawnSelection, PhysicalControl, StartupCameraPolicy};

    #[test]
    fn an_unconfigured_project_does_not_grow_a_pawn() {
        let document = lighting_lab_level();
        let mut world = document.instantiate().unwrap();
        let before = world.entity_count();
        let control = PlayControl::attach(&mut world, &GameSettings::inactive()).unwrap();
        assert!(control.pawn().is_none());
        assert!(!control.wants_capture());
        assert_eq!(world.entity_count(), before);
    }

    #[test]
    fn the_default_free_fly_pawn_moves_from_actions_and_uses_its_own_camera() {
        let document = lighting_lab_level();
        let authored = document.instantiate().unwrap();
        let revision = authored.revision();
        let mut runtime = document.instantiate().unwrap();
        let mut settings = GameSettings::inactive();
        settings.player_controller = true;
        settings.pawn = PawnSelection::DefaultFreeFly;
        settings.mapping_context = Some("JARVIG.Default".into());
        settings.startup_camera = StartupCameraPolicy::Pawn;
        let mut control = PlayControl::attach(&mut runtime, &settings).unwrap();
        let pawn = control.pawn().unwrap();
        assert_ne!(pawn, authored.entity_outline()[0].uuid);
        assert!(runtime.authored_camera(pawn).is_some());
        assert_eq!(control.pawn_camera(), Some(pawn));
        let labels: Vec<_> = runtime.component_stack(pawn).unwrap().iter().map(|item| item.role.label()).collect();
        assert_eq!(labels, vec!["Transform", "Camera", "Pawn", "Free Fly"]);
        let start = runtime.authored_local_pose(pawn).unwrap().0;
        assert!(start.x.abs() < 1.0e-9 && (start.y - 1.6).abs() < 1.0e-9 && (start.z - 8.0).abs() < 1.0e-9, "{start:?}");
        control.device_mut().held.insert(PhysicalControl::KeyW);
        control.tick(&mut runtime, 0.1).unwrap();
        let moved = runtime.authored_local_pose(pawn).unwrap().0;
        assert!(moved.z < start.z - 0.4, "{moved:?}");
        assert_eq!(authored.revision(), revision);
        assert!(authored.entity_outline().iter().all(|row| row.name != "Player"));
    }

    #[test]
    fn the_editor_can_place_the_free_fly_pawn_without_moving_an_unconfigured_world() {
        let document = lighting_lab_level();
        let mut idle = document.instantiate().unwrap();
        let before = idle.entity_count();
        let mut idle_control = PlayControl::attach(&mut idle, &GameSettings::inactive()).unwrap();
        idle_control.place_free_fly_view(&mut idle, Vec3::new(1.0, 2.0, 3.0), 0.0, 0.0).unwrap();
        assert!(idle_control.pawn().is_none());
        assert_eq!(idle.entity_count(), before);

        let authored = document.instantiate().unwrap();
        let revision = authored.revision();
        let mut runtime = document.instantiate().unwrap();
        let mut settings = GameSettings::inactive();
        settings.player_controller = true;
        settings.pawn = PawnSelection::DefaultFreeFly;
        settings.mapping_context = Some("JARVIG.Default".into());
        settings.startup_camera = StartupCameraPolicy::Pawn;
        let mut control = PlayControl::attach(&mut runtime, &settings).unwrap();
        let pawn = control.pawn().unwrap();
        let default_spawn = runtime.authored_local_pose(pawn).unwrap().0;
        assert!((default_spawn.z - 8.0).abs() < 1.0e-9, "{default_spawn:?}");
        control.place_free_fly_view(&mut runtime, Vec3::new(2.0, 3.0, 4.0), std::f64::consts::FRAC_PI_2, 0.0).unwrap();
        let placed = runtime.authored_local_pose(pawn).unwrap().0;
        assert!((placed.x - 2.0).abs() < 1.0e-9 && (placed.y - 3.0).abs() < 1.0e-9 && (placed.z - 4.0).abs() < 1.0e-9, "{placed:?}");
        control.device_mut().held.insert(PhysicalControl::KeyW);
        control.tick(&mut runtime, 0.1).unwrap();
        let moved = runtime.authored_local_pose(pawn).unwrap().0;
        assert!(moved.x < placed.x - 0.4, "{moved:?}");
        assert!((moved.z - placed.z).abs() < 0.05, "{moved:?}");
        assert_eq!(authored.revision(), revision);
        assert!(authored.entity_outline().iter().all(|row| row.name != "Player"));
    }
}
