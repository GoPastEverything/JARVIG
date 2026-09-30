//! Conventional shadow-map math. Not a GPU texture and not a material.
//!
//! Maps are light-space. A camera move changes only the sampling matrix, which
//! folds `camera - anchor` in f64 before the float32 upload. The depth stored
//! in the map does not depend on the camera. Reversed-Z: near is 1, far is 0.
//!
//! `jarvig_direct_visibility` is the shader seam. A later virtual shadow map
//! replaces the map class, not the material graph.

use crate::{camera_relative_f32, reflection_cube_faces, rotation_looking_toward, Mat4, Quat, SpaceError, Vec3};

pub const SHADOW_MAP_RESOLUTION: u32 = 1024;
pub const POINT_SHADOW_RESOLUTION: u32 = 256;
/// Texels along one cascade. The directional atlas is a 2×2 of these. Not a vendor preset.
pub const CASCADE_RESOLUTION: u32 = 1024;
pub const DIRECTIONAL_ATLAS_RESOLUTION: u32 = CASCADE_RESOLUTION * 2;
pub const MAX_SHADOW_CASCADES: usize = 4;
pub const DEFAULT_CASCADE_COUNT: u32 = 4;
/// 0 is uniform, 1 is logarithmic. 0.5 is the practical split.
pub const DEFAULT_CASCADE_LAMBDA: f32 = 0.5;
/// Fraction of a cascade's view range blended into the next. Not a hard seam.
pub const CASCADE_BLEND: f32 = 0.10;
/// Shadow-map passes submitted in one frame before the rest wait. Not a draw-count cap.
pub const SHADOW_PASS_BUDGET: u32 = 16;
/// Casters larger than this stay out of shadow maps. They still receive shadows.
/// A multi-million triangle mesh would otherwise be redrawn into every cascade and punctual face when the camera moves.
pub const SHADOW_CASTER_TRIANGLE_LIMIT: u32 = 250_000;

/// Shadow maps redraw when the camera or a light moves. A mesh past the triangle limit is not part of that redraw.
pub fn casts_into_shadow_map(triangle_count: u32) -> bool {
    triangle_count > 0 && triangle_count <= SHADOW_CASTER_TRIANGLE_LIMIT
}
/// One projective map per class until the sampler takes an atlas layer.
pub const SHADOW_NEAR_M: f32 = 0.05;
pub const SHADOW_FAR_M: f32 = 40.0;
pub const DIRECTIONAL_HALF_EXTENT_M: f32 = 20.0;
/// Constant depth bias in meters, along the light. Not an NDC fraction.
/// Facing receivers use only this term, so a floor contact stays on the surface.
pub const SHADOW_DEPTH_BIAS_M: f32 = 0.002;
/// Extra meters at a fully grazing receiver (`n·l = 0`). Zero when the face points at the light.
pub const SHADOW_SLOPE_BIAS_M: f32 = 0.010;
/// Meters along the geometric normal, scaled by the grazing factor. Optional. Default is on.
pub const SHADOW_NORMAL_BIAS_M: f32 = 0.004;
/// PCF radius in texels, also the maximum PCSS kernel. 1 is 3×3. 2 is 5×5.
pub const SHADOW_PCF_RADIUS: f32 = 2.0;
/// Directional penumbra scale. `tan` of a small solar disk. Close blockers stay sharp.
pub const SUN_ANGULAR_TAN: f32 = 0.008;
/// Spot/point lamp radius used by PCSS, in meters. Not a blur of the whole map.
pub const PUNCTUAL_LIGHT_RADIUS_M: f32 = 0.05;
/// Screen-space contact march. Supplementary. Not a replacement for the shadow map.
pub const CONTACT_SHADOW_DISTANCE_M: f32 = 0.12;
/// Legacy names. These are meters now. The old 0.001 NDC constant floated contacts on a 40 m ortho.
pub const SHADOW_DEPTH_BIAS: f32 = SHADOW_DEPTH_BIAS_M;
pub const SHADOW_SLOPE_BIAS: f32 = SHADOW_SLOPE_BIAS_M;

