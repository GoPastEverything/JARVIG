//! Perspective editor navigation. Session state, not a scene entity.
//!
//! Position is binary64 in the root frame. Orientation is a quaternion built
//! from yaw about +Y and pitch about local +X. Roll stays zero. The world is
//! not revised when this moves.

use jarvig_core::{Quat, ResolvedPose, Vec3};

/// Ordinary editor look cannot flip over the pole.
pub const PITCH_LIMIT_RADIANS: f64 = 89.0 * std::f64::consts::PI / 180.0;
/// A stalled frame must not fling the camera.
pub const MAX_DELTA_SECONDS: f64 = 0.1;
pub const MIN_SPEED_M_S: f64 = 0.05;
pub const MAX_SPEED_M_S: f64 = 10_000.0;
pub const MIN_ORBIT_M: f64 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraNavMode {
    Fly,
    Orbit,
    Pan,
}

/// What the viewport is doing this frame. The window fills it. The controller consumes it.
#[derive(Clone, Debug, Default)]
pub struct ViewportNavInput {
    pub forward: i32,
    pub strafe: i32,
    pub vertical: i32,
    pub boost: bool,
    pub look: bool,
    pub pan: bool,
    pub orbit: bool,
    pub mouse_dx: f64,
    pub mouse_dy: f64,
    pub wheel_notches: i32,
    pub alt: bool,
    pub capture: bool,
}

impl ViewportNavInput {
    pub fn clear_gestures(&mut self) {
        self.mouse_dx = 0.0;
        self.mouse_dy = 0.0;
        self.wheel_notches = 0;
        self.look = false;
        self.pan = false;
        self.orbit = false;
        self.capture = false;
        self.forward = 0;
        self.strafe = 0;
        self.vertical = 0;
        self.boost = false;
    }
}

#[derive(Clone, Debug)]
pub struct EditorCameraController {
    pub yaw: f64,
    pub pitch: f64,
    /// Resolved root-frame position. Meters. Binary64.
    pub position: Vec3,
    pub speed_m_s: f64,
    pub boost_multiplier: f64,
    pub radians_per_pixel: f64,
    pub vertical_fov_radians: f64,
    pub near_m: f32,
    pub orbit_pivot: Vec3,
    pub orbit_distance: f64,
    pub mode: CameraNavMode,
    pub updates: u64,
    pub focus_actions: u64,
    /// Bootstrap camera frame this pose was seeded from. Navigation does not write it.
    pub reference_frame: jarvig_core::FrameId,
}

impl EditorCameraController {
    pub fn from_pose(reference_frame: jarvig_core::FrameId, pose: ResolvedPose, vertical_fov_radians: f64, near_m: f32) -> Self {
        let (yaw, pitch) = yaw_pitch_from_quaternion(pose.rotation);
        let forward = pose.rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
        Self {
            reference_frame,
            yaw,
            pitch,
            position: pose.translation,
            speed_m_s: 5.0,
            boost_multiplier: 4.0,
            radians_per_pixel: 0.005,
            vertical_fov_radians,
            near_m,
            orbit_pivot: pose.translation + forward.scale(5.0),
            orbit_distance: 5.0,
            mode: CameraNavMode::Fly,
            updates: 0,
            focus_actions: 0,
        }
    }

    pub fn pose(&self) -> ResolvedPose {
        ResolvedPose { translation: self.position, rotation: self.orientation() }
    }

