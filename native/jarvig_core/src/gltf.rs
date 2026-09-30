//! glTF 2.0 / GLB import. The result is one canonical mesh, not a level and not a scene.
//!
//! Node transforms are baked into vertex positions. Nodes do not become actors.
//! Required extensions, sparse accessors, and non-triangle primitives are rejected
//! before any asset record exists.

use crate::json_lite::{parse_json, Json};
use crate::mesh::{mesh_from_surfaces, CanonicalSurface, Mesh, MeshError};

#[derive(Debug)]
pub struct ImportError(pub String);

impl std::fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One glTF material. Image bytes are the embedded source, still encoded. Not a GPU texture.
#[derive(Clone, Debug)]
pub struct GltfImage {
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// Factors and embedded images for one material slot. Missing images stay empty.
#[derive(Clone, Debug)]
pub struct GltfSurface {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub normal_scale: f32,
    /// glTF default is one-sided. `true` means the material asked for both faces.
    pub double_sided: bool,
    /// glTF `alphaMode`. Default is `OPAQUE`. Mask and blend are recorded and not interpreted here.
    pub alpha_mode: String,
    pub base_color_image: Option<GltfImage>,
    pub metallic_roughness_image: Option<GltfImage>,
    pub normal_image: Option<GltfImage>,
}

/// Read a `.glb` or a `.gltf` whose one binary buffer is either embedded or a relative `.bin`.
/// `bin_for` loads a relative URI. GLB does not call it.
pub fn import_gltf(bytes: &[u8], bin_for: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>) -> Result<Mesh, ImportError> {
    let (json, bin) = if bytes.len() >= 4 && &bytes[0..4] == b"glTF" {
        decode_glb(bytes)?
    } else {
        decode_gltf_json(bytes, bin_for)?
    };
    let document = parse_json(&json).map_err(|error| ImportError(format!("glTF JSON was rejected: {error}")))?;
    mesh_from_document(&document, &bin)
}

/// Material slots in file order. A mesh-only file returns an empty list. This does not build geometry.
pub fn gltf_surfaces(bytes: &[u8]) -> Result<Vec<GltfSurface>, ImportError> {
    let (json, bin) = if bytes.len() >= 4 && &bytes[0..4] == b"glTF" {
        decode_glb(bytes)?
    } else {
        return Ok(Vec::new());
    };
    let document = parse_json(&json).map_err(|error| ImportError(format!("glTF JSON was rejected: {error}")))?;
    let Some(materials) = document.get("materials").and_then(Json::as_array) else {
        return Ok(Vec::new());
    };
    let mut surfaces = Vec::with_capacity(materials.len());
    for material in materials {
        let pbr = material.get("pbrMetallicRoughness");
        let base_color = pbr.and_then(|value| value.get("baseColorFactor")).and_then(json_float4).unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let metallic = pbr.and_then(|value| value.get("metallicFactor")).and_then(Json::as_f64).unwrap_or(1.0) as f32;
        let roughness = pbr.and_then(|value| value.get("roughnessFactor")).and_then(Json::as_f64).unwrap_or(1.0) as f32;
        let normal_scale = material.get("normalTexture").and_then(|value| value.get("scale")).and_then(Json::as_f64).unwrap_or(1.0) as f32;
        let double_sided = material.get("doubleSided").and_then(Json::as_bool).unwrap_or(false);
        let alpha_mode = material.get("alphaMode").and_then(Json::as_str).unwrap_or("OPAQUE").to_string();
        if !base_color.iter().all(|channel| channel.is_finite()) || !metallic.is_finite() || !roughness.is_finite() || !normal_scale.is_finite() {
            return Err(ImportError("glTF material factors are not finite".into()));
        }
        if alpha_mode != "OPAQUE" && alpha_mode != "MASK" && alpha_mode != "BLEND" {
            return Err(ImportError(format!("glTF alphaMode {alpha_mode} is not OPAQUE, MASK, or BLEND")));
        }
        surfaces.push(GltfSurface {
            base_color,
            metallic,
            roughness,
            normal_scale,
            double_sided,
            alpha_mode,
            base_color_image: texture_image(&document, &bin, pbr.and_then(|value| value.get("baseColorTexture")))?,
            metallic_roughness_image: texture_image(&document, &bin, pbr.and_then(|value| value.get("metallicRoughnessTexture")))?,
            normal_image: texture_image(&document, &bin, material.get("normalTexture"))?,
        });
    }
    Ok(surfaces)
}

fn json_float4(json: &Json) -> Option<[f32; 4]> {
    let values = json.as_array()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
        values[3].as_f64()? as f32,
    ])
}

