//! Project mesh assets. An id is the identity. Paths are metadata.
//!
//! An external file is copied into `Content/Meshes`. The catalog remembers that
//! external path for reimport and the project-relative copy it actually reads.
//! The level stores the id. The derived file is JARVIG's canonical mesh.

use std::fs;
use std::path::{Path, PathBuf};

use crate::gltf::{curved_prop_glb, gltf_surfaces, import_gltf, GltfImage, ImportError};
use crate::json_lite::{parse_json, Json};
use crate::mesh::{Mesh, MeshError, MeshIndexFormat};
use crate::{ColorSpace, MipContent, Texture, EntityId};

pub const ASSET_CATALOG_SCHEMA: &str = "jarvig.assets";
pub const ASSET_CATALOG_VERSION: u32 = 1;
pub const MESH_IMPORTER_VERSION: u32 = 1;
const DERIVED_MAGIC: &[u8; 8] = b"JARVMESH";

/// Persistent mesh identity. Not an entity, not a `MeshId`, and not a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssetId(pub EntityId);

impl AssetId {
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    pub fn parse(text: &str) -> Option<Self> {
        EntityId::parse(text).map(Self)
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug)]
pub struct MeshAssetRecord {
    pub id: AssetId,
    pub name: String,
    /// Project-relative copy, such as `Content/Meshes/townshop.glb`. Not an absolute path.
    pub source_relative: String,
    /// The file the user imported. Metadata for reimport. Not the level reference.
    pub external_source: String,
    pub source_fingerprint: String,
    pub triangle_count: u32,
    pub submesh_count: u32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

/// Decoded maps from the mesh's own glTF material. Not a staged set and not Tiles101.
#[derive(Clone, Debug)]
pub struct ImportedSurface {
    pub base_color_factor: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub normal_scale: f32,
    /// From glTF `doubleSided`. The shaded master is still the shared standard material.
    pub double_sided: bool,
    pub alpha_mode: String,
    pub base_color: Option<Texture>,
    pub orm: Option<Texture>,
    pub normal: Option<Texture>,
}

#[derive(Clone, Default)]
pub struct MeshAssetLibrary {
    records: Vec<MeshAssetRecord>,
    meshes: Vec<(AssetId, Mesh)>,
    surfaces: Vec<(AssetId, ImportedSurface)>,
    meshlets: Vec<(AssetId, crate::MeshletSet)>,
}

impl MeshAssetLibrary {
    pub fn records(&self) -> &[MeshAssetRecord] {
        &self.records
    }

    pub fn names(&self) -> Vec<String> {
        self.records.iter().map(|record| record.name.clone()).collect()
    }

    pub fn get(&self, id: AssetId) -> Option<&Mesh> {
        self.meshes.iter().find(|(stored, _)| *stored == id).map(|(_, mesh)| mesh)
    }

    pub fn find_name(&self, name: &str) -> Option<AssetId> {
        self.records.iter().find(|record| record.name == name).map(|record| record.id)
    }

    pub fn record(&self, id: AssetId) -> Option<&MeshAssetRecord> {
        self.records.iter().find(|record| record.id == id)
    }

    /// Slot 0 of the file's own material, when the glTF had one.
    pub fn imported_surface(&self, id: AssetId) -> Option<&ImportedSurface> {
        self.surfaces.iter().find(|(stored, _)| *stored == id).map(|(_, surface)| surface)
    }