    pub fn orientation(&self) -> Quat {
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), self.yaw).unwrap_or(Quat::IDENTITY);
        let pitch = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), self.pitch).unwrap_or(Quat::IDENTITY);
        // Pitch is applied in the yawed local frame: yaw, then local pitch.
        normalize_quat(yaw.mul(pitch))
    }

    pub fn forward(&self) -> Vec3 {
        self.orientation().rotate(Vec3::new(0.0, 0.0, -1.0))
    }

    pub fn right(&self) -> Vec3 {
        self.orientation().rotate(Vec3::new(1.0, 0.0, 0.0))
    }

    pub fn camera_up(&self) -> Vec3 {
        self.orientation().rotate(Vec3::new(0.0, 1.0, 0.0))
    }

    pub fn tick(&mut self, input: &mut ViewportNavInput, delta_seconds: f64) {
        let dt = delta_seconds.clamp(0.0, MAX_DELTA_SECONDS);
        if input.look || input.orbit {
            self.look(input.mouse_dx, input.mouse_dy);
        }
        if input.orbit {
            self.mode = CameraNavMode::Orbit;
            self.reorbit();
        } else if input.pan {
            self.mode = CameraNavMode::Pan;
            self.pan(input.mouse_dx, input.mouse_dy);
        } else if input.look || input.forward != 0 || input.strafe != 0 || input.vertical != 0 {
            self.mode = CameraNavMode::Fly;
        }
        if input.alt && input.wheel_notches != 0 {
            self.dolly(input.wheel_notches);
        } else if input.wheel_notches != 0 {
            self.adjust_speed(input.wheel_notches);
        }
        let moving = input.forward != 0 || input.strafe != 0 || input.vertical != 0;
        if moving && !input.orbit && !input.pan {
            let boost = if input.boost { self.boost_multiplier } else { 1.0 };
            self.fly(input.forward, input.strafe, input.vertical, boost, dt);
        }
        input.mouse_dx = 0.0;
        input.mouse_dy = 0.0;
        input.wheel_notches = 0;
        self.updates = self.updates.saturating_add(1);
    }

    pub fn look(&mut self, dx_pixels: f64, dy_pixels: f64) {
        self.yaw -= dx_pixels * self.radians_per_pixel;
        self.pitch -= dy_pixels * self.radians_per_pixel;
        self.pitch = self.pitch.clamp(-PITCH_LIMIT_RADIANS, PITCH_LIMIT_RADIANS);
    }

    /// W follows the look direction, including pitch. Q/E use world up.
    pub fn fly(&mut self, forward: i32, strafe: i32, vertical: i32, boost: f64, dt: f64) {
        let mut direction = self.forward().scale(forward as f64) + self.right().scale(strafe as f64) + Vec3::new(0.0, vertical as f64, 0.0);
        let length = vector_length(direction);
        if length > 1.0e-8 {
            direction = direction.scale(1.0 / length);
            let step = self.speed_m_s * boost * dt;
            self.position = self.position + direction.scale(step);
            self.orbit_pivot = self.orbit_pivot + direction.scale(step);
        }
    }

    pub fn pan(&mut self, dx_pixels: f64, dy_pixels: f64) {
        let scale = self.orbit_distance.max(1.0) * 0.002;
        let delta = self.right().scale(dx_pixels * scale) + self.camera_up().scale(-dy_pixels * scale);
        self.position = self.position + delta;
        self.orbit_pivot = self.orbit_pivot + delta;
    }

    pub fn reorbit(&mut self) {
        let forward = self.forward();
        self.position = sub(self.orbit_pivot, forward.scale(self.orbit_distance));
    }

    pub fn dolly(&mut self, notches: i32) {
        let factor = 1.1_f64.powi(-notches);
        self.orbit_distance = (self.orbit_distance * factor).clamp(MIN_ORBIT_M, 1.0e7);
        self.reorbit();
    }

    pub fn adjust_speed(&mut self, notches: i32) {
        self.speed_m_s = (self.speed_m_s * 1.25_f64.powi(notches)).clamp(MIN_SPEED_M_S, MAX_SPEED_M_S);
    }

    /// Keep the current look direction. Place the camera so that direction hits `origin`.
    pub fn focus_origin(&mut self, origin: Vec3, radius_m: f64) -> bool {
        if !origin.x.is_finite() || radius_m <= 0.0 || !self.vertical_fov_radians.is_finite() {
            return false;
        }
        let half = (self.vertical_fov_radians * 0.5).tan();
        if half.abs() < 1.0e-6 {
            return false;
        }
        let distance = (radius_m / half * 1.25).max(MIN_ORBIT_M);
        let forward = self.forward();
        self.orbit_pivot = origin;
        self.orbit_distance = distance;
        self.position = sub(origin, forward.scale(distance));
        self.focus_actions = self.focus_actions.saturating_add(1);
        true
    }

    pub fn reset_to(&mut self, pose: ResolvedPose) {
        let speed = self.speed_m_s;
        let fov = self.vertical_fov_radians;
        let near = self.near_m;
        let frame = self.reference_frame;
        let updates = self.updates;
        let focus_actions = self.focus_actions;
        *self = Self::from_pose(frame, pose, fov, near);
        self.speed_m_s = speed;
        self.updates = updates;
        self.focus_actions = focus_actions;
    }
}

