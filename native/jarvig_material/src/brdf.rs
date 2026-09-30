//! Baseline direct-light BRDF. Cook-Torrance, GGX, Smith, Schlick, Lambert.
//!
//! This is shading policy. It is not a world light. The generated shader uses the
//! same constants and the same equations. AO is not applied to direct light.

use crate::surface::MaterialSurface;

use std::f32::consts::PI;

/// One place. Do not clamp roughness ad hoc in a shader.
pub const MIN_PERCEPTUAL_ROUGHNESS: f32 = 0.045;

pub const DIELECTRIC_F0: f32 = 0.04;

/// One guard for the inverse-square singularity. Not a per-call epsilon.
pub const MIN_LIGHT_DISTANCE_M: f32 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrdfSample {
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
}

impl BrdfSample {
    pub fn direct(self) -> [f32; 3] {
        add(self.diffuse, self.specular)
    }

    pub fn outgoing(self) -> [f32; 3] {
        add(self.direct(), self.emissive)
    }
}

pub fn perceptual_alpha(roughness: f32) -> f32 {
    let roughness = roughness.clamp(MIN_PERCEPTUAL_ROUGHNESS, 1.0);
    roughness * roughness
}

pub fn f0_from_metallic(base_color: [f32; 3], metallic: f32) -> [f32; 3] {
    let metallic = metallic.clamp(0.0, 1.0);
    mix([DIELECTRIC_F0; 3], base_color, metallic)
}

pub fn diffuse_weight(base_color: [f32; 3], metallic: f32) -> [f32; 3] {
    scale(base_color, 1.0 - metallic.clamp(0.0, 1.0))
}

pub fn ggx_ndf(n_dot_h: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let nh = n_dot_h.max(0.0);
    let denom = nh * nh * (a2 - 1.0) + 1.0;
    a2 / (PI * denom * denom).max(1.0e-8)
}

pub fn smith_g1(n_dot_x: f32, alpha: f32) -> f32 {
    let nx = n_dot_x.max(0.0);
    let a2 = alpha * alpha;
    let denom = nx + (a2 + (1.0 - a2) * nx * nx).sqrt();
    (2.0 * nx) / denom.max(1.0e-8)
}

pub fn fresnel_schlick(v_dot_h: f32, f0: [f32; 3]) -> [f32; 3] {
    let one_minus = (1.0 - v_dot_h.clamp(0.0, 1.0)).powi(5);
    [
        f0[0] + (1.0 - f0[0]) * one_minus,
        f0[1] + (1.0 - f0[1]) * one_minus,
        f0[2] + (1.0 - f0[2]) * one_minus,
    ]
}

/// Diffuse irradiance from one two-hemisphere environment.
///
/// `upper_radiance` and `lower_radiance` are linear radiance, constant over the
/// world +Y and −Y hemispheres. The cosine-weighted integral of a uniform
/// hemisphere is `radiance * π`. Lambert divides by `π`, so those factors cancel.
/// The blend `normal.y * 0.5 + 0.5` is exact when the normal is up, down, or level.
///
/// Metallic removes this diffuse lobe. It does not add a reflection. AO scales
/// this term only. [`evaluate_direct`] still ignores AO. Disabled, or a
/// non-positive intensity, contributes nothing. This is not `color += 0.1`.
pub fn environment_diffuse(
    surface: &MaterialSurface,
    world_normal: [f32; 3],
    upper_radiance: [f32; 3],
    lower_radiance: [f32; 3],
    intensity: f32,
    enabled: bool,
) -> [f32; 3] {
    if !enabled || !intensity.is_finite() || intensity <= 0.0 {
        return [0.0; 3];
    }
    let surface = surface.sanitized();
    let normal = normalize(world_normal);
    let sky = (normal[1].clamp(-1.0, 1.0)) * 0.5 + 0.5;
    let incident = mix(lower_radiance, upper_radiance, sky);
    let diffuse = scale(diffuse_weight(surface.base_color, surface.metallic), surface.ambient_occlusion * intensity);
    scale_rgb(diffuse, incident, 1.0)
}

