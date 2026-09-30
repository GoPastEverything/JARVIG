//! PNG bytes to tightly packed RGBA8. Not a GPU texture and not a level file.

pub fn decode_png_rgba8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; reader.output_buffer_size().ok_or("png buffer size is unknown")?];
    let info = reader.next_frame(&mut buffer).map_err(|error| error.to_string())?;
    let width = info.width;
    let height = info.height;
    let raw = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => {
            let mut expanded = Vec::with_capacity(raw.len() / 3 * 4);
            for pixel in raw.chunks_exact(3) {
                expanded.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            expanded
        }
        png::ColorType::Grayscale => {
            let mut expanded = Vec::with_capacity(raw.len() * 4);
            for gray in raw {
                expanded.extend_from_slice(&[*gray, *gray, *gray, 255]);
            }
            expanded
        }
        png::ColorType::GrayscaleAlpha => {
            let mut expanded = Vec::with_capacity(raw.len() / 2 * 4);
            for pixel in raw.chunks_exact(2) {
                expanded.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            expanded
        }
        other => return Err(format!("unsupported png color {other:?}")),
    };
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("png pixel count does not match the header".into());
    }
    Ok((width, height, rgba))
}

/// Byte box used by the color-size test. Material ingest filters in linear light instead.
#[cfg_attr(not(test), allow(dead_code))]
pub fn limit_long_side(width: u32, height: u32, pixels: Vec<u8>, limit: u32) -> (u32, u32, Vec<u8>) {
    let mut width = width;
    let mut height = height;
    let mut pixels = pixels;
    while width.max(height) > limit && width % 2 == 0 && height % 2 == 0 {
        let next_w = width / 2;
        let next_h = height / 2;
        let mut next = vec![0u8; next_w as usize * next_h as usize * 4];
        for y in 0..next_h {
            for x in 0..next_w {
                for channel in 0..4 {
                    let mut sum = 0u16;
                    for oy in 0..2 {
                        for ox in 0..2 {
                            let source = (((y * 2 + oy) * width + (x * 2 + ox)) * 4 + channel) as usize;
                            sum += u16::from(pixels[source]);
                        }
                    }
                    next[((y * next_w + x) * 4 + channel) as usize] = (sum / 4) as u8;
                }
            }
        }
        width = next_w;
        height = next_h;
        pixels = next;
    }
    (width, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles101_color_decodes_and_fits_the_baseline() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/materials/Tiles101_4K-PNG/Tiles101_4K-PNG_Color.png");
        let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let (width, height, pixels) = decode_png_rgba8(&bytes).unwrap();
        assert!(width >= 2048 && height >= 2048, "{width}x{height}");
        let (width, height, pixels) = limit_long_side(width, height, pixels, 2048);
        assert!(width <= 2048 && height <= 2048);
        assert_eq!(pixels.len(), width as usize * height as usize * 4);
    }

    #[test]
    fn tiles101_directx_and_opengl_are_one_green_flip_on_the_floor_frame() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/materials/Tiles101_4K-PNG");
        let dx_bytes = std::fs::read(root.join("Tiles101_4K-PNG_NormalDX.png")).unwrap();
        let gl_bytes = std::fs::read(root.join("Tiles101_4K-PNG_NormalGL.png")).unwrap();
        let (dx_w, dx_h, dx) = decode_png_rgba8(&dx_bytes).unwrap();
        let (gl_w, gl_h, gl) = decode_png_rgba8(&gl_bytes).unwrap();
        assert_eq!((dx_w, dx_h), (gl_w, gl_h));
        let mut compared = 0u32;
        let mut green_miss = 0u32;
        let mut color_miss = 0u32;
        let mut strong = None;
        let step = 64u32;
        for y in (0..dx_h).step_by(step as usize) {
            for x in (0..dx_w).step_by(step as usize) {
                let index = ((y * dx_w + x) * 4) as usize;
                compared += 1;
                if (dx[index] as i32 - gl[index] as i32).abs() > 1 || (dx[index + 2] as i32 - gl[index + 2] as i32).abs() > 1 {
                    color_miss += 1;
                }
                if (dx[index + 1] as i32 - (255 - gl[index + 1]) as i32).abs() > 1 {
                    green_miss += 1;
                }
                if strong.is_none() && (dx[index + 1] as i32 - 128).abs() > 40 {
                    strong = Some(index);
                }
            }
        }
        assert!(compared > 100, "sampled {compared} texels");
        assert!(green_miss * 20 < compared, "DirectX green is not the OpenGL inverse: {green_miss}/{compared}");
        assert!(color_miss * 20 < compared, "DirectX and OpenGL disagree in red or blue: {color_miss}/{compared}");
        let index = strong.expect("Tiles101 has a real bump");
        let dx_n = jarvig_core::decode_normal(
            [dx[index] as f32 / 255.0, dx[index + 1] as f32 / 255.0, dx[index + 2] as f32 / 255.0],
            1.0,
        );
        let gl_raw = jarvig_core::decode_normal(
            [gl[index] as f32 / 255.0, gl[index + 1] as f32 / 255.0, gl[index + 2] as f32 / 255.0],
            1.0,
        );
        let gl_flipped = jarvig_core::decode_normal(
            [gl[index] as f32 / 255.0, (255 - gl[index + 1]) as f32 / 255.0, gl[index + 2] as f32 / 255.0],
            1.0,
        );
        let model = jarvig_core::TangentMat3::scale(1.0, 1.0, 1.0);
        let floor_n = [0.0, 1.0, 0.0];
        let floor_t = [1.0, 0.0, 0.0, 1.0];
        let shaded_dx = jarvig_core::shade_normal(model, floor_n, floor_t, dx_n);
        let shaded_gl = jarvig_core::shade_normal(model, floor_n, floor_t, gl_flipped);
        let shaded_raw = jarvig_core::shade_normal(model, floor_n, floor_t, gl_raw);
        for channel in 0..3 {
            assert!((shaded_dx[channel] - shaded_gl[channel]).abs() < 0.02, "DX and flipped GL diverged: {shaded_dx:?} {shaded_gl:?}");
        }
        assert!((shaded_dx[2] - shaded_raw[2]).abs() > 0.02, "unflipped OpenGL matched DirectX, so the frame would not be -Y");
    }
}
