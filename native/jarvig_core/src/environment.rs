//! One scene environment. Not a direct light, not a material, and not a view setting.
//!
//! The bootstrap source is two uniform hemispheres split by world +Y. Colors are
//! linear radiance. There is no world position, so the packet stays valid at the
//! billion-meter root and does not change when the camera moves.

use crate::{finite_color, finite_intensity, Quat, SpaceError, Vec3};

/// Cool sky. Linear radiance, not an sRGB byte.
pub const BOOTSTRAP_UPPER_HEMISPHERE_LINEAR: [f32; 3] = [0.55, 0.68, 0.86];
/// Darker, slightly warm ground. Linear radiance.
pub const BOOTSTRAP_LOWER_HEMISPHERE_LINEAR: [f32; 3] = [0.22, 0.16, 0.11];
/// Low enough that a 14 cd point light stays dominant on a lit dielectric.
pub const BOOTSTRAP_ENVIRONMENT_INTENSITY: f32 = 0.20;

/// The only environment light. At most one is active. Orientation is world +Y
/// until a later ticket needs a rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvironmentLight {
    pub upper_hemisphere_linear_rgb: [f32; 3],
    pub lower_hemisphere_linear_rgb: [f32; 3],
    pub intensity: f32,
    pub enabled: bool,
}

impl EnvironmentLight {
    pub fn bootstrap() -> Self {
        Self {
            upper_hemisphere_linear_rgb: BOOTSTRAP_UPPER_HEMISPHERE_LINEAR,
            lower_hemisphere_linear_rgb: BOOTSTRAP_LOWER_HEMISPHERE_LINEAR,
            intensity: BOOTSTRAP_ENVIRONMENT_INTENSITY,
            enabled: true,
        }
    }

    pub fn validate(self) -> Result<Self, SpaceError> {
        Ok(Self {
            upper_hemisphere_linear_rgb: finite_color(self.upper_hemisphere_linear_rgb)?,
            lower_hemisphere_linear_rgb: finite_color(self.lower_hemisphere_linear_rgb)?,
            intensity: finite_intensity(self.intensity)?,
            enabled: self.enabled,
        })
    }
}

/// GPU uniform for one environment. 32 bytes. Two `vec4`s.
///
/// `upper.xyz` is upper radiance and `upper.w` is intensity.
/// `lower.xyz` is lower radiance and `lower.w` is 1 when enabled, else 0.
/// No position and no camera basis. Direct-light records stay a different packet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuEnvironmentPacket {
    pub upper: [f32; 3],
    pub intensity: f32,
    pub lower: [f32; 3],
    pub enabled: f32,
}

impl GpuEnvironmentPacket {
    pub const BYTES: usize = 32;

    pub fn from_light(light: &EnvironmentLight) -> Self {
        Self {
            upper: light.upper_hemisphere_linear_rgb,
            intensity: light.intensity,
            lower: light.lower_hemisphere_linear_rgb,
            enabled: if light.enabled { 1.0 } else { 0.0 },
        }
    }

    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut bytes = [0u8; Self::BYTES];
        write3(&mut bytes[0..12], self.upper);
        bytes[12..16].copy_from_slice(&self.intensity.to_le_bytes());
        write3(&mut bytes[16..28], self.lower);
        bytes[28..32].copy_from_slice(&self.enabled.to_le_bytes());
        bytes
    }
}

/// Object +Z after `rotation`. Unit length. No translation.
pub fn plus_z_normal(rotation: Quat) -> [f32; 3] {
    let turned = rotation.rotate(Vec3::new(0.0, 0.0, 1.0));
    let length = (turned.x * turned.x + turned.y * turned.y + turned.z * turned.z).sqrt();
    if length <= 1.0e-12 {
        [0.0, 0.0, 1.0]
    } else {
        [(turned.x / length) as f32, (turned.y / length) as f32, (turned.z / length) as f32]
    }
}