fn texture_image(document: &Json, bin: &[u8], texture_info: Option<&Json>) -> Result<Option<GltfImage>, ImportError> {
    let Some(info) = texture_info else {
        return Ok(None);
    };
    let Some(texture_index) = info.get("index").and_then(Json::as_f64) else {
        return Ok(None);
    };
    let textures = document.get("textures").and_then(Json::as_array).unwrap_or(&[]);
    let Some(texture) = textures.get(texture_index as usize) else {
        return Err(ImportError(format!("glTF texture {texture_index} is missing")));
    };
    let Some(source) = texture.get("source").and_then(Json::as_f64) else {
        return Ok(None);
    };
    let images = document.get("images").and_then(Json::as_array).unwrap_or(&[]);
    let Some(image) = images.get(source as usize) else {
        return Err(ImportError(format!("glTF image {source} is missing")));
    };
    let mime = image.get("mimeType").and_then(Json::as_str).unwrap_or("").to_string();
    let Some(view_index) = image.get("bufferView").and_then(Json::as_f64) else {
        return Ok(None);
    };
    let views = document.get("bufferViews").and_then(Json::as_array).unwrap_or(&[]);
    let Some(view) = views.get(view_index as usize) else {
        return Err(ImportError(format!("glTF image buffer view {view_index} is missing")));
    };
    let offset = view.get("byteOffset").and_then(Json::as_f64).unwrap_or(0.0);
    let length = view.get("byteLength").and_then(Json::as_f64).unwrap_or(0.0);
    if offset < 0.0 || length < 1.0 || offset.fract() != 0.0 || length.fract() != 0.0 {
        return Err(ImportError("glTF image buffer view is not a byte range".into()));
    }
    let start = offset as usize;
    let end = start + length as usize;
    if end > bin.len() {
        return Err(ImportError("glTF image is outside the binary chunk".into()));
    }
    Ok(Some(GltfImage { mime, bytes: bin[start..end].to_vec() }))
}

fn decode_glb(bytes: &[u8]) -> Result<(String, Vec<u8>), ImportError> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return Err(ImportError("file is not a GLB".into()));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if version != 2 {
        return Err(ImportError(format!("GLB version {version} is not glTF 2.0")));
    }
    if length > bytes.len() || length < 20 {
        return Err(ImportError("GLB length does not match the file".into()));
    }
    let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json_type = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    if json_type != 0x4E4F_534A {
        return Err(ImportError("GLB is missing its JSON chunk".into()));
    }
    let json_end = 20usize.checked_add(json_length).ok_or_else(|| ImportError("GLB JSON chunk is truncated".into()))?;
    if json_end > length {
        return Err(ImportError("GLB JSON chunk is truncated".into()));
    }
    let json = std::str::from_utf8(&bytes[20..json_end]).map_err(|_| ImportError("GLB JSON is not UTF-8".into()))?.trim_end_matches([' ', '\0']).to_string();
    let mut bin = Vec::new();
    if json_end + 8 <= length {
        let bin_length = u32::from_le_bytes(bytes[json_end..json_end + 4].try_into().unwrap()) as usize;
        let bin_type = u32::from_le_bytes(bytes[json_end + 4..json_end + 8].try_into().unwrap());
        if bin_type != 0x004E_4942 {
            return Err(ImportError("GLB binary chunk has the wrong type".into()));
        }
        let bin_end = json_end + 8 + bin_length;
        if bin_end > length {
            return Err(ImportError("GLB binary chunk is truncated".into()));
        }
        bin = bytes[json_end + 8..bin_end].to_vec();
    }
    Ok((json, bin))
}

fn decode_gltf_json(bytes: &[u8], bin_for: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>) -> Result<(String, Vec<u8>), ImportError> {
    let json = std::str::from_utf8(bytes).map_err(|_| ImportError("glTF file is not UTF-8".into()))?.to_string();
    let document = parse_json(&json).map_err(|error| ImportError(format!("glTF JSON was rejected: {error}")))?;
    let buffers = document.get("buffers").and_then(Json::as_array).unwrap_or(&[]);
    if buffers.len() > 1 {
        return Err(ImportError("glTF files with more than one buffer are not supported".into()));
    }
    let bin = if let Some(buffer) = buffers.first() {
        match buffer.get("uri").and_then(Json::as_str) {
            None => Vec::new(),
            Some(uri) if uri.starts_with("data:") || uri.contains(':') || uri.contains('\\') || uri.starts_with('/') => {
                return Err(ImportError("glTF buffer URI must be a relative file beside the glTF".into()));
            }
            Some(uri) => bin_for(uri).map_err(|error| ImportError(error))?,
        }
    } else {
        Vec::new()
    };
    Ok((json, bin))
}

