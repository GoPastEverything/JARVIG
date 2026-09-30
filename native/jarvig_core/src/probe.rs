//! One local reflection probe. World lighting data, not a material, mesh, view, or light kind.
//!
//! The authoritative position is a reference-frame pose in binary64. The GPU upload
//! subtracts the current view origin in f64 and only then becomes float32.
//! A probe does not choose itself from the editor camera. The shaded position does.
//!
//! Capture is a renderer concern. This module only names the probe, its influence,
//! and which one sample is active. The cubemap is not stored here.

use crate::{camera_relative_f32, finite_intensity, Quat, SpaceError, Vec3};

/// Cool-side placement under the bootstrap scene frame. The far card at local z `-5`
/// is inside. A card moved to local z `-30` is outside.
pub const BOOTSTRAP_PROBE_LOCAL_M: (f64, f64, f64) = (0.0, 0.2, -3.5);
pub const BOOTSTRAP_PROBE_RADIUS_M: f64 = 8.0;
pub const BOOTSTRAP_PROBE_INTENSITY: f32 = 1.0;
pub const BOOTSTRAP_PROBE_PRIORITY: i32 = 0;

/// Face size of the bootstrap and `--self-test` capture. Not the interactive budget.
/// An integrated GPU's authored default is 64. A discrete GPU's is 128. The level file wins.
pub const REFLECTION_PROBE_RESOLUTION: u32 = 32;

/// Supported capture faces. Anything else is rejected so a typo cannot allocate an accidental cube.
pub fn reflection_probe_resolution_supported(resolution: u32) -> bool {
    matches!(resolution, 32 | 64 | 128 | 256)
}

/// `log2(resolution) + 1`. Mip 0 is the sharp capture.
pub fn reflection_probe_mip_count_for(resolution: u32) -> u32 {
    if !reflection_probe_resolution_supported(resolution) {
        return REFLECTION_PROBE_MIP_COUNT;
    }
    resolution.ilog2() + 1
}
/// `log2(resolution) + 1`. Mip 0 is the sharp capture. Higher mips are the rough end.
pub const REFLECTION_PROBE_MIP_COUNT: u32 = 6;

/// Logical probe. Not an [`crate::EntityUuid`] and not a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProbeId(pub u64);

/// One sphere of influence. Position is the resolved frame origin, not a GPU vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderReflectionProbe {
    pub id: ProbeId,
    pub translation: Vec3,
    pub radius_m: f64,
    pub priority: i32,
    pub intensity: f32,
    pub enabled: bool,
    /// Face size of this probe's capture. 32, 64, 128, or 256.
    pub resolution: u32,
    pub mip_count: u32,
}

/// When the renderer may rebuild a probe cube. Not a material setting and not a per-frame loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeUpdatePolicy {
    /// Capture once. A manual recapture command still runs.
    Static,
    /// No automatic rebuild after the first capture. The command is the only refresh.
    OnDemand,
    /// Rebuild when the world revision changes. Camera motion does not change that revision.
    OnTransformChange,
    /// Rebuild when lights, the environment, or an emissive edit change. A pure transform does not.
    OnLightingChange,
    /// Same dirty rules as transform plus lighting, but the GPU work is spread across frames.
    /// One probe step per frame. Not a recapture of every probe every frame.
    /// A later dynamic mode spends this same per-frame budget. It does not recapture every probe every frame.
    TimeSliced,
}

impl ProbeUpdatePolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Static => "Static",
            Self::OnDemand => "On Demand",
            Self::OnTransformChange => "On Transform",
            Self::OnLightingChange => "On Lighting",
            Self::TimeSliced => "Time Sliced",
        }
    }

    pub fn from_label(text: &str) -> Option<Self> {
        match text {
            "Static" => Some(Self::Static),
            "On Demand" => Some(Self::OnDemand),
            "On Transform" => Some(Self::OnTransformChange),
            "On Lighting" => Some(Self::OnLightingChange),
            "Time Sliced" => Some(Self::TimeSliced),
            _ => None,
        }
    }
}

