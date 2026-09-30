//! Display mapping for one view. Not a material and not a scene object.
//!
//! Scene light stays linear. This curve runs once, in the output pass.
//! It is Krzysztof Narkowicz's fitted ACES approximation, not the Academy
//! ACES RRT and ODT. Negatives are clamped to zero before the curve.
//! The result is linear display-referred RGB. An sRGB swapchain encodes it.
//! This module does not apply `pow(1/2.2)`.

/// Editor exposure stops. Zero is the baseline. Plus one is twice as bright.
pub const EXPOSURE_EV_MIN: f32 = -16.0;
pub const EXPOSURE_EV_MAX: f32 = 16.0;

pub fn exposure_multiplier(ev: f32) -> f32 {
    if !ev.is_finite() {
        return 1.0;
    }
    ev.clamp(EXPOSURE_EV_MIN, EXPOSURE_EV_MAX).exp2()
}

/// Fitted ACES, one channel. Monotonic for nonnegative input. Output is in 0..1.
#[cfg_attr(not(test), allow(dead_code))]
pub fn aces_fitted(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let x = x.max(0.0);
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    let mapped = (x * (a * x + b)) / (x * (c * x + d) + e);
    mapped.clamp(0.0, 1.0)
}

/// What the output pass shows. View state. Not saved in the level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PresentationMode {
    /// Tone curve, then optional dither. The swapchain still encodes sRGB.
    #[default]
    Tonemap = 0,
    /// Same curve, then an explicit 8-bit sRGB step before the swapchain.
    Quantized = 1,
    /// Exposure only, clamped to 0..1. No tone curve. The swapchain is still 8-bit.
    BeforeCurve = 2,
}

impl PresentationMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tonemap => "tonemap",
            Self::Quantized => "8bit",
            Self::BeforeCurve => "before-curve",
        }
    }

    pub(crate) fn as_f32(self) -> f32 {
        match self {
            Self::Tonemap => 0.0,
            Self::Quantized => 1.0,
            Self::BeforeCurve => 2.0,
        }
    }
}

pub const OUTPUT_SHADER: &str = r#"// JARVIG display pass. Linear HDR in. Linear display-referred RGB out.
// The sRGB swapchain encodes. This shader does not apply a display gamma on top of that.
// Dither is triangular noise in that encoding, then converted back to linear.
// Interleaved-gradient noise is not used. Without temporal AA it draws diagonal bands.
// Narkowicz fitted ACES. Not the Academy ACES RRT/ODT.
// Negatives are clamped to 0 before the curve. Alpha is 1 and is not tone-mapped.
// presentation: 0 tonemap, 1 explicit 8-bit step, 2 exposure only.
struct OutputParams {
    exposure: f32,
    dither_enabled: f32,
    presentation: f32,
    pad2: f32,
}
@group(0) @binding(0) var hdr_scene: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var<uniform> params: OutputParams;

@vertex
fn vs(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

fn jarvig_srgb_encode(linear: f32) -> f32 {
    let c = max(linear, 0.0);
    if (c <= 0.0031308) { return c * 12.92; }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

fn jarvig_srgb_decode(encoded: f32) -> f32 {
    let c = clamp(encoded, 0.0, 1.0);
    if (c <= 0.04045) { return c / 12.92; }
    return pow((c + 0.055) / 1.055, 2.4);
}

fn jarvig_hash(p: vec2<f32>) -> f32 {
    let p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    let h = p3 + dot(p3, p3.yzx + 33.33);
    return fract((h.x + h.y) * h.z);
}

fn jarvig_display_dither(pixel: vec2<f32>) -> f32 {
    // Two independent hashes. Their difference is triangular and has no diagonal weave.
    let a = jarvig_hash(pixel);
    let b = jarvig_hash(pixel + vec2<f32>(19.19, 73.13));
    return a + b - 1.0;
}

fn jarvig_present_channel(linear: f32, dither: f32, quantize: bool) -> f32 {
    var code = jarvig_srgb_encode(linear) + dither / 255.0;
    code = clamp(code, 0.0, 1.0);
    if (quantize) { code = floor(code * 255.0 + 0.5) / 255.0; }
    return jarvig_srgb_decode(code);
}

@fragment
fn fs(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(hdr_scene));
    let uv = position.xy / max(size, vec2<f32>(1.0));
    let hdr = textureSample(hdr_scene, hdr_sampler, uv).rgb;
    let exposed = max(hdr, vec3<f32>(0.0)) * params.exposure;
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    let tone = clamp((exposed * (a * exposed + b)) / (exposed * (c * exposed + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
    var linear = tone;
    if (params.presentation > 1.5) { linear = clamp(exposed, vec3<f32>(0.0), vec3<f32>(1.0)); }
    let quantize = params.presentation > 0.5 && params.presentation < 1.5;
    var dither = 0.0;
    if (params.dither_enabled > 0.5) { dither = jarvig_display_dither(position.xy); }
    let mapped = vec3<f32>(
        jarvig_present_channel(linear.r, dither, quantize),
        jarvig_present_channel(linear.g, dither, quantize),
        jarvig_present_channel(linear.b, dither, quantize)
    );
    return vec4<f32>(mapped, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_is_monotonic_and_compresses_highlights() {
        assert_eq!(aces_fitted(0.0), 0.0);
        assert_eq!(aces_fitted(-4.0), 0.0);
        assert_eq!(aces_fitted(f32::NAN), 0.0);
        let samples = [0.0, 0.01, 0.18, 1.0, 2.0, 8.0, 32.0, 64.0, 10_000.0];
        for pair in samples.windows(2) {
            let left = aces_fitted(pair[0]);
            let right = aces_fitted(pair[1]);
            assert!(left.is_finite() && right.is_finite(), "{pair:?}");
            assert!(right + 1.0e-5 >= left, "{pair:?} decreased");
            assert!(right <= 1.0);
        }
        assert!(aces_fitted(2.0) > aces_fitted(1.0));
        assert!(aces_fitted(8.0) > aces_fitted(2.0));
        assert!(aces_fitted(10_000.0) <= 1.0);
        assert!(aces_fitted(8.0) - aces_fitted(2.0) > 0.02);
        assert!(!OUTPUT_SHADER.contains("2.2"));
        assert!(OUTPUT_SHADER.contains("2.51"));
        assert!(OUTPUT_SHADER.contains("jarvig_display_dither"));
        assert!(!OUTPUT_SHADER.contains("0.06711056"));
        assert!(OUTPUT_SHADER.contains("mapped, 1.0"));
    }

    #[test]
    fn one_stop_doubles_the_linear_exposure() {
        let base = exposure_multiplier(0.0);
        assert!((base - 1.0).abs() < 1.0e-6);
        assert!((exposure_multiplier(1.0) / base - 2.0).abs() < 1.0e-5);
        assert!((exposure_multiplier(-1.0) / base - 0.5).abs() < 1.0e-5);
        assert!((exposure_multiplier(2.0) - 4.0).abs() < 1.0e-5);
        assert!(exposure_multiplier(EXPOSURE_EV_MIN).is_finite());
        assert!(exposure_multiplier(EXPOSURE_EV_MAX).is_finite());
        assert_eq!(exposure_multiplier(1_000.0), exposure_multiplier(EXPOSURE_EV_MAX));
        let exposed = [0.5_f32, 1.0, 2.0];
        for pair in exposed.windows(2) {
            assert!(pair[1] > pair[0]);
            assert!(aces_fitted(pair[1] * 4.0) > aces_fitted(pair[0] * 4.0));
        }
    }
}
