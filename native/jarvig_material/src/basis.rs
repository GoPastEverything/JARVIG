//! Tangent frame and normal-map decode.
//!
//! Directions only. Translation never enters. The normal matrix is the inverse
//! transpose of the object's linear 3x3.
//!
//! The shader bitangent is `cross(normal, tangent) * tangent.w`. It does not flip Y.
//! On the floor, handedness is +1 and that bitangent points opposite texture +V, so a
//! positive green channel tilts toward −V. The sphere stores handedness −1 so its
//! bitangent does the same. That is the DirectX normal-map convention. It is a
//! property of this tangent frame, not of the graphics API. An OpenGL map matches
//! only after its green channel is negated.

/// Column-major 3x3. No translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3 {
    pub cols: [[f32; 3]; 3],
}

impl Mat3 {
    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        Self { cols: [[x, 0.0, 0.0], [0.0, y, 0.0], [0.0, 0.0, z]] }
    }

    pub fn rotation_y(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        Self { cols: [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]] }
    }

    pub fn mul_vec(self, v: [f32; 3]) -> [f32; 3] {
        [
            self.cols[0][0] * v[0] + self.cols[1][0] * v[1] + self.cols[2][0] * v[2],
            self.cols[0][1] * v[0] + self.cols[1][1] * v[1] + self.cols[2][1] * v[2],
            self.cols[0][2] * v[0] + self.cols[1][2] * v[1] + self.cols[2][2] * v[2],
        ]
    }
}

pub fn normal_matrix(linear: Mat3) -> Mat3 {
    transpose(inverse(linear))
}

/// Visible-side normal for a two-sided material. One-sided materials do not call this.
/// A back face negates the geometric normal and the tangent handedness before the TBN.
pub fn apply_visible_side(geometric: [f32; 3], tangent: [f32; 4], front_facing: bool) -> ([f32; 3], [f32; 4]) {
    if front_facing {
        (geometric, tangent)
    } else {
        (
            [-geometric[0], -geometric[1], -geometric[2]],
            [tangent[0], tangent[1], tangent[2], -tangent[3]],
        )
    }
}

/// `tangent.w` is handedness. The result is the render-space normal.
pub fn shade_normal(linear: Mat3, geometric: [f32; 3], tangent: [f32; 4], normal_ts: [f32; 3]) -> [f32; 3] {
    let n = normalize(normal_matrix(linear).mul_vec(geometric));
    let t_raw = normalize(linear.mul_vec([tangent[0], tangent[1], tangent[2]]));
    let t = normalize(sub(t_raw, scale(n, dot(t_raw, n))));
    let b = scale(cross(n, t), tangent[3]);
    normalize(add(add(scale(t, normal_ts[0]), scale(b, normal_ts[1])), scale(n, normal_ts[2])))
}

pub fn decode_normal(encoded: [f32; 3], normal_scale: f32) -> [f32; 3] {
    let raw = [encoded[0] * 2.0 - 1.0, encoded[1] * 2.0 - 1.0, encoded[2] * 2.0 - 1.0];
    normalize([raw[0] * normal_scale, raw[1] * normal_scale, raw[2]])
}

/// Camera-relative position. The camera is at the render origin, so the view
/// vector does not carry the billion-meter world translation.
pub fn view_direction(render_position: [f32; 3]) -> [f32; 3] {
    normalize(scale(render_position, -1.0))
}

fn inverse(m: Mat3) -> Mat3 {
    let c0 = m.cols[0];
    let c1 = m.cols[1];
    let c2 = m.cols[2];
    let a = cross(c1, c2);
    let det = dot(c0, a);
    let inv_det = 1.0 / det.max(1.0e-8);
    let r0 = scale(a, inv_det);
    let r1 = scale(cross(c2, c0), inv_det);
    let r2 = scale(cross(c0, c1), inv_det);
    // Rows of the inverse are r0, r1, r2. Store as columns.
    Mat3 { cols: [[r0[0], r1[0], r2[0]], [r0[1], r1[1], r2[1]], [r0[2], r1[2], r2[2]]] }
}