    pub fn meshlets(&self, id: AssetId) -> Option<&crate::MeshletSet> {
        self.meshlets.iter().find(|(stored, _)| *stored == id).map(|(_, set)| set)
    }
}

/// Load the project catalog. A missing catalog is an empty library. A listed asset that cannot
/// be read or imported fails the whole load and does not invent a stand-in mesh.
pub fn load_project_mesh_assets(project_root: &Path, content_directory: &str, intermediate_directory: &str) -> Result<MeshAssetLibrary, String> {
    let catalog_path = catalog_path(project_root, content_directory);
    if !catalog_path.is_file() {
        return Ok(MeshAssetLibrary::default());
    }
    let text = fs::read_to_string(&catalog_path).map_err(|error| format!("asset catalog was not read: {error}"))?;
    let json = parse_json(&text).map_err(|error| format!("asset catalog was rejected: {error}"))?;
    if json.get("schema").and_then(Json::as_str) != Some(ASSET_CATALOG_SCHEMA) {
        return Err("asset catalog schema is not jarvig.assets".into());
    }
    let version = json.get("format_version").and_then(Json::as_f64).unwrap_or(0.0);
    if version != ASSET_CATALOG_VERSION as f64 {
        return Err(format!("asset catalog version {version} is not supported"));
    }
    let mut library = MeshAssetLibrary::default();
    for asset in json.get("assets").and_then(Json::as_array).unwrap_or(&[]) {
        let id = AssetId::parse(asset.get("id").and_then(Json::as_str).unwrap_or("")).ok_or_else(|| "asset catalog entry has no id".to_string())?;
        let name = asset.get("name").and_then(Json::as_str).unwrap_or("").to_string();
        let source_relative = asset.get("source").and_then(Json::as_str).unwrap_or("").to_string();
        if name.is_empty() || source_relative.is_empty() || source_relative.contains(':') || Path::new(&source_relative).is_absolute() {
            return Err(format!("asset {id} has no usable source"));
        }
        let source_path = project_root.join(Path::new(&source_relative));
        let bytes = fs::read(&source_path).map_err(|_| format!("source for mesh asset {name} is missing at {source_relative}"))?;
        let fingerprint = fingerprint(&bytes);
        let stored_fingerprint = asset.get("source_fingerprint").and_then(Json::as_str).unwrap_or("");
        let derived = derived_path(project_root, intermediate_directory, id);
        let mesh = if derived.is_file() && stored_fingerprint == fingerprint {
            let stored = fs::read(&derived).map_err(|error| error.to_string())?;
            let persisted = derived_includes_pick_bvh(&stored);
            match decode_derived_mesh(&stored) {
                Ok(mesh) => {
                    if !persisted {
                        let _ = write_derived(&derived, &mesh);
                    }
                    mesh
                }
                Err(_) => import_and_store(&bytes, &source_path, &derived)?,
            }
        } else {
            import_and_store(&bytes, &source_path, &derived)?
        };
        let bounds = mesh.bounds();
        let record = MeshAssetRecord {
            id,
            name,
            source_relative,
            external_source: asset.get("external_source").and_then(Json::as_str).unwrap_or("").to_string(),
            source_fingerprint: fingerprint.clone(),
            triangle_count: mesh.index_count() / 3,
            submesh_count: mesh.submeshes().len() as u32,
            bounds_min: bounds.aabb.min,
            bounds_max: bounds.aabb.max,
        };
        let clusters = load_or_build_meshlets(project_root, intermediate_directory, id, &mesh, &fingerprint);
        library.meshes.push((id, mesh));
        library.records.push(record);
        library.meshlets.push((id, clusters));
        ensure_imported_surface(&mut library, id, &bytes);
    }
    if stored_fingerprint_mismatch(&library, &json) {
        write_catalog(&catalog_path, &library)?;
    }
    Ok(library)
}

fn stored_fingerprint_mismatch(library: &MeshAssetLibrary, json: &Json) -> bool {
    let Some(assets) = json.get("assets").and_then(Json::as_array) else { return false };
    assets.iter().any(|asset| {
        let id = asset.get("id").and_then(Json::as_str).and_then(AssetId::parse);
        let stored = asset.get("source_fingerprint").and_then(Json::as_str).unwrap_or("");
        id.and_then(|id| library.record(id)).is_some_and(|record| record.source_fingerprint != stored)
    })
}

/// Copy an external GLB or glTF into `Content/Meshes`, write the canonical mesh, and return its `AssetId`.
///
/// The external file is only read. A rejected file does not create a catalog entry or a project copy.
/// Importing the same project copy again keeps the existing id.
pub fn import_mesh_into_project(project_root: &Path, content_directory: &str, intermediate_directory: &str, source_file: &Path) -> Result<(MeshAssetLibrary, AssetId), String> {
    let bytes = fs::read(source_file).map_err(|error| format!("mesh file was not read: {error}"))?;
    let file_name = source_file.file_name().and_then(|name| name.to_str()).ok_or("mesh file name is not usable")?;
    if !(file_name.to_ascii_lowercase().ends_with(".glb") || file_name.to_ascii_lowercase().ends_with(".gltf")) {
        return Err("only .glb and .gltf files can be imported".into());
    }
    let parent = source_file.parent().unwrap_or(Path::new(""));
    let mesh = import_gltf(&bytes, &mut |uri| {
        fs::read(parent.join(uri)).map_err(|error| format!("glTF buffer {uri} was not read: {error}"))
    })
    .map_err(|ImportError(text)| text)?;
    let source_relative = format!("{}/Meshes/{file_name}", content_directory.trim_matches('/'));
    let destination = project_root.join(Path::new(&source_relative));
    let external_source = source_file.display().to_string();
    let digest = fingerprint(&bytes);
    let mut library = load_project_mesh_assets(project_root, content_directory, intermediate_directory)?;
    if let Some(index) = library.records.iter().position(|record| record.source_relative.replace('\\', "/") == source_relative) {
        let id = library.records[index].id;
        if library.records[index].source_fingerprint == digest && library.get(id).is_some() {
            if library.records[index].external_source != external_source {
                library.records[index].external_source = external_source;
                write_catalog(&catalog_path(project_root, content_directory), &library)?;
            }
            ensure_imported_surface(&mut library, id, &bytes);
            return Ok((library, id));
        }
        let created_copy = store_project_copy(&destination, source_file, &bytes)?;
        let derived = derived_path(project_root, intermediate_directory, id);
        if let Err(error) = write_derived(&derived, &mesh) {
            if created_copy {
                let _ = fs::remove_file(&destination);
            }
            return Err(error);
        }
        let bounds = mesh.bounds();
        let record = &mut library.records[index];
        record.external_source = external_source;
        record.source_fingerprint = digest.clone();
        record.triangle_count = mesh.index_count() / 3;
        record.submesh_count = mesh.submeshes().len() as u32;
        record.bounds_min = bounds.aabb.min;
        record.bounds_max = bounds.aabb.max;
        let clusters = load_or_build_meshlets(project_root, intermediate_directory, id, &mesh, &digest);
        if let Some(slot) = library.meshes.iter_mut().find(|(stored, _)| *stored == id) {
            slot.1 = mesh;
        } else {
            library.meshes.push((id, mesh));
        }
        library.meshlets.retain(|(stored, _)| *stored != id);
        library.meshlets.push((id, clusters));
        ensure_imported_surface(&mut library, id, &bytes);
        if let Err(error) = write_catalog(&catalog_path(project_root, content_directory), &library) {
            return Err(error);
        }
        return Ok((library, id));
    }
    let id = AssetId::new();
    let mut name = source_file.file_stem().and_then(|stem| stem.to_str()).unwrap_or("Imported Mesh").to_string();
    if library.find_name(&name).is_some() {
        name = format!("{name} {}", &id.to_string()[..8]);
    }
    let created_copy = store_project_copy(&destination, source_file, &bytes)?;
    let derived = derived_path(project_root, intermediate_directory, id);
    if let Err(error) = write_derived(&derived, &mesh) {
        if created_copy {
            let _ = fs::remove_file(&destination);
        }
        return Err(error);
    }
    let bounds = mesh.bounds();
    library.records.push(MeshAssetRecord {
        id,
        name,
        source_relative,
        external_source,
        source_fingerprint: digest.clone(),
        triangle_count: mesh.index_count() / 3,
        submesh_count: mesh.submeshes().len() as u32,
        bounds_min: bounds.aabb.min,
        bounds_max: bounds.aabb.max,
    });
    let clusters = load_or_build_meshlets(project_root, intermediate_directory, id, &mesh, &digest);
    library.meshes.push((id, mesh));
    library.meshlets.push((id, clusters));
    ensure_imported_surface(&mut library, id, &bytes);
    if let Err(error) = write_catalog(&catalog_path(project_root, content_directory), &library) {
        let _ = fs::remove_file(&derived);
        if created_copy {
            let _ = fs::remove_file(&destination);
        }
        return Err(error);
    }
    Ok((library, id))
}

/// Writes the project copy when it is not already the file being imported.
/// Returns whether this call created that copy.
fn store_project_copy(destination: &Path, source_file: &Path, bytes: &[u8]) -> Result<bool, String> {
    if let Some(folder) = destination.parent() {
        fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    if same_file(destination, source_file) {
        return Ok(false);
    }
    if destination.is_file() {
        let existing = fs::read(destination).map_err(|error| format!("project mesh was not read: {error}"))?;
        if existing != bytes {
            return Err(format!(
                "{} already exists and does not match the file being imported. The project copy and the catalog were left unchanged.",
                destination.display()
            ));
        }
        return Ok(false);
    }
    fs::write(destination, bytes).map_err(|error| format!("project mesh copy was not written: {error}"))?;
    Ok(true)
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn import_and_store(bytes: &[u8], source_path: &Path, derived: &Path) -> Result<Mesh, String> {
    let parent = source_path.parent().unwrap_or(Path::new(""));
    let mesh = import_gltf(bytes, &mut |uri| fs::read(parent.join(uri)).map_err(|error| error.to_string())).map_err(|ImportError(text)| text)?;
    write_derived(derived, &mesh)?;
    Ok(mesh)
}

fn write_derived(path: &Path, mesh: &Mesh) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    fs::write(path, encode_derived_mesh(mesh)).map_err(|error| format!("derived mesh was not written: {error}"))
}

pub fn encode_derived_mesh(mesh: &Mesh) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(DERIVED_MAGIC);
    bytes.extend_from_slice(&MESH_IMPORTER_VERSION.to_le_bytes());
    bytes.extend_from_slice(&mesh.vertex_count().to_le_bytes());
    bytes.extend_from_slice(&mesh.index_count().to_le_bytes());
    bytes.extend_from_slice(&(mesh.submeshes().len() as u32).to_le_bytes());
    bytes.extend_from_slice(&mesh.streams()[0].bytes);
    let mut indices = Vec::new();
    for index in 0..mesh.index_count() {
        let value = match mesh.index_format() {
            MeshIndexFormat::Uint16 => {
                let start = index as usize * 2;
                u16::from_le_bytes(mesh.index_bytes()[start..start + 2].try_into().unwrap()) as u32
            }
            MeshIndexFormat::Uint32 => {
                let start = index as usize * 4;
                u32::from_le_bytes(mesh.index_bytes()[start..start + 4].try_into().unwrap())
            }
        };
        indices.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&indices);
    for submesh in mesh.submeshes() {
        bytes.extend_from_slice(&submesh.first_index.to_le_bytes());
        bytes.extend_from_slice(&submesh.index_count.to_le_bytes());
        bytes.extend_from_slice(&submesh.material_slot.to_le_bytes());
        for lane in submesh.bounds.aabb.min.iter().chain(submesh.bounds.aabb.max.iter()) {
            bytes.extend_from_slice(&lane.to_le_bytes());
        }
    }
    mesh.pick_bvh().write_bytes(&mut bytes);
    bytes
}

pub fn decode_derived_mesh(bytes: &[u8]) -> Result<Mesh, String> {
    if bytes.len() < 24 || &bytes[0..8] != DERIVED_MAGIC {
        return Err("derived mesh header is not JARVIG canonical geometry".into());
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if version != MESH_IMPORTER_VERSION {
        return Err(format!("derived mesh version {version} is not supported"));
    }
    let vertex_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let index_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let submesh_count = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let vertex_bytes = vertex_count * 60;
    let index_bytes = index_count * 4;
    let tail = 24 + vertex_bytes + index_bytes;
    if bytes.len() < tail + submesh_count * 36 {
        return Err("derived mesh is truncated".into());
    }
    let mut surfaces = Vec::with_capacity(submesh_count);
    let vertices = &bytes[24..24 + vertex_bytes];
    let indices = &bytes[24 + vertex_bytes..tail];
    for submesh_index in 0..submesh_count {
        let start = tail + submesh_index * 36;
        let first = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap());
        let count = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
        let slot = u32::from_le_bytes(bytes[start + 8..start + 12].try_into().unwrap());
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut texcoords = Vec::new();
        let mut tangents = Vec::new();
        let mut local_indices = Vec::new();
        let mut remap = vec![u32::MAX; vertex_count];
        for offset in 0..count {
            let index_at = (first + offset) as usize * 4;
            if index_at + 4 > indices.len() {
                return Err("derived mesh index is outside the buffer".into());
            }
            let vertex = u32::from_le_bytes(indices[index_at..index_at + 4].try_into().unwrap()) as usize;
            if vertex >= vertex_count {
                return Err("derived mesh index is outside the vertices".into());
            }
            if remap[vertex] == u32::MAX {
                let from = vertex * 60;
                remap[vertex] = positions.len() as u32;
                positions.push(read3(&vertices[from..from + 12]));
                texcoords.push(read2(&vertices[from + 24..from + 32]));
                normals.push(read3(&vertices[from + 32..from + 44]));
                tangents.push(read4(&vertices[from + 44..from + 60]));
            }
            local_indices.push(remap[vertex]);
        }
        surfaces.push(crate::mesh::CanonicalSurface { positions, normals, texcoords, tangents, indices: local_indices, material_slot: slot });
    }
    let geometry_end = tail + submesh_count * 36;
    if let Ok(bvh) = crate::bvh::TriangleBvh::read_bytes(&bytes[geometry_end..]) {
        crate::mesh::mesh_from_surfaces_with_bvh(&surfaces, bvh).map_err(|error: MeshError| error.to_string())
    } else {
        crate::mesh::mesh_from_surfaces(&surfaces).map_err(|error: MeshError| error.to_string())
    }
}

pub fn derived_includes_pick_bvh(bytes: &[u8]) -> bool {
    if bytes.len() < 24 || &bytes[0..8] != DERIVED_MAGIC {
        return false;
    }
    let vertex_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap_or([0; 4])) as usize;
    let index_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap_or([0; 4])) as usize;
    let submesh_count = u32::from_le_bytes(bytes[20..24].try_into().unwrap_or([0; 4])) as usize;
    let geometry_end = 24 + vertex_count * 60 + index_count * 4 + submesh_count * 36;
    bytes.get(geometry_end..geometry_end + 8).is_some_and(|magic| magic == b"JARVBVH1")
}