/// Specular reflection of one two-hemisphere environment.
///
/// This is the split-sum stand-in for a prefiltered cubemap, not final image-based
/// lighting. The mirror ray samples the hemisphere along the reflection. GGX
/// `alpha = roughness²` blends that sample toward the hemisphere facing the
/// normal, which is the broad lobe. It is not `color * (1 - roughness)`.
///
/// The Fresnel scale and bias are Brian Karis's analytical environment BRDF.
/// `F0` matches the direct term: dielectric `0.04`, metal uses base color.
/// Ambient occlusion is not applied. There is no specular-occlusion model yet.
/// A later cubemap replaces only the hemisphere lookup.
pub fn environment_specular(
    surface: &MaterialSurface,
    world_normal: [f32; 3],
    world_view: [f32; 3],
    upper_radiance: [f32; 3],
    lower_radiance: [f32; 3],
    intensity: f32,
    enabled: bool,
) -> [f32; 3] {
    if !enabled || !intensity.is_finite() || intensity <= 0.0 {
        return [0.0; 3];
    }
    let surface = surface.sanitized();
    if !surface.perceptual_roughness.is_finite() {
        return [0.0; 3];
    }
    let normal = normalize(world_normal);
    let view = normalize(world_view);
    let n_dot_v = dot(normal, view);
    if n_dot_v <= 0.0 {
        return [0.0; 3];
    }
    let n_dot_v = n_dot_v.max(1.0e-4);
    let roughness = surface.perceptual_roughness.clamp(MIN_PERCEPTUAL_ROUGHNESS, 1.0);
    let alpha = roughness * roughness;
    let mirrored = scale(normal, 2.0 * n_dot_v);
    let reflection = normalize([mirrored[0] - view[0], mirrored[1] - view[1], mirrored[2] - view[2]]);
    let mirror = hemisphere_radiance(reflection, upper_radiance, lower_radiance);
    let broad = hemisphere_radiance(normal, upper_radiance, lower_radiance);
    let prefiltered = mix(mirror, broad, alpha);
    let f0 = f0_from_metallic(surface.base_color, surface.metallic);
    let [scale_f, bias] = env_brdf_approx(roughness, n_dot_v);
    let fresnel = [f0[0] * scale_f + bias, f0[1] * scale_f + bias, f0[2] * scale_f + bias];
    let result = scale_rgb(prefiltered, fresnel, intensity);
    if result.iter().any(|channel| !channel.is_finite()) {
        [0.0; 3]
    } else {
        result
    }
}

/// Karis 2013 environment-BRDF fit. Returns `(scale, bias)` for `F0 * scale + bias`.
pub fn env_brdf_approx(perceptual_roughness: f32, n_dot_v: f32) -> [f32; 2] {
    let c0 = [-1.0, -0.0275, -0.572, 0.022];
    let c1 = [1.0, 0.0425, 1.04, -0.04];
    let r = [
        perceptual_roughness * c0[0] + c1[0],
        perceptual_roughness * c0[1] + c1[1],
        perceptual_roughness * c0[2] + c1[2],
        perceptual_roughness * c0[3] + c1[3],
    ];
    let a004 = (r[0] * r[0]).min((-9.28 * n_dot_v).exp2()) * r[0] + r[1];
    [-1.04 * a004 + r[2], 1.04 * a004 + r[3]]
}

fn hemisphere_radiance(direction: [f32; 3], upper: [f32; 3], lower: [f32; 3]) -> [f32; 3] {
    let sky = direction[1].clamp(-1.0, 1.0) * 0.5 + 0.5;
    mix(lower, upper, sky)
}

/// Direct reflectance for a light of radiance 1 coming from `light`.
/// `n_dot_l <= 0` contributes no direct light. Emissive is unchanged.
/// Ambient occlusion is ignored.
pub fn evaluate_direct(surface: &MaterialSurface, normal: [f32; 3], view: [f32; 3], light: [f32; 3]) -> BrdfSample {
    let surface = surface.sanitized();
    let n = normalize(normal);
    let v = normalize(view);
    let l = normalize(light);
    let n_dot_l = dot(n, l);
    let n_dot_v = dot(n, v);
    let emissive = surface.emissive;
    if n_dot_l <= 0.0 || n_dot_v <= 0.0 {
        return BrdfSample { diffuse: [0.0; 3], specular: [0.0; 3], emissive };
    }
    let h = normalize(add(v, l));
    let n_dot_h = dot(n, h).max(0.0);
    let v_dot_h = dot(v, h).max(0.0);
    let alpha = perceptual_alpha(surface.perceptual_roughness);
    let f0 = f0_from_metallic(surface.base_color, surface.metallic);
    let d = ggx_ndf(n_dot_h, alpha);
    let g = smith_g1(n_dot_v.max(1.0e-4), alpha) * smith_g1(n_dot_l, alpha);
    let f = fresnel_schlick(v_dot_h, f0);
    let spec = scale(f, d * g / (4.0 * n_dot_v.max(1.0e-4) * n_dot_l).max(1.0e-6));
    let diffuse = scale(diffuse_weight(surface.base_color, surface.metallic), n_dot_l / PI);
    let specular = scale(spec, n_dot_l);
    let _ao_is_not_applied = surface.ambient_occlusion;
    BrdfSample { diffuse, specular, emissive }
}

