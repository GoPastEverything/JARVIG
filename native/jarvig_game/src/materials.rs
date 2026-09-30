//! Staged material sets for the game host. Set names come from the level. Paths do not.

use std::fs;
use std::path::{Path, PathBuf};

use jarvig_core::{
    classify_material_file, negate_normal_green, pack_orm_rgba8, select_normal, ColorSpace, ComponentRecord, LevelDocument, MaterialMapRole,
    MaterialScheme, MipContent, Texture,
};
use jarvig_engine::EngineSession;

pub fn staged_names(document: &LevelDocument) -> Vec<String> {
    let mut names = Vec::new();
    for entity in &document.entities {
        for component in &entity.components {
            if let ComponentRecord::MeshRenderer { material, .. } = component {
                if material.scheme == MaterialScheme::Staged && !names.iter().any(|stored: &String| stored == &material.name) {
                    names.push(material.name.clone());
                }
            }
        }
    }
    names
}

pub fn install_staged_materials(engine: &mut EngineSession, project_file: &Path, document: &LevelDocument) -> Result<(), String> {
    for name in staged_names(document) {
        let folder = find_material_dir(project_file, &name).ok_or_else(|| format!("staged material {name} was not found"))?;
        let mut files = Vec::new();
        for entry in fs::read_dir(&folder).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if let Some(file) = entry.file_name().to_str() {
                files.push(file.to_string());
            }
        }
        let classified: Vec<_> = files.iter().filter_map(|file| classify_material_file(&name, file).map(|role| (file.clone(), role))).collect();
        let color_file = classified
            .iter()
            .find(|(_, role)| *role == MaterialMapRole::BaseColor)
            .map(|(file, _)| file.clone())
            .ok_or_else(|| format!("{name} has no Color map"))?;
        let (source_convention, flip_green) = select_normal(&classified.iter().map(|(_, role)| *role).collect::<Vec<_>>()).ok_or_else(|| format!("{name} has no normal map"))?;
        let normal_file = classified
            .iter()
            .find(|(_, role)| *role == MaterialMapRole::Normal(source_convention))
            .map(|(file, _)| file.clone())
            .ok_or_else(|| format!("{name} normal file is missing"))?;
        let (width, height, color) = decode_map(&folder, &color_file, MipContent::Srgb)?;
        let (_, _, mut normal) = decode_map(&folder, &normal_file, MipContent::Normal)?;
        if flip_green {
            negate_normal_green(&mut normal);
        }
        if normal.len() != color.len() {
            return Err(format!("{name} normal size does not match the color map"));
        }
        let (ao, rough, metal) = orm_planes(&classified, &folder)?;
        let orm = pack_orm_rgba8(width, height, ao.as_deref(), rough.as_deref(), metal.as_deref()).map_err(|error| error.to_string())?;
        let color_tex = Texture::with_mips(width, height, ColorSpace::Srgb, color, MipContent::Srgb).map_err(|error| format!("{error:?}"))?;
        let normal_tex = Texture::with_mips(width, height, ColorSpace::Linear, normal, MipContent::Normal).map_err(|error| format!("{error:?}"))?;
        let orm_tex = Texture::with_mips(width, height, ColorSpace::Linear, orm, MipContent::Linear).map_err(|error| format!("{error:?}"))?;
        engine.register_staged_material(&name, color_tex, orm_tex, normal_tex);
        eprintln!("JARVIG game material {name} from {}", folder.display());
    }
    Ok(())
}

fn decode_map(folder: &Path, file: &str, content: MipContent) -> Result<(u32, u32, Vec<u8>), String> {
    let bytes = fs::read(folder.join(file)).map_err(|error| error.to_string())?;
    let (width, height, pixels) = decode_png_rgba8(&bytes)?;
    Ok(jarvig_core::downsample_long_side(width, height, pixels, 2048, content))
}

fn orm_planes(classified: &[(String, MaterialMapRole)], folder: &Path) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>), String> {
    let plane = |role: MaterialMapRole| -> Result<Option<Vec<u8>>, String> {
        let Some((file, _)) = classified.iter().find(|(_, stored)| *stored == role) else {
            return Ok(None);
        };
        let (_, _, pixels) = decode_map(folder, file, MipContent::Linear)?;
        Ok(Some(pixels))
    };
    Ok((plane(MaterialMapRole::AmbientOcclusion)?, plane(MaterialMapRole::Roughness)?, plane(MaterialMapRole::Metallic)?))
}

pub fn find_material_dir(project_file: &Path, name: &str) -> Option<PathBuf> {
    let folder = format!("{name}_4K-PNG");
    let mut roots = Vec::new();
    if let Some(parent) = project_file.parent() {
        roots.push(parent.join("Content").join("Materials"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("Content").join("Materials"));
        }
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/materials"));
    roots.into_iter().map(|root| root.join(&folder)).find(|path| path.is_dir())
}

fn decode_png_rgba8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
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
