//! Logical textures and samplers. Not GPU objects, not files, and not the asset database.
//!
//! Color space is metadata on the texture. An sRGB color texture stores sRGB bytes and is
//! sampled through an sRGB GPU format, which returns linear values. A data texture stays linear.
//! ADR-0027. The shader does not choose.

use jarvig_material::{ColorSpace, Rgba8Storage, SamplerId, SamplerState, TextureDimension, TextureId};

/// In-memory RGBA8 image. Row bytes are tightly packed. Backend alignment is not this layout.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    dimension: TextureDimension,
    width: u32,
    height: u32,
    mip_count: u32,
    color_space: ColorSpace,
    revision: u64,
    pixels: Vec<u8>,
}

impl Texture {
    pub fn rgba8(width: u32, height: u32, mip_count: u32, color_space: ColorSpace, pixels: Vec<u8>) -> Result<Self, TextureError> {
        if width == 0 || height == 0 || mip_count == 0 {
            return Err(TextureError::BadSize);
        }
        let packed = width as usize * height as usize * 4;
        if pixels.len() != packed {
            return Err(TextureError::BadPixels);
        }
        Ok(Self {
            dimension: TextureDimension::Texture2D,
            width,
            height,
            mip_count,
            color_space,
            revision: 1,
            pixels,
        })
    }