/// 32-byte uniform. Two `vec4`s.
///
/// `center_radius.xyz` is the probe origin minus the view origin, in world axes, float32.
/// `center_radius.w` is the influence radius in meters.
/// `params.x` is intensity. `params.y` is 1 when this probe may be sampled.
/// `params.z` is `mip_count - 1`, the roughness lod scale. `params.w` is unused.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuReflectionProbePacket {
    pub center: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
    pub enabled: f32,
    pub max_lod: f32,
    pub pad: f32,
}

impl GpuReflectionProbePacket {
    pub const BYTES: usize = 32;

    pub fn disabled() -> Self {
        Self { center: [0.0; 3], radius: 0.0, intensity: 0.0, enabled: 0.0, max_lod: 0.0, pad: 0.0 }
    }

    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut bytes = [0u8; Self::BYTES];
        write3(&mut bytes[0..12], self.center);
        bytes[12..16].copy_from_slice(&self.radius.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.intensity.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.enabled.to_le_bytes());
        bytes[24..28].copy_from_slice(&self.max_lod.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.pad.to_le_bytes());
        bytes
    }
}

/// Highest priority, then lowest id. One sample. No multi-probe radiance blend.
pub fn select_reflection_probe(probes: &[RenderReflectionProbe]) -> Option<&RenderReflectionProbe> {
    probes.iter().filter(|probe| probe_is_selectable(probe)).max_by(|left, right| {
        left.priority.cmp(&right.priority).then(right.id.0.cmp(&left.id.0))
    })
}

/// Smoothstep from the probe center to the sphere edge. 1 at the center, 0 outside.
///
/// `t = saturate(1 - distance / radius)`, `weight = t² (3 - 2t)`.
/// Incoming radiance is what gets blended. The BRDF is applied after the mix.
/// A non-positive intensity does not override the global sky with black.
pub fn reflection_probe_weight(probe: &RenderReflectionProbe, point: Vec3) -> f32 {
    if !probe_is_selectable(probe) {
        return 0.0;
    }
    let dx = point.x - probe.translation.x;
    let dy = point.y - probe.translation.y;
    let dz = point.z - probe.translation.z;
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    if !distance.is_finite() {
        return 0.0;
    }
    let t = (1.0 - distance / probe.radius_m).clamp(0.0, 1.0) as f32;
    t * t * (3.0 - 2.0 * t)
}

pub fn reflection_probe_influence(probes: &[RenderReflectionProbe], point: Vec3) -> f32 {
    select_reflection_probe(probes).map(|probe| reflection_probe_weight(probe, point)).unwrap_or(0.0)
}

/// View-relative center. Subtract in f64. Never a billion-meter float32 position.
pub fn reflection_probe_center(probe: &RenderReflectionProbe, camera_translation: Vec3) -> [f32; 3] {
    camera_relative_f32(probe.translation, camera_translation)
}

/// GGX samples for a narrow prefilter lobe at 64². One capture, not a per-frame cost.
/// Wider lobes and larger faces use [`reflection_probe_prefilter_sample_count`].
pub const REFLECTION_PROBE_PREFILTER_SAMPLES: u32 = 160;

/// Default capture face for a quality level when a level did not name one.
///
/// This is not an adapter class. Baseline is 64 on every GPU. A saved `.jarviglevel` keeps its own resolution.
pub fn reflection_probe_budget_resolution(quality: crate::RenderQuality) -> u32 {
    quality.budget().probe_resolution
}

/// Samples for one prefiltered mip. Mip 0 is the capture and does not use this.
///
/// Narrow lobes (the mips just above the mirror) get the full count, because a bright
/// light is only a few texels there. Rough lobes average out and take half.
/// A 256² face has sixteen times the texels of 64², so the count scales down enough
/// that the one-time prefilter stays usable on the baseline GPU. It does not drop
/// below 48, which is still a real GGX integral rather than a box blur.
pub fn reflection_probe_prefilter_sample_count(resolution: u32, mip: u32, mip_count: u32) -> u32 {
    let roughness = reflection_probe_prefilter_roughness(mip, mip_count);
    let narrow = if roughness <= 0.45 { REFLECTION_PROBE_PREFILTER_SAMPLES } else { REFLECTION_PROBE_PREFILTER_SAMPLES / 2 };
    let scale = (64.0 / resolution.max(32) as f32).clamp(0.35, 1.0);
    ((narrow as f32) * scale).round().clamp(48.0, REFLECTION_PROBE_PREFILTER_SAMPLES as f32) as u32
}

