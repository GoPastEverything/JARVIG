//! Staging PBR sets. A name such as `Tiles101` is the identity. Pixels are not.
//!
//! The folder on disk is `{name}_4K-PNG`. The level stores the name, not a path and not
//! the PNG bytes. A later asset database can resolve the same name.

use jarvig_material::TextureSemantic;

/// Tangent-space green channel. Not a vendor renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalConvention {
    /// Green points along texture +V. OpenGL.
    OpenGlPositiveY,
    /// Green points along texture −V. DirectX.
    DirectXNegativeY,
}

impl NormalConvention {
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenGlPositiveY => "opengl-positive-y",
            Self::DirectXNegativeY => "directx-negative-y",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "opengl-positive-y" => Some(Self::OpenGlPositiveY),
            "directx-negative-y" => Some(Self::DirectXNegativeY),
            _ => None,
        }
    }
}

/// One file in a set. Height is recorded and is not a BRDF input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialMapRole {
    Preview,
    BaseColor,
    Roughness,
    Metallic,
    AmbientOcclusion,
    Normal(NormalConvention),
    Height,
    Emissive,
    Sidecar,
}

/// JARVIG's mesh bitangent is `cross(normal, tangent)` with handedness +1.
/// On the floor that bitangent points opposite texture +V, so a DirectX normal
/// matches the shader without negating green. An OpenGL map would need that negate.
pub fn engine_normal_convention() -> NormalConvention {
    NormalConvention::DirectXNegativeY
}

/// Classify one filename. `set` is `Tiles101`, not the folder and not a path.
pub fn classify_material_file(set: &str, file_name: &str) -> Option<MaterialMapRole> {
    let file_name = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    if file_name.eq_ignore_ascii_case(&format!("{set}.png")) {
        return Some(MaterialMapRole::Preview);
    }
    let stem = file_name.strip_suffix(".png").or_else(|| file_name.strip_suffix(".PNG"))?;
    let prefix = format!("{set}_4K-PNG_");
    let role = stem.strip_prefix(&prefix).or_else(|| {
        // The preview stem has no role. Sidecars use the folder name without a role suffix.
        None
    })?;
    Some(match role {
        "Color" => MaterialMapRole::BaseColor,
        "Roughness" => MaterialMapRole::Roughness,
        "Metalness" => MaterialMapRole::Metallic,
        "AmbientOcclusion" => MaterialMapRole::AmbientOcclusion,
        "NormalDX" => MaterialMapRole::Normal(NormalConvention::DirectXNegativeY),
        "NormalGL" => MaterialMapRole::Normal(NormalConvention::OpenGlPositiveY),
        "Displacement" => MaterialMapRole::Height,
        "Emission" => MaterialMapRole::Emissive,
        _ => return None,
    })
}

pub fn is_material_sidecar(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".mtlx") || lower.ends_with(".tres") || lower.ends_with(".blend") || lower.ends_with(".usdc")
}

/// Which normal file the shader should sample, and whether ingest must negate green.
///
/// DirectX is preferred because it matches [`engine_normal_convention`] with no flip.
/// OpenGL is used only when DirectX is absent, and then green is negated once at ingest.
pub fn select_normal(roles: &[MaterialMapRole]) -> Option<(NormalConvention, bool)> {
    let has_dx = roles.iter().any(|role| matches!(role, MaterialMapRole::Normal(NormalConvention::DirectXNegativeY)));
    let has_gl = roles.iter().any(|role| matches!(role, MaterialMapRole::Normal(NormalConvention::OpenGlPositiveY)));
    if has_dx {
        Some((NormalConvention::DirectXNegativeY, false))
    } else if has_gl {
        Some((NormalConvention::OpenGlPositiveY, true))
    } else {
        None
    }
}

pub fn role_color_space(role: MaterialMapRole) -> jarvig_material::ColorSpace {
    match role {
        MaterialMapRole::BaseColor | MaterialMapRole::Emissive | MaterialMapRole::Preview => jarvig_material::ColorSpace::Srgb,
        MaterialMapRole::Normal(_) | MaterialMapRole::Roughness | MaterialMapRole::Metallic | MaterialMapRole::AmbientOcclusion | MaterialMapRole::Height => {
            jarvig_material::ColorSpace::Linear
        }
        MaterialMapRole::Sidecar => jarvig_material::ColorSpace::Linear,
    }
}