    pub fn dimension(&self) -> TextureDimension {
        self.dimension
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn mip_count(&self) -> u32 {
        self.mip_count
    }

    pub fn color_space(&self) -> ColorSpace {
        self.color_space
    }

    pub fn storage(&self) -> Rgba8Storage {
        self.color_space.rgba8_storage()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// True when [`Self::pixels`] holds every mip, not only the base.
    /// [`Self::rgba8`] records a mip count without storing the chain. That still returns false.
    pub fn has_mip_chain(&self) -> bool {
        self.pixels.len() == chain_bytes(self.width, self.height, self.mip_count)
    }

    pub fn mip_extent(&self, level: u32) -> Option<(u32, u32)> {
        if level >= self.mip_count {
            None
        } else {
            Some(mip_extent(self.width, self.height, level))
        }
    }

    /// One mip, tightly packed RGBA8. `None` when the chain was not stored.
    pub fn mip_pixels(&self, level: u32) -> Option<&[u8]> {
        if !self.has_mip_chain() || level >= self.mip_count {
            return None;
        }
        let mut offset = 0usize;
        for current in 0..level {
            let (level_width, level_height) = mip_extent(self.width, self.height, current);
            offset += level_width as usize * level_height as usize * 4;
        }
        let (level_width, level_height) = mip_extent(self.width, self.height, level);
        let len = level_width as usize * level_height as usize * 4;
        self.pixels.get(offset..offset + len)
    }

    /// Base image plus a full chain. The base bytes are mip 0. Strength and UV scale are not applied here.
    pub fn with_mips(width: u32, height: u32, color_space: ColorSpace, base: Vec<u8>, content: MipContent) -> Result<Self, TextureError> {
        if width == 0 || height == 0 {
            return Err(TextureError::BadSize);
        }
        if base.len() != width as usize * height as usize * 4 {
            return Err(TextureError::BadPixels);
        }
        let mip_count = full_mip_count(width, height);
        let mut pixels = Vec::with_capacity(chain_bytes(width, height, mip_count));
        pixels.extend_from_slice(&base);
        let mut level_width = width;
        let mut level_height = height;
        let mut previous = base;
        for _ in 1..mip_count {
            let (next, next_width, next_height) = reduce_once(level_width, level_height, &previous, content);
            pixels.extend_from_slice(&next);
            previous = next;
            level_width = next_width;
            level_height = next_height;
        }
        Ok(Self {
            dimension: TextureDimension::Texture2D,
            width,
            height,
            mip_count,
            color_space,
            revision: 1,
            pixels,
        })
    }

    /// Tightly packed row of the base mip. Not the backend upload stride.
    pub fn packed_row_bytes(&self) -> u32 {
        self.width.saturating_mul(4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureError {
    BadSize,
    BadPixels,
}

/// IEC 61966-2-1. Used so a color texture's bytes are actually sRGB, not raw linear.
pub fn linear_channel_to_srgb_byte(linear: f32) -> u8 {
    let linear = linear.clamp(0.0, 1.0);
    let encoded = if linear <= 0.0031308 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

pub fn srgb_pixel(linear: [f32; 4]) -> [u8; 4] {
    [
        linear_channel_to_srgb_byte(linear[0]),
        linear_channel_to_srgb_byte(linear[1]),
        linear_channel_to_srgb_byte(linear[2]),
        (linear[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

/// IEC 61966-2-1 inverse. Mip filtering of an sRGB texture averages this, not the stored bytes.
pub fn srgb_byte_to_linear(byte: u8) -> f32 {
    let encoded = byte as f32 / 255.0;
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// How a smaller mip is made. The GPU format stays on [`Texture::color_space`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MipContent {
    /// Average in linear light, then store sRGB. Base color.
    Srgb,
    /// Average the stored bytes. Roughness, occlusion, and packed ORM.
    Linear,
    /// Average decoded normals and renormalize. Do not average the bytes.
    Normal,
}

/// Levels from the base through 1×1. A 2048 texture has 12.
pub fn full_mip_count(width: u32, height: u32) -> u32 {
    width.max(height).max(1).ilog2() + 1
}

fn mip_extent(width: u32, height: u32, level: u32) -> (u32, u32) {
    ((width >> level).max(1), (height >> level).max(1))
}

fn chain_bytes(width: u32, height: u32, mips: u32) -> usize {
    let mut total = 0usize;
    for level in 0..mips {
        let (level_width, level_height) = mip_extent(width, height, level);
        total += level_width as usize * level_height as usize * 4;
    }
    total
}

fn sample_index(width: u32, x: u32, y: u32) -> usize {
    (y as usize * width as usize + x as usize) * 4
}

fn reduce_once(width: u32, height: u32, pixels: &[u8], content: MipContent) -> (Vec<u8>, u32, u32) {
    let next_width = (width / 2).max(1);
    let next_height = (height / 2).max(1);
    if next_width == width && next_height == height {
        return (pixels.to_vec(), width, height);
    }
    let mut next = vec![0u8; next_width as usize * next_height as usize * 4];
    for y in 0..next_height {
        for x in 0..next_width {
            let x0 = (x * 2).min(width - 1);
            let x1 = (x * 2 + 1).min(width - 1);
            let y0 = (y * 2).min(height - 1);
            let y1 = (y * 2 + 1).min(height - 1);
            let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)];
            let dest = sample_index(next_width, x, y);
            match content {
                MipContent::Linear => {
                    let mut sum = [0u32; 4];
                    for (sx, sy) in corners {
                        let index = sample_index(width, sx, sy);
                        for channel in 0..4 {
                            sum[channel] += u32::from(pixels[index + channel]);
                        }
                    }
                    for channel in 0..4 {
                        next[dest + channel] = (sum[channel] / 4) as u8;
                    }
                }
                MipContent::Srgb => {
                    let mut sum = [0.0f32; 3];
                    let mut alpha = 0.0f32;
                    for (sx, sy) in corners {
                        let index = sample_index(width, sx, sy);
                        for channel in 0..3 {
                            sum[channel] += srgb_byte_to_linear(pixels[index + channel]);
                        }
                        alpha += f32::from(pixels[index + 3]);
                    }
                    for channel in 0..3 {
                        next[dest + channel] = linear_channel_to_srgb_byte(sum[channel] * 0.25);
                    }
                    next[dest + 3] = (alpha * 0.25).round().clamp(0.0, 255.0) as u8;
                }
                MipContent::Normal => {
                    let mut sum = [0.0f32; 3];
                    for (sx, sy) in corners {
                        let index = sample_index(width, sx, sy);
                        for channel in 0..3 {
                            sum[channel] += f32::from(pixels[index + channel]) / 255.0 * 2.0 - 1.0;
                        }
                    }
                    let length = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
                    let normal = if length < 1.0e-6 { [0.0, 0.0, 1.0] } else { [sum[0] / length, sum[1] / length, sum[2] / length] };
                    for channel in 0..3 {
                        next[dest + channel] = ((normal[channel] * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                    next[dest + 3] = 255;
                }
            }
        }
    }
    (next, next_width, next_height)
}

/// Box-filter until the long side is at most `limit`. Even sizes are the ingest path.
pub fn downsample_long_side(width: u32, height: u32, pixels: Vec<u8>, limit: u32, content: MipContent) -> (u32, u32, Vec<u8>) {
    let mut width = width;
    let mut height = height;
    let mut pixels = pixels;
    while width.max(height) > limit.max(1) && (width > 1 || height > 1) {
        let (next, next_width, next_height) = reduce_once(width, height, &pixels, content);
        if next_width == width && next_height == height {
            break;
        }
        width = next_width;
        height = next_height;
        pixels = next;
    }
    (width, height, pixels)
}

/// 8×8 checker. One texel per cell, so filtering and repeat are visible.
pub fn solid(color_space: ColorSpace, pixel: [u8; 4]) -> Texture {
    Texture::rgba8(1, 1, 1, color_space, pixel.to_vec()).expect("solid texture")
}

pub fn checkerboard(color_space: ColorSpace, first: [u8; 4], second: [u8; 4]) -> Texture {
    let width = 8u32;
    let height = 8u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let cell = if (x + y) % 2 == 0 { first } else { second };
            pixels.extend_from_slice(&cell);
        }
    }
    Texture::rgba8(width, height, 1, color_space, pixels).expect("checkerboard")
}

/// Magenta and black. Development stand-in for a missing color texture. sRGB, because it is color.
pub fn error_color_texture() -> Texture {
    checkerboard(ColorSpace::Srgb, [255, 0, 255, 255], [0, 0, 0, 255])
}

struct LogicalSampler {
    state: SamplerState,
}

/// Masters do not own these. Instances only store ids.
#[derive(Default)]
pub struct TextureLibrary {
    textures: Vec<(TextureId, Texture)>,
    samplers: Vec<(SamplerId, LogicalSampler)>,
    error_color: Option<TextureId>,
    white_srgb: Option<TextureId>,
    black_srgb: Option<TextureId>,
    flat_normal: Option<TextureId>,
    /// Linear white. Identity multiplier for AO and for a packed map the graph scales.
    neutral_orm: Option<TextureId>,
    /// Linear zero. Unbound metallic, height, and generic data.
    zero_linear: Option<TextureId>,
    /// Linear 0.5 stored as 128. Unbound roughness. 128/255 is the closest byte.
    half_linear: Option<TextureId>,
    next_texture: u64,
    next_sampler: u64,
}

impl TextureLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, texture: Texture) -> TextureId {
        self.next_texture += 1;
        let id = TextureId(self.next_texture);
        self.textures.push((id, texture));
        id
    }

    pub fn insert_error_color(&mut self) -> TextureId {
        let id = self.insert(error_color_texture());
        self.error_color = Some(id);
        id
    }

    pub fn insert_sampler(&mut self, state: SamplerState) -> SamplerId {
        self.next_sampler += 1;
        let id = SamplerId(self.next_sampler);
        self.samplers.push((id, LogicalSampler { state }));
        id
    }

    pub fn get(&self, id: TextureId) -> Option<&Texture> {
        self.textures.iter().find(|(stored, _)| *stored == id).map(|(_, texture)| texture)
    }

    pub fn sampler(&self, id: SamplerId) -> Option<SamplerState> {
        self.samplers.iter().find(|(stored, _)| *stored == id).map(|(_, sampler)| sampler.state)
    }

    pub fn error_color(&self) -> Option<TextureId> {
        self.error_color
    }

    pub fn white_srgb(&self) -> Option<TextureId> {
        self.white_srgb
    }

    pub fn black_srgb(&self) -> Option<TextureId> {
        self.black_srgb
    }

    pub fn flat_normal(&self) -> Option<TextureId> {
        self.flat_normal
    }

    pub fn neutral_orm(&self) -> Option<TextureId> {
        self.neutral_orm
    }

    pub fn zero_linear(&self) -> Option<TextureId> {
        self.zero_linear
    }

    pub fn half_linear(&self) -> Option<TextureId> {
        self.half_linear
    }

    /// Shared 1×1 defaults. Unbound optional parameters use these. Invalid ids do not.
    pub fn ensure_defaults(&mut self) {
        if self.white_srgb.is_none() {
            let id = self.insert(solid(ColorSpace::Srgb, [255, 255, 255, 255]));
            self.white_srgb = Some(id);
        }
        if self.black_srgb.is_none() {
            let id = self.insert(solid(ColorSpace::Srgb, [0, 0, 0, 255]));
            self.black_srgb = Some(id);
        }
        if self.flat_normal.is_none() {
            let id = self.insert(solid(ColorSpace::Linear, [128, 128, 255, 255]));
            self.flat_normal = Some(id);
        }
        if self.neutral_orm.is_none() {
            // Identity. A graph that multiplies channels by factors keeps those factors.
            // This is not an engine-wide ORM layout.
            let id = self.insert(solid(ColorSpace::Linear, [255, 255, 255, 255]));
            self.neutral_orm = Some(id);
        }
        if self.zero_linear.is_none() {
            let id = self.insert(solid(ColorSpace::Linear, [0, 0, 0, 255]));
            self.zero_linear = Some(id);
        }
        if self.half_linear.is_none() {
            let id = self.insert(solid(ColorSpace::Linear, [128, 128, 128, 255]));
            self.half_linear = Some(id);
        }
        if self.error_color.is_none() {
            self.insert_error_color();
        }
    }

    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    pub fn sampler_count(&self) -> usize {
        self.samplers.len()
    }
}

/// Bootstrap color checkers. Bytes are sRGB-encoded. The GPU format, not the shader, decodes them.
pub fn bootstrap_color_textures() -> (TextureLibrary, TextureId, TextureId, SamplerId) {
    let mut library = TextureLibrary::new();
    library.insert_error_color();
    let near = library.insert(checkerboard(
        ColorSpace::Srgb,
        srgb_pixel([0.85, 0.08, 0.08, 1.0]),
        srgb_pixel([0.90, 0.85, 0.15, 1.0]),
    ));
    let far = library.insert(checkerboard(
        ColorSpace::Srgb,
        srgb_pixel([0.10, 0.35, 0.85, 1.0]),
        srgb_pixel([0.10, 0.75, 0.45, 1.0]),
    ));
    let sampler = library.insert_sampler(SamplerState::linear_repeat());
    library.ensure_defaults();
    (library, near, far, sampler)
}

/// Two base-color checkers plus the shared linear ORM, flat normal, and black emissive.
pub fn bootstrap_pbr_textures() -> (TextureLibrary, TextureId, TextureId, SamplerId) {
    let mut library = TextureLibrary::new();
    library.ensure_defaults();
    let near = library.insert(checkerboard(
        ColorSpace::Srgb,
        srgb_pixel([0.85, 0.08, 0.08, 1.0]),
        srgb_pixel([0.90, 0.85, 0.15, 1.0]),
    ));
    let far = library.insert(checkerboard(
        ColorSpace::Srgb,
        srgb_pixel([0.15, 0.45, 0.85, 1.0]),
        srgb_pixel([0.10, 0.75, 0.55, 1.0]),
    ));
    let sampler = library.insert_sampler(SamplerState::linear_repeat());
    (library, near, far, sampler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_color_bytes_differ_from_linear_data_and_rows_stay_packed() {
        assert_eq!(linear_channel_to_srgb_byte(0.0), 0);
        assert_eq!(linear_channel_to_srgb_byte(1.0), 255);
        assert_ne!(linear_channel_to_srgb_byte(0.5), 128);
        let color = checkerboard(ColorSpace::Srgb, srgb_pixel([0.5, 0.0, 0.0, 1.0]), [0, 0, 0, 255]);
        let data = Texture::rgba8(4, 4, 2, ColorSpace::Linear, vec![0; 64]).unwrap();
        assert_eq!(color.storage(), Rgba8Storage::Srgb);
        assert_eq!(data.storage(), Rgba8Storage::Unorm);
        assert_eq!(color.mip_count(), 1);
        assert_eq!(data.mip_count(), 2);
        assert_eq!(color.dimension(), TextureDimension::Texture2D);
        assert_eq!(color.packed_row_bytes(), 32);
        assert_eq!(color.pixels().len(), 8 * 8 * 4);
        assert!(Texture::rgba8(4, 4, 0, ColorSpace::Linear, vec![0; 64]).is_err());
        assert!(Texture::rgba8(4, 4, 1, ColorSpace::Linear, vec![0; 8]).is_err());
        let mut library = TextureLibrary::new();
        let error = library.insert_error_color();
        assert_eq!(library.error_color(), Some(error));
        assert_eq!(library.texture_count(), 1);
        assert_eq!(library.get(error).unwrap().color_space(), ColorSpace::Srgb);
        assert!(!data.has_mip_chain());
    }

    #[test]
    fn srgb_mips_average_linear_light_and_normals_are_renormalized() {
        let black = [0u8, 0, 0, 255];
        let white = [255u8, 255, 255, 255];
        let mut base = Vec::new();
        base.extend_from_slice(&black);
        base.extend_from_slice(&white);
        base.extend_from_slice(&black);
        base.extend_from_slice(&white);
        let (width, height, down) = downsample_long_side(2, 2, base.clone(), 1, MipContent::Srgb);
        assert_eq!((width, height), (1, 1));
        assert_eq!(down[0], linear_channel_to_srgb_byte(0.5));
        assert_ne!(down[0], 128);
        let chained = Texture::with_mips(2, 2, ColorSpace::Srgb, base, MipContent::Srgb).unwrap();
        assert!(chained.has_mip_chain());
        assert_eq!(chained.mip_count(), 2);
        assert_eq!(chained.mip_pixels(1).unwrap()[0], linear_channel_to_srgb_byte(0.5));
        let flat = [128u8, 128, 255, 255];
        let mut normals = Vec::new();
        normals.extend_from_slice(&flat);
        normals.extend_from_slice(&[255, 128, 128, 255]);
        normals.extend_from_slice(&flat);
        normals.extend_from_slice(&[0, 128, 128, 255]);
        let normal = Texture::with_mips(2, 2, ColorSpace::Linear, normals, MipContent::Normal).unwrap();
        let mip = normal.mip_pixels(1).unwrap();
        assert!((mip[0] as i32 - 128).abs() <= 1, "opposing x tilts cancel, got {}", mip[0]);
        assert!(mip[2] > 250, "renormalized z stays up, got {}", mip[2]);
    }
}