/// Candela to illuminance. `range_m == 0` does not fade the light. It only
/// rejects samples outside a positive range. The distance clamp is [`MIN_LIGHT_DISTANCE_M`].
pub fn punctual_illuminance(intensity_cd: f32, distance_m: f32, range_m: f32) -> f32 {
    if !intensity_cd.is_finite() || !distance_m.is_finite() || intensity_cd <= 0.0 {
        return 0.0;
    }
    if range_m > 0.0 && distance_m > range_m {
        return 0.0;
    }
    let distance = distance_m.max(MIN_LIGHT_DISTANCE_M);
    intensity_cd / (distance * distance)
}

/// Directional illuminance is the authored lux. Distance is not an input.
pub fn directional_illuminance(lux: f32) -> f32 {
    if lux.is_finite() && lux > 0.0 {
        lux
    } else {
        0.0
    }
}

/// Smooth cone from the emission axis. `cos_inner >= cos_outer` because the
/// outer half-angle is larger. Equal cosines are a hard edge, not a NaN.
pub fn spot_angular(cos_theta: f32, cos_inner: f32, cos_outer: f32) -> f32 {
    if cos_theta >= cos_inner {
        return 1.0;
    }
    if cos_theta <= cos_outer {
        return 0.0;
    }
    let span = (cos_inner - cos_outer).max(1.0e-6);
    let t = ((cos_theta - cos_outer) / span).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Reflected radiance for one incident illuminance. Emissive is not included.
pub fn reflected_direct(
    surface: &MaterialSurface,
    normal: [f32; 3],
    view: [f32; 3],
    to_light: [f32; 3],
    color_linear: [f32; 3],
    illuminance: f32,
) -> [f32; 3] {
    let sample = evaluate_direct(surface, normal, view, to_light);
    scale_rgb(sample.direct(), color_linear, illuminance)
}

pub fn emissive_radiance(surface: &MaterialSurface) -> [f32; 3] {
    surface.sanitized().emissive
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn scale_rgb(v: [f32; 3], color: [f32; 3], illuminance: f32) -> [f32; 3] {
    [v[0] * color[0] * illuminance, v[1] * color[1] * illuminance, v[2] * color[2] * illuminance]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt().max(1.0e-8);
    scale(v, 1.0 / len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white_dielectric(roughness: f32) -> MaterialSurface {
        MaterialSurface {
            base_color: [1.0, 1.0, 1.0],
            metallic: 0.0,
            perceptual_roughness: roughness,
            ambient_occlusion: 1.0,
            ..MaterialSurface::default()
        }
    }

    fn aligned() -> BrdfSample {
        evaluate_direct(&white_dielectric(0.5), [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0])
    }

    #[test]
    fn metallic_workflow_and_roughness_are_stable() {
        let dielectric = f0_from_metallic([0.8, 0.1, 0.1], 0.0);
        assert!((dielectric[0] - DIELECTRIC_F0).abs() < 1.0e-6);
        let metal = f0_from_metallic([0.8, 0.1, 0.1], 1.0);
        assert!((metal[0] - 0.8).abs() < 1.0e-5);
        assert_eq!(diffuse_weight([0.2, 0.3, 0.4], 1.0), [0.0; 3]);
        assert_eq!(diffuse_weight([0.2, 0.3, 0.4], 0.0), [0.2, 0.3, 0.4]);
        let smooth = perceptual_alpha(0.0);
        let rough = perceptual_alpha(0.8);
        assert!((smooth - MIN_PERCEPTUAL_ROUGHNESS * MIN_PERCEPTUAL_ROUGHNESS).abs() < 1.0e-6);
        assert!(ggx_ndf(1.0, smooth) > ggx_ndf(1.0, rough));
        let head = fresnel_schlick(1.0, [DIELECTRIC_F0; 3]);
        let graze = fresnel_schlick(0.0, [DIELECTRIC_F0; 3]);
        assert!(graze[0] > head[0]);
        assert!(head[0] < DIELECTRIC_F0 + 1.0e-4);
    }

    #[test]
    fn direct_light_stays_finite_and_ignores_ao() {
        let sample = aligned();
        assert!(sample.outgoing().iter().all(|v| v.is_finite()));
        assert!(sample.diffuse[0] > 0.0);
        let mut dark_ao = white_dielectric(0.5);
        dark_ao.ambient_occlusion = 0.0;
        let ao = evaluate_direct(&dark_ao, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert_eq!(ao.direct(), sample.direct());
        let mut metal = white_dielectric(0.5);
        metal.metallic = 1.0;
        metal.base_color = [0.7, 0.7, 0.7];
        let metal_sample = evaluate_direct(&metal, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert_eq!(metal_sample.diffuse, [0.0; 3]);
        let back = evaluate_direct(&white_dielectric(0.5), [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]);
        assert_eq!(back.direct(), [0.0; 3]);
        let mut glowing = white_dielectric(0.5);
        glowing.emissive = [2.0, 0.0, 0.0];
        let glow = evaluate_direct(&glowing, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]);
        assert!((glow.emissive[0] - 2.0).abs() < 1.0e-5);
        assert_eq!(glow.direct(), [0.0; 3]);
        let graze = evaluate_direct(&white_dielectric(0.2), [0.0, 0.0, 1.0], [1.0, 0.0, 1.0e-4], [0.0, 0.0, 1.0]);
        assert!(graze.outgoing().iter().all(|v| v.is_finite()));
        let rough = evaluate_direct(&white_dielectric(0.9), [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert!(rough.outgoing()[0] < 4.0);
        let smooth = evaluate_direct(&white_dielectric(0.0), [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert!(smooth.outgoing().iter().all(|v| v.is_finite()));
        assert!(smooth.specular[0] > sample.specular[0]);
    }

    #[test]
    fn physical_units_and_cones_match_the_shading_policy() {
        let near = punctual_illuminance(100.0, 1.0, 0.0);
        let far = punctual_illuminance(100.0, 2.0, 0.0);
        assert!((near / far - 4.0).abs() < 1.0e-4);
        assert!(punctual_illuminance(100.0, 0.0, 0.0).is_finite());
        assert_eq!(punctual_illuminance(100.0, 5.0, 2.0), 0.0);
        assert!((punctual_illuminance(100.0, 1.0, 2.0) - 100.0).abs() < 1.0e-4);
        assert!((directional_illuminance(3.0) - 3.0).abs() < 1.0e-6);
        assert_eq!(spot_angular(1.0, 0.9, 0.5), 1.0);
        assert_eq!(spot_angular(0.1, 0.9, 0.5), 0.0);
        let mid = spot_angular(0.7, 0.9, 0.5);
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(spot_angular(0.5, 0.5, 0.5), 1.0);
        let mut glowing = white_dielectric(0.5);
        glowing.emissive = [4.0, 0.0, 0.0];
        assert_eq!(emissive_radiance(&glowing), [4.0, 0.0, 0.0]);
        let lit = reflected_direct(&white_dielectric(0.5), [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 1.0);
        let mut ao = white_dielectric(0.5);
        ao.ambient_occlusion = 0.0;
        let dark = reflected_direct(&ao, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 1.0);
        assert_eq!(lit, dark);
        let back = reflected_direct(&white_dielectric(0.5), [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0], [1.0, 1.0, 1.0], 10.0);
        assert_eq!(back, [0.0; 3]);
    }

    #[test]
    fn environment_diffuse_follows_the_normal_and_leaves_direct_light_alone() {
        let upper = [0.55, 0.68, 0.86];
        let lower = [0.22, 0.16, 0.11];
        let dielectric = white_dielectric(0.85);
        let up = environment_diffuse(&dielectric, [0.0, 1.0, 0.0], upper, lower, 0.20, true);
        let down = environment_diffuse(&dielectric, [0.0, -1.0, 0.0], upper, lower, 0.20, true);
        let side = environment_diffuse(&dielectric, [0.0, 0.0, 1.0], upper, lower, 0.20, true);
        assert!((up[2] - 0.172).abs() < 1.0e-4);
        assert!(up[2] > side[2] && side[2] > down[2]);
        assert!(down[0] > down[2]);
        let mut metal = dielectric;
        metal.metallic = 1.0;
        metal.emissive = [3.0, 0.0, 0.0];
        assert_eq!(environment_diffuse(&metal, [0.0, 1.0, 0.0], upper, lower, 0.20, true), [0.0; 3]);
        assert_eq!(emissive_radiance(&metal), [3.0, 0.0, 0.0]);
        let mut dark = dielectric;
        dark.ambient_occlusion = 0.0;
        assert_eq!(environment_diffuse(&dark, [0.0, 1.0, 0.0], upper, lower, 0.20, true), [0.0; 3]);
        let lit = evaluate_direct(&dielectric, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        let lit_dark = evaluate_direct(&dark, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert_eq!(lit.direct(), lit_dark.direct());
        let away = evaluate_direct(&dielectric, [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        assert_eq!(away.direct(), [0.0; 3]);
        assert!(down[0] > 0.02);
        assert_eq!(environment_diffuse(&dielectric, [0.0, 1.0, 0.0], upper, lower, 0.20, false), [0.0; 3]);
        let uniform = environment_diffuse(&dielectric, [0.0, 1.0, 0.0], upper, upper, 0.20, true);
        let uniform_down = environment_diffuse(&dielectric, [0.0, -1.0, 0.0], upper, upper, 0.20, true);
        assert_eq!(uniform, uniform_down);
    }

    #[test]
    fn environment_specular_follows_roughness_view_and_fresnel() {
        let upper = [0.55, 0.68, 0.86];
        let lower = [0.22, 0.16, 0.11];
        let mut metal = white_dielectric(0.2);
        metal.metallic = 1.0;
        metal.base_color = [0.92, 0.62, 0.18];
        metal.ambient_occlusion = 0.0;
        let head_view = [0.0, 0.0, 1.0];
        let head_normal = [0.0, 0.0, 1.0];
        let spec = environment_specular(&metal, head_normal, head_view, upper, lower, 0.20, true);
        assert!(spec.iter().all(|channel| channel.is_finite() && *channel > 0.01), "{spec:?}");
        assert_eq!(environment_diffuse(&metal, head_normal, upper, lower, 0.20, true), [0.0; 3]);
        metal.ambient_occlusion = 1.0;
        assert_eq!(environment_specular(&metal, head_normal, head_view, upper, lower, 0.20, true), spec);
        let mut rough = metal;
        rough.perceptual_roughness = 0.9;
        let broad = environment_specular(&rough, [0.0, 0.7, 0.71414], head_view, upper, lower, 0.20, true);
        let sharp = environment_specular(&metal, [0.0, 0.7, 0.71414], head_view, upper, lower, 0.20, true);
        assert_ne!(broad, sharp);
        let turned = environment_specular(&metal, [0.0, 1.0, 0.0], head_view, upper, lower, 0.20, true);
        assert_ne!(turned, spec);
        let raised = normalize([0.0, 0.55, 0.835]);
        let moved = environment_specular(&metal, head_normal, raised, upper, lower, 0.20, true);
        assert_ne!(moved, spec);
        let mut dielectric = white_dielectric(0.4);
        let flat = [0.4, 0.4, 0.4];
        let facing = environment_specular(&dielectric, head_normal, head_view, flat, flat, 1.0, true);
        let grazing = environment_specular(&dielectric, head_normal, normalize([0.99, 0.0, 0.141]), flat, flat, 1.0, true);
        let sum = |color: [f32; 3]| color[0] + color[1] + color[2];
        assert!(sum(grazing) > sum(facing), "facing {facing:?} grazing {grazing:?}");
        dielectric.metallic = 0.0;
        let glass = environment_specular(&dielectric, head_normal, head_view, upper, lower, 0.20, true);
        assert!(sum(spec) > sum(glass));
        assert_eq!(environment_specular(&metal, head_normal, head_view, upper, lower, 0.20, false), [0.0; 3]);
        assert_eq!(environment_specular(&metal, head_normal, [0.0, 0.0, -1.0], upper, lower, 0.20, true), [0.0; 3]);
        let broken = environment_specular(&metal, [0.0; 3], [0.0; 3], upper, lower, 0.20, true);
        assert!(broken.iter().all(|channel| channel.is_finite()));
    }
}
