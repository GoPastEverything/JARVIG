//! Project asset registry. Identity and metadata, not decoded source files.
//!
//! Opening a project reads this record and reconciles paths. It does not reimport
//! a mesh or decode a texture that has not changed.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::asset::AssetId;
use crate::json_lite::{parse_json, Json};
use crate::texture::{downsample_long_side, MipContent};

pub const REGISTRY_SCHEMA: &str = "jarvig.registry";
pub const REGISTRY_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryAsset {
    pub id: AssetId,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub subtype: String,
    pub format: String,
    pub bytes: u64,
    pub modified_ms: u64,
    pub width: u32,
    pub height: u32,
    pub triangles: u32,
    pub thumbnail: String,
}

#[derive(Clone, Debug)]
pub struct AssetRegistry {
    pub assets: Vec<RegistryAsset>,
    pub load_us: u64,
    pub scan_us: u64,
    pub reused: u32,
    pub changed: u32,
    /// True when a previous registry was read and no file was added, removed, or rewritten.
    pub from_cache: bool,
}

#[derive(Clone, Debug)]
pub struct RegistryRoots {
    pub content: String,
    pub config: String,
    pub intermediate: String,
}

pub fn reconcile_asset_registry(root: &Path, roots: &RegistryRoots) -> Result<AssetRegistry, String> {
    let started = std::time::Instant::now();
    let registry_path = registry_file(root, &roots.intermediate);
    let (previous, previous_existed) = read_registry(&registry_path)?;
    let load_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    let scan_started = std::time::Instant::now();
    let catalog = catalog_mesh_index(root, &roots.content);
    let mut found = Vec::new();
    walk_root(root, &roots.content, &mut found)?;
    walk_root(root, &roots.config, &mut found)?;
    let source = root.join("Source");
    if source.is_dir() {
        walk_root(root, "Source", &mut found)?;
    }
    found.sort_by(|left, right| left.path.cmp(&right.path));
    found.dedup_by(|left, right| left.path == right.path);
    let mut assets = Vec::new();
    let mut reused = 0u32;
    let mut changed = 0u32;
    let mut seen_previous = vec![false; previous.len()];
    for file in found {
        let prior = previous.iter().position(|asset| asset.path == file.path);
        let (id, thumbnail, prior_same) = if let Some(index) = prior {
            seen_previous[index] = true;
            let prior = &previous[index];
            let same = prior.bytes == file.bytes && prior.modified_ms == file.modified_ms;
            (prior.id, if same { prior.thumbnail.clone() } else { String::new() }, same)
        } else if let Some(mesh) = catalog.iter().find(|mesh| mesh.path == file.path) {
            (mesh.id, String::new(), false)
        } else {
            (AssetId::new(), String::new(), false)
        };
        if prior_same {
            reused = reused.saturating_add(1);
        } else {
            changed = changed.saturating_add(1);
        }
        let triangles = catalog.iter().find(|mesh| mesh.path == file.path).map(|mesh| mesh.triangles).unwrap_or(0);
        assets.push(RegistryAsset {
            id,
            path: file.path,
            name: file.name,
            kind: file.kind,
            subtype: file.subtype,
            format: file.format,
            bytes: file.bytes,
            modified_ms: file.modified_ms,
            width: 0,
            height: 0,
            triangles,
            thumbnail,
        });
    }
    for (index, asset) in previous.iter().enumerate() {
        if !seen_previous[index] {
            changed = changed.saturating_add(1);
            let _ = asset;
        }
    }
    let scan_us = scan_started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    let from_cache = previous_existed && changed == 0 && reused == assets.len() as u32;
    let registry = AssetRegistry { assets, load_us, scan_us, reused, changed, from_cache };
    write_registry(&registry_path, &registry)?;
    Ok(registry)
}

/// Shrink an RGBA image until its long side is at most 128. The source buffer is consumed.
pub fn thumbnail_rgba(width: u32, height: u32, pixels: Vec<u8>) -> (u32, u32, Vec<u8>) {
    downsample_long_side(width, height, pixels, 128, MipContent::Srgb)
}