/// Receiver bias in meters. Facing surfaces get `depth_bias_m`. Grazing surfaces add up to `slope_bias_m`.
pub fn shadow_receiver_bias(depth_bias_m: f32, slope_bias_m: f32, n_dot_l: f32) -> f32 {
    let grazing = (1.0 - n_dot_l.max(0.0)).clamp(0.0, 1.0);
    depth_bias_m + slope_bias_m * grazing
}

/// Reversed-Z bias added to the receiver before the compare. Inputs are meters, not NDC.
///
/// Orthographic cascades divide meters by that cascade's depth span, so a 2 mm bias stays
/// 2 mm on a tight cascade and on a wide one. Perspective point and spot maps use the
/// local derivative `near * far / ((far - near) * distance²)`. `jarvig_shadow_bias` is this function.
pub fn shadow_bias_ndc(kind: ShadowMapClass, depth_bias_m: f32, slope_bias_m: f32, n_dot_l: f32, distance_m: f32, near_m: f32, far_m: f32) -> f32 {
    let meters = shadow_receiver_bias(depth_bias_m, slope_bias_m, n_dot_l).max(0.0);
    let span = (far_m - near_m).max(1.0e-3);
    if matches!(kind, ShadowMapClass::Directional | ShadowMapClass::None) {
        return meters / span;
    }
    let distance = distance_m.max(near_m).max(1.0e-3);
    near_m * far_m * meters / (span * distance * distance)
}

/// Authored shadow settings on a light. Not a view flag and not a GPU handle.
///
/// `resolution == 0`, `distance_m == 0`, and `cascade_count == 0` select the baseline budget.
/// Bias fields are meters. `cascade_distribution` is the practical-split lambda in `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightShadowSettings {
    pub cast: bool,
    pub resolution: u32,
    pub depth_bias_m: f32,
    pub slope_bias_m: f32,
    pub normal_bias_m: f32,
    pub filter_radius: f32,
    pub distance_m: f32,
    pub cascade_count: u32,
    pub cascade_distribution: f32,
}

impl Default for LightShadowSettings {
    fn default() -> Self {
        Self {
            cast: true,
            resolution: 0,
            depth_bias_m: SHADOW_DEPTH_BIAS_M,
            slope_bias_m: SHADOW_SLOPE_BIAS_M,
            normal_bias_m: SHADOW_NORMAL_BIAS_M,
            filter_radius: SHADOW_PCF_RADIUS,
            distance_m: 0.0,
            cascade_count: 0,
            cascade_distribution: DEFAULT_CASCADE_LAMBDA,
        }
    }
}

impl LightShadowSettings {
    pub fn effective_distance_m(self) -> f32 {
        if self.distance_m > SHADOW_NEAR_M { self.distance_m } else { SHADOW_FAR_M }
    }

    pub fn effective_cascades(self) -> u32 {
        if self.cascade_count == 0 {
            DEFAULT_CASCADE_COUNT
        } else {
            self.cascade_count.clamp(1, MAX_SHADOW_CASCADES as u32)
        }
    }

    pub fn effective_resolution(self, budget: u32) -> u32 {
        if self.resolution == 0 { budget } else { self.resolution.clamp(64, 4096) }
    }

    pub fn finite(self) -> Result<Self, SpaceError> {
        let ok = self.depth_bias_m.is_finite()
            && self.slope_bias_m.is_finite()
            && self.normal_bias_m.is_finite()
            && self.filter_radius.is_finite()
            && self.distance_m.is_finite()
            && self.cascade_distribution.is_finite()
            && self.depth_bias_m >= 0.0
            && self.slope_bias_m >= 0.0
            && self.normal_bias_m >= 0.0
            && self.filter_radius >= 0.0
            && self.distance_m >= 0.0
            && (0.0..=1.0).contains(&self.cascade_distribution)
            && self.cascade_count <= MAX_SHADOW_CASCADES as u32;
        if ok { Ok(self) } else { Err(SpaceError::BadLight) }
    }
}

/// View-space split distances. The first entry is the camera near. The last is the shadow distance.
pub fn practical_splits(near_m: f64, far_m: f64, count: u32, lambda: f64) -> Vec<f64> {
    let count = count.clamp(1, MAX_SHADOW_CASCADES as u32) as usize;
    let near = near_m.max(f64::from(SHADOW_NEAR_M));
    let far = far_m.max(near + 0.25);
    let lambda = lambda.clamp(0.0, 1.0);
    let mut splits = Vec::with_capacity(count + 1);
    splits.push(near);
    for index in 1..count {
        let t = index as f64 / count as f64;
        let uniform = near + (far - near) * t;
        let logarithmic = near * (far / near).powf(t);
        splits.push(uniform * (1.0 - lambda) + logarithmic * lambda);
    }
    splits.push(far);
    splits
}