fn mesh_from_document(document: &Json, bin: &[u8]) -> Result<Mesh, ImportError> {
    if let Some(required) = document.get("extensionsRequired").and_then(Json::as_array) {
        if !required.is_empty() {
            let name = required.first().and_then(Json::as_str).unwrap_or("unknown");
            return Err(ImportError(format!("required glTF extension {name} is not supported")));
        }
    }
    let accessors = document.get("accessors").and_then(Json::as_array).unwrap_or(&[]);
    let views = document.get("bufferViews").and_then(Json::as_array).unwrap_or(&[]);
    let mut surfaces = Vec::new();
    if let Some(scene) = scene_nodes(document) {
        for node in scene {
            collect_node(document, bin, accessors, views, node, identity(), &mut surfaces)?;
        }
    } else if let Some(meshes) = document.get("meshes").and_then(Json::as_array) {
        for mesh in meshes {
            collect_mesh(bin, accessors, views, mesh, identity(), &mut surfaces)?;
        }
    }
    if surfaces.is_empty() {
        return Err(ImportError("glTF contains no triangle mesh".into()));
    }
    let triangles: usize = surfaces.iter().map(|surface| surface.indices.len() / 3).sum();
    // townshop.glb is about 6.1 million triangles. That is the production fixture. Larger files are rejected.
    if triangles < 1 || triangles > 8_000_000 {
        return Err(ImportError(format!("imported triangle count {triangles} is outside 1..=8000000")));
    }
    mesh_from_surfaces(&surfaces).map_err(|error| ImportError(mesh_message(error)))
}

fn scene_nodes(document: &Json) -> Option<Vec<usize>> {
    let scenes = document.get("scenes").and_then(Json::as_array)?;
    if scenes.is_empty() {
        return None;
    }
    let index = document.get("scene").and_then(Json::as_f64).unwrap_or(0.0) as usize;
    let scene = scenes.get(index)?;
    let nodes = scene.get("nodes").and_then(Json::as_array)?;
    Some(nodes.iter().filter_map(|node| node.as_f64().map(|value| value as usize)).collect())
}

fn collect_node(document: &Json, bin: &[u8], accessors: &[Json], views: &[Json], index: usize, parent: [f32; 16], surfaces: &mut Vec<CanonicalSurface>) -> Result<(), ImportError> {
    let nodes = document.get("nodes").and_then(Json::as_array).ok_or_else(|| ImportError("glTF scene node is missing".into()))?;
    let node = nodes.get(index).ok_or_else(|| ImportError(format!("glTF node {index} is missing")))?;
    let local = node_matrix(node)?;
    let world = mul_matrix(parent, local);
    if let Some(mesh_index) = node.get("mesh").and_then(Json::as_f64) {
        let meshes = document.get("meshes").and_then(Json::as_array).ok_or_else(|| ImportError("glTF mesh list is missing".into()))?;
        let mesh = meshes.get(mesh_index as usize).ok_or_else(|| ImportError(format!("glTF mesh {mesh_index} is missing")))?;
        collect_mesh(bin, accessors, views, mesh, world, surfaces)?;
    }
    if let Some(children) = node.get("children").and_then(Json::as_array) {
        for child in children {
            let child = child.as_f64().ok_or_else(|| ImportError("glTF child is not a node index".into()))? as usize;
            collect_node(document, bin, accessors, views, child, world, surfaces)?;
        }
    }
    Ok(())
}