pub fn role_semantic(role: MaterialMapRole) -> Option<TextureSemantic> {
    match role {
        MaterialMapRole::BaseColor => Some(TextureSemantic::BaseColor),
        MaterialMapRole::Roughness => Some(TextureSemantic::Roughness),
        MaterialMapRole::Metallic => Some(TextureSemantic::Metallic),
        MaterialMapRole::AmbientOcclusion => Some(TextureSemantic::AmbientOcclusion),
        MaterialMapRole::Normal(_) => Some(TextureSemantic::Normal),
        MaterialMapRole::Height => Some(TextureSemantic::Height),
        MaterialMapRole::Emissive => Some(TextureSemantic::Emissive),
        MaterialMapRole::Preview | MaterialMapRole::Sidecar => None,
    }
}

/// Packed ORM. R is AO, G is roughness, B is metallic. Missing AO is 1. Missing roughness is 0.5.
/// Missing metallic is 0. Height is not a channel.
pub fn pack_orm_rgba8(width: u32, height: u32, ao: Option<&[u8]>, roughness: Option<&[u8]>, metallic: Option<&[u8]>) -> Result<Vec<u8>, &'static str> {
    let pixels = width as usize * height as usize;
    let expect = pixels * 4;
    for source in [ao, roughness, metallic].into_iter().flatten() {
        if source.len() != expect {
            return Err("orm source size does not match");
        }
    }
    let mut packed = vec![0u8; expect];
    for index in 0..pixels {
        let ao_b = ao.map(|bytes| bytes[index * 4]).unwrap_or(255);
        let rough_b = roughness.map(|bytes| bytes[index * 4]).unwrap_or(128);
        let metal_b = metallic.map(|bytes| bytes[index * 4]).unwrap_or(0);
        packed[index * 4] = ao_b;
        packed[index * 4 + 1] = rough_b;
        packed[index * 4 + 2] = metal_b;
        packed[index * 4 + 3] = 255;
    }
    Ok(packed)
}

/// Negate the green channel of an RGBA8 normal map. Used only when the source is OpenGL and DirectX is absent.
pub fn negate_normal_green(pixels: &mut [u8]) {
    let mut index = 1;
    while index < pixels.len() {
        pixels[index] = 255 - pixels[index];
        index += 4;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambientcg_names_map_to_roles_and_directx_is_sampled_without_a_flip() {
        let set = "Tiles101";
        let files = [
            "Tiles101.png",
            "Tiles101_4K-PNG_Color.png",
            "Tiles101_4K-PNG_AmbientOcclusion.png",
            "Tiles101_4K-PNG_Displacement.png",
            "Tiles101_4K-PNG_NormalDX.png",
            "Tiles101_4K-PNG_NormalGL.png",
            "Tiles101_4K-PNG_Roughness.png",
            "Tiles101_4K-PNG.mtlx",
        ];
        let roles: Vec<_> = files.iter().filter_map(|file| classify_material_file(set, file)).collect();
        assert!(roles.contains(&MaterialMapRole::BaseColor));
        assert!(roles.contains(&MaterialMapRole::Roughness));
        assert!(roles.contains(&MaterialMapRole::AmbientOcclusion));
        assert!(roles.contains(&MaterialMapRole::Height));
        assert!(!roles.iter().any(|role| matches!(role, MaterialMapRole::Metallic)));
        assert!(TextureSemantic::Height.color_space() == jarvig_material::ColorSpace::Linear);
        assert!(!TextureSemantic::Height.feeds_standard_brdf());
        assert_eq!(select_normal(&roles), Some((NormalConvention::DirectXNegativeY, false)));
        assert!(is_material_sidecar("Tiles101_4K-PNG.mtlx"));
        assert_eq!(classify_material_file("Facade020B", "Facade020B_4K-PNG_Metalness.png"), Some(MaterialMapRole::Metallic));
        assert_eq!(classify_material_file("Facade020B", "Facade020B_4K-PNG_Emission.png"), Some(MaterialMapRole::Emissive));
    }

    #[test]
    fn a_missing_metal_map_packs_zero_and_a_missing_roughness_packs_half() {
        let ao = [255u8, 0, 0, 255];
        let rough = [200u8, 0, 0, 255];
        let packed = pack_orm_rgba8(1, 1, Some(&ao), Some(&rough), None).unwrap();
        assert_eq!(packed, [255, 200, 0, 255]);
        let fallback = pack_orm_rgba8(1, 1, None, None, None).unwrap();
        assert_eq!(fallback, [255, 128, 0, 255]);
        let only_gl = [MaterialMapRole::Normal(NormalConvention::OpenGlPositiveY)];
        assert_eq!(select_normal(&only_gl), Some((NormalConvention::OpenGlPositiveY, true)));
    }
}