/// One cascade. `clip` expects positions relative to `anchor`. Built in f64, uploaded as f32.
#[derive(Clone, Copy, Debug)]
pub struct CascadeSlice {
    pub clip: Mat4,
    pub anchor: Vec3,
    pub split_end_m: f32,
    pub half_extent_m: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct DirectionalCascadeSet {
    pub slices: [CascadeSlice; MAX_SHADOW_CASCADES],
    pub count: u32,
    pub fingerprint: u64,
}

impl Default for DirectionalCascadeSet {
    fn default() -> Self {
        let empty = CascadeSlice {
            clip: Mat4::IDENTITY,
            anchor: Vec3::ZERO,
            split_end_m: SHADOW_FAR_M,
            half_extent_m: DIRECTIONAL_HALF_EXTENT_M,
        };
        Self { slices: [empty; MAX_SHADOW_CASCADES], count: 0, fingerprint: 0 }
    }
}

/// Camera-relative cascades. Texel snap happens in light space, in f64, before the f32 matrix.
/// A sub-texel camera move keeps `fingerprint`. Casters and the light rotation are in it too.
pub fn fit_directional_cascades(
    camera: crate::ResolvedPose,
    light_rotation: Quat,
    fov_y_radians: f64,
    aspect: f64,
    near_m: f64,
    shadow_distance_m: f64,
    cascade_count: u32,
    lambda: f64,
    resolution: u32,
    world_revision: u64,
) -> Result<DirectionalCascadeSet, SpaceError> {
    let count = cascade_count.clamp(1, MAX_SHADOW_CASCADES as u32);
    let splits = practical_splits(near_m, shadow_distance_m, count, lambda);
    let mut set = DirectionalCascadeSet::default();
    set.count = count;
    let mut fingerprint = 0xC45C_ADE0_u64;
    fingerprint = mix_hash(fingerprint, world_revision);
    fingerprint = mix_hash(fingerprint, u64::from(count));
    fingerprint = mix_hash(fingerprint, quant_milli(lambda));
    for index in 0..count as usize {
        let start = splits[index];
        let mut end = splits[index + 1];
        if index + 1 < count as usize {
            let span = (splits[index + 1] - splits[index]).max(0.05);
            end = (end + span * f64::from(CASCADE_BLEND)).min(splits[splits.len() - 1] * 1.05);
        }
        let (center, radius) = frustum_slice_sphere(camera, fov_y_radians, aspect, start.max(f64::from(SHADOW_NEAR_M)), end.max(start + 0.05));
        let anchor = snap_light_anchor(center, radius, light_rotation, resolution);
        let half = radius.max(0.25) as f32;
        let clip = cascade_view_proj(light_rotation, half)?;
        set.slices[index] = CascadeSlice { clip, anchor, split_end_m: splits[index + 1] as f32, half_extent_m: half };
        fingerprint = mix_hash(fingerprint, quant_milli(anchor.x));
        fingerprint = mix_hash(fingerprint, quant_milli(anchor.y));
        fingerprint = mix_hash(fingerprint, quant_milli(anchor.z));
        fingerprint = mix_hash(fingerprint, quant_milli(f64::from(half)));
    }
    set.fingerprint = fingerprint;
    Ok(set)
}

fn cascade_view_proj(light_rotation: Quat, half_extent_m: f32) -> Result<Mat4, SpaceError> {
    let far = half_extent_m * 2.0 + 1.0;
    let view = Mat4::from_rotation_translation(light_rotation.conjugate(), [0.0, 0.0, -half_extent_m]);
    let projection = orthographic_reverse_z(half_extent_m, SHADOW_NEAR_M, far)?;
    Ok(projection.mul(view))
}

fn snap_light_anchor(center_world: Vec3, radius: f64, light_rotation: Quat, resolution: u32) -> Vec3 {
    let light_center = light_rotation.conjugate().rotate(center_world);
    let texel = (2.0 * radius.max(0.25)) / f64::from(resolution.max(1));
    let snapped = Vec3::new(
        (light_center.x / texel).round() * texel,
        (light_center.y / texel).round() * texel,
        (light_center.z / texel).round() * texel,
    );
    light_rotation.rotate(snapped)
}

fn frustum_slice_sphere(camera: crate::ResolvedPose, fov_y: f64, aspect: f64, near: f64, far: f64) -> (Vec3, f64) {
    let tan_y = (fov_y * 0.5).tan();
    let mut corners = [Vec3::ZERO; 8];
    let mut slot = 0;
    for distance in [near, far] {
        let half_y = distance * tan_y;
        let half_x = half_y * aspect.max(0.01);
        for y in [-half_y, half_y] {
            for x in [-half_x, half_x] {
                corners[slot] = camera.translation + camera.rotation.rotate(Vec3::new(x, y, -distance));
                slot += 1;
            }
        }
    }
    let mut center = Vec3::ZERO;
    for corner in corners {
        center = center + corner;
    }
    center = center.scale(1.0 / 8.0);
    let mut radius = 0.0f64;
    for corner in corners {
        let dx = corner.x - center.x;
        let dy = corner.y - center.y;
        let dz = corner.z - center.z;
        radius = radius.max((dx * dx + dy * dy + dz * dz).sqrt());
    }
    (center, radius.max(0.25) * 1.05)
}

fn quant_milli(value: f64) -> u64 {
    (value * 1000.0).round() as i64 as u64
}

fn mix_hash(hash: u64, value: u64) -> u64 {
    hash ^ value.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowMapClass {
    None = 0,
    Directional = 1,
    Spot = 2,
    Point = 3,
}

impl ShadowMapClass {
    pub fn gpu_tag(self) -> f32 {
        self as u32 as f32
    }
}

/// Finite reversed-Z. Near is 1. Far is 0. Distance is positive meters in front of the light.
pub fn shadow_ndc_perspective(distance_m: f32, near_m: f32, far_m: f32) -> f32 {
    if !(distance_m > 0.0) || !(far_m > near_m) || !(near_m > 0.0) {
        return 0.0;
    }
    let ndc = near_m * (far_m - distance_m) / ((far_m - near_m) * distance_m);
    ndc.clamp(0.0, 1.0)
}

pub fn shadow_ndc_orthographic(depth_m: f32, near_m: f32, far_m: f32) -> f32 {
    if !(far_m > near_m) {
        return 0.0;
    }
    ((far_m - depth_m) / (far_m - near_m)).clamp(0.0, 1.0)
}

/// Reversed-Z: the receiver is occluded when it is farther than the stored caster.
pub fn receiver_is_shadowed(receiver_ndc: f32, caster_ndc: f32, bias: f32) -> bool {
    receiver_ndc + bias < caster_ndc
}

pub fn perspective_reverse_z(fov_y_radians: f32, aspect: f32, near_m: f32, far_m: f32) -> Result<Mat4, SpaceError> {
    if !(fov_y_radians > 0.0) || !(aspect > 0.0) || !(near_m > 0.0) || !(far_m > near_m) {
        return Err(SpaceError::BadProjection);
    }
    let height = 1.0 / (fov_y_radians * 0.5).tan();
    let zn = near_m / (far_m - near_m);
    let zf = far_m * near_m / (far_m - near_m);
    Ok(Mat4 {
        cols: [[height / aspect, 0.0, 0.0, 0.0], [0.0, height, 0.0, 0.0], [0.0, 0.0, zn, -1.0], [0.0, 0.0, zf, 0.0]],
    })
}

pub fn orthographic_reverse_z(half_extent_m: f32, near_m: f32, far_m: f32) -> Result<Mat4, SpaceError> {
    if !(half_extent_m > 0.0) || !(near_m > 0.0) || !(far_m > near_m) {
        return Err(SpaceError::BadProjection);
    }
    let span = half_extent_m * 2.0;
    let depth = far_m - near_m;
    Ok(Mat4 {
        cols: [
            [2.0 / span, 0.0, 0.0, 0.0],
            [0.0, 2.0 / span, 0.0, 0.0],
            [0.0, 0.0, 1.0 / depth, 0.0],
            [0.0, 0.0, far_m / depth, 1.0],
        ],
    })
}

/// Light-relative view-projection. The anchor is the origin. The light looks along local −Z.
/// The camera is pulled back so the anchor sits in front of the near plane.
/// Ortho far plane. The light sits `DIRECTIONAL_HALF_EXTENT_M` behind the anchor.
pub fn directional_shadow_far_m() -> f32 {
    DIRECTIONAL_HALF_EXTENT_M * 2.0 + 1.0
}

pub fn directional_shadow_view_proj(light_rotation: Quat) -> Result<Mat4, SpaceError> {
    let back = DIRECTIONAL_HALF_EXTENT_M;
    let view = Mat4::from_rotation_translation(light_rotation.conjugate(), [0.0, 0.0, -back]);
    let projection = orthographic_reverse_z(DIRECTIONAL_HALF_EXTENT_M, SHADOW_NEAR_M, directional_shadow_far_m())?;
    Ok(projection.mul(view))
}

pub fn spot_shadow_view_proj(light_rotation: Quat, outer_radians: f32, far_m: f32) -> Result<Mat4, SpaceError> {
    let fov = (outer_radians * 2.0).clamp(0.05, std::f32::consts::PI - 0.05);
    let far = if far_m > SHADOW_NEAR_M { far_m } else { SHADOW_FAR_M };
    let view = Mat4::from_rotation(light_rotation.conjugate());
    let projection = perspective_reverse_z(fov, 1.0, SHADOW_NEAR_M, far)?;
    Ok(projection.mul(view))
}

pub fn point_shadow_face_view_proj(face: u32, far_m: f32) -> Result<Mat4, SpaceError> {
    let faces = reflection_cube_faces();
    let (forward, up) = faces[face as usize % 6];
    let rotation = rotation_looking_toward(forward, up)?;
    let far = if far_m > SHADOW_NEAR_M { far_m } else { SHADOW_FAR_M };
    let view = Mat4::from_rotation(rotation.conjugate());
    let projection = perspective_reverse_z(std::f32::consts::FRAC_PI_2, 1.0, SHADOW_NEAR_M, far)?;
    Ok(projection.mul(view))
}

/// `P_camera_relative + (camera - anchor)`, then the light matrix. No absolute f32 origin.
pub fn clip_matrix_for_camera(light_view_proj: Mat4, camera: Vec3, anchor: Vec3) -> Mat4 {
    let offset = camera_relative_f32(camera, anchor);
    let shift = Mat4::from_rotation_translation(Quat::IDENTITY, offset);
    light_view_proj.mul(shift)
}

pub fn light_relative_point(point: Vec3, anchor: Vec3) -> [f32; 3] {
    camera_relative_f32(point, anchor)
}

/// Column-major bytes. The first 112 bytes are the single-map record.
/// Cascades 1–3, split ends, half extents, and the filter packet follow.
/// `bias` is depth bias in meters. `pad[0]` is slope bias in meters. `pad[1]` is the
/// PCF/PCSS radius in texels. `pad[2]` is normal bias in meters.
/// `extra`: cascade count, blend fraction, PCSS flag (1 = on), light size (tan or meters).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuShadowRecord {
    pub clip_from_camera_relative: Mat4,
    pub atlas_origin: [f32; 2],
    pub atlas_scale: [f32; 2],
    pub kind: f32,
    pub enabled: f32,
    pub bias: f32,
    pub far_m: f32,
    pub near_m: f32,
    pub pad: [f32; 3],
    pub clip1: Mat4,
    pub clip2: Mat4,
    pub clip3: Mat4,
    pub splits: [f32; 4],
    pub half_extents: [f32; 4],
    pub extra: [f32; 4],
}

impl GpuShadowRecord {
    pub const BYTES: usize = 352;

