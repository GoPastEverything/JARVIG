//! Canonical surface properties. Not lights, not WGSL, and not a GPU resource.
//!
//! A StandardMetalRough graph writes these. The shading model, not the graph, decides
//! how they meet a light. JRV-0054 owns lights.

use crate::TextureSemantic;

/// Opaque surface. Alpha stays 1 until a blend mode exists for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialSurface {
    pub base_color: [f32; 3],
    pub metallic: f32,
    pub perceptual_roughness: f32,
    pub normal_ts: [f32; 3],
    pub ambient_occlusion: f32,
    pub emissive: [f32; 3],
}

pub const DEFAULT_BASE_COLOR: [f32; 3] = [1.0, 1.0, 1.0];
pub const DEFAULT_METALLIC: f32 = 0.0;
pub const DEFAULT_ROUGHNESS: f32 = 0.5;
pub const DEFAULT_NORMAL_TS: [f32; 3] = [0.0, 0.0, 1.0];
pub const DEFAULT_AMBIENT_OCCLUSION: f32 = 1.0;
pub const DEFAULT_EMISSIVE: [f32; 3] = [0.0, 0.0, 0.0];

impl Default for MaterialSurface {
    fn default() -> Self {
        Self {
            base_color: DEFAULT_BASE_COLOR,
            metallic: DEFAULT_METALLIC,
            perceptual_roughness: DEFAULT_ROUGHNESS,
            normal_ts: DEFAULT_NORMAL_TS,
            ambient_occlusion: DEFAULT_AMBIENT_OCCLUSION,
            emissive: DEFAULT_EMISSIVE,
        }
    }
}

impl MaterialSurface {
    pub fn sanitized(self) -> Self {
        Self {
            base_color: self.base_color,
            metallic: self.metallic.clamp(0.0, 1.0),
            perceptual_roughness: self.perceptual_roughness.clamp(0.0, 1.0),
            normal_ts: self.normal_ts,
            ambient_occlusion: self.ambient_occlusion.clamp(0.0, 1.0),
            emissive: self.emissive,
        }
    }
}

impl TextureSemantic {
    /// Base color and emissive are sRGB color. Every data map, including height, is linear.
    pub fn color_space(self) -> crate::ColorSpace {
        match self {
            Self::BaseColor | Self::Emissive | Self::UnlitColor => crate::ColorSpace::Srgb,
            Self::Metallic | Self::Roughness | Self::AmbientOcclusion | Self::Normal | Self::Height | Self::Orm | Self::Data => {
                crate::ColorSpace::Linear
            }
        }
    }

    /// Height is linear data for a later displacement path. It is not a BRDF input.
    pub fn feeds_standard_brdf(self) -> bool {
        !matches!(self, Self::Height | Self::Data | Self::UnlitColor)
    }
}