/// Diffuse environment for a white-or-authored albedo. The equation lives in
/// `jarvig_material`. This wrapper keeps the editor off that crate.
pub fn environment_diffuse_for(
    light: &EnvironmentLight,
    world_normal: [f32; 3],
    base_color: [f32; 3],
    metallic: f32,
    ambient_occlusion: f32,
) -> [f32; 3] {
    let surface = jarvig_material::MaterialSurface {
        base_color,
        metallic,
        ambient_occlusion,
        ..jarvig_material::MaterialSurface::default()
    };
    jarvig_material::environment_diffuse(
        &surface,
        world_normal,
        light.upper_hemisphere_linear_rgb,
        light.lower_hemisphere_linear_rgb,
        light.intensity,
        light.enabled,
    )
}

/// Environment reflection for an authored albedo. Directions only. No world position.
/// AO is intentionally absent: specular occlusion is not modeled yet.
pub fn environment_specular_for(
    light: &EnvironmentLight,
    world_normal: [f32; 3],
    world_view: [f32; 3],
    base_color: [f32; 3],
    metallic: f32,
    roughness: f32,
) -> [f32; 3] {
    let surface = jarvig_material::MaterialSurface {
        base_color,
        metallic,
        perceptual_roughness: roughness,
        ..jarvig_material::MaterialSurface::default()
    };
    jarvig_material::environment_specular(
        &surface,
        world_normal,
        world_view,
        light.upper_hemisphere_linear_rgb,
        light.lower_hemisphere_linear_rgb,
        light.intensity,
        light.enabled,
    )
}

/// Geometric normal after the two-sided visible-side correction.
pub fn visible_side_normal(geometric: [f32; 3], front_facing: bool) -> [f32; 3] {
    jarvig_material::apply_visible_side(geometric, [1.0, 0.0, 0.0, 1.0], front_facing).0
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
    fn the_packet_is_camera_independent_and_has_no_world_position() {
        let light = EnvironmentLight::bootstrap();
        let bytes = GpuEnvironmentPacket::from_light(&light).to_bytes();
        assert_eq!(GpuEnvironmentPacket::BYTES, 32);
        assert_eq!(f32::from_le_bytes(bytes[12..16].try_into().unwrap()), BOOTSTRAP_ENVIRONMENT_INTENSITY);
        assert_eq!(f32::from_le_bytes(bytes[28..32].try_into().unwrap()), 1.0);
        for chunk in bytes.chunks(4) {
            let value = f32::from_le_bytes(chunk.try_into().unwrap());
            assert!(value.is_finite() && value.abs() < 4.0, "{value}");
        }
        let mut off = light;
        off.enabled = false;
        assert_eq!(f32::from_le_bytes(GpuEnvironmentPacket::from_light(&off).to_bytes()[28..32].try_into().unwrap()), 0.0);
        assert!(EnvironmentLight {
            upper_hemisphere_linear_rgb: [-1.0, 0.0, 0.0],
            ..light
        }
        .validate()
        .is_err());
    }

    #[test]
    fn orientation_changes_the_hemisphere_and_metal_and_ao_do_not_light_direct() {
        let light = EnvironmentLight::bootstrap();
        let up = environment_diffuse_for(&light, [0.0, 1.0, 0.0], [1.0, 1.0, 1.0], 0.0, 1.0);
        let flat = environment_diffuse_for(&light, [0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 0.0, 1.0);
        let down = environment_diffuse_for(&light, [0.0, -1.0, 0.0], [1.0, 1.0, 1.0], 0.0, 1.0);
        assert!(up[2] > up[0]);
        assert!(down[0] > down[2]);
        assert!(up[1] > flat[1] && flat[1] > down[1]);
        let metal = environment_diffuse_for(&light, [0.0, 1.0, 0.0], [0.8, 0.8, 0.8], 1.0, 1.0);
        assert!(metal.iter().all(|channel| channel.abs() < 1.0e-6));
        let occluded = environment_diffuse_for(&light, [0.0, 1.0, 0.0], [1.0, 1.0, 1.0], 0.0, 0.0);
        assert_eq!(occluded, [0.0; 3]);
        let mut disabled = light;
        disabled.enabled = false;
        assert_eq!(environment_diffuse_for(&disabled, [0.0, 1.0, 0.0], [1.0, 1.0, 1.0], 0.0, 1.0), [0.0; 3]);
        let normal = plus_z_normal(Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), -std::f64::consts::FRAC_PI_2).unwrap());
        assert!(normal[1] > 0.99);
    }
}