fn transpose(m: Mat3) -> Mat3 {
    Mat3 {
        cols: [
            [m.cols[0][0], m.cols[1][0], m.cols[2][0]],
            [m.cols[0][1], m.cols[1][1], m.cols[2][1]],
            [m.cols[0][2], m.cols[1][2], m.cols[2][2]],
        ],
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    scale(v, 1.0 / dot(v, v).sqrt().max(1.0e-8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len(v: [f32; 3]) -> f32 {
        dot(v, v).sqrt()
    }

    #[test]
    fn normals_ignore_translation_and_respect_scale_and_handedness() {
        let flat = decode_normal([0.5, 0.5, 1.0], 1.0);
        assert!(flat[0].abs() < 1.0e-5 && flat[1].abs() < 1.0e-5);
        assert!((flat[2] - 1.0).abs() < 1.0e-5);
        let tilted = decode_normal([1.0, 0.5, 1.0], 1.0);
        let flattened = decode_normal([1.0, 0.5, 1.0], 0.0);
        assert!(tilted[0] > 0.4);
        assert!(flattened[0].abs() < 1.0e-5);
        let linear = Mat3::rotation_y(std::f32::consts::FRAC_PI_2);
        let turned = shade_normal(linear, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert!((turned[0] - 1.0).abs() < 1.0e-4, "{turned:?}");
        assert!(turned[2].abs() < 1.0e-4);
        let scaled = shade_normal(Mat3::scale(2.0, 1.0, 1.0), [1.0, 1.0, 0.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        let naive = normalize(Mat3::scale(2.0, 1.0, 1.0).mul_vec([1.0, 1.0, 0.0]));
        assert!((scaled[0] - naive[0]).abs() > 0.05);
        let here = shade_normal(Mat3::scale(1.0, 1.0, 1.0), [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        let far = shade_normal(Mat3::scale(1.0, 1.0, 1.0), [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert_eq!(here, far);
        let positive = shade_normal(Mat3::scale(1.0, 1.0, 1.0), [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        let negative = shade_normal(Mat3::scale(1.0, 1.0, 1.0), [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0, 0.0]);
        assert!(positive[1] > 0.5);
        assert!(negative[1] < -0.5);
        assert!((len(here) - 1.0).abs() < 1.0e-4);
        // Floor frame: N = +Y, T = +X, w = +1, B = cross(N, T) = −Z. Texture +V is +Z.
        // A bump toward +V must end with a positive world Z. DirectX stores that as negative green.
        // OpenGL stores it as positive green, which points the wrong way until green is negated.
        let model = Mat3::scale(1.0, 1.0, 1.0);
        let floor_n = [0.0, 1.0, 0.0];
        let floor_t = [1.0, 0.0, 0.0, 1.0];
        let gl_positive_v = decode_normal([0.5, 0.7, 1.0], 1.0);
        let dx_positive_v = decode_normal([0.5, 1.0 - 0.7, 1.0], 1.0);
        let shaded_gl = shade_normal(model, floor_n, floor_t, gl_positive_v);
        let shaded_dx = shade_normal(model, floor_n, floor_t, dx_positive_v);
        assert!(shaded_dx[2] > 0.05, "DirectX +V bump should tilt toward +Z, got {shaded_dx:?}");
        assert!(shaded_gl[2] < -0.05, "OpenGL +V bump is inverted on this frame, got {shaded_gl:?}");
        assert!((shaded_dx[0] - shaded_gl[0]).abs() < 1.0e-4);
        assert!((shaded_dx[1] - shaded_gl[1]).abs() < 1.0e-4);
        assert!((shaded_dx[2] + shaded_gl[2]).abs() < 1.0e-4);
        let flat_strength = shade_normal(model, floor_n, floor_t, decode_normal([0.5, 0.3, 1.0], 0.0));
        assert!(flat_strength[1] > 0.99, "strength 0 is the geometric normal, got {flat_strength:?}");
        // Sphere equator at +X. T = +Z, w = -1. Texture +V is -Y.
        // cross(+X, +Z) = -Y, times -1, so the shader bitangent is +Y, opposite +V.
        let sphere_n = [1.0, 0.0, 0.0];
        let sphere_t = [0.0, 0.0, 1.0, -1.0];
        let sphere_dx = shade_normal(model, sphere_n, sphere_t, dx_positive_v);
        let sphere_gl = shade_normal(model, sphere_n, sphere_t, gl_positive_v);
        assert!(sphere_dx[1] < -0.05, "DirectX +V on the sphere should tilt toward -Y, got {sphere_dx:?}");
        assert!(sphere_gl[1] > 0.05, "OpenGL +V is inverted on this frame, got {sphere_gl:?}");
        assert!((sphere_dx[0] - sphere_gl[0]).abs() < 1.0e-4);
        assert!((sphere_dx[1] + sphere_gl[1]).abs() < 1.0e-4);
        let toward = view_direction([0.0, 0.0, -2.0]);
        assert!(toward[2] > 0.9);
        let far_view = view_direction([0.0, 0.0, -2.0]);
        let _billion_is_removed_before_this_call = 1.0e9_f32;
        assert_eq!(toward, far_view);
    }
}