impl CameraNavMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fly => "fly",
            Self::Orbit => "orbit",
            Self::Pan => "pan",
        }
    }
}

pub fn navigation_allowed(viewport_focused: bool, text_focused: bool, viewport_visible: bool) -> bool {
    viewport_focused && !text_focused && viewport_visible
}

fn sub(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x - right.x, left.y - right.y, left.z - right.z)
}

fn vector_length(value: Vec3) -> f64 {
    (value.x * value.x + value.y * value.y + value.z * value.z).sqrt()
}

fn normalize_quat(value: Quat) -> Quat {
    let length = (value.x * value.x + value.y * value.y + value.z * value.z + value.w * value.w).sqrt();
    if length < 1.0e-8 {
        Quat::IDENTITY
    } else {
        Quat { x: value.x / length, y: value.y / length, z: value.z / length, w: value.w / length }
    }
}

fn yaw_pitch_from_quaternion(rotation: Quat) -> (f64, f64) {
    let forward = rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
    let pitch = forward.y.clamp(-1.0, 1.0).asin();
    let yaw = forward.x.atan2(-forward.z);
    (yaw, pitch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> EditorCameraController {
        EditorCameraController::from_pose(
            jarvig_core::FrameId(1),
            ResolvedPose { translation: Vec3::new(0.0, 1.0, 0.0), rotation: Quat::IDENTITY },
            60.0_f64.to_radians(),
            0.1,
        )
    }

    fn length(value: Vec3) -> f64 {
        vector_length(value)
    }

    #[test]
    fn default_forward_is_negative_z_and_pitch_clamps_without_roll() {
        let mut camera = camera();
        let forward = camera.forward();
        assert!((forward.z + 1.0).abs() < 1.0e-6);
        assert!(forward.x.abs() < 1.0e-6 && forward.y.abs() < 1.0e-6);
        camera.look(-1000.0, -100_000.0);
        assert!((camera.pitch - PITCH_LIMIT_RADIANS).abs() < 1.0e-9);
        camera.look(0.0, 100_000.0);
        assert!((camera.pitch + PITCH_LIMIT_RADIANS).abs() < 1.0e-9);
        let right = camera.right();
        assert!(right.y.abs() < 1.0e-6, "ordinary navigation has no roll");
        let rotation = camera.orientation();
        let qlen = (rotation.x * rotation.x + rotation.y * rotation.y + rotation.z * rotation.z + rotation.w * rotation.w).sqrt();
        assert!((qlen - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn yaw_turns_about_up_and_movement_uses_delta_time() {
        let mut camera = camera();
        camera.look(90.0_f64.to_radians() / camera.radians_per_pixel, 0.0);
        let forward = camera.forward();
        assert!(forward.x > 0.9, "mouse right looks toward +X, got {forward:?}");
        assert!(forward.y.abs() < 1.0e-3);
        let before = camera.position;
        camera.fly(1, 0, 0, 1.0, 0.5);
        let step = sub(camera.position, before);
        assert!((length(step) - 2.5).abs() < 1.0e-4);
        let before = camera.position;
        camera.fly(0, 1, 0, 1.0, 0.2);
        assert!((camera.position.y - before.y).abs() < 1.0e-6);
        let before = camera.position;
        camera.fly(0, 0, 1, 4.0, 0.1);
        assert!((camera.position.y - (before.y + 2.0)).abs() < 1.0e-4);
        let slow = camera.speed_m_s;
        camera.adjust_speed(1);
        assert!(camera.speed_m_s > slow);
        camera.speed_m_s = MAX_SPEED_M_S;
        camera.adjust_speed(10);
        assert_eq!(camera.speed_m_s, MAX_SPEED_M_S);
        camera.speed_m_s = MIN_SPEED_M_S;
        camera.adjust_speed(-10);
        assert_eq!(camera.speed_m_s, MIN_SPEED_M_S);
    }

    #[test]
    fn orbit_keeps_distance_pan_moves_the_pivot_and_focus_frames_a_radius() {
        let mut camera = camera();
        camera.orbit_pivot = Vec3::new(0.0, 1.0, -5.0);
        camera.orbit_distance = 5.0;
        camera.reorbit();
        let before = length(sub(camera.position, camera.orbit_pivot));
        camera.look(-20.0, 10.0);
        camera.reorbit();
        let after = length(sub(camera.position, camera.orbit_pivot));
        assert!((before - after).abs() < 1.0e-4);
        assert!((after - 5.0).abs() < 1.0e-4);
        let pivot = camera.orbit_pivot;
        let position = camera.position;
        camera.pan(10.0, 0.0);
        assert!(length(sub(camera.orbit_pivot, pivot)) > 0.0);
        assert!((length(sub(camera.position, position)) - length(sub(camera.orbit_pivot, pivot))).abs() < 1.0e-6);
        camera.dolly(3);
        assert!(camera.orbit_distance < 5.0);
        assert!(camera.orbit_distance >= MIN_ORBIT_M);
        camera.dolly(80);
        assert!(camera.orbit_distance.is_finite() && camera.position.x.is_finite());
        assert!(camera.orbit_distance >= MIN_ORBIT_M);
        let origin = Vec3::new(1.0e9, 2.0, -4.0);
        assert!(camera.focus_origin(origin, 1.0));
        let distance = length(sub(camera.position, origin));
        assert!(distance > 1.0);
        assert!(length(sub(camera.orbit_pivot, origin)) < 1.0e-6);
        camera.position = origin;
        camera.fly(1, 0, 0, 1.0, 0.01 / camera.speed_m_s);
        assert!((length(sub(camera.position, origin)) - 0.01).abs() < 1.0e-6);
    }

    #[test]
    fn text_focus_blocks_navigation_and_reset_is_deterministic() {
        assert!(navigation_allowed(true, false, true));
        assert!(!navigation_allowed(true, true, true));
        assert!(!navigation_allowed(false, false, true));
        assert!(!navigation_allowed(true, false, false));
        let mut camera = camera();
        let pose = camera.pose();
        camera.fly(1, 0, 0, 1.0, 1.0);
        camera.speed_m_s = 12.0;
        camera.reset_to(pose);
        assert_eq!(camera.position, pose.translation);
        assert_eq!(camera.speed_m_s, 12.0);
        let mut input = ViewportNavInput { forward: 1, ..ViewportNavInput::default() };
        let moved = camera.position;
        if navigation_allowed(true, true, true) {
            camera.tick(&mut input, 1.0);
        }
        assert_eq!(camera.position, moved);
    }

    #[test]
    fn stalled_frame_is_clamped_and_shift_does_not_change_base_speed() {
        let mut camera = camera();
        let mut input = ViewportNavInput { forward: 1, boost: true, ..ViewportNavInput::default() };
        camera.tick(&mut input, 10.0);
        let step = length(sub(camera.position, Vec3::new(0.0, 1.0, 0.0)));
        assert!((step - camera.speed_m_s * camera.boost_multiplier * MAX_DELTA_SECONDS).abs() < 1.0e-4);
        assert_eq!(camera.speed_m_s, 5.0);
    }
}