fn read3(bytes: &[u8]) -> [f32; 3] {
    [
        f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    ]
}

fn read2(bytes: &[u8]) -> [f32; 2] {
    [f32::from_le_bytes(bytes[0..4].try_into().unwrap()), f32::from_le_bytes(bytes[4..8].try_into().unwrap())]
}

fn read4(bytes: &[u8]) -> [f32; 4] {
    [read3(bytes)[0], read3(bytes)[1], read3(bytes)[2], f32::from_le_bytes(bytes[12..16].try_into().unwrap())]
}

fn meshlet_path(root: &Path, intermediate_directory: &str, id: AssetId) -> PathBuf {
    root.join(intermediate_directory).join("Meshes").join(format!("{id}.jarvigmeshlets"))
}

fn load_or_build_meshlets(project_root: &Path, intermediate_directory: &str, id: AssetId, mesh: &Mesh, fingerprint: &str) -> crate::MeshletSet {
    let path = meshlet_path(project_root, intermediate_directory, id);
    if let Ok(bytes) = fs::read(&path) {
        let started = std::time::Instant::now();
        if let Ok(mut set) = crate::decode_meshlets(&bytes) {
            if meshlet_sidecar_matches(&set, mesh, fingerprint) {
                set.stats.load_ms = started.elapsed().as_secs_f32() * 1000.0;
                set.stats.derived_bytes = bytes.len() as u64;
                return set;
            }
        }
    }
    let mut set = crate::build_meshlets(mesh);
    set.importer_version = MESH_IMPORTER_VERSION;
    set.source_fingerprint = fingerprint.to_string();
    set.source_vertices = mesh.vertex_count();
    set.stats.derived_bytes = crate::encode_meshlets(&set).len() as u64;
    if let Some(folder) = path.parent() {
        let _ = fs::create_dir_all(folder);
    }
    let writing = std::time::Instant::now();
    let encoded = crate::encode_meshlets(&set);
    let _ = fs::write(&path, &encoded);
    set.stats.write_ms = writing.elapsed().as_secs_f32() * 1000.0;
    set.stats.derived_bytes = encoded.len() as u64;
    set
}

fn meshlet_sidecar_matches(set: &crate::MeshletSet, mesh: &Mesh, fingerprint: &str) -> bool {
    set.stats.source_triangles == mesh.index_count() / 3
        && set.source_vertices == mesh.vertex_count()
        && set.builder_version == crate::MESHLET_BUILDER_VERSION
        && set.importer_version == MESH_IMPORTER_VERSION
        && set.max_vertices == crate::MESHLET_MAX_VERTICES
        && set.max_triangles == crate::MESHLET_MAX_TRIANGLES
        && set.source_fingerprint == fingerprint
}

fn ensure_imported_surface(library: &mut MeshAssetLibrary, id: AssetId, bytes: &[u8]) {
    if library.imported_surface(id).is_some() {
        return;
    }
    if let Some(surface) = decode_imported_surface(bytes) {
        library.surfaces.push((id, surface));
    }
}

fn decode_imported_surface(bytes: &[u8]) -> Option<ImportedSurface> {
    let source = gltf_surfaces(bytes).ok()?.into_iter().next()?;
    Some(ImportedSurface {
        base_color_factor: source.base_color,
        metallic: source.metallic,
        roughness: source.roughness,
        normal_scale: source.normal_scale,
        double_sided: source.double_sided,
        alpha_mode: source.alpha_mode,
        base_color: decode_gltf_image(source.base_color_image.as_ref(), ImageKind::Color),
        orm: decode_gltf_image(source.metallic_roughness_image.as_ref(), ImageKind::Orm),
        normal: decode_gltf_image(source.normal_image.as_ref(), ImageKind::Normal),
    })
}

enum ImageKind {
    Color,
    Orm,
    Normal,
}

