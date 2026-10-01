//! Joint metadata on an entity that already has a local frame.
//!
//! The pose is that frame. This module does not store a second translation or
//! rotation, and it does not step physics. Stiffness and damping are stored for
//! a later solver.
//!
//! `relative = inverse(rest_rotation) * current_rotation`.
//! Hinge keeps one twist about `axis`. Ball is `swing * twist` with twist applied
//! first. Universal is `rotation(primary) * rotation(secondary)`.

use crate::{AuthoringError, FieldId, PropertyValue, Quat, Vec3, FIELD_JOINT_AXIS, FIELD_JOINT_DAMPING, FIELD_JOINT_HINGE_MAX, FIELD_JOINT_HINGE_MIN, FIELD_JOINT_KIND, FIELD_JOINT_LINEAR_MAX, FIELD_JOINT_LINEAR_MIN, FIELD_JOINT_PRIMARY_MAX, FIELD_JOINT_PRIMARY_MIN, FIELD_JOINT_REST_ROTATION, FIELD_JOINT_REST_TRANSLATION, FIELD_JOINT_SECONDARY_AXIS, FIELD_JOINT_SECONDARY_MAX, FIELD_JOINT_SECONDARY_MIN, FIELD_JOINT_STIFFNESS, FIELD_JOINT_SWING, FIELD_JOINT_TWIST_MAX, FIELD_JOINT_TWIST_MIN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointKind {
    Fixed,
    Hinge,
    Ball,
    Universal,
    Prismatic,
}

impl JointKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed",
            Self::Hinge => "Hinge",
            Self::Ball => "Ball",
            Self::Universal => "Universal",
            Self::Prismatic => "Prismatic",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "Fixed" => Self::Fixed,
            "Hinge" => Self::Hinge,
            "Ball" => Self::Ball,
            "Universal" => Self::Universal,
            "Prismatic" => Self::Prismatic,
            _ => return None,
        })
    }
}

/// Limits in the rest frame. Angles are radians. The prismatic slide is meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointLimits {
    pub axis: Vec3,
    pub secondary_axis: Vec3,
    pub hinge_min: f64,
    pub hinge_max: f64,
    pub swing: f64,
    pub twist_min: f64,
    pub twist_max: f64,
    pub primary_min: f64,
    pub primary_max: f64,
    pub secondary_min: f64,
    pub secondary_max: f64,
    pub linear_min: f64,
    pub linear_max: f64,
}