/// Cosine samples for one irradiance texel. Built once from the capture, not every frame.
pub const INDIRECT_DIFFUSE_SAMPLES: u32 = 64;

/// Cosine-weighted direction around `normal`. `xi` is a Hammersley point in `0..1`.
///
/// The PDF is `cos / π`, so the stored irradiance is the average of the samples.
/// The shader multiplies albedo. It does not divide by `π` again.
pub fn indirect_diffuse_direction(xi_x: f32, xi_y: f32, normal: [f32; 3]) -> [f32; 3] {
    let phi = 2.0 * std::f32::consts::PI * xi_x.clamp(0.0, 1.0);
    let cos_theta = xi_y.clamp(0.0, 1.0).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let local = [sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta];
    let normal = normalize3(normal);
    let up = if normal[2].abs() < 0.999 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let tangent = normalize3(cross3(up, normal));
    let bitangent = cross3(normal, tangent);
    normalize3([
        tangent[0] * local[0] + bitangent[0] * local[1] + normal[0] * local[2],
        tangent[1] * local[0] + bitangent[1] * local[1] + normal[1] * local[2],
        tangent[2] * local[0] + bitangent[2] * local[1] + normal[2] * local[2],
    ])
}

/// Roughness stored in mip `mip`. Mip 0 is the unfiltered capture, so this returns 0.
/// The last mip is roughness 1. Mip `i` is the inverse of [`reflection_probe_lod`].
///
/// The perceptual minimum sits on mip 0. The rest of the roughness range is spread
/// across the chain, so a smooth metal is not pulled into a coarse mip.
pub fn reflection_probe_prefilter_roughness(mip: u32, mip_count: u32) -> f32 {
    if mip == 0 || mip_count <= 1 {
        return 0.0;
    }
    let t = (mip as f32 / (mip_count - 1) as f32).clamp(0.0, 1.0);
    let min_r = jarvig_material::MIN_PERCEPTUAL_ROUGHNESS;
    (min_r + t * (1.0 - min_r)).clamp(min_r, 1.0)
}

/// Direction a cubemap texel stores. Inverse of the WebGPU / D3D sample mapping.
/// `u` and `v` are the texel center in `0..1`. Face 0 is +X, then −X, +Y, −Y, +Z, −Z.
pub fn reflection_cube_texel_direction(face: u32, u: f32, v: f32) -> [f32; 3] {
    let s = u * 2.0 - 1.0;
    let t = v * 2.0 - 1.0;
    let direction = match face {
        0 => [1.0, -t, -s],
        1 => [-1.0, -t, s],
        2 => [s, 1.0, t],
        3 => [s, -1.0, -t],
        4 => [s, -t, 1.0],
        _ => [-s, -t, -1.0],
    };
    normalize3(direction)
}

/// One GGX importance sample around `normal`. `xi` is a Hammersley point in `0..1`.
/// The half-vector frame matches Karis 2013. Roughness 0 returns `normal`.
pub fn reflection_probe_ggx_direction(xi_x: f32, xi_y: f32, roughness: f32, normal: [f32; 3]) -> [f32; 3] {
    let roughness = roughness.clamp(0.0, 1.0);
    let alpha = roughness * roughness;
    let phi = 2.0 * std::f32::consts::PI * xi_x;
    let cos_theta = ((1.0 - xi_y) / (1.0 + (alpha * alpha - 1.0) * xi_y)).max(0.0).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let half = [sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta];
    let normal = normalize3(normal);
    let up = if normal[2].abs() < 0.999 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let tangent = normalize3(cross3(up, normal));
    let bitangent = cross3(normal, tangent);
    normalize3([
        tangent[0] * half[0] + bitangent[0] * half[1] + normal[0] * half[2],
        tangent[1] * half[0] + bitangent[1] * half[1] + normal[1] * half[2],
        tangent[2] * half[0] + bitangent[2] * half[1] + normal[2] * half[2],
    ])
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length < 1.0e-8 {
        return [0.0, 0.0, 1.0];
    }
    [value[0] / length, value[1] / length, value[2] / length]
}

fn cross3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