fn collect_mesh(bin: &[u8], accessors: &[Json], views: &[Json], mesh: &Json, transform: [f32; 16], surfaces: &mut Vec<CanonicalSurface>) -> Result<(), ImportError> {
    let primitives = mesh.get("primitives").and_then(Json::as_array).ok_or_else(|| ImportError("glTF mesh has no primitives".into()))?;
    for primitive in primitives {
        let mode = primitive.get("mode").and_then(Json::as_f64).unwrap_or(4.0) as u32;
        if mode != 4 {
            return Err(ImportError(format!("glTF primitive mode {mode} is not triangles")));
        }
        let attributes = primitive.get("attributes").ok_or_else(|| ImportError("glTF primitive has no attributes".into()))?;
        let position_index = attributes.get("POSITION").and_then(Json::as_f64).ok_or_else(|| ImportError("glTF primitive has no positions".into()))? as usize;
        let positions = read_vec3(bin, accessors, views, position_index)?;
        let normals = match attributes.get("NORMAL").and_then(Json::as_f64) {
            Some(index) => read_vec3(bin, accessors, views, index as usize)?,
            None => Vec::new(),
        };
        let texcoords = match attributes.get("TEXCOORD_0").and_then(Json::as_f64) {
            Some(index) => read_vec2(bin, accessors, views, index as usize)?,
            None => vec![[0.0, 0.0]; positions.len()],
        };
        let tangents = match attributes.get("TANGENT").and_then(Json::as_f64) {
            Some(index) => read_vec4(bin, accessors, views, index as usize)?,
            None => Vec::new(),
        };
        let Some(index_accessor) = primitive.get("indices").and_then(Json::as_f64) else {
            return Err(ImportError("glTF primitive has no indices".into()));
        };
        let mut indices = read_indices(bin, accessors, views, index_accessor as usize)?;
        if indices.len() % 3 != 0 || indices.is_empty() {
            return Err(ImportError("glTF indices are not a triangle list".into()));
        }
        let normal_matrix = normal_matrix(transform);
        let positions = positions.into_iter().map(|position| transform_point(transform, position)).collect::<Vec<_>>();
        let normals = normals.into_iter().map(|normal| transform_direction(normal_matrix, normal)).collect::<Vec<_>>();
        let tangents = tangents
            .into_iter()
            .map(|tangent| {
                let direction = transform_direction(normal_matrix, [tangent[0], tangent[1], tangent[2]]);
                [direction[0], direction[1], direction[2], tangent[3]]
            })
            .collect::<Vec<_>>();
        if !indices.iter().all(|index| (*index as usize) < positions.len()) {
            return Err(ImportError("glTF index is outside the primitive's vertices".into()));
        }
        let slot = primitive.get("material").and_then(Json::as_f64).unwrap_or(0.0);
        if !(0.0..256.0).contains(&slot) || slot.fract() != 0.0 {
            return Err(ImportError("glTF material slot is not a small integer".into()));
        }
        // Winding stays the source winding. A negative scale flips it below by swapping the first edge.
        if matrix_determinant(transform) < 0.0 {
            for triangle in indices.chunks_exact_mut(3) {
                triangle.swap(0, 1);
            }
        }
        surfaces.push(CanonicalSurface {
            positions,
            normals,
            texcoords,
            tangents,
            indices,
            material_slot: slot as u32,
        });
    }
    Ok(())
}

fn read_vec3(bin: &[u8], accessors: &[Json], views: &[Json], index: usize) -> Result<Vec<[f32; 3]>, ImportError> {
    let values = read_accessor(bin, accessors, views, index, "VEC3")?;
    Ok(values.chunks_exact(3).map(|value| [value[0], value[1], value[2]]).collect())
}

fn read_vec2(bin: &[u8], accessors: &[Json], views: &[Json], index: usize) -> Result<Vec<[f32; 2]>, ImportError> {
    let values = read_accessor(bin, accessors, views, index, "VEC2")?;
    Ok(values.chunks_exact(2).map(|value| [value[0], value[1]]).collect())
}

fn read_vec4(bin: &[u8], accessors: &[Json], views: &[Json], index: usize) -> Result<Vec<[f32; 4]>, ImportError> {
    let values = read_accessor(bin, accessors, views, index, "VEC4")?;
    Ok(values.chunks_exact(4).map(|value| [value[0], value[1], value[2], value[3]]).collect())
}

fn read_indices(bin: &[u8], accessors: &[Json], views: &[Json], index: usize) -> Result<Vec<u32>, ImportError> {
    let accessor = accessors.get(index).ok_or_else(|| ImportError(format!("glTF accessor {index} is missing")))?;
    if accessor.get("sparse").is_some() {
        return Err(ImportError("sparse glTF accessors are not supported".into()));
    }
    if accessor.get("type").and_then(Json::as_str) != Some("SCALAR") {
        return Err(ImportError("glTF indices are not scalar".into()));
    }
    let count = count_of(accessor)?;
    let component = component_type(accessor)?;
    let (offset, stride) = view_span(accessor, views, component, 1)?;
    let mut indices = Vec::with_capacity(count);
    for item in 0..count {
        let start = offset + item * stride;
        let value = read_component(bin, start, component).map_err(|_| ImportError("glTF index is outside the buffer".into()))?;
        if value < 0.0 || value > u32::MAX as f64 || value.fract() != 0.0 {
            return Err(ImportError("glTF index is not a valid vertex index".into()));
        }
        indices.push(value as u32);
    }
    Ok(indices)
}