impl JointLimits {
    pub fn unlocked(axis: Vec3, secondary_axis: Vec3) -> Self {
        let pi = std::f64::consts::PI;
        Self {
            axis,
            secondary_axis,
            hinge_min: -pi,
            hinge_max: pi,
            swing: pi,
            twist_min: -pi,
            twist_max: pi,
            primary_min: -pi,
            primary_max: pi,
            secondary_min: -pi,
            secondary_max: pi,
            linear_min: -1.0,
            linear_max: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointRecord {
    pub kind: JointKind,
    pub rest_translation: Vec3,
    pub rest_rotation: Quat,
    pub limits: JointLimits,
    pub stiffness: f64,
    pub damping: f64,
}

impl JointRecord {
    /// Locked to the pose it is given. Limits stay wide so a later kind change can move.
    pub fn fixed_at(translation: Vec3, rotation: Quat) -> Self {
        Self {
            kind: JointKind::Fixed,
            rest_translation: translation,
            rest_rotation: rotation,
            limits: JointLimits::unlocked(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
            stiffness: 0.0,
            damping: 0.0,
        }
    }

    pub fn validate(self) -> Result<Self, AuthoringError> {
        if !finite_vec(self.rest_translation) || !finite_quat(self.rest_rotation) {
            return Err(AuthoringError::InvalidValue);
        }
        if !self.stiffness.is_finite() || self.stiffness < 0.0 || !self.damping.is_finite() || self.damping < 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        let limits = &self.limits;
        let numbers = [
            limits.hinge_min, limits.hinge_max, limits.swing, limits.twist_min, limits.twist_max, limits.primary_min, limits.primary_max,
            limits.secondary_min, limits.secondary_max, limits.linear_min, limits.linear_max,
        ];
        if numbers.iter().any(|value| !value.is_finite()) || !finite_vec(limits.axis) || !finite_vec(limits.secondary_axis) {
            return Err(AuthoringError::InvalidValue);
        }
        if limits.hinge_min > limits.hinge_max || limits.twist_min > limits.twist_max || limits.primary_min > limits.primary_max || limits.secondary_min > limits.secondary_max || limits.linear_min > limits.linear_max || limits.swing < 0.0 {
            return Err(AuthoringError::InvalidValue);
        }
        if self.kind != JointKind::Fixed && length(limits.axis) < 1.0e-8 {
            return Err(AuthoringError::InvalidValue);
        }
        if self.kind == JointKind::Universal && length(limits.secondary_axis) < 1.0e-8 {
            return Err(AuthoringError::InvalidValue);
        }
        Ok(self)
    }
}

/// One overlay segment in the joint's local frame, before the world pose is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointDebugLine {
    pub from: Vec3,
    pub to: Vec3,
    pub color: [f32; 4],
    pub limits: bool,
}

/// Clamp a parent-relative pose. The same inputs produce the same numbers.
pub fn clamp_joint_pose(joint: &JointRecord, translation: Vec3, rotation: Quat) -> (Vec3, Quat) {
    let rest_rotation = unit_or_identity(joint.rest_rotation);
    let rotation = unit_or_identity(rotation);
    let relative = rest_rotation.conjugate().mul(rotation);
    let (translation, relative) = match joint.kind {
        JointKind::Fixed => (joint.rest_translation, Quat::IDENTITY),
        JointKind::Hinge => (joint.rest_translation, clamp_hinge(relative, joint.limits)),
        JointKind::Ball => (joint.rest_translation, clamp_ball(relative, joint.limits)),
        JointKind::Universal => (joint.rest_translation, clamp_universal(relative, joint.limits)),
        JointKind::Prismatic => (clamp_slide(translation, joint), Quat::IDENTITY),
    };
    (translation, unit_or_identity(rest_rotation.mul(relative)))
}

pub fn apply_joint_property(joint: &JointRecord, field: FieldId, value: PropertyValue) -> Result<JointRecord, AuthoringError> {
    let mut next = *joint;
    match (field, value) {
        (FIELD_JOINT_KIND, PropertyValue::String(kind)) => {
            next.kind = JointKind::parse(&kind).ok_or(AuthoringError::InvalidValue)?;
        }
        (FIELD_JOINT_REST_TRANSLATION, PropertyValue::Vec3(translation)) => next.rest_translation = translation,
        (FIELD_JOINT_REST_ROTATION, PropertyValue::Quat(rotation)) => next.rest_rotation = unit_or_identity(rotation),
        (FIELD_JOINT_AXIS, PropertyValue::Vec3(axis)) => next.limits.axis = normalize(axis).unwrap_or(axis),
        (FIELD_JOINT_SECONDARY_AXIS, PropertyValue::Vec3(axis)) => next.limits.secondary_axis = normalize(axis).unwrap_or(axis),
        (FIELD_JOINT_HINGE_MIN, PropertyValue::F64(degrees)) => next.limits.hinge_min = degrees.to_radians(),
        (FIELD_JOINT_HINGE_MAX, PropertyValue::F64(degrees)) => next.limits.hinge_max = degrees.to_radians(),
        (FIELD_JOINT_SWING, PropertyValue::F64(degrees)) => next.limits.swing = degrees.to_radians(),
        (FIELD_JOINT_TWIST_MIN, PropertyValue::F64(degrees)) => next.limits.twist_min = degrees.to_radians(),
        (FIELD_JOINT_TWIST_MAX, PropertyValue::F64(degrees)) => next.limits.twist_max = degrees.to_radians(),
        (FIELD_JOINT_PRIMARY_MIN, PropertyValue::F64(degrees)) => next.limits.primary_min = degrees.to_radians(),
        (FIELD_JOINT_PRIMARY_MAX, PropertyValue::F64(degrees)) => next.limits.primary_max = degrees.to_radians(),
        (FIELD_JOINT_SECONDARY_MIN, PropertyValue::F64(degrees)) => next.limits.secondary_min = degrees.to_radians(),
        (FIELD_JOINT_SECONDARY_MAX, PropertyValue::F64(degrees)) => next.limits.secondary_max = degrees.to_radians(),
        (FIELD_JOINT_LINEAR_MIN, PropertyValue::F64(meters)) => next.limits.linear_min = meters,
        (FIELD_JOINT_LINEAR_MAX, PropertyValue::F64(meters)) => next.limits.linear_max = meters,
        (FIELD_JOINT_STIFFNESS, PropertyValue::F64(stiffness)) => next.stiffness = stiffness,
        (FIELD_JOINT_DAMPING, PropertyValue::F64(damping)) => next.damping = damping,
        _ => return Err(AuthoringError::WrongType),
    }
    next.validate()
}

/// Half-length of the joint cross, meters.
const JOINT_MARKER_HALF_M: f64 = 0.012;
/// Length of the joint axis tick, meters.
const JOINT_MARKER_AXIS_M: f64 = 0.04;
/// Radius of the selected joint's limit mark, meters.
const JOINT_LIMIT_RADIUS_M: f64 = 0.04;

pub fn joint_debug_lines(joint: &JointRecord, include_limits: bool) -> Vec<JointDebugLine> {
    let axis = normalize(joint.limits.axis).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let half = JOINT_MARKER_HALF_M;
    let mut lines = vec![
        line(Vec3::new(-half, 0.0, 0.0), Vec3::new(half, 0.0, 0.0), [1.0, 1.0, 1.0, 1.0], false),
        line(Vec3::new(0.0, -half, 0.0), Vec3::new(0.0, half, 0.0), [1.0, 1.0, 1.0, 1.0], false),
        line(Vec3::new(0.0, 0.0, -half), Vec3::new(0.0, 0.0, half), [1.0, 1.0, 1.0, 1.0], false),
        line(Vec3::ZERO, axis.scale(JOINT_MARKER_AXIS_M), [0.25, 0.85, 1.0, 1.0], false),
    ];
    if !include_limits {
        return lines;
    }
    match joint.kind {
        JointKind::Hinge => arc(&mut lines, axis, joint.limits.hinge_min, joint.limits.hinge_max, JOINT_LIMIT_RADIUS_M, [1.0, 0.75, 0.2, 1.0]),
        JointKind::Ball => {
            cone(&mut lines, axis, joint.limits.swing, JOINT_LIMIT_RADIUS_M, [0.35, 0.9, 0.45, 1.0]);
            arc(&mut lines, axis, joint.limits.twist_min, joint.limits.twist_max, JOINT_LIMIT_RADIUS_M * 0.7, [0.9, 0.35, 0.8, 1.0]);
        }
        JointKind::Universal => {
            let secondary = orthonormal_secondary(axis, joint.limits.secondary_axis);
            arc(&mut lines, axis, joint.limits.primary_min, joint.limits.primary_max, JOINT_LIMIT_RADIUS_M, [1.0, 0.75, 0.2, 1.0]);
            arc(&mut lines, secondary, joint.limits.secondary_min, joint.limits.secondary_max, JOINT_LIMIT_RADIUS_M * 0.8, [0.9, 0.35, 0.8, 1.0]);
        }
        JointKind::Prismatic => {
            lines.push(line(axis.scale(joint.limits.linear_min), axis.scale(joint.limits.linear_max), [0.95, 0.85, 0.2, 1.0], true));
        }
        JointKind::Fixed => {}
    }
    lines
}

fn clamp_hinge(relative: Quat, limits: JointLimits) -> Quat {
    let axis = normalize(limits.axis).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let (_swing, twist) = swing_twist(relative, axis);
    let angle = signed_angle(twist, axis).clamp(limits.hinge_min, limits.hinge_max);
    axis_angle(axis, angle)
}

fn clamp_ball(relative: Quat, limits: JointLimits) -> Quat {
    let axis = normalize(limits.axis).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let (swing, twist) = swing_twist(relative, axis);
    let (swing_axis, swing_angle) = axis_and_angle(swing);
    let swing = if swing_angle > limits.swing { axis_angle(swing_axis, limits.swing) } else { swing };
    let angle = signed_angle(twist, axis).clamp(limits.twist_min, limits.twist_max);
    unit_or_identity(swing.mul(axis_angle(axis, angle)))
}

fn clamp_universal(relative: Quat, limits: JointLimits) -> Quat {
    let primary = normalize(limits.axis).unwrap_or(Vec3::new(1.0, 0.0, 0.0));
    let secondary = orthonormal_secondary(primary, limits.secondary_axis);
    let bone = normalize(primary.cross(secondary)).unwrap_or(Vec3::new(0.0, 0.0, 1.0));
    let direction = relative.rotate(bone);
    let along_primary = dot(direction, primary).clamp(-1.0, 1.0);
    let secondary_angle = along_primary.asin();
    let cos_secondary = (1.0 - along_primary * along_primary).sqrt();
    let primary_angle = if cos_secondary < 1.0e-6 {
        0.0
    } else {
        (-dot(direction, secondary)).atan2(dot(direction, bone))
    };
    let primary_angle = primary_angle.clamp(limits.primary_min, limits.primary_max);
    let secondary_angle = secondary_angle.clamp(limits.secondary_min, limits.secondary_max);
    unit_or_identity(axis_angle(primary, primary_angle).mul(axis_angle(secondary, secondary_angle)))
}

fn clamp_slide(translation: Vec3, joint: &JointRecord) -> Vec3 {
    let axis = normalize(joint.limits.axis).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let delta = sub(translation, joint.rest_translation);
    let distance = dot(delta, axis).clamp(joint.limits.linear_min, joint.limits.linear_max);
    add(joint.rest_translation, axis.scale(distance))
}

/// `relative = swing * twist`, twist about `axis`.
fn swing_twist(relative: Quat, axis: Vec3) -> (Quat, Quat) {
    let projection = dot(Vec3::new(relative.x, relative.y, relative.z), axis);
    let twist = Quat { x: axis.x * projection, y: axis.y * projection, z: axis.z * projection, w: relative.w };
    let scale = (twist.x * twist.x + twist.y * twist.y + twist.z * twist.z + twist.w * twist.w).sqrt();
    if scale < 1.0e-12 {
        return (relative, Quat::IDENTITY);
    }
    let twist = Quat { x: twist.x / scale, y: twist.y / scale, z: twist.z / scale, w: twist.w / scale };
    (unit_or_identity(relative.mul(twist.conjugate())), twist)
}

fn signed_angle(rotation: Quat, axis: Vec3) -> f64 {
    let along = dot(Vec3::new(rotation.x, rotation.y, rotation.z), axis);
    2.0 * along.atan2(rotation.w)
}

fn axis_and_angle(rotation: Quat) -> (Vec3, f64) {
    let rotation = if rotation.w < 0.0 {
        Quat { x: -rotation.x, y: -rotation.y, z: -rotation.z, w: -rotation.w }
    } else {
        rotation
    };
    let angle = 2.0 * rotation.w.clamp(-1.0, 1.0).acos();
    let sine = (1.0 - rotation.w * rotation.w).sqrt();
    if sine < 1.0e-8 {
        (Vec3::new(0.0, 1.0, 0.0), 0.0)
    } else {
        (Vec3::new(rotation.x / sine, rotation.y / sine, rotation.z / sine), angle)
    }
}

fn axis_angle(axis: Vec3, radians: f64) -> Quat {
    Quat::from_axis_angle(axis, radians).unwrap_or(Quat::IDENTITY)
}

fn orthonormal_secondary(primary: Vec3, secondary: Vec3) -> Vec3 {
    let rejected = sub(secondary, primary.scale(dot(secondary, primary)));
    normalize(rejected).unwrap_or_else(|| {
        let helper = if primary.y.abs() < 0.9 { Vec3::new(0.0, 1.0, 0.0) } else { Vec3::new(1.0, 0.0, 0.0) };
        normalize(primary.cross(helper)).unwrap_or(Vec3::new(1.0, 0.0, 0.0))
    })
}

fn arc(lines: &mut Vec<JointDebugLine>, axis: Vec3, min_rad: f64, max_rad: f64, radius: f64, color: [f32; 4]) {
    let (u, _v) = basis(axis);
    let steps = 12;
    for step in 0..steps {
        let t0 = min_rad + (max_rad - min_rad) * (step as f64 / steps as f64);
        let t1 = min_rad + (max_rad - min_rad) * ((step + 1) as f64 / steps as f64);
        let a = axis_angle(axis, t0).rotate(u).scale(radius);
        let b = axis_angle(axis, t1).rotate(u).scale(radius);
        lines.push(line(a, b, color, true));
    }
}

fn cone(lines: &mut Vec<JointDebugLine>, axis: Vec3, swing: f64, radius: f64, color: [f32; 4]) {
    let (u, v) = basis(axis);
    let steps = 16;
    for step in 0..steps {
        let t0 = std::f64::consts::TAU * (step as f64 / steps as f64);
        let t1 = std::f64::consts::TAU * ((step + 1) as f64 / steps as f64);
        let a = add(axis.scale(swing.cos()), add(u.scale(t0.cos()), v.scale(t0.sin())).scale(swing.sin())).scale(radius);
        let b = add(axis.scale(swing.cos()), add(u.scale(t1.cos()), v.scale(t1.sin())).scale(swing.sin())).scale(radius);
        lines.push(line(a, b, color, true));
    }
}

fn basis(axis: Vec3) -> (Vec3, Vec3) {
    let helper = if axis.y.abs() < 0.9 { Vec3::new(0.0, 1.0, 0.0) } else { Vec3::new(1.0, 0.0, 0.0) };
    let u = normalize(axis.cross(helper)).unwrap_or(Vec3::new(1.0, 0.0, 0.0));
    let v = normalize(axis.cross(u)).unwrap_or(Vec3::new(0.0, 0.0, 1.0));
    (u, v)
}

fn line(from: Vec3, to: Vec3, color: [f32; 4], limits: bool) -> JointDebugLine {
    JointDebugLine { from, to, color, limits }
}

fn unit_or_identity(rotation: Quat) -> Quat {
    let scale = (rotation.x * rotation.x + rotation.y * rotation.y + rotation.z * rotation.z + rotation.w * rotation.w).sqrt();
    if !scale.is_finite() || scale < 1.0e-12 {
        Quat::IDENTITY
    } else {
        Quat { x: rotation.x / scale, y: rotation.y / scale, z: rotation.z / scale, w: rotation.w / scale }
    }
}

fn finite_vec(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

fn finite_quat(value: Quat) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite() && value.w.is_finite()
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn length(value: Vec3) -> f64 {
    dot(value, value).sqrt()
}

fn normalize(value: Vec3) -> Option<Vec3> {
    let len = length(value);
    if !len.is_finite() || len < 1.0e-12 { None } else { Some(value.scale(1.0 / len)) }
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hinge(min_deg: f64, max_deg: f64) -> JointRecord {
        let mut joint = JointRecord::fixed_at(Vec3::ZERO, Quat::IDENTITY);
        joint.kind = JointKind::Hinge;
        joint.limits.axis = Vec3::new(0.0, 0.0, 1.0);
        joint.limits.hinge_min = min_deg.to_radians();
        joint.limits.hinge_max = max_deg.to_radians();
        joint.validate().unwrap()
    }

    fn angle_about_z(rotation: Quat) -> f64 {
        signed_angle(rotation, Vec3::new(0.0, 0.0, 1.0)).to_degrees()
    }

    #[test]
    fn the_joint_marker_is_a_small_cross_and_a_short_axis() {
        let joint = JointRecord::fixed_at(Vec3::ZERO, Quat::IDENTITY);
        let lines = joint_debug_lines(&joint, false);
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|line| !line.limits));
        assert!((lines[0].from.x + JOINT_MARKER_HALF_M).abs() < 1.0e-9);
        assert!((lines[0].to.x - JOINT_MARKER_HALF_M).abs() < 1.0e-9);
        assert!((lines[3].to.y - JOINT_MARKER_AXIS_M).abs() < 1.0e-9);
        assert!(JOINT_MARKER_HALF_M < 0.02 && JOINT_MARKER_AXIS_M < 0.06);
    }

    #[test]
    fn hinge_clamp_is_deterministic_and_drops_swing() {
        let joint = hinge(-10.0, 140.0);
        let past = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), 200.0_f64.to_radians()).unwrap();
        let (translation, rotation) = clamp_joint_pose(&joint, Vec3::new(1.0, 2.0, 3.0), past);
        let again = clamp_joint_pose(&joint, translation, rotation);
        assert_eq!(translation, Vec3::ZERO);
        assert_eq!((translation, rotation), again);
        assert!((angle_about_z(rotation) - 140.0).abs() < 1.0e-6);
        let swung = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 0.4).unwrap().mul(Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), 0.5).unwrap());
        let (_kept, rotation) = clamp_joint_pose(&joint, Vec3::ZERO, swung);
        let rebuilt = clamp_joint_pose(&joint, Vec3::ZERO, rotation);
        assert_eq!(rotation.x, rebuilt.1.x);
        assert_eq!(rotation.y, rebuilt.1.y);
        assert_eq!(rotation.z, rebuilt.1.z);
        assert_eq!(rotation.w, rebuilt.1.w);
        assert!(rotation.x.abs() < 1.0e-6 && rotation.y.abs() < 1.0e-6);
    }

    #[test]
    fn ball_cone_and_twist_clamp_and_repeat() {
        let mut joint = JointRecord::fixed_at(Vec3::ZERO, Quat::IDENTITY);
        joint.kind = JointKind::Ball;
        joint.limits.axis = Vec3::new(0.0, 1.0, 0.0);
        joint.limits.swing = 30.0_f64.to_radians();
        joint.limits.twist_min = -10.0_f64.to_radians();
        joint.limits.twist_max = 10.0_f64.to_radians();
        let swing = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), -90.0_f64.to_radians()).unwrap();
        let twist = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 40.0_f64.to_radians()).unwrap();
        let (_translation, rotation) = clamp_joint_pose(&joint, Vec3::ZERO, swing.mul(twist));
        let again = clamp_joint_pose(&joint, Vec3::ZERO, rotation);
        assert_eq!(rotation, again.1);
        let moved = rotation.rotate(Vec3::new(0.0, 1.0, 0.0));
        let cone = moved.y.clamp(-1.0, 1.0).acos().to_degrees();
        assert!((cone - 30.0).abs() < 1.0e-4, "{cone}");
        let (_swing, twist) = swing_twist(rotation, Vec3::new(0.0, 1.0, 0.0));
        assert!((signed_angle(twist, Vec3::new(0.0, 1.0, 0.0)).to_degrees() - 10.0).abs() < 1.0e-4);
    }

    #[test]
    fn fixed_locks_rest_and_prismatic_slides_on_its_axis() {
        let rest = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.2).unwrap();
        let mut joint = JointRecord::fixed_at(Vec3::new(0.0, 1.0, 0.0), rest);
        let (translation, rotation) = clamp_joint_pose(&joint, Vec3::new(4.0, 5.0, 6.0), Quat::IDENTITY);
        assert_eq!(translation, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(rotation, rest);
        joint.kind = JointKind::Prismatic;
        joint.limits.axis = Vec3::new(0.0, 1.0, 0.0);
        joint.limits.linear_min = 0.0;
        joint.limits.linear_max = 0.25;
        let (translation, rotation) = clamp_joint_pose(&joint, Vec3::new(3.0, 9.0, -2.0), Quat::IDENTITY);
        let again = clamp_joint_pose(&joint, translation, rotation);
        assert_eq!((translation, rotation), again);
        assert!((translation.x).abs() < 1.0e-12 && (translation.z).abs() < 1.0e-12);
        assert!((translation.y - 1.25).abs() < 1.0e-12);
        assert_eq!(rotation, rest);
    }

    #[test]
    fn universal_drops_the_third_angle() {
        let mut joint = JointRecord::fixed_at(Vec3::ZERO, Quat::IDENTITY);
        joint.kind = JointKind::Universal;
        joint.limits.axis = Vec3::new(1.0, 0.0, 0.0);
        joint.limits.secondary_axis = Vec3::new(0.0, 1.0, 0.0);
        joint.limits.primary_min = -0.3;
        joint.limits.primary_max = 0.3;
        joint.limits.secondary_min = -0.2;
        joint.limits.secondary_max = 0.2;
        let spun = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), 1.2).unwrap();
        let (_translation, rotation) = clamp_joint_pose(&joint, Vec3::new(8.0, 0.0, 0.0), spun);
        let again = clamp_joint_pose(&joint, Vec3::ZERO, rotation);
        assert_eq!(rotation, again.1);
        let bone = rotation.rotate(Vec3::new(0.0, 0.0, 1.0));
        assert!(bone.x.abs() <= 0.2 + 1.0e-6);
        assert!(bone.y.abs() <= 0.3 + 1.0e-6);
    }
}