    pub fn off() -> Self {
        Self {
            clip_from_camera_relative: Mat4::IDENTITY,
            atlas_origin: [0.0; 2],
            atlas_scale: [1.0; 2],
            kind: 0.0,
            enabled: 0.0,
            bias: SHADOW_DEPTH_BIAS_M,
            far_m: SHADOW_FAR_M,
            near_m: SHADOW_NEAR_M,
            pad: [SHADOW_SLOPE_BIAS_M, SHADOW_PCF_RADIUS, SHADOW_NORMAL_BIAS_M],
            clip1: Mat4::IDENTITY,
            clip2: Mat4::IDENTITY,
            clip3: Mat4::IDENTITY,
            splits: [SHADOW_FAR_M; 4],
            half_extents: [DIRECTIONAL_HALF_EXTENT_M; 4],
            extra: [1.0, CASCADE_BLEND, 0.0, 0.0],
        }
    }

    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut bytes = [0u8; Self::BYTES];
        self.clip_from_camera_relative.write_column_major(&mut bytes[0..64]);
        write2(&mut bytes[64..72], self.atlas_origin);
        write2(&mut bytes[72..80], self.atlas_scale);
        bytes[80..84].copy_from_slice(&self.kind.to_le_bytes());
        bytes[84..88].copy_from_slice(&self.enabled.to_le_bytes());
        bytes[88..92].copy_from_slice(&self.bias.to_le_bytes());
        bytes[92..96].copy_from_slice(&self.far_m.to_le_bytes());
        bytes[96..100].copy_from_slice(&self.near_m.to_le_bytes());
        bytes[100..104].copy_from_slice(&self.pad[0].to_le_bytes());
        bytes[104..108].copy_from_slice(&self.pad[1].to_le_bytes());
        bytes[108..112].copy_from_slice(&self.pad[2].to_le_bytes());
        self.clip1.write_column_major(&mut bytes[112..176]);
        self.clip2.write_column_major(&mut bytes[176..240]);
        self.clip3.write_column_major(&mut bytes[240..304]);
        write4(&mut bytes[304..320], self.splits);
        write4(&mut bytes[320..336], self.half_extents);
        write4(&mut bytes[336..352], self.extra);
        bytes
    }
}