fn read_accessor(bin: &[u8], accessors: &[Json], views: &[Json], index: usize, expected: &str) -> Result<Vec<f32>, ImportError> {
    let accessor = accessors.get(index).ok_or_else(|| ImportError(format!("glTF accessor {index} is missing")))?;
    if accessor.get("sparse").is_some() {
        return Err(ImportError("sparse glTF accessors are not supported".into()));
    }
    if accessor.get("type").and_then(Json::as_str) != Some(expected) {
        return Err(ImportError(format!("glTF accessor {index} is not {expected}")));
    }
    let width = match expected {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        _ => return Err(ImportError(format!("glTF type {expected} is not supported"))),
    };
    let count = count_of(accessor)?;
    let component = component_type(accessor)?;
    let normalized = accessor.get("normalized").and_then(Json::as_bool).unwrap_or(false);
    let (offset, stride) = view_span(accessor, views, component, width)?;
    let mut values = Vec::with_capacity(count * width);
    for item in 0..count {
        for lane in 0..width {
            let start = offset + item * stride + lane * component_size(component);
            let raw = read_component(bin, start, component).map_err(|_| ImportError("glTF accessor is outside the buffer".into()))?;
            let value = if normalized { normalize_component(raw, component) } else { raw as f32 };
            if !value.is_finite() {
                return Err(ImportError("glTF accessor contains a non-finite value".into()));
            }
            values.push(value);
        }
    }
    Ok(values)
}

fn count_of(accessor: &Json) -> Result<usize, ImportError> {
    let count = accessor.get("count").and_then(Json::as_f64).ok_or_else(|| ImportError("glTF accessor count is missing".into()))?;
    if count < 1.0 || count > 24_000_000.0 || count.fract() != 0.0 {
        return Err(ImportError(format!("glTF accessor count {count} is outside 1..=24000000")));
    }
    Ok(count as usize)
}

fn component_type(accessor: &Json) -> Result<u32, ImportError> {
    let component = accessor.get("componentType").and_then(Json::as_f64).ok_or_else(|| ImportError("glTF component type is missing".into()))? as u32;
    if !matches!(component, 5120 | 5121 | 5122 | 5123 | 5125 | 5126) {
        return Err(ImportError(format!("glTF component type {component} is not supported")));
    }
    Ok(component)
}

fn view_span(accessor: &Json, views: &[Json], component: u32, width: usize) -> Result<(usize, usize), ImportError> {
    let view_index = accessor.get("bufferView").and_then(Json::as_f64).ok_or_else(|| ImportError("glTF accessor has no buffer view".into()))? as usize;
    let view = views.get(view_index).ok_or_else(|| ImportError(format!("glTF buffer view {view_index} is missing")))?;
    if view.get("buffer").and_then(Json::as_f64).unwrap_or(0.0) != 0.0 {
        return Err(ImportError("only glTF buffer 0 is supported".into()));
    }
    let view_offset = view.get("byteOffset").and_then(Json::as_f64).unwrap_or(0.0);
    let accessor_offset = accessor.get("byteOffset").and_then(Json::as_f64).unwrap_or(0.0);
    if view_offset < 0.0 || accessor_offset < 0.0 || view_offset.fract() != 0.0 || accessor_offset.fract() != 0.0 {
        return Err(ImportError("glTF byte offset is not a positive integer".into()));
    }
    let tight = component_size(component) * width;
    let stride = view.get("byteStride").and_then(Json::as_f64).map(|value| value as usize).unwrap_or(tight);
    if stride < tight || stride > 256 || stride % component_size(component) != 0 {
        return Err(ImportError("glTF buffer view stride does not fit the accessor".into()));
    }
    Ok((view_offset as usize + accessor_offset as usize, stride))
}

fn component_size(component: u32) -> usize {
    match component {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        _ => 4,
    }
}

fn read_component(bin: &[u8], start: usize, component: u32) -> Result<f64, ()> {
    let size = component_size(component);
    let end = start.checked_add(size).ok_or(())?;
    let bytes = bin.get(start..end).ok_or(())?;
    Ok(match component {
        5120 => i8::from_le_bytes([bytes[0]]) as f64,
        5121 => bytes[0] as f64,
        5122 => i16::from_le_bytes(bytes.try_into().unwrap()) as f64,
        5123 => u16::from_le_bytes(bytes.try_into().unwrap()) as f64,
        5125 => u32::from_le_bytes(bytes.try_into().unwrap()) as f64,
        5126 => f32::from_le_bytes(bytes.try_into().unwrap()) as f64,
        _ => return Err(()),
    })
}

fn normalize_component(value: f64, component: u32) -> f32 {
    let scale = match component {
        5120 => 127.0,
        5121 => 255.0,
        5122 => 32767.0,
        5123 => 65535.0,
        _ => 1.0,
    };
    (value / scale) as f32
}