pub fn texture_subtype(file_name: &str) -> &'static str {
    let name = file_name.to_ascii_lowercase();
    if name.ends_with(".hdr") || name.ends_with(".exr") || name.contains("hdr") || name.contains("env") {
        return "HDR";
    }
    if name.contains("normal") || name.contains("nrm") || name.contains("_n.") || name.ends_with("_n.png") || name.ends_with("_n.jpg") {
        return "Normal";
    }
    if name.contains("rough") {
        return "Roughness";
    }
    if name.contains("metal") {
        return "Metallic";
    }
    if name.contains("ao") || name.contains("occlusion") {
        return "AO";
    }
    if name.contains("height") || name.contains("disp") {
        return "Height";
    }
    if name.contains("emiss") {
        return "Emissive";
    }
    if name.contains("mask") {
        return "Mask";
    }
    if name.contains("base") || name.contains("albedo") || name.contains("diffuse") || name.contains("color") || name.contains("_bc") || name.contains("_d.") {
        return "Base Color";
    }
    "Other"
}

pub fn model_subtype(path: &str) -> &'static str {
    let value = path.to_ascii_lowercase();
    if value.contains("/characters/") || value.contains("/character/") {
        return "Character";
    }
    if value.contains("/environment/") {
        return "Environment";
    }
    if value.contains("/props/") || value.contains("/prop/") {
        return "Prop";
    }
    "Static Mesh"
}

struct FoundFile {
    path: String,
    name: String,
    kind: String,
    subtype: String,
    format: String,
    bytes: u64,
    modified_ms: u64,
}

struct CatalogMesh {
    id: AssetId,
    path: String,
    triangles: u32,
}

fn registry_file(root: &Path, intermediate: &str) -> PathBuf {
    root.join(intermediate).join("AssetRegistry.jarvigregistry")
}

fn walk_root(root: &Path, relative: &str, out: &mut Vec<FoundFile>) -> Result<(), String> {
    let start = root.join(relative);
    if !start.is_dir() {
        return Ok(());
    }
    let mut pending = vec![start];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| format!("{} was not read: {error}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || hidden_name(&name) {
                continue;
            }
            let meta = entry.metadata().map_err(|error| error.to_string())?;
            if meta.is_dir() {
                pending.push(path);
                continue;
            }
            if name.eq_ignore_ascii_case("catalog.jarvigassets") {
                continue;
            }
            let relative_path = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            let Some(classified) = classify(&relative_path, &name) else { continue };
            out.push(FoundFile {
                path: relative_path,
                name: path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(&name).to_string(),
                kind: classified.0.to_string(),
                subtype: classified.1.to_string(),
                format: classified.2.to_string(),
                bytes: meta.len(),
                modified_ms: modified_ms(&meta),
            });
        }
    }
    Ok(())
}

fn hidden_name(name: &str) -> bool {
    matches!(name, "Intermediate" | "Saved" | "DerivedDataCache" | "target" | "Build" | "node_modules" | "docs" | ".git" | ".grok")
}

fn classify(path: &str, file_name: &str) -> Option<(&'static str, String, String)> {
    let lower = file_name.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, extension)| extension).unwrap_or("");
    let format = extension.to_string();
    if matches!(extension, "png" | "jpg" | "jpeg" | "hdr" | "exr") {
        return Some(("Texture", texture_subtype(file_name).to_string(), format));
    }
    if matches!(extension, "glb" | "gltf") {
        return Some(("Model", model_subtype(path).to_string(), format));
    }
    if extension == "jarviglevel" {
        return Some(("Level", "Level".into(), format));
    }
    if extension == "jarvigprefab" {
        return Some(("Prefab", "Prefab".into(), format));
    }
    if extension == "jarvigcharacter" {
        return Some(("Character", "Character".into(), format));
    }
    if extension == "jarvigplayer" {
        return Some(("Player", "Player".into(), format));
    }
    if matches!(extension, "wav" | "ogg") {
        return Some(("Audio", "Audio".into(), format));
    }
    if matches!(extension, "wgsl" | "hlsl" | "glsl") {
        return Some(("Shader", "Shader".into(), format));
    }
    if path.starts_with("Source/") || path.starts_with("Source\\") {
        if matches!(extension, "rs" | "ts" | "js" | "wgsl" | "hlsl") {
            return Some(("Source", "Source".into(), format));
        }
    }
    if path_has_dir(path, "Materials") && matches!(extension, "json" | "jarvigmaterial" | "mat") {
        return Some(("Material", "Imported".into(), format));
    }
    if path_has_dir(path, "Config") && matches!(extension, "json" | "jarvigsettings" | "ini" | "toml") {
        return Some(("Config", "Config".into(), format));
    }
    None
}