fn write2(target: &mut [u8], value: [f32; 2]) {
    target[0..4].copy_from_slice(&value[0].to_le_bytes());
    target[4..8].copy_from_slice(&value[1].to_le_bytes());
}

fn write4(target: &mut [u8], value: [f32; 4]) {
    for (index, channel) in value.iter().enumerate() {
        let start = index * 4;
        target[start..start + 4].copy_from_slice(&channel.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ndc_z(matrix: Mat4, point: [f32; 3]) -> f32 {
        let clip = matrix.transform_point(point);
        clip[2] / clip[3]
    }

    #[test]
    fn reversed_z_puts_near_at_one_for_finite_shadow_projections() {
        let perspective = perspective_reverse_z(std::f32::consts::FRAC_PI_2, 1.0, 0.05, 40.0).unwrap();
        let near = ndc_z(perspective, [0.0, 0.0, -0.05]);
        let far = ndc_z(perspective, [0.0, 0.0, -40.0]);
        assert!((near - 1.0).abs() < 1.0e-4, "{near}");
        assert!(far.abs() < 1.0e-3, "{far}");
        assert!((shadow_ndc_perspective(0.05, 0.05, 40.0) - 1.0).abs() < 1.0e-4);
        assert!(shadow_ndc_perspective(40.0, 0.05, 40.0).abs() < 1.0e-4);
        let ortho = orthographic_reverse_z(20.0, 0.05, 41.0).unwrap();
        assert!((ndc_z(ortho, [0.0, 0.0, -0.05]) - 1.0).abs() < 1.0e-4);
        assert!(ndc_z(ortho, [0.0, 0.0, -41.0]).abs() < 1.0e-3);
    }

    #[test]
    fn a_farther_receiver_is_shadowed_and_the_caster_surface_is_not() {
        let caster = shadow_ndc_perspective(4.0, SHADOW_NEAR_M, SHADOW_FAR_M);
        let farther = shadow_ndc_perspective(8.0, SHADOW_NEAR_M, SHADOW_FAR_M);
        let closer = shadow_ndc_perspective(2.0, SHADOW_NEAR_M, SHADOW_FAR_M);
        assert!(caster > farther);
        assert!(receiver_is_shadowed(farther, caster, SHADOW_DEPTH_BIAS));
        assert!(!receiver_is_shadowed(closer, caster, SHADOW_DEPTH_BIAS));
        assert!(!receiver_is_shadowed(caster, caster, SHADOW_DEPTH_BIAS));
    }

    #[test]
    fn directional_depth_ignores_the_camera_and_stays_meter_scale_at_a_billion_meters() {
        let light = Quat::IDENTITY;
        let map = directional_shadow_view_proj(light).unwrap();
        let anchor = Vec3::new(1.0e9, 0.0, -3.0);
        let caster = Vec3::new(1.0e9, 0.0, -5.0);
        let receiver = Vec3::new(1.0e9, 0.0, -8.0);
        let caster_z = ndc_z(map, light_relative_point(caster, anchor));
        let receiver_z = ndc_z(map, light_relative_point(receiver, anchor));
        assert!(caster_z.is_finite() && receiver_z.is_finite());
        assert!(light_relative_point(caster, anchor).iter().all(|value| value.abs() < 100.0));
        assert!(receiver_is_shadowed(receiver_z, caster_z, SHADOW_DEPTH_BIAS));
        let camera_a = Vec3::new(1.0e9 + 2.0, 1.0, 4.0);
        let camera_b = Vec3::new(1.0e9 - 30.0, 8.0, -12.0);
        let sample_a = clip_matrix_for_camera(map, camera_a, anchor);
        let sample_b = clip_matrix_for_camera(map, camera_b, anchor);
        assert_ne!(sample_a.cols[3], sample_b.cols[3]);
        let rel_a = camera_relative_f32(caster, camera_a);
        let rel_b = camera_relative_f32(caster, camera_b);
        let za = ndc_z(sample_a, rel_a);
        let zb = ndc_z(sample_b, rel_b);
        assert!((za - caster_z).abs() < 1.0e-4, "{za} {caster_z}");
        assert!((zb - caster_z).abs() < 1.0e-4, "{zb} {caster_z}");
        assert!(sample_a.cols.iter().flatten().all(|value| value.is_finite() && value.abs() < 1.0e6));
    }

    #[test]
    fn spot_and_point_faces_keep_a_forward_point_in_front() {
        let spot = spot_shadow_view_proj(Quat::IDENTITY, 0.75, 40.0).unwrap();
        let ahead = ndc_z(spot, [0.0, 0.0, -4.0]);
        assert!(ahead > 0.0 && ahead < 1.0, "{ahead}");
        let face = point_shadow_face_view_proj(5, 40.0).unwrap();
        let on_minus_z = ndc_z(face, [0.0, 0.0, -4.0]);
        let farther = ndc_z(face, [0.0, 0.0, -16.0]);
        assert!(on_minus_z > farther && on_minus_z < 1.0, "{on_minus_z} {farther}");
    }

    fn distance_from_shadow_ndc(ndc: f32) -> f32 {
        let span = SHADOW_FAR_M - SHADOW_NEAR_M;
        SHADOW_NEAR_M * SHADOW_FAR_M / (ndc * span + SHADOW_NEAR_M)
    }

    #[test]
    fn meter_bias_stays_millimeters_on_a_tight_cascade_and_on_a_point_light() {
        let half = 2.0f32;
        let far = half * 2.0 + 1.0;
        let facing = shadow_bias_ndc(ShadowMapClass::Directional, SHADOW_DEPTH_BIAS_M, 0.0, 1.0, 2.0, SHADOW_NEAR_M, far);
        let facing_m = facing * (far - SHADOW_NEAR_M);
        assert!(facing_m > 0.0015 && facing_m < 0.0025, "facing cascade bias {facing_m} m");
        let grazing = shadow_bias_ndc(ShadowMapClass::Directional, SHADOW_DEPTH_BIAS_M, SHADOW_SLOPE_BIAS_M, 0.0, 2.0, SHADOW_NEAR_M, far);
        let grazing_m = grazing * (far - SHADOW_NEAR_M);
        assert!((grazing_m - (SHADOW_DEPTH_BIAS_M + SHADOW_SLOPE_BIAS_M)).abs() < 1.0e-4, "{grazing_m}");
        let distance = 3.44;
        let stored = shadow_ndc_perspective(distance, SHADOW_NEAR_M, SHADOW_FAR_M);
        let point = shadow_bias_ndc(ShadowMapClass::Point, SHADOW_DEPTH_BIAS_M, SHADOW_SLOPE_BIAS_M, 0.54, distance, SHADOW_NEAR_M, SHADOW_FAR_M);
        let shift = distance - distance_from_shadow_ndc(stored + point);
        assert!(shift > 0.001 && shift < 0.03, "point contact shift {shift} m");
    }

    #[test]
    fn cascades_are_practical_texel_stable_and_finite_at_a_billion_meters() {
        let splits = practical_splits(0.1, 40.0, 4, 0.5);
        assert_eq!(splits.len(), 5);
        assert!((splits[0] - 0.1).abs() < 1.0e-6);
        assert!((splits[4] - 40.0).abs() < 1.0e-4);
        for pair in splits.windows(2) {
            assert!(pair[1] > pair[0]);
        }
        let uniform = practical_splits(0.1, 40.0, 4, 0.0);
        let logarithmic = practical_splits(0.1, 40.0, 4, 1.0);
        assert!(splits[1] > logarithmic[1] && splits[1] < uniform[1]);
        let camera = crate::ResolvedPose {
            translation: Vec3::new(1.0e9, 2.0, -4.0),
            rotation: Quat::IDENTITY,
        };
        let fitted = fit_directional_cascades(camera, Quat::IDENTITY, 60.0_f64.to_radians(), 16.0 / 9.0, 0.1, 40.0, 4, 0.5, CASCADE_RESOLUTION, 1).unwrap();
        assert_eq!(fitted.count, 4);
        assert!(fitted.slices[0].split_end_m < fitted.slices[1].split_end_m);
        assert!(fitted.slices[0].half_extent_m < fitted.slices[3].half_extent_m);
        for slice in &fitted.slices[..4] {
            assert!(slice.clip.cols.iter().flatten().all(|value| value.is_finite() && value.abs() < 1.0e6));
            let relative = light_relative_point(slice.anchor, camera.translation);
            assert!(relative.iter().all(|value| value.is_finite() && value.abs() < 100.0), "{relative:?}");
        }
        let forward = crate::ResolvedPose {
            translation: Vec3::new(camera.translation.x + 1.0e-5, camera.translation.y, camera.translation.z),
            rotation: camera.rotation,
        };
        let back = crate::ResolvedPose {
            translation: Vec3::new(camera.translation.x - 1.0e-5, camera.translation.y, camera.translation.z),
            rotation: camera.rotation,
        };
        let still_forward = fit_directional_cascades(forward, Quat::IDENTITY, 60.0_f64.to_radians(), 16.0 / 9.0, 0.1, 40.0, 4, 0.5, CASCADE_RESOLUTION, 1).unwrap();
        let still_back = fit_directional_cascades(back, Quat::IDENTITY, 60.0_f64.to_radians(), 16.0 / 9.0, 0.1, 40.0, 4, 0.5, CASCADE_RESOLUTION, 1).unwrap();
        assert!(
            still_forward.fingerprint == fitted.fingerprint || still_back.fingerprint == fitted.fingerprint,
            "a 0.01 mm camera step crossed every cascade texel"
        );
        let flown = crate::ResolvedPose {
            translation: Vec3::new(camera.translation.x + 6.0, camera.translation.y + 1.0, camera.translation.z - 4.0),
            rotation: camera.rotation,
        };
        let moved = fit_directional_cascades(flown, Quat::IDENTITY, 60.0_f64.to_radians(), 16.0 / 9.0, 0.1, 40.0, 4, 0.5, CASCADE_RESOLUTION, 1).unwrap();
        assert_ne!(moved.fingerprint, fitted.fingerprint);
        let casters = fit_directional_cascades(camera, Quat::IDENTITY, 60.0_f64.to_radians(), 16.0 / 9.0, 0.1, 40.0, 4, 0.5, CASCADE_RESOLUTION, 2).unwrap();
        assert_ne!(casters.fingerprint, fitted.fingerprint);
        assert!(casts_into_shadow_map(12));
        assert!(casts_into_shadow_map(SHADOW_CASTER_TRIANGLE_LIMIT));
        assert!(!casts_into_shadow_map(SHADOW_CASTER_TRIANGLE_LIMIT + 1));
        assert!(!casts_into_shadow_map(6_122_214));
        assert_eq!(GpuShadowRecord::BYTES, 352);
        assert!(LightShadowSettings::default().finite().is_ok());
        assert!(LightShadowSettings { cascade_distribution: 1.5, ..LightShadowSettings::default() }.finite().is_err());
    }
}