fn node_matrix(node: &Json) -> Result<[f32; 16], ImportError> {
    if let Some(matrix) = node.get("matrix").and_then(Json::as_array) {
        if matrix.len() != 16 {
            return Err(ImportError("glTF node matrix is not 16 numbers".into()));
        }
        let mut out = [0.0_f32; 16];
        for (index, value) in matrix.iter().enumerate() {
            let number = value.as_f64().ok_or_else(|| ImportError("glTF node matrix is not numeric".into()))?;
            if !number.is_finite() {
                return Err(ImportError("glTF node matrix is not finite".into()));
            }
            out[index] = number as f32;
        }
        return Ok(out);
    }
    let translation = optional_vec3(node, "translation")?;
    let scale = optional_vec3(node, "scale")?.unwrap_or([1.0, 1.0, 1.0]);
    let rotation = optional_quat(node)?;
    if scale.iter().any(|value| *value == 0.0) {
        return Err(ImportError("glTF node scale has a zero axis".into()));
    }
    Ok(mul_matrix(translation_matrix(translation.unwrap_or([0.0; 3])), mul_matrix(rotation_matrix(rotation), scale_matrix(scale))))
}

fn optional_vec3(node: &Json, key: &str) -> Result<Option<[f32; 3]>, ImportError> {
    let Some(values) = node.get(key).and_then(Json::as_array) else { return Ok(None) };
    if values.len() != 3 {
        return Err(ImportError(format!("glTF node {key} is not a vec3")));
    }
    let mut out = [0.0_f32; 3];
    for (index, value) in values.iter().enumerate() {
        let number = value.as_f64().filter(|number| number.is_finite()).ok_or_else(|| ImportError(format!("glTF node {key} is not finite")))?;
        out[index] = number as f32;
    }
    Ok(Some(out))
}

fn optional_quat(node: &Json) -> Result<[f32; 4], ImportError> {
    let Some(values) = node.get("rotation").and_then(Json::as_array) else { return Ok([0.0, 0.0, 0.0, 1.0]) };
    if values.len() != 4 {
        return Err(ImportError("glTF node rotation is not a quaternion".into()));
    }
    let mut out = [0.0_f32; 4];
    for (index, value) in values.iter().enumerate() {
        out[index] = value.as_f64().filter(|number| number.is_finite()).ok_or_else(|| ImportError("glTF node rotation is not finite".into()))? as f32;
    }
    Ok(out)
}

fn identity() -> [f32; 16] {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}

fn translation_matrix(translation: [f32; 3]) -> [f32; 16] {
    let mut matrix = identity();
    matrix[12] = translation[0];
    matrix[13] = translation[1];
    matrix[14] = translation[2];
    matrix
}

fn scale_matrix(scale: [f32; 3]) -> [f32; 16] {
    let mut matrix = identity();
    matrix[0] = scale[0];
    matrix[5] = scale[1];
    matrix[10] = scale[2];
    matrix
}

fn rotation_matrix(quaternion: [f32; 4]) -> [f32; 16] {
    let (x, y, z, w) = (quaternion[0], quaternion[1], quaternion[2], quaternion[3]);
    let length = (x * x + y * y + z * z + w * w).sqrt();
    let (x, y, z, w) = if length < 1.0e-8 { (0.0, 0.0, 0.0, 1.0) } else { (x / length, y / length, z / length, w / length) };
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        0.0,
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        0.0,
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

fn mul_matrix(left: [f32; 16], right: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0_f32; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| left[k * 4 + row] * right[column * 4 + k]).sum();
        }
    }
    out
}

fn transform_point(matrix: [f32; 16], point: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12],
        matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13],
        matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14],
    ]
}

fn normal_matrix(matrix: [f32; 16]) -> [f32; 9] {
    let a = [matrix[0], matrix[1], matrix[2], matrix[4], matrix[5], matrix[6], matrix[8], matrix[9], matrix[10]];
    let det = a[0] * (a[4] * a[8] - a[5] * a[7]) - a[1] * (a[3] * a[8] - a[5] * a[6]) + a[2] * (a[3] * a[7] - a[4] * a[6]);
    if det.abs() < 1.0e-12 {
        return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    }
    let inv = 1.0 / det;
    // Inverse, then transpose is the cofactor matrix divided by det.
    [
        (a[4] * a[8] - a[5] * a[7]) * inv,
        (a[5] * a[6] - a[3] * a[8]) * inv,
        (a[3] * a[7] - a[4] * a[6]) * inv,
        (a[2] * a[7] - a[1] * a[8]) * inv,
        (a[0] * a[8] - a[2] * a[6]) * inv,
        (a[1] * a[6] - a[0] * a[7]) * inv,
        (a[1] * a[5] - a[2] * a[4]) * inv,
        (a[2] * a[3] - a[0] * a[5]) * inv,
        (a[0] * a[4] - a[1] * a[3]) * inv,
    ]
}