fn path_has_dir(path: &str, name: &str) -> bool {
    path.split(['/', '\\']).any(|part| part.eq_ignore_ascii_case(name))
}

fn modified_ms(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn catalog_mesh_index(root: &Path, content: &str) -> Vec<CatalogMesh> {
    let path = root.join(content).join("Assets").join("catalog.jarvigassets");
    let Ok(text) = fs::read_to_string(path) else { return Vec::new() };
    let Ok(json) = parse_json(&text) else { return Vec::new() };
    let mut meshes = Vec::new();
    for asset in json.get("assets").and_then(Json::as_array).unwrap_or(&[]) {
        let Some(id) = asset.get("id").and_then(Json::as_str).and_then(AssetId::parse) else { continue };
        let Some(source) = asset.get("source").and_then(Json::as_str) else { continue };
        let triangles = asset.get("triangles").and_then(Json::as_f64).unwrap_or(0.0) as u32;
        meshes.push(CatalogMesh { id, path: source.replace('\\', "/"), triangles });
    }
    meshes
}

fn read_registry(path: &Path) -> Result<(Vec<RegistryAsset>, bool), String> {
    if !path.is_file() {
        return Ok((Vec::new(), false));
    }
    let text = fs::read_to_string(path).map_err(|error| format!("asset registry was not read: {error}"))?;
    let json = parse_json(&text).map_err(|error| format!("asset registry was rejected: {error}"))?;
    if json.get("schema").and_then(Json::as_str) != Some(REGISTRY_SCHEMA) {
        return Ok((Vec::new(), false));
    }
    let version = json.get("format_version").and_then(Json::as_f64).unwrap_or(0.0);
    if version != REGISTRY_VERSION as f64 {
        return Ok((Vec::new(), false));
    }
    let mut assets = Vec::new();
    for asset in json.get("assets").and_then(Json::as_array).unwrap_or(&[]) {
        let Some(id) = asset.get("id").and_then(Json::as_str).and_then(AssetId::parse) else { continue };
        assets.push(RegistryAsset {
            id,
            path: asset.get("path").and_then(Json::as_str).unwrap_or("").replace('\\', "/"),
            name: asset.get("name").and_then(Json::as_str).unwrap_or("").to_string(),
            kind: asset.get("kind").and_then(Json::as_str).unwrap_or("").to_string(),
            subtype: asset.get("subtype").and_then(Json::as_str).unwrap_or("").to_string(),
            format: asset.get("format").and_then(Json::as_str).unwrap_or("").to_string(),
            bytes: asset.get("bytes").and_then(Json::as_f64).unwrap_or(0.0) as u64,
            modified_ms: asset.get("modified_ms").and_then(Json::as_f64).unwrap_or(0.0) as u64,
            width: asset.get("width").and_then(Json::as_f64).unwrap_or(0.0) as u32,
            height: asset.get("height").and_then(Json::as_f64).unwrap_or(0.0) as u32,
            triangles: asset.get("triangles").and_then(Json::as_f64).unwrap_or(0.0) as u32,
            thumbnail: asset.get("thumbnail").and_then(Json::as_str).unwrap_or("").to_string(),
        });
    }
    Ok((assets, true))
}

fn write_registry(path: &Path, registry: &AssetRegistry) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let assets = registry.assets.iter().map(|asset| {
        Json::object(vec![
            ("id", Json::string(asset.id.to_string())),
            ("path", Json::string(&asset.path)),
            ("name", Json::string(&asset.name)),
            ("kind", Json::string(&asset.kind)),
            ("subtype", Json::string(&asset.subtype)),
            ("format", Json::string(&asset.format)),
            ("bytes", Json::number(asset.bytes as f64)),
            ("modified_ms", Json::number(asset.modified_ms as f64)),
            ("width", Json::int(i64::from(asset.width))),
            ("height", Json::int(i64::from(asset.height))),
            ("triangles", Json::number(f64::from(asset.triangles))),
            ("thumbnail", Json::string(&asset.thumbnail)),
        ])
    });
    let document = Json::object(vec![
        ("schema", Json::string(REGISTRY_SCHEMA)),
        ("format_version", Json::int(i64::from(REGISTRY_VERSION))),
        ("assets", Json::array(assets.collect())),
    ]);
    let text = document.write();
    let temporary = path.with_extension("jarvigregistry.tmp");
    fs::write(&temporary, text).map_err(|error| format!("asset registry was not written: {error}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("asset registry was not replaced: {error}"))?;
    }
    fs::rename(&temporary, path).map_err(|error| format!("asset registry was not published: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_lists_game_files_keeps_ids_and_skips_generated_data() {
        let root = std::env::temp_dir().join(format!("jarvig-registry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Content/Meshes")).unwrap();
        fs::create_dir_all(root.join("Content/Textures")).unwrap();
        fs::create_dir_all(root.join("Content/Levels")).unwrap();
        fs::create_dir_all(root.join("Content/Assets")).unwrap();
        fs::create_dir_all(root.join("Config")).unwrap();
        fs::create_dir_all(root.join("Intermediate")).unwrap();
        fs::create_dir_all(root.join("Saved")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("Content/Meshes/shop.glb"), b"glb").unwrap();
        fs::write(root.join("Content/Textures/stone_n.png"), b"png").unwrap();
        fs::write(root.join("Content/Levels/World.jarviglevel"), b"{}").unwrap();
        fs::write(root.join("Content/Assets/catalog.jarvigassets"), b"{}").unwrap();
        fs::write(root.join("Config/Project.jarvigsettings"), b"{}").unwrap();
        fs::write(root.join("Intermediate/cache.bin"), b"secret").unwrap();
        fs::write(root.join("Saved/shot.png"), b"png").unwrap();
        fs::write(root.join("docs/readme.md"), b"docs").unwrap();
        let roots = RegistryRoots { content: "Content".into(), config: "Config".into(), intermediate: "Intermediate".into() };
        let first = reconcile_asset_registry(&root, &roots).unwrap();
        let paths: Vec<_> = first.assets.iter().map(|asset| asset.path.clone()).collect();
        assert!(paths.iter().any(|path| path.ends_with("shop.glb")));
        assert!(paths.iter().any(|path| path.ends_with("stone_n.png")));
        assert!(paths.iter().any(|path| path.ends_with("World.jarviglevel")));
        assert!(paths.iter().any(|path| path.ends_with("Project.jarvigsettings")));
        assert!(paths.iter().all(|path| !path.contains("Intermediate") && !path.contains("Saved") && !path.contains("docs") && !path.contains("catalog.jarvigassets")));
        let stone = first.assets.iter().find(|asset| asset.name == "stone_n").unwrap();
        assert_eq!(stone.kind, "Texture");
        assert_eq!(stone.subtype, "Normal");
        let shop = first.assets.iter().find(|asset| asset.name == "shop").unwrap();
        assert_eq!(shop.kind, "Model");
        assert_eq!(shop.subtype, "Static Mesh");
        assert!(!first.from_cache);
        let second = reconcile_asset_registry(&root, &roots).unwrap();
        assert!(second.from_cache, "reused {} changed {}", second.reused, second.changed);
        assert_eq!(second.assets.len(), first.assets.len());
        for asset in &first.assets {
            let again = second.assets.iter().find(|item| item.path == asset.path).unwrap();
            assert_eq!(again.id, asset.id);
        }
        let wide = thumbnail_rgba(256, 128, vec![255u8; 256 * 128 * 4]);
        assert!(wide.0 <= 128 && wide.1 <= 128);
        assert_eq!(wide.2.len(), wide.0 as usize * wide.1 as usize * 4);
        assert!(wide.2.len() < 256 * 128 * 4);
        let _ = fs::remove_dir_all(&root);
    }
}