fn decode_gltf_image(image: Option<&GltfImage>, kind: ImageKind) -> Option<Texture> {
    let image = image?;
    if !image.mime.eq_ignore_ascii_case("image/jpeg") && !image.bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let (width, height, mut pixels) = decode_jpeg_rgba(&image.bytes).ok()?;
    match kind {
        ImageKind::Color => {}
        ImageKind::Orm => {
            for pixel in pixels.chunks_exact_mut(4) {
                let roughness = pixel[1];
                let metallic = pixel[2];
                pixel[0] = 255;
                pixel[1] = roughness;
                pixel[2] = metallic;
                pixel[3] = 255;
            }
        }
        ImageKind::Normal => crate::negate_normal_green(&mut pixels),
    }
    let content = match kind {
        ImageKind::Color => MipContent::Srgb,
        ImageKind::Orm => MipContent::Linear,
        ImageKind::Normal => MipContent::Normal,
    };
    let (width, height, pixels) = crate::downsample_long_side(width, height, pixels, 2048, content);
    let space = if matches!(kind, ImageKind::Color) { ColorSpace::Srgb } else { ColorSpace::Linear };
    Texture::rgba8(width, height, 1, space, pixels).ok()
}

fn decode_jpeg_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    use std::io::Cursor;
    use zune_jpeg::JpegDecoder;
    let mut decoder = JpegDecoder::new(Cursor::new(bytes));
    let pixels = decoder.decode().map_err(|error| error.to_string())?;
    let (width, height) = decoder.dimensions().ok_or_else(|| "jpeg has no dimensions".to_string())?;
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err("jpeg size is not usable".into());
    }
    let rgba = if pixels.len() == width * height * 4 {
        pixels
    } else if pixels.len() == width * height * 3 {
        let mut expanded = Vec::with_capacity(width * height * 4);
        for pixel in pixels.chunks_exact(3) {
            expanded.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        expanded
    } else {
        return Err(format!("jpeg decoded {} bytes for a {width}x{height} image", pixels.len()));
    };
    Ok((width as u32, height as u32, rgba))
}

fn catalog_path(root: &Path, content_directory: &str) -> PathBuf {
    root.join(content_directory).join("Assets").join("catalog.jarvigassets")
}

fn derived_path(root: &Path, intermediate_directory: &str, id: AssetId) -> PathBuf {
    root.join(intermediate_directory).join("Meshes").join(format!("{id}.jarvigmesh"))
}