fn transform_direction(matrix: [f32; 9], direction: [f32; 3]) -> [f32; 3] {
    let value = [
        matrix[0] * direction[0] + matrix[3] * direction[1] + matrix[6] * direction[2],
        matrix[1] * direction[0] + matrix[4] * direction[1] + matrix[7] * direction[2],
        matrix[2] * direction[0] + matrix[5] * direction[1] + matrix[8] * direction[2],
    ];
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length < 1.0e-8 { [0.0, 1.0, 0.0] } else { [value[0] / length, value[1] / length, value[2] / length] }
}

fn matrix_determinant(matrix: [f32; 16]) -> f32 {
    let a = [matrix[0], matrix[1], matrix[2], matrix[4], matrix[5], matrix[6], matrix[8], matrix[9], matrix[10]];
    a[0] * (a[4] * a[8] - a[5] * a[7]) - a[1] * (a[3] * a[8] - a[5] * a[6]) + a[2] * (a[3] * a[7] - a[4] * a[6])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_curved_two_slot_glb_imports_as_one_mesh_and_a_bad_file_does_not() {
        let mesh = import_gltf(&curved_prop_glb(80_000), &mut |_| Err("no buffer".into())).expect("prop");
        let triangles = mesh.index_count() / 3;
        assert!((50_000..=250_000).contains(&triangles), "{triangles}");
        assert!(mesh.submeshes().len() >= 2);
        assert!(mesh.submeshes().iter().any(|submesh| submesh.material_slot == 0));
        assert!(mesh.submeshes().iter().any(|submesh| submesh.material_slot == 1));
        assert!(mesh.position(0).is_some());
        assert!(import_gltf(b"not a glb", &mut |_| Err("no".into())).is_err());
        let (json, bin) = decode_glb(&curved_prop_glb(8)).unwrap();
        let sided = json.replacen("{\"name\":\"Slot0\"}", "{\"name\":\"Slot0\",\"alphaMode\":\"OPAQUE\",\"doubleSided\":true}", 1);
        let surfaces = gltf_surfaces(&rebuild_glb(sided.as_bytes(), &bin)).unwrap();
        assert!(surfaces[0].double_sided);
        assert_eq!(surfaces[0].alpha_mode, "OPAQUE");
        assert!(!surfaces[1].double_sided);
        assert_eq!(surfaces[1].alpha_mode, "OPAQUE");
        let mut broken = curved_prop_glb(8);
        broken[0] = b'x';
        assert!(import_gltf(&broken, &mut |_| Err("no".into())).is_err());
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/meshes/curved-prop.glb");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, curved_prop_glb(80_000)).unwrap();
    }

    #[test]
    fn a_translated_node_is_baked_and_does_not_become_a_second_mesh() {
        let glb = curved_prop_glb(8);
        let (json, bin) = decode_glb(&glb).unwrap();
        let moved = json.replacen("\"mesh\":0", "\"mesh\":0,\"translation\":[3,0,0]", 1);
        let bytes = rebuild_glb(moved.as_bytes(), &bin);
        let mesh = import_gltf(&bytes, &mut |_| Err("no".into())).unwrap();
        assert_eq!(mesh.submeshes().len(), 2);
        let far = mesh.bounds().aabb.max[0];
        assert!(far > 1.0, "{far}");
    }

    fn rebuild_glb(json: &[u8], bin: &[u8]) -> Vec<u8> {
        let mut json = json.to_vec();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut out = Vec::new();
        let total = 12 + 8 + json.len() + 8 + bin.len();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        out.extend_from_slice(&json);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        out.extend_from_slice(bin);
        out
    }
}

fn mesh_message(error: MeshError) -> String {
    format!("imported mesh was rejected: {error}")
}

/// A curved two-slot prop, written as a real GLB. `triangles` is the first primitive; a second primitive adds a band.
pub fn curved_prop_glb(sphere_triangles: u32) -> Vec<u8> {
    let slices = 200u32;
    let stacks = (sphere_triangles / slices / 2).max(2);
    let (positions, normals, uvs, indices) = uv_sphere(stacks, slices, 0.6);
    let (band_positions, band_normals, band_uvs, band_indices) = uv_sphere(8, 48, 0.15);
    let band_positions: Vec<[f32; 3]> = band_positions.into_iter().map(|position| [position[0] + 0.9, position[1] + 0.35, position[2]]).collect();
    encode_glb(&[
        ("Sphere", 0, &positions, &normals, &uvs, &indices),
        ("Band", 1, &band_positions, &band_normals, &band_uvs, &band_indices),
    ])
}

