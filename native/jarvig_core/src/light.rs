//! World lights. Not materials, not meshes, and not GPU buffers.
//!
//! A light emits along its frame's local -Z. Color is linear RGB.
//! Directional intensity is illuminance in lux. Point and spot intensity is
//! luminous intensity in candela. The snapshot keeps the resolved binary64 pose.
//! Camera-relative float32 is a per-view record, not world state.

use crate::{Quat, ResolvedPose, SpaceError, Vec3};

/// Native light identity. Not an object, frame, mesh, material, or RHI id.
/// Not a C ABI handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LightId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    Directional,
    Point,
    Spot,
}

impl LightKind {
    pub fn gpu_tag(self) -> u32 {
        match self {
            Self::Directional => 0,
            Self::Point => 1,
            Self::Spot => 2,
        }
    }
}

/// Half-angles from the emission axis. `inner <= outer`, both in `0..=π/2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotCone {
    pub inner_radians: f32,
    pub outer_radians: f32,
}

impl SpotCone {
    pub fn validate(self) -> Result<(), SpaceError> {
        let max = std::f32::consts::FRAC_PI_2;
        let ok = self.inner_radians.is_finite()
            && self.outer_radians.is_finite()
            && self.inner_radians >= 0.0
            && self.outer_radians <= max
            && self.inner_radians <= self.outer_radians;
        if ok {
            Ok(())
        } else {
            Err(SpaceError::BadLight)
        }
    }

    pub fn cosines(self) -> (f32, f32) {
        (self.inner_radians.cos(), self.outer_radians.cos())
    }
}

/// One enabled light after extraction. High-precision pose. Not a GPU record
/// and not already relative to one camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderLight {
    pub id: LightId,
    pub kind: LightKind,
    pub pose: ResolvedPose,
    pub color_linear: [f32; 3],
    pub intensity: f32,
    /// `0` means no influence cutoff. It does not change candela or lux.
    pub range_m: f32,
    pub cos_inner: f32,
    pub cos_outer: f32,
    /// Authored visibility settings. The GPU shadow record is built from this. It is not this struct.
    pub shadow: crate::LightShadowSettings,
}

/// One light in one view's render space. Internal packet. Not a world object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuLightRecord {
    pub kind: u32,
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub range_m: f32,
    pub cos_inner: f32,
    pub cos_outer: f32,
}

impl GpuLightRecord {
    pub const BYTES: usize = 80;

    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut bytes = [0u8; Self::BYTES];
        bytes[0..4].copy_from_slice(&self.kind.to_le_bytes());
        write3(&mut bytes[16..28], self.position);
        write3(&mut bytes[32..44], self.direction);
        write3(&mut bytes[48..60], self.color);
        bytes[60..64].copy_from_slice(&self.intensity.to_le_bytes());
        bytes[64..68].copy_from_slice(&self.range_m.to_le_bytes());
        bytes[68..72].copy_from_slice(&self.cos_inner.to_le_bytes());
        bytes[72..76].copy_from_slice(&self.cos_outer.to_le_bytes());
        bytes
    }
}

/// Local -Z of `rotation`, in the same space as that rotation.
pub fn emission_forward(rotation: Quat) -> Vec3 {
    rotation.rotate(Vec3::new(0.0, 0.0, -1.0))
}

/// Rotation whose emitted -Z axis points along `forward`.
pub fn rotation_emitting_toward(forward: Vec3) -> Result<Quat, SpaceError> {
    let from = Vec3::new(0.0, 0.0, -1.0);
    let to = unit(forward).map_err(|_| SpaceError::BadLight)?;
    let dot = (from.x * to.x + from.y * to.y + from.z * to.z).clamp(-1.0, 1.0);
    if dot > 1.0 - 1.0e-8 {
        return Ok(Quat::IDENTITY);
    }
    if dot < -1.0 + 1.0e-8 {
        return Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), std::f64::consts::PI);
    }
    let axis = from.cross(to);
    Quat::from_axis_angle(axis, dot.acos())
}

/// View-space light. Directional position is unused. Point and spot positions
/// are this camera's render origin subtracted in f64, then rotated into the
/// same eye space as the fragment.
pub fn render_light_record(light: &RenderLight, camera: &ResolvedPose) -> GpuLightRecord {
    let view = camera.rotation.conjugate();
    let world_forward = emission_forward(light.pose.rotation);
    let turned = view.rotate(world_forward);
    let direction = normalize_f32([turned.x as f32, turned.y as f32, turned.z as f32]);
    let position = if light.kind == LightKind::Directional {
        [0.0; 3]
    } else {
        let relative = Vec3::new(
            light.pose.translation.x - camera.translation.x,
            light.pose.translation.y - camera.translation.y,
            light.pose.translation.z - camera.translation.z,
        );
        let rotated = view.rotate(relative);
        [rotated.x as f32, rotated.y as f32, rotated.z as f32]
    };
    GpuLightRecord {
        kind: light.kind.gpu_tag(),
        position,
        direction,
        color: light.color_linear,
        intensity: light.intensity,
        range_m: light.range_m,
        cos_inner: light.cos_inner,
        cos_outer: light.cos_outer,
    }
}

pub fn finite_color(color: [f32; 3]) -> Result<[f32; 3], SpaceError> {
    if color.iter().all(|channel| channel.is_finite() && *channel >= 0.0) {
        Ok(color)
    } else {
        Err(SpaceError::BadLight)
    }
}

pub fn finite_intensity(intensity: f32) -> Result<f32, SpaceError> {
    if intensity.is_finite() && intensity >= 0.0 {
        Ok(intensity)
    } else {
        Err(SpaceError::BadLight)
    }
}

/// `0` clears the cutoff. A positive finite range is metadata for culling.
pub fn finite_range(range_m: f32) -> Result<f32, SpaceError> {
    if range_m == 0.0 || (range_m.is_finite() && range_m > 0.0) {
        Ok(range_m)
    } else {
        Err(SpaceError::BadLight)
    }
}

fn unit(vector: Vec3) -> Result<Vec3, ()> {
    let length = (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt();
    if length == 0.0 {
        Err(())
    } else {
        Ok(vector.scale(1.0 / length))
    }
}

fn normalize_f32(vector: [f32; 3]) -> [f32; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length <= 1.0e-8 {
        [0.0, 0.0, 1.0]
    } else {
        [vector[0] / length, vector[1] / length, vector[2] / length]
    }
}

fn write3(target: &mut [u8], value: [f32; 3]) {
    for (index, channel) in value.iter().enumerate() {
        let start = index * 4;
        target[start..start + 4].copy_from_slice(&channel.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_emits_along_negative_z_and_a_bad_cone_is_rejected() {
        let forward = emission_forward(Quat::IDENTITY);
        assert!((forward.x).abs() < 1.0e-9);
        assert!((forward.y).abs() < 1.0e-9);
        assert!((forward.z + 1.0).abs() < 1.0e-9);
        let aimed = rotation_emitting_toward(Vec3::new(0.0, 0.0, -4.0)).unwrap();
        let again = emission_forward(aimed);
        assert!(again.z < -0.9);
        assert!(SpotCone { inner_radians: 0.2, outer_radians: 0.5 }.validate().is_ok());
        assert!(SpotCone { inner_radians: 0.5, outer_radians: 0.2 }.validate().is_err());
        assert!(SpotCone { inner_radians: -0.1, outer_radians: 0.2 }.validate().is_err());
        assert!(SpotCone { inner_radians: 0.0, outer_radians: 2.0 }.validate().is_err());
    }
}