/// Roughness selects a mip. This is not `color * (1 - roughness)`.
///
/// The perceptual minimum, which is the smooth-metal floor, is exactly mip 0.
/// That mip is the sharp capture. Rougher values spread across the remaining
/// chain. Mip `i` was prefiltered at [`reflection_probe_prefilter_roughness`]`(i)`.
pub fn reflection_probe_lod(perceptual_roughness: f32, mip_count: u32) -> f32 {
    if !perceptual_roughness.is_finite() || mip_count <= 1 {
        return 0.0;
    }
    let min_r = jarvig_material::MIN_PERCEPTUAL_ROUGHNESS;
    let roughness = perceptual_roughness.clamp(min_r, 1.0);
    let t = ((roughness - min_r) / (1.0 - min_r)).clamp(0.0, 1.0);
    t * (mip_count - 1) as f32
}

pub fn reflection_probe_packet(
    probe: Option<&RenderReflectionProbe>,
    camera_translation: Vec3,
    mip_count: u32,
    captured: bool,
) -> GpuReflectionProbePacket {
    let Some(probe) = probe else {
        return GpuReflectionProbePacket::disabled();
    };
    if !captured || !probe_is_selectable(probe) {
        return GpuReflectionProbePacket::disabled();
    }
    let intensity = finite_intensity(probe.intensity).unwrap_or(0.0);
    GpuReflectionProbePacket {
        center: reflection_probe_center(probe, camera_translation),
        radius: probe.radius_m as f32,
        intensity,
        enabled: 1.0,
        max_lod: mip_count.saturating_sub(1) as f32,
        pad: 0.0,
    }
}