fn uv_sphere(stacks: u32, slices: u32, radius: f32) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for stack in 0..=stacks {
        let v = stack as f32 / stacks as f32;
        let phi = v * std::f32::consts::PI;
        for slice in 0..=slices {
            let u = slice as f32 / slices as f32;
            let theta = u * std::f32::consts::TAU;
            let normal = [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()];
            positions.push([normal[0] * radius, normal[1] * radius, normal[2] * radius]);
            normals.push(normal);
            uvs.push([u, 1.0 - v]);
        }
    }
    let mut indices = Vec::new();
    let row = slices + 1;
    for stack in 0..stacks {
        for slice in 0..slices {
            let a = stack * row + slice;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (positions, normals, uvs, indices)
}

fn encode_glb(primitives: &[(&str, u32, &[[f32; 3]], &[[f32; 3]], &[[f32; 2]], &[u32])]) -> Vec<u8> {
    let mut bin = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut primitive_json = Vec::new();
    for (_name, slot, positions, normals, uvs, indices) in primitives {
        let position_view = views.len();
        views.push(view_json(push_f32x3(&mut bin, positions)));
        views.push(view_json(push_f32x3(&mut bin, normals)));
        views.push(view_json(push_f32x2(&mut bin, uvs)));
        views.push(view_json(push_u32(&mut bin, indices)));
        let position_accessor = accessors.len();
        let (min, max) = bounds_min_max(positions);
        accessors.push(format!(
            "{{\"bufferView\":{position_view},\"componentType\":5126,\"count\":{},\"type\":\"VEC3\",\"min\":[{},{},{}],\"max\":[{},{},{}]}}",
            positions.len(), min[0], min[1], min[2], max[0], max[1], max[2]
        ));
        accessors.push(format!("{{\"bufferView\":{},\"componentType\":5126,\"count\":{},\"type\":\"VEC3\"}}", position_view + 1, normals.len()));
        accessors.push(format!("{{\"bufferView\":{},\"componentType\":5126,\"count\":{},\"type\":\"VEC2\"}}", position_view + 2, uvs.len()));
        accessors.push(format!("{{\"bufferView\":{},\"componentType\":5125,\"count\":{},\"type\":\"SCALAR\"}}", position_view + 3, indices.len()));
        primitive_json.push(format!(
            "{{\"attributes\":{{\"POSITION\":{position_accessor},\"NORMAL\":{},\"TEXCOORD_0\":{}}},\"indices\":{},\"material\":{slot},\"mode\":4}}",
            position_accessor + 1,
            position_accessor + 2,
            position_accessor + 3,
        ));
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let views_json = views.join(",");
    let accessors_json = accessors.join(",");
    let json = format!(
        "{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"JARVIG\"}},\"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\"nodes\":[{{\"mesh\":0}}],\"materials\":[{{\"name\":\"Slot0\"}},{{\"name\":\"Slot1\"}}],\"meshes\":[{{\"primitives\":[{}]}}],\"buffers\":[{{\"byteLength\":{}}}],\"bufferViews\":[{views_json}],\"accessors\":[{accessors_json}]}}",
        primitive_json.join(","),
        bin.len(),
    );
    let mut json_bytes = json.into_bytes();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let mut glb = Vec::new();
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    glb.extend_from_slice(&bin);
    glb
}

struct ViewSpan {
    offset: usize,
    length: usize,
}

fn push_f32x3(bin: &mut Vec<u8>, values: &[[f32; 3]]) -> ViewSpan {
    let offset = bin.len();
    for value in values {
        for lane in value {
            bin.extend_from_slice(&lane.to_le_bytes());
        }
    }
    ViewSpan { offset, length: bin.len() - offset }
}

fn push_f32x2(bin: &mut Vec<u8>, values: &[[f32; 2]]) -> ViewSpan {
    let offset = bin.len();
    for value in values {
        for lane in value {
            bin.extend_from_slice(&lane.to_le_bytes());
        }
    }
    ViewSpan { offset, length: bin.len() - offset }
}

fn push_u32(bin: &mut Vec<u8>, values: &[u32]) -> ViewSpan {
    let offset = bin.len();
    for value in values {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    ViewSpan { offset, length: bin.len() - offset }
}

fn view_json(view: ViewSpan) -> String {
    format!("{{\"buffer\":0,\"byteOffset\":{},\"byteLength\":{}}}", view.offset, view.length)
}

fn bounds_min_max(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for position in positions {
        for lane in 0..3 {
            min[lane] = min[lane].min(position[lane]);
            max[lane] = max[lane].max(position[lane]);
        }
    }
    (min, max)
}