fn write_catalog(path: &Path, library: &MeshAssetLibrary) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let assets = library.records.iter().map(|record| {
        format!(
            "{{\"id\":\"{}\",\"kind\":\"mesh\",\"name\":{},\"source\":\"{}\",\"external_source\":{},\"importer\":\"jarvig.gltf\",\"importer_version\":{MESH_IMPORTER_VERSION},\"source_fingerprint\":{},\"triangles\":{},\"submeshes\":{},\"bounds_min\":{},\"bounds_max\":{}}}",
            record.id,
            json_string(&record.name),
            record.source_relative.replace('\\', "/"),
            json_string(&record.external_source),
            json_string(&record.source_fingerprint),
            record.triangle_count,
            record.submesh_count,
            json_vec3(record.bounds_min),
            json_vec3(record.bounds_max)
        )
    });
    let text = format!("{{\"schema\":\"{ASSET_CATALOG_SCHEMA}\",\"format_version\":{ASSET_CATALOG_VERSION},\"assets\":[{}]}}", assets.collect::<Vec<_>>().join(","));
    fs::write(path, text).map_err(|error| format!("asset catalog was not written: {error}"))
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn json_vec3(value: [f32; 3]) -> String {
    format!("[{},{},{}]", value[0] as f64, value[1] as f64, value[2] as f64)
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The external proof mesh. About eighty thousand curved triangles plus a second material slot.
pub fn proof_mesh_glb() -> Vec<u8> {
    curved_prop_glb(80_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EnvironmentLight, HighPrecisionPose, LevelDocument, MaterialAssetRef, SceneWorld};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn import_writes_an_asset_id_and_a_rejected_file_writes_nothing() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("jarvig-mesh-{stamp}"));
        std::fs::create_dir_all(root.join("Content")).unwrap();
        let source = root.join("incoming.glb");
        std::fs::write(&source, curved_prop_glb(4_000)).unwrap();
        let bad = root.join("bad.glb");
        std::fs::write(&bad, b"not a mesh").unwrap();
        assert!(import_mesh_into_project(&root, "Content", "Intermediate", &bad).is_err());
        assert!(!root.join("Content/Assets/catalog.jarvigassets").exists());
        assert!(!root.join("Content/Meshes/bad.glb").exists());
        let (library, id) = import_mesh_into_project(&root, "Content", "Intermediate", &source).unwrap();
        let record = library.record(id).unwrap();
        assert!(record.triangle_count >= 4_000);
        assert!(record.submesh_count >= 2);
        assert_eq!(record.source_relative, "Content/Meshes/incoming.glb");
        assert!(record.external_source.contains("incoming.glb"));
        assert!(record.bounds_min[0] < record.bounds_max[0]);
        assert!(root.join(&record.source_relative).is_file());
        assert!(!root.join("Content/Source").exists());
        let mesh = library.get(id).unwrap();
        assert!(mesh.submeshes().iter().any(|submesh| submesh.material_slot == 0));
        assert!(mesh.submeshes().iter().any(|submesh| submesh.material_slot == 1));
        let bytes = &mesh.streams()[0].bytes;
        let normal = [f32::from_le_bytes(bytes[32..36].try_into().unwrap()), f32::from_le_bytes(bytes[36..40].try_into().unwrap()), f32::from_le_bytes(bytes[40..44].try_into().unwrap())];
        let uv = [f32::from_le_bytes(bytes[24..28].try_into().unwrap()), f32::from_le_bytes(bytes[28..32].try_into().unwrap())];
        let tangent_w = f32::from_le_bytes(bytes[56..60].try_into().unwrap());
        assert!(normal.iter().all(|lane| lane.is_finite()));
        assert!(uv.iter().all(|lane| lane.is_finite()));
        assert!((tangent_w.abs() - 1.0).abs() < 1.0e-5);
        let (again_library, again_id) = import_mesh_into_project(&root, "Content", "Intermediate", &source).unwrap();
        assert_eq!(again_id, id);
        assert_eq!(again_library.records().len(), 1);
        let loaded = load_project_mesh_assets(&root, "Content", "Intermediate").unwrap();
        assert_eq!(loaded.records()[0].id, id);
        assert_eq!(loaded.records()[0].external_source, record.external_source);
        assert_eq!(loaded.get(id).unwrap().index_count(), library.get(id).unwrap().index_count());
        let mut world = SceneWorld::new_session();
        world.install_imported_meshes(&loaded);
        world.spawn_saved_world_settings(crate::EntityId::new(), "World Settings", EnvironmentLight::bootstrap()).unwrap();
        let material = MaterialAssetRef::builtin("standard_white", [1.0, 1.0, 1.0, 1.0], 0.0, 0.5, [0.0, 0.0, 0.0, 0.0]);
        world.place_imported_mesh(id, &record.name, material.clone()).unwrap();
        world.place_imported_mesh(id, &record.name, material).unwrap();
        let document = LevelDocument::capture(&world, crate::EntityId::new(), "Import").unwrap();
        let json = document.to_json();
        assert!(json.contains("jarvig.asset"));
        assert!(json.contains(&id.to_string()));
        assert!(!json.contains("incoming.glb"));
        assert!(!json.contains("Content/Meshes"));
        assert!(!json.contains(&record.external_source));
        let parsed = crate::parse_level(&json).unwrap();
        let again = parsed.instantiate_with(&loaded).unwrap();
        assert_eq!(again.object_count(), 2);
        let snapshot = again.extract(crate::RenderFrameId(1)).unwrap();
        assert_eq!(snapshot.instances()[0].mesh, snapshot.instances()[1].mesh);
        assert!(snapshot.instances().iter().all(|instance| instance.cast_shadows && instance.receive_shadows && instance.visible));
        assert_eq!(again.mesh_material_slots(snapshot.instances()[0].source), vec![0, 1]);
        let _ = HighPrecisionPose::IDENTITY;
        let clusters = library.meshlets(id).expect("meshlets");
        assert!(clusters.stats.meshlet_count >= 1);
        assert!(clusters.meshlets.iter().any(|meshlet| meshlet.material_slot == 0));
        assert!(clusters.meshlets.iter().any(|meshlet| meshlet.material_slot == 1));
        assert!(crate::meshlets_cover_source(library.get(id).unwrap(), clusters));
        let sidecar = root.join("Intermediate/Meshes").join(format!("{id}.jarvigmeshlets"));
        let modified = fs::metadata(&sidecar).unwrap().modified().unwrap();
        let reloaded = load_project_mesh_assets(&root, "Content", "Intermediate").unwrap();
        assert_eq!(fs::metadata(&sidecar).unwrap().modified().unwrap(), modified, "reload rebuilt meshlets");
        assert!(reloaded.meshlets(id).unwrap().stats.load_ms >= 0.0);
        fs::write(&sidecar, b"corrupt").unwrap();
        let recovered = load_project_mesh_assets(&root, "Content", "Intermediate").unwrap();
        assert!(crate::meshlets_cover_source(recovered.get(id).unwrap(), recovered.meshlets(id).unwrap()));
        let mut poked = fs::read(&sidecar).unwrap();
        poked[56] ^= 1;
        fs::write(&sidecar, &poked).unwrap();
        let invalidated = load_project_mesh_assets(&root, "Content", "Intermediate").unwrap();
        assert!(crate::meshlets_cover_source(invalidated.get(id).unwrap(), invalidated.meshlets(id).unwrap()));
        assert_ne!(fs::metadata(&sidecar).unwrap().modified().unwrap(), modified);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "clusters the stored townshop derived mesh"]
    fn townshop_meshlets_partition_the_derived_mesh() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/lighting-lab");
        let library = load_project_mesh_assets(&root, "Content", "Intermediate").expect("assets");
        let record = library.records().iter().find(|record| record.name == "townshop").expect("townshop");
        let mesh = library.get(record.id).expect("mesh");
        let set = library.meshlets(record.id).expect("meshlets");
        assert_eq!(set.stats.source_triangles, mesh.index_count() / 3);
        assert!(set.stats.meshlet_count > 1_000, "count {}", set.stats.meshlet_count);
        assert!(set.stats.max_vertices <= crate::MESHLET_MAX_VERTICES);
        assert!(set.stats.max_triangles <= crate::MESHLET_MAX_TRIANGLES);
        assert!(crate::meshlets_cover_source(mesh, set));
        let draw_at = std::time::Instant::now();
        let draw = crate::meshlet_draw(set);
        let draw_ms = draw_at.elapsed().as_secs_f32() * 1000.0;
        assert_eq!(draw.colors.len() as u32, set.stats.source_triangles);
        assert_eq!(draw.indices.len() as u32, set.stats.source_triangles * 3);
        let radius_sum: f64 = set.meshlets.iter().map(|meshlet| f64::from(meshlet.sphere_radius)).sum();
        let mean_radius = radius_sum / set.meshlets.len() as f64;
        let max_radius = set.meshlets.iter().map(|meshlet| meshlet.sphere_radius).fold(0.0f32, f32::max);
        let mesh_radius = mesh.bounds().sphere.radius;
        assert!(mean_radius < f64::from(mesh_radius) * 0.5, "mean cluster radius {mean_radius} is not smaller than the mesh {mesh_radius}");
        let path = root.join("Intermediate/Meshes").join(format!("{}.jarvigmeshlets", record.id));
        assert!(path.is_file());
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let reloaded_at = std::time::Instant::now();
        let again = load_project_mesh_assets(&root, "Content", "Intermediate").expect("reload");
        let reload_ms = reloaded_at.elapsed().as_secs_f32() * 1000.0;
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified, "reload rebuilt the meshlet file");
        let again_set = again.meshlets(record.id).expect("reloaded meshlets");
        assert_eq!(again_set.stats.meshlet_count, set.stats.meshlet_count);
        assert_eq!(again_set.stats.build_ms, set.stats.build_ms);
        println!(
            "JRV-0028 townshop triangles={} vertices={} meshlets={} avg_tris={:.2} avg_verts={:.2} min_tris={} max_tris={} min_verts={} max_verts={} bytes={} build_ms={:.1} write_ms={:.1} load_ms={:.1} reload_ms={:.1} draw_ms={:.1} grid={} builder={} importer={} fingerprint={} format=JARVMLET2 mean_radius={:.4} max_radius={:.4} mesh_radius={:.4} cover=exact",
            set.stats.source_triangles,
            mesh.vertex_count(),
            set.stats.meshlet_count,
            set.stats.average_triangles,
            set.stats.average_vertices,
            set.stats.min_triangles,
            set.stats.max_triangles,
            set.stats.min_vertices,
            set.stats.max_vertices,
            set.stats.derived_bytes,
            set.stats.build_ms,
            set.stats.write_ms,
            again_set.stats.load_ms,
            reload_ms,
            draw_ms,
            set.grid_resolution,
            set.builder_version,
            set.importer_version,
            set.source_fingerprint,
            mean_radius,
            max_radius,
            mesh_radius
        );
    }

    /// External fixture. Not part of the default suite: it reads `C:\tmp\townshop.glb` and writes the Lighting Lab project.
    #[test]
    #[ignore = "imports C:\\tmp\\townshop.glb into samples/lighting-lab"]
    fn townshop_external_file_imports_into_lighting_lab() {
        let source = PathBuf::from(r"C:\tmp\townshop.glb");
        assert!(source.is_file(), "external fixture missing at {}", source.display());
        let before_external = fs::read(&source).expect("external fixture was not read");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/lighting-lab");
        let level_path = root.join("Content/Levels/LightingLab.jarviglevel");
        let level_before = fs::read(&level_path).expect("Lighting Lab level missing");
        let (library, id) = import_mesh_into_project(&root, "Content", "Intermediate", &source).expect("townshop import");
        assert_eq!(fs::read(&source).unwrap(), before_external, "the external file was modified");
        assert_eq!(fs::read(&level_path).unwrap(), level_before, "import rewrote the level");
        let record = library.record(id).expect("record");
        assert_eq!(record.source_relative, "Content/Meshes/townshop.glb");
        assert!(record.external_source.to_ascii_lowercase().ends_with(r"tmp\townshop.glb") || record.external_source.replace('/', "\\").to_ascii_lowercase().ends_with(r"tmp\townshop.glb"));
        assert!(record.triangle_count > 5_000_000, "triangles {}", record.triangle_count);
        assert!(record.submesh_count >= 1);
        assert!(record.bounds_min.iter().all(|lane| lane.is_finite()));
        assert!(record.bounds_max[0] > record.bounds_min[0]);
        let surface = library.imported_surface(id).expect("townshop material");
        assert!(surface.base_color.is_some(), "base color image was not decoded");
        assert!(surface.orm.is_some(), "metallic-roughness image was not decoded");
        assert!(surface.normal.is_some(), "normal image was not decoded");
        assert!(root.join("Content/Meshes/townshop.glb").is_file());
        assert!(!level_before.windows(6).any(|window| window == b"C:\\tmp") );
        let parsed = crate::parse_level(std::str::from_utf8(&level_before).unwrap()).unwrap();
        let original_ids: Vec<_> = parsed.entities.iter().map(|entity| entity.uuid).collect();
        let mut world = parsed.instantiate_with(&library).expect("instantiate");
        let round_trip = LevelDocument::capture(&world, parsed.level_uuid, parsed.name.clone()).unwrap();
        let round_ids: Vec<_> = round_trip.entities.iter().map(|entity| entity.uuid).collect();
        assert_eq!(round_ids.len(), original_ids.len());
        for id in &original_ids {
            assert!(round_ids.contains(id), "capture dropped {id}");
        }
        let already = world.entity_outline().iter().any(|row| matches!(world.authored_mesh(row.uuid), Some((_, _, _, _, _, _, crate::MeshAssetRef::Asset { id: stored, .. }, _)) if stored == id));
        if !already {
            let material = MaterialAssetRef {
                scheme: crate::MaterialScheme::Mesh,
                name: "Imported".into(),
                base_color: [1.0, 1.0, 1.0, 1.0],
                metallic: 1.0,
                roughness: 1.0,
                emissive: [0.0, 0.0, 0.0, 0.0],
                uv_scale: 1.0,
                normal_scale: 1.0,
                normal_convention: Some(crate::NormalConvention::DirectXNegativeY),
            };
            let entity = world.place_imported_mesh(id, &record.name, material).unwrap();
            world.set_entity_local_translation(entity, crate::Vec3::new(4.0, 0.85, -4.0)).unwrap();
        }
        let saved = LevelDocument::capture(&world, parsed.level_uuid, parsed.name.clone()).unwrap();
        let saved_json = saved.to_json();
        assert!(saved_json.contains(&id.to_string()));
        assert!(!saved_json.contains("townshop.glb"));
        assert!(!saved_json.to_ascii_lowercase().contains("c:\\tmp"));
        assert!(!saved_json.to_ascii_lowercase().contains("c:/tmp"));
        for entity in &parsed.entities {
            assert!(saved.entities.iter().any(|kept| kept.uuid == entity.uuid), "save dropped {}", entity.name);
        }
        let backup = root.join("Saved/Backup");
        crate::save_level_atomic(&level_path, &backup, &saved).expect("level save");
        let reloaded = crate::load_level_file(&level_path).unwrap();
        let loaded_assets = load_project_mesh_assets(&root, "Content", "Intermediate").unwrap();
        let runtime = reloaded.instantiate_with(&loaded_assets).unwrap();
        let snapshot = runtime.extract(crate::RenderFrameId(1)).unwrap();
        let placed = snapshot.instances().iter().find(|instance| instance.entity == saved.entities.iter().find(|entity| entity.name == "townshop").unwrap().uuid).unwrap();
        assert!(placed.cast_shadows && placed.receive_shadows && placed.visible);
        assert_eq!(runtime.mesh_material_slots(placed.source), vec![0]);
        let mut app = crate::GameApplication::new();
        app.set_mesh_assets(loaded_assets);
        app.load(reloaded).unwrap();
        app.start().unwrap();
        let playing = app.runtime_world().unwrap();
        assert!(playing.entity_outline().iter().any(|row| row.name == "townshop"));
        assert!(playing.entity_outline().iter().any(|row| row.name == "Metal Sphere"));
    }

    /// Local Sketchfab map. Not a character, and not part of the default suite.
    #[test]
    #[ignore = "imports the local Ghost City glb into samples/ghost-city and does not touch Lighting Lab"]
    fn ghost_city_glb_imports_without_defining_a_joint() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/ghost-city/third-party/source/BLD_Ghost_city.glb");
        assert!(source.is_file(), "missing {}", source.display());
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/ghost-city");
        let lab = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/lighting-lab/Content/Levels/LightingLab.jarviglevel");
        let lab_before = fs::read(&lab).expect("lighting lab");
        let (library, id) = import_mesh_into_project(&root, "Content", "Intermediate", &source).expect("ghost city import");
        assert_eq!(fs::read(&lab).unwrap(), lab_before, "import rewrote Lighting Lab");
        let record = library.record(id).expect("record");
        assert!(record.triangle_count > 10_000 && record.triangle_count < 200_000, "triangles {}", record.triangle_count);
        assert!(record.submesh_count >= 1, "submeshes {}", record.submesh_count);
        let surface = library.imported_surface(id);
        let decoded = surface.is_some_and(|surface| surface.base_color.is_some() || surface.normal.is_some() || surface.orm.is_some());
        let base_color = surface.map(|surface| surface.base_color_factor).unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let metallic = surface.map(|surface| surface.metallic).unwrap_or(1.0);
        let roughness = surface.map(|surface| surface.roughness).unwrap_or(1.0);
        let normal_scale = surface.map(|surface| surface.normal_scale).unwrap_or(1.0);
        let mut world = SceneWorld::new_session();
        world.install_imported_meshes(&library);
        let settings = crate::EntityId::parse("22222222-2222-4222-8222-2222222222ff").unwrap();
        world.spawn_saved_world_settings(settings, "World Settings", EnvironmentLight::bootstrap()).unwrap();
        let material = MaterialAssetRef {
            scheme: crate::MaterialScheme::Mesh,
            name: "Imported".into(),
            base_color,
            metallic,
            roughness,
            emissive: [0.0, 0.0, 0.0, 0.0],
            uv_scale: 1.0,
            normal_scale,
            normal_convention: Some(crate::NormalConvention::DirectXNegativeY),
        };
        world.place_imported_mesh(id, &record.name, material).unwrap();
        let level_id = crate::EntityId::parse("22222222-2222-4222-8222-2222222222ee").unwrap();
        let level = LevelDocument::capture(&world, level_id, "Ghost City").unwrap();
        assert_eq!(level.format_version, crate::LEVEL_FORMAT_VERSION);
        assert!(!level.to_json().contains("\"Joint\""));
        let level_path = root.join("Content/Levels/GhostCity.jarviglevel");
        fs::create_dir_all(level_path.parent().unwrap()).unwrap();
        fs::write(&level_path, level.to_json()).unwrap();
        fs::create_dir_all(root.join("Config")).unwrap();
        fs::write(
            root.join("GhostCity.jarvigproject"),
            "{\n  \"schema\": \"jarvig.project\",\n  \"format_version\": 1,\n  \"project_uuid\": \"22222222-2222-4222-8222-2222222222dd\",\n  \"display_name\": \"Ghost City\",\n  \"engine_version\": \"0.0.1\",\n  \"startup_level\": \"Content/Levels/GhostCity.jarviglevel\",\n  \"content_directory\": \"Content\",\n  \"saved_directory\": \"Saved\",\n  \"config_directory\": \"Config\",\n  \"intermediate_directory\": \"Intermediate\",\n  \"settings\": \"Config/Project.jarvigsettings\"\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("Config/Project.jarvigsettings"),
            "{\n  \"schema\": \"jarvig.settings\",\n  \"format_version\": 1,\n  \"default_player_controller\": \"JARVIG.PlayerController\",\n  \"default_pawn\": \"JARVIG.DefaultFreeFlyPawn\",\n  \"default_mapping_context\": \"JARVIG.Default\",\n  \"startup_camera\": \"pawn\"\n}\n",
        )
        .unwrap();
        eprintln!("GHOST_CITY triangles={} submeshes={} decoded_image={}", record.triangle_count, record.submesh_count, decoded);
    }

    /// Local default bodies. One standing rigid copy each, then a character document.
    #[test]
    #[ignore = "imports the local Base and Base Male source glb files into samples/base-characters and does not touch Lighting Lab"]
    fn base_characters_import_as_bind_pose_meshes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/base-characters");
        let source_dir = root.join("third-party/source");
        let base_file = source_dir.join("Base.source.glb");
        let male_file = source_dir.join("Base Male.source.glb");
        assert!(base_file.is_file(), "missing {}", base_file.display());
        assert!(male_file.is_file(), "missing {}", male_file.display());
        let base_assembly = crate::assemble_rigid_bind_pose(&fs::read(&base_file).unwrap()).expect("base assembly");
        let male_assembly = crate::assemble_rigid_bind_pose(&fs::read(&male_file).unwrap()).expect("male assembly");
        assert_bind_body(&base_assembly, 66, 158_224, 409_188, 70, 1.0e-6);
        assert_bind_body(&male_assembly, 66, 150_752, 312_596, 52, 0.08);
        assert!(base_assembly.parts.iter().all(|part| part.source_parent == "RootNode"));
        assert!(male_assembly.parts.iter().all(|part| part.source_parent == "RootNode"));
        assert!(male_assembly.parts.iter().any(|part| part.name == "upperArmMesh.002"));
        assert!(male_assembly.parts.iter().any(|part| part.name == "shoulderMesh.002"));
        assert!(male_assembly.outliers.iter().any(|part| part.name == "foreArmMesh.002"));
        assert!(!male_assembly.parts.iter().any(|part| part.name == "foreArmMesh.002"));
        assert!(mirror_gap(&male_assembly, "upperArmMesh.001", "upperArmMesh.002") <= 0.03);
        let lab = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/lighting-lab/Content/Levels/LightingLab.jarviglevel");
        let lab_before = fs::read(&lab).expect("lighting lab");
        let _ = fs::remove_dir_all(root.join("Content"));
        let _ = fs::remove_dir_all(root.join("Intermediate"));
        let parts_dir = root.join("third-party/parts");
        let _ = fs::remove_dir_all(&parts_dir);
        fs::create_dir_all(&parts_dir).unwrap();
        let (_base_library, base_assets) = import_bind_parts(&root, &parts_dir, "Base", &base_assembly);
        let (library, male_assets) = import_bind_parts(&root, &parts_dir, "BaseMale", &male_assembly);
        assert_eq!(fs::read(&lab).unwrap(), lab_before, "import rewrote Lighting Lab");
        assert_eq!(library.record(base_assets[0]).is_some(), true);
        let base_uuid = crate::EntityId::parse("33333333-3333-4333-8333-333333333310").unwrap();
        let male_uuid = crate::EntityId::parse("33333333-3333-4333-8333-333333333320").unwrap();
        let base_build = crate::character_from_bind_pose("Base", base_uuid, 0x10, &base_assembly, &base_assets).expect("base character");
        let male_build = crate::character_from_bind_pose("Base Male", male_uuid, 0x20, &male_assembly, &male_assets).expect("male character");
        assert_eq!(crate::parse_character(&base_build.document.to_json()).unwrap(), base_build.document);
        assert_eq!(crate::parse_character(&male_build.document.to_json()).unwrap(), male_build.document);
        let base_origin = viewing_origin(&base_assembly, -0.175);
        let male_origin = viewing_origin(&male_assembly, 0.175);
        assert_eq!(base_origin.y, 0.0);
        assert_eq!(male_origin.y, 0.0);
        let mut world = SceneWorld::new_session();
        world.install_imported_meshes(&library);
        let settings = crate::EntityId::parse("33333333-3333-4333-8333-3333333333ff").unwrap();
        world.spawn_saved_world_settings(settings, "World Settings", EnvironmentLight::bootstrap()).unwrap();
        base_build.document.instantiate_preserving_ids(&mut world, base_origin).unwrap();
        male_build.document.instantiate_preserving_ids(&mut world, male_origin).unwrap();
        assert_placed_pose(&world, &base_build.document, &base_assembly, base_origin);
        assert_placed_pose(&world, &male_build.document, &male_assembly, male_origin);
        let level_id = crate::EntityId::parse("33333333-3333-4333-8333-3333333333ee").unwrap();
        let level = LevelDocument::capture(&world, level_id, "Base Characters").unwrap();
        assert_eq!(level.format_version, crate::LEVEL_JOINT_VERSION);
        let json = level.to_json();
        assert!(json.contains("\"Joint\""));
        assert!(json.contains("upperArmMesh.002"));
        assert!(!json.contains("foreArmMesh.002"));
        assert!(!json.to_ascii_lowercase().contains("c:\\tmp"));
        assert!(!json.to_ascii_lowercase().contains(".fbx"));
        assert_eq!(json.matches("\"cast_shadows\": true").count(), 132);
        assert_eq!(json.matches("\"kind\": \"Fixed\"").count(), 2);
        assert_eq!(json.matches("\"kind\": \"Ball\"").count(), 130);
        let level_path = root.join("Content/Levels/Base.jarviglevel");
        fs::create_dir_all(level_path.parent().unwrap()).unwrap();
        fs::write(&level_path, &json).unwrap();
        let characters = root.join("Content/Characters");
        fs::create_dir_all(&characters).unwrap();
        fs::write(characters.join("Base.jarvigcharacter"), base_build.document.to_json()).unwrap();
        fs::write(characters.join("Base Male.jarvigcharacter"), male_build.document.to_json()).unwrap();
        let mut report = String::new();
        report.push_str(&base_build.report);
        report.push_str(&format!("\nviewing_offset_m: one rigid root translation t=({:.6}, 0, {:.6}). Part deltas match the source. The character file stays in source space.\n\n", base_origin.x, base_origin.z));
        report.push_str(&male_build.report);
        report.push_str(&format!("\nviewing_offset_m: one rigid root translation t=({:.6}, 0, {:.6}). Part deltas match the source. The character file stays in source space.\n", male_origin.x, male_origin.z));
        fs::write(root.join("bind-pose-report.txt"), &report).unwrap();
        fs::create_dir_all(root.join("Config")).unwrap();
        fs::create_dir_all(root.join("Saved/Editor")).unwrap();
        fs::write(
            root.join("BaseCharacters.jarvigproject"),
            "{\n  \"schema\": \"jarvig.project\",\n  \"format_version\": 1,\n  \"project_uuid\": \"33333333-3333-4333-8333-3333333333dd\",\n  \"display_name\": \"Base Characters\",\n  \"engine_version\": \"0.0.1\",\n  \"startup_level\": \"Content/Levels/Base.jarviglevel\",\n  \"content_directory\": \"Content\",\n  \"saved_directory\": \"Saved\",\n  \"config_directory\": \"Config\",\n  \"intermediate_directory\": \"Intermediate\",\n  \"settings\": \"Config/Project.jarvigsettings\"\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("Config/Project.jarvigsettings"),
            "{\n  \"schema\": \"jarvig.settings\",\n  \"format_version\": 1,\n  \"default_player_controller\": \"JARVIG.PlayerController\",\n  \"default_pawn\": \"JARVIG.DefaultFreeFlyPawn\",\n  \"default_mapping_context\": \"JARVIG.Default\",\n  \"startup_camera\": \"pawn\"\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("Saved/Editor/viewport.json"),
            format!(
                "{{\n  \"schema\": \"jarvig.editor-viewport\",\n  \"format_version\": 1,\n  \"position\": [{}, 0.95, 4.6],\n  \"yaw\": 0,\n  \"pitch\": -0.08,\n  \"speed_m_s\": 2\n}}\n",
                crate::BOOTSTRAP_ROOT_M
            ),
        )
        .unwrap();
        eprintln!(
            "BASE_CHARACTERS base_parts={} base_triangles=158224 male_parts={} male_triangles=150752 feet_base={} feet_male={} symmetry_male={}",
            base_assembly.parts.len(),
            male_assembly.parts.len(),
            base_assembly.feet_y,
            male_assembly.feet_y,
            male_assembly.symmetry_max_m
        );
    }

    fn assert_bind_body(assembly: &crate::BindPoseAssembly, parts: usize, triangles: u32, outline: u32, outliers: usize, symmetry_max: f64) {
        assert_eq!(assembly.parts.len(), parts);
        assert_eq!(assembly.outliers.len(), outliers);
        assert_eq!(assembly.parts.iter().map(|part| part.triangles).sum::<u32>(), triangles);
        assert_eq!(assembly.outline_triangles, outline);
        assert!(assembly.feet_y.abs() < 0.02, "feet {}", assembly.feet_y);
        assert!((1.40..2.05).contains(&assembly.height_m), "height {}", assembly.height_m);
        let ratio = (assembly.pelvis_y - assembly.feet_y) / assembly.height_m;
        assert!((0.45..0.70).contains(&ratio), "pelvis ratio {ratio}");
        assert!(assembly.symmetry_max_m <= symmetry_max, "symmetry {}", assembly.symmetry_max_m);
        assert!(assembly.parts.iter().all(|part| !part.outlier && part.neighbor_gap_m <= 0.021), "a kept part is farther than 0.02 m from its neighbor");
        for part in &assembly.parts {
            for axis in 0..3 {
                let socket = part.socket_world[axis] as f32;
                let low = part.bounds_min[axis] - 0.08;
                let high = part.bounds_max[axis] + 0.08;
                assert!(
                    socket >= low && socket <= high,
                    "{} axis {axis} socket {socket} bounds {}..{}",
                    part.name, part.bounds_min[axis], part.bounds_max[axis]
                );
            }
        }
        assert!(assembly.parts.iter().all(|part| part.triangles <= crate::SHADOW_CASTER_TRIANGLE_LIMIT));
        assert!(assembly.pelvis_name.to_ascii_lowercase().contains("hip"));
    }

    fn mirror_gap(assembly: &crate::BindPoseAssembly, left: &str, right: &str) -> f64 {
        let left = assembly.parts.iter().find(|part| part.name == left).unwrap();
        let right = assembly.parts.iter().find(|part| part.name == right).unwrap();
        let left_center = part_center(left);
        let right_center = part_center(right);
        let low = assembly.parts.iter().map(|part| part.bounds_min[0]).fold(f32::MAX, f32::min);
        let high = assembly.parts.iter().map(|part| part.bounds_max[0]).fold(f32::MIN, f32::max);
        let mid = (low + high) as f64 * 0.5;
        let target = [mid * 2.0 - left_center[0], left_center[1], left_center[2]];
        let delta = [right_center[0] - target[0], right_center[1] - target[1], right_center[2] - target[2]];
        (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
    }

    fn part_center(part: &crate::BindPart) -> [f64; 3] {
        [
            (part.bounds_min[0] + part.bounds_max[0]) as f64 * 0.5,
            (part.bounds_min[1] + part.bounds_max[1]) as f64 * 0.5,
            (part.bounds_min[2] + part.bounds_max[2]) as f64 * 0.5,
        ]
    }

    fn viewing_origin(assembly: &crate::BindPoseAssembly, edge_x: f64) -> crate::Vec3 {
        let low_x = assembly.parts.iter().map(|part| part.bounds_min[0]).fold(f32::MAX, f32::min) as f64;
        let high_x = assembly.parts.iter().map(|part| part.bounds_max[0]).fold(f32::MIN, f32::max) as f64;
        let low_z = assembly.parts.iter().map(|part| part.bounds_min[2]).fold(f32::MAX, f32::min) as f64;
        let high_z = assembly.parts.iter().map(|part| part.bounds_max[2]).fold(f32::MIN, f32::max) as f64;
        let shift_x = if edge_x < 0.0 { edge_x - high_x } else { edge_x - low_x };
        crate::Vec3::new(shift_x, 0.0, -((low_z + high_z) * 0.5))
    }

    fn import_bind_parts(root: &PathBuf, parts_dir: &PathBuf, prefix: &str, assembly: &crate::BindPoseAssembly) -> (MeshAssetLibrary, Vec<AssetId>) {
        let mut library = None;
        let mut assets = Vec::new();
        for (index, part) in assembly.parts.iter().enumerate() {
            let mut clean = String::new();
            for ch in part.name.chars() {
                if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
                    clean.push(ch);
                } else {
                    clean.push('_');
                }
            }
            let path = parts_dir.join(format!("{prefix}__{index:02}_{clean}.glb"));
            let bytes = crate::rigid_part_glb(&part.positions, &part.normals, &part.uvs, &part.indices, assembly.base_color, assembly.metallic, assembly.roughness);
            fs::write(&path, bytes).unwrap();
            let (loaded, id) = import_mesh_into_project(root, "Content", "Intermediate", &path).expect("part import");
            assert_eq!(loaded.record(id).unwrap().triangle_count, part.triangles, "{}", part.name);
            assets.push(id);
            library = Some(loaded);
        }
        (library.expect("parts"), assets)
    }

    fn assert_placed_pose(world: &SceneWorld, document: &crate::CharacterDocument, assembly: &crate::BindPoseAssembly, origin: crate::Vec3) {
        for part in &assembly.parts {
            let entity = document
                .entities
                .iter()
                .find(|entity| entity.name == part.name || (part.name == assembly.pelvis_name && entity.parent_uuid.is_none()))
                .unwrap_or_else(|| panic!("missing {}", part.name));
            let pose = world.entity_world_pose(entity.uuid).unwrap();
            let dx = pose.translation.x - crate::BOOTSTRAP_ROOT_M - origin.x - part.socket_world[0];
            let dy = pose.translation.y - origin.y - part.socket_world[1];
            let dz = pose.translation.z - origin.z - part.socket_world[2];
            assert!(dx.abs().max(dy.abs()).max(dz.abs()) < 1.0e-4, "{} pose delta {dx} {dy} {dz}", part.name);
            let source = crate::Quat { x: part.world.rotation[0], y: part.world.rotation[1], z: part.world.rotation[2], w: part.world.rotation[3] };
            let dot = (pose.rotation.x * source.x + pose.rotation.y * source.y + pose.rotation.z * source.z + pose.rotation.w * source.w).abs();
            assert!(dot > 1.0 - 1.0e-5, "{} rotation {dot}", part.name);
            let (_, _, scale, _, _, _, _, _) = world.authored_mesh(entity.uuid).unwrap();
            assert!((scale.x - part.local.scale[0]).abs() < 1.0e-6, "{} scale", part.name);
            assert!((scale.y - part.local.scale[1]).abs() < 1.0e-6);
            assert!((scale.z - part.local.scale[2]).abs() < 1.0e-6);
        }
    }
}