/// WebGPU / D3D cubemap faces. `forward` is the direction the face camera looks.
/// `up` is the face's image up, which is not always world +Y.
///
/// The capture position is the probe origin. It is not the shaded point.
/// Capturing every opaque object, including the reflector, is the bootstrap policy.
pub fn reflection_cube_faces() -> [(Vec3, Vec3); 6] {
    [
        (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
        (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
        (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
        (Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)),
        (Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
        (Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, -1.0, 0.0)),
    ]
}

/// Camera rotation whose local −Z is `forward` and whose local +Y is `up`.
pub fn rotation_looking_toward(forward: Vec3, up: Vec3) -> Result<Quat, SpaceError> {
    let forward = unit(forward)?;
    let up = unit(up)?;
    let local_z = forward.scale(-1.0);
    let local_x = unit(up.cross(local_z))?;
    let local_y = unit(local_z.cross(local_x))?;
    Ok(quat_from_axes(local_x, local_y, local_z))
}

fn probe_is_selectable(probe: &RenderReflectionProbe) -> bool {
    probe.enabled && probe.radius_m.is_finite() && probe.radius_m > 0.0 && probe.intensity.is_finite() && probe.intensity > 0.0
}

fn unit(vector: Vec3) -> Result<Vec3, SpaceError> {
    let length = (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt();
    if !length.is_finite() || length < 1.0e-8 {
        return Err(SpaceError::BadRotation);
    }
    Ok(vector.scale(1.0 / length))
}

fn quat_from_axes(x_axis: Vec3, y_axis: Vec3, z_axis: Vec3) -> Quat {
    let (m00, m10, m20) = (x_axis.x, x_axis.y, x_axis.z);
    let (m01, m11, m21) = (y_axis.x, y_axis.y, y_axis.z);
    let (m02, m12, m22) = (z_axis.x, z_axis.y, z_axis.z);
    let trace = m00 + m11 + m22;
    let (x, y, z, w) = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        ((m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s)
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        (0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s)
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        ((m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s)
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        ((m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s)
    };
    let length = (x * x + y * y + z * z + w * w).sqrt();
    Quat { x: x / length, y: y / length, z: z / length, w: w / length }
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

    fn probe_at(id: u64, z: f64, priority: i32, enabled: bool) -> RenderReflectionProbe {
        RenderReflectionProbe {
            id: ProbeId(id),
            translation: Vec3::new(0.0, 0.0, z),
            radius_m: 8.0,
            priority,
            intensity: 1.0,
            enabled,
            resolution: REFLECTION_PROBE_RESOLUTION,
            mip_count: REFLECTION_PROBE_MIP_COUNT,
        }
    }

    #[test]
    fn weight_is_smooth_at_the_edge_and_zero_outside() {
        let probe = probe_at(1, 0.0, 0, true);
        assert!((reflection_probe_weight(&probe, Vec3::ZERO) - 1.0).abs() < 1.0e-5);
        let edge = reflection_probe_weight(&probe, Vec3::new(8.0, 0.0, 0.0));
        assert!(edge.abs() < 1.0e-5);
        assert_eq!(reflection_probe_weight(&probe, Vec3::new(8.1, 0.0, 0.0)), 0.0);
        let mid = reflection_probe_weight(&probe, Vec3::new(4.0, 0.0, 0.0));
        assert!(mid > 0.4 && mid < 0.6, "{mid}");
        let mut off = probe;
        off.enabled = false;
        assert_eq!(reflection_probe_weight(&off, Vec3::ZERO), 0.0);
        off.enabled = true;
        off.intensity = 0.0;
        assert_eq!(reflection_probe_weight(&off, Vec3::ZERO), 0.0);
    }

    #[test]
    fn one_probe_wins_by_priority_then_id_and_absence_is_the_global_path() {
        let low = probe_at(2, 0.0, 0, true);
        let high = probe_at(3, 10.0, 2, true);
        let older = probe_at(1, 0.0, 2, true);
        let probes = [low, high, older];
        assert_eq!(select_reflection_probe(&probes).unwrap().id, ProbeId(1));
        assert_eq!(reflection_probe_influence(&[], Vec3::ZERO), 0.0);
        let mut disabled = [high];
        disabled[0].enabled = false;
        assert_eq!(reflection_probe_influence(&disabled, Vec3::ZERO), 0.0);
    }

    #[test]
    fn roughness_picks_a_higher_mip_and_the_center_stays_view_relative() {
        let sharp = reflection_probe_lod(0.1, REFLECTION_PROBE_MIP_COUNT);
        let rough = reflection_probe_lod(0.9, REFLECTION_PROBE_MIP_COUNT);
        assert!(sharp < rough);
        assert!(sharp < 1.0 && rough > 4.0);
        let mut probe = probe_at(1, -3.5, 0, true);
        probe.translation.x = crate::BOOTSTRAP_ROOT_M;
        probe.translation.y = 0.2;
        let center = reflection_probe_center(&probe, Vec3::new(crate::BOOTSTRAP_ROOT_M, 0.0, 0.0));
        assert!(center[0].abs() < 1.0e-3, "{center:?}");
        assert!((center[1] - 0.2).abs() < 1.0e-4);
        assert!((center[2] + 3.5).abs() < 1.0e-3);
        let bytes = reflection_probe_packet(Some(&probe), Vec3::new(crate::BOOTSTRAP_ROOT_M, 0.0, 0.0), 6, true).to_bytes();
        assert_eq!(GpuReflectionProbePacket::BYTES, 32);
        assert_eq!(f32::from_le_bytes(bytes[20..24].try_into().unwrap()), 1.0);
        assert_eq!(reflection_probe_packet(Some(&probe), Vec3::ZERO, 6, false).enabled, 0.0);
        assert_eq!(reflection_probe_prefilter_roughness(0, 6), 0.0);
        let first = reflection_probe_prefilter_roughness(1, 6);
        assert!(first > jarvig_material::MIN_PERCEPTUAL_ROUGHNESS && first < 0.3, "{first}");
        assert!((reflection_probe_prefilter_roughness(5, 6) - 1.0).abs() < 1.0e-5);
        assert!((reflection_probe_lod(reflection_probe_prefilter_roughness(3, 6), 6) - 3.0).abs() < 1.0e-4);
    }

    #[test]
    fn prefilter_directions_match_the_cubemap_faces_and_a_mirror_sample_is_the_normal() {
        let centers = [
            [1.0_f32, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        for (face, expected) in centers.iter().enumerate() {
            let direction = reflection_cube_texel_direction(face as u32, 0.5, 0.5);
            assert!((direction[0] - expected[0]).abs() < 1.0e-5, "{face} {direction:?}");
            assert!((direction[1] - expected[1]).abs() < 1.0e-5, "{face} {direction:?}");
            assert!((direction[2] - expected[2]).abs() < 1.0e-5, "{face} {direction:?}");
            let mirrored = reflection_probe_ggx_direction(0.0, 0.0, 0.8, *expected);
            assert!((mirrored[0] - expected[0]).abs() < 1.0e-4);
            assert!((mirrored[1] - expected[1]).abs() < 1.0e-4);
            assert!((mirrored[2] - expected[2]).abs() < 1.0e-4);
        }
        let spread = reflection_probe_ggx_direction(0.5, 0.95, 1.0, [0.0, 0.0, 1.0]);
        let along = spread[0] * 0.0 + spread[1] * 0.0 + spread[2] * 1.0;
        assert!(along < 0.5, "{spread:?}");
        assert_eq!(REFLECTION_PROBE_PREFILTER_SAMPLES, 160);
        assert_eq!(reflection_probe_prefilter_sample_count(64, 1, 7), 160);
        assert!(reflection_probe_prefilter_sample_count(64, 6, 7) < 160);
        assert!(reflection_probe_prefilter_sample_count(256, 1, 9) >= 48);
        assert!(reflection_probe_prefilter_sample_count(256, 1, 9) < reflection_probe_prefilter_sample_count(64, 1, 7));
        assert_eq!(reflection_probe_budget_resolution(crate::RenderQuality::Baseline), 64);
        assert_eq!(reflection_probe_budget_resolution(crate::RenderQuality::Enhanced), 128);
        assert_eq!(reflection_probe_budget_resolution(crate::RenderQuality::High), 256);
        assert_eq!(INDIRECT_DIFFUSE_SAMPLES, 64);
    }

    #[test]
    fn capture_resolution_is_four_powers_and_the_mip_count_follows() {
        for (resolution, mips) in [(32, 6), (64, 7), (128, 8), (256, 9)] {
            assert!(reflection_probe_resolution_supported(resolution), "{resolution}");
            assert_eq!(reflection_probe_mip_count_for(resolution), mips, "{resolution}");
        }
        assert!(!reflection_probe_resolution_supported(48));
        assert!(!reflection_probe_resolution_supported(512));
        assert!(reflection_probe_lod(0.045, reflection_probe_mip_count_for(64)).abs() < 1.0e-5);
        assert!((reflection_probe_lod(1.0, reflection_probe_mip_count_for(64)) - 6.0).abs() < 1.0e-4);
    }

    #[test]
    fn cosine_samples_stay_in_the_normal_hemisphere_and_a_pole_sample_is_the_normal() {
        let normal = [0.0_f32, 1.0, 0.0];
        let pole = indirect_diffuse_direction(0.2, 1.0, normal);
        assert!((pole[0]).abs() < 1.0e-4 && (pole[1] - 1.0).abs() < 1.0e-4 && pole[2].abs() < 1.0e-4, "{pole:?}");
        let mut sum = [0.0_f32; 3];
        let count = 32u32;
        for index in 0..count {
            let xi_x = index as f32 / count as f32;
            let bits = index.reverse_bits();
            let xi_y = bits as f32 * 2.3283064365386963e-10;
            let sample = indirect_diffuse_direction(xi_x, xi_y, normal);
            let facing = sample[0] * normal[0] + sample[1] * normal[1] + sample[2] * normal[2];
            assert!(facing >= -1.0e-4, "{sample:?}");
            sum[0] += sample[0];
            sum[1] += sample[1];
            sum[2] += sample[2];
        }
        assert!(sum[1] / count as f32 > 0.5, "{sum:?}");
    }

    #[test]
    fn cube_faces_look_along_their_axis() {
        let faces = reflection_cube_faces();
        assert_eq!(faces.len(), 6);
        for (forward, up) in faces {
            let rotation = rotation_looking_toward(forward, up).unwrap();
            let look = rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
            let raised = rotation.rotate(Vec3::new(0.0, 1.0, 0.0));
            assert!((look.x - forward.x).abs() < 1.0e-6);
            assert!((look.y - forward.y).abs() < 1.0e-6);
            assert!((look.z - forward.z).abs() < 1.0e-6);
            assert!((raised.x - up.x).abs() < 1.0e-6);
            assert!((raised.y - up.y).abs() < 1.0e-6);
            assert!((raised.z - up.z).abs() < 1.0e-6);
        }
    }
}
