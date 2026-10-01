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
            collect_mesh(bin, accessors, views, mesh, identity(), None, &mut surfaces)?;
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
        collect_mesh(bin, accessors, views, mesh, world, None, surfaces)?;
    }
    if let Some(children) = node.get("children").and_then(Json::as_array) {
        for child in children {
            let child = child.as_f64().ok_or_else(|| ImportError("glTF child is not a node index".into()))? as usize;
            collect_node(document, bin, accessors, views, child, world, surfaces)?;
        }
    }
    Ok(())
}

fn collect_mesh(bin: &[u8], accessors: &[Json], views: &[Json], mesh: &Json, transform: [f32; 16], material: Option<u32>, surfaces: &mut Vec<CanonicalSurface>) -> Result<(), ImportError> {
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
        if material.is_some_and(|material| material != slot as u32) {
            continue;
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

/// Parts whose world boxes come within this distance belong to the same rigid copy.
pub const BIND_POSE_CONNECT_M: f64 = 0.02;

/// Source node TRS. Translation is meters. Rotation is xyzw. Scale is the node scale.
#[derive(Clone, Debug)]
pub struct RigidTransform {
    pub translation: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
}

/// One rigid mesh node. Vertices stay in the node frame. The transform is not baked in.
#[derive(Clone, Debug)]
pub struct BindPart {
    pub name: String,
    pub source_parent: String,
    pub local: RigidTransform,
    pub world: RigidTransform,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub triangles: u32,
    pub neighbor_gap_m: f64,
    pub outlier: bool,
    /// Joint origin in source world space. The mesh stays where the source put it.
    pub socket_world: [f64; 3],
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// One standing body, plus the material-0 parts that were not connected to it.
#[derive(Clone, Debug)]
pub struct BindPoseAssembly {
    pub parts: Vec<BindPart>,
    pub outliers: Vec<BindPart>,
    pub outline_triangles: u32,
    pub feet_y: f64,
    pub pelvis_name: String,
    pub pelvis_y: f64,
    pub height_m: f64,
    pub width_m: f64,
    pub depth_m: f64,
    pub symmetry_max_m: f64,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

/// A part GLB whose node is identity. The caller puts the source TRS on the entity.
pub fn rigid_part_glb(positions: &[[f32; 3]], normals: &[[f32; 3]], uvs: &[[f32; 2]], indices: &[u32], base_color: [f32; 4], metallic: f32, roughness: f32) -> Vec<u8> {
    let mut bin = Vec::new();
    let position_view = view_json(push_f32x3(&mut bin, positions));
    let normal_view = view_json(push_f32x3(&mut bin, normals));
    let uv_view = view_json(push_f32x2(&mut bin, uvs));
    let index_view = view_json(push_u32(&mut bin, indices));
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let (min, max) = bounds_min_max(positions);
    let json = format!(
        "{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"JARVIG\"}},\"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\"nodes\":[{{\"mesh\":0}}],\"materials\":[{{\"name\":\"check\",\"pbrMetallicRoughness\":{{\"baseColorFactor\":[{},{},{},{}],\"metallicFactor\":{},\"roughnessFactor\":{}}}}}],\"meshes\":[{{\"primitives\":[{{\"attributes\":{{\"POSITION\":0,\"NORMAL\":1,\"TEXCOORD_0\":2}},\"indices\":3,\"material\":0,\"mode\":4}}]}}],\"buffers\":[{{\"byteLength\":{}}}],\"bufferViews\":[{position_view},{normal_view},{uv_view},{index_view}],\"accessors\":[{{\"bufferView\":0,\"componentType\":5126,\"count\":{},\"type\":\"VEC3\",\"min\":[{},{},{}],\"max\":[{},{},{}]}},{{\"bufferView\":1,\"componentType\":5126,\"count\":{},\"type\":\"VEC3\"}},{{\"bufferView\":2,\"componentType\":5126,\"count\":{},\"type\":\"VEC2\"}},{{\"bufferView\":3,\"componentType\":5125,\"count\":{},\"type\":\"SCALAR\"}}]}}",
        base_color[0], base_color[1], base_color[2], base_color[3], metallic, roughness, bin.len(),
        positions.len(), min[0], min[1], min[2], max[0], max[1], max[2], normals.len(), uvs.len(), indices.len(),
    );
    pack_glb(json.into_bytes(), bin)
}

/// Keep one standing connected body. Material 0 only. Node TRS stays on the part.
///
/// The ordinary importer still bakes every node into one mesh. This does not.
pub fn assemble_rigid_bind_pose(bytes: &[u8]) -> Result<BindPoseAssembly, ImportError> {
    let (json, bin) = decode_glb(bytes)?;
    let document = parse_json(&json).map_err(|error| ImportError(format!("glTF JSON was rejected: {error}")))?;
    if let Some(required) = document.get("extensionsRequired").and_then(Json::as_array) {
        if !required.is_empty() {
            let name = required.first().and_then(Json::as_str).unwrap_or("unknown");
            return Err(ImportError(format!("required glTF extension {name} is not supported")));
        }
    }
    let nodes = document.get("nodes").and_then(Json::as_array).ok_or_else(|| ImportError("glTF nodes are missing".into()))?;
    let parent_of = parent_index(nodes);
    let world = composed_transforms(nodes, &document)?;
    let accessors = document.get("accessors").and_then(Json::as_array).unwrap_or(&[]);
    let meshes = document.get("meshes").and_then(Json::as_array).ok_or_else(|| ImportError("glTF mesh list is missing".into()))?;
    let mut probes = Vec::new();
    let mut outline_triangles = 0u32;
    for (index, node) in nodes.iter().enumerate() {
        let Some(mesh_index) = node.get("mesh").and_then(Json::as_f64) else { continue };
        let mesh = meshes.get(mesh_index as usize).ok_or_else(|| ImportError(format!("glTF mesh {mesh_index} is missing")))?;
        let primitives = mesh.get("primitives").and_then(Json::as_array).ok_or_else(|| ImportError("glTF mesh has no primitives".into()))?;
        let mut triangles = 0u32;
        let mut low = [f32::MAX; 3];
        let mut high = [f32::MIN; 3];
        let mut any = false;
        for primitive in primitives {
            let count = primitive_triangles(accessors, primitive)?;
            let slot = primitive.get("material").and_then(Json::as_f64).unwrap_or(0.0);
            if slot != 0.0 {
                outline_triangles = outline_triangles.saturating_add(count);
                continue;
            }
            let position = primitive.get("attributes").and_then(|value| value.get("POSITION")).and_then(Json::as_f64).ok_or_else(|| ImportError("glTF primitive has no positions".into()))? as usize;
            let (minimum, maximum) = accessor_bounds(accessors, position)?;
            let matrix = trs_matrix(&world[index]);
            for x in [minimum[0], maximum[0]] {
                for y in [minimum[1], maximum[1]] {
                    for z in [minimum[2], maximum[2]] {
                        let point = transform_point(matrix, [x, y, z]);
                        for axis in 0..3 {
                            low[axis] = low[axis].min(point[axis]);
                            high[axis] = high[axis].max(point[axis]);
                        }
                    }
                }
            }
            triangles = triangles.saturating_add(count);
            any = true;
        }
        if !any || triangles == 0 {
            continue;
        }
        let parent = parent_of.get(&index).copied().and_then(|parent| nodes.get(parent));
        let source_parent = parent.and_then(|parent| parent.get("name").and_then(Json::as_str)).unwrap_or("scene").to_string();
        probes.push(PartProbe {
            index,
            name: node.get("name").and_then(Json::as_str).unwrap_or("part").to_string(),
            source_parent,
            local: node_trs(node)?,
            world: world[index].clone(),
            bounds_min: low,
            bounds_max: high,
            triangles,
        });
    }
    if probes.is_empty() {
        return Err(ImportError("glTF contains no material-0 triangle mesh".into()));
    }
    let labels = cluster_ids(&probes, BIND_POSE_CONNECT_M as f32);
    let mut best: Option<usize> = None;
    for label in 0..probes.len() {
        let members: Vec<usize> = (0..probes.len()).filter(|index| labels[*index] == label).collect();
        if members.is_empty() {
            continue;
        }
        let (low, high) = union_bounds(&probes, &members);
        if !standing_body(low, high) {
            continue;
        }
        let triangles: u32 = members.iter().map(|index| probes[*index].triangles).sum();
        let replace = match best {
            None => true,
            Some(current) => {
                let current_members: Vec<usize> = (0..probes.len()).filter(|index| labels[*index] == current).collect();
                triangles > current_members.iter().map(|index| probes[*index].triangles).sum()
            }
        };
        if replace {
            best = Some(label);
        }
    }
    let Some(chosen) = best else {
        return Err(ImportError("no standing body was connected within 0.02 m".into()));
    };
    let views = document.get("bufferViews").and_then(Json::as_array).unwrap_or(&[]);
    let mut parts = Vec::new();
    let mut outliers = Vec::new();
    for (probe_index, probe) in probes.iter().enumerate() {
        let outlier = labels[probe_index] != chosen;
        let (positions, normals, uvs, indices) = if outlier {
            (Vec::new(), Vec::new(), Vec::new(), Vec::new())
        } else {
            let mesh_index = nodes[probe.index].get("mesh").and_then(Json::as_f64).unwrap() as usize;
            read_local_mesh(&bin, accessors, views, &meshes[mesh_index])?
        };
        let part = BindPart {
            name: probe.name.clone(),
            source_parent: probe.source_parent.clone(),
            local: probe.local.clone(),
            world: probe.world.clone(),
            bounds_min: probe.bounds_min,
            bounds_max: probe.bounds_max,
            triangles: probe.triangles,
            neighbor_gap_m: 0.0,
            outlier,
            socket_world: probe.world.translation,
            positions,
            normals,
            uvs,
            indices,
        };
        if outlier { outliers.push(part); } else { parts.push(part); }
    }
    let mut parts = seat_rigid_sockets(parts);
    parts.sort_by(|left, right| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()));
    outliers.sort_by(|left, right| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()));
    fill_neighbor_gaps(&mut parts, &mut outliers);
    let members: Vec<usize> = (0..parts.len()).collect();
    let (low, high) = union_part_bounds(&parts, &members);
    let pelvis = pelvis_part(&parts);
    let (base_color, metallic, roughness) = first_material(&document);
    Ok(BindPoseAssembly {
        symmetry_max_m: symmetry_error(&parts, low, high),
        feet_y: low[1] as f64,
        pelvis_name: parts[pelvis].name.clone(),
        pelvis_y: ((parts[pelvis].bounds_min[1] + parts[pelvis].bounds_max[1]) * 0.5) as f64,
        height_m: (high[1] - low[1]) as f64,
        width_m: (high[0] - low[0]) as f64,
        depth_m: (high[2] - low[2]) as f64,
        parts,
        outliers,
        outline_triangles,
        base_color,
        metallic,
        roughness,
    })
}

/// Islands within 1.5 cm are one rigid piece. Its pivot is the ball nearest its parent.
fn seat_rigid_sockets(parts: Vec<BindPart>) -> Vec<BindPart> {
    let mut seated = Vec::new();
    let mut balls = Vec::new();
    for part in parts {
        let (pieces, piece_balls) = split_part_islands(part);
        seated.extend(pieces);
        balls.extend(piece_balls);
    }
    let parents = contact_parents(&seated);
    for index in 0..seated.len() {
        let socket = choose_socket(&seated, &balls, &parents, index);
        rebase_to_socket(&mut seated[index], socket);
    }
    seated
}

struct MeshIsland {
    verts: Vec<u32>,
    min: [f32; 3],
    max: [f32; 3],
    centroid: [f64; 3],
    radius: f64,
    cv: f64,
}

fn split_part_islands(part: BindPart) -> (Vec<BindPart>, Vec<Vec<[f64; 3]>>) {
    if part.positions.is_empty() || part.indices.len() < 3 {
        let mut part = part;
        part.socket_world = part.world.translation;
        return (vec![part], vec![Vec::new()]);
    }
    let rotation = unit_rotation(part.world.rotation);
    let islands = mesh_islands(&part, rotation);
    if islands.is_empty() {
        let mut part = part;
        part.socket_world = part.world.translation;
        return (vec![part], vec![Vec::new()]);
    }
    let groups = cluster_islands(&islands);
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&left, &right| {
        island_group_x(&islands, &groups[left]).total_cmp(&island_group_x(&islands, &groups[right]))
    });
    let mut pieces = Vec::new();
    let mut piece_balls = Vec::new();
    for (ordinal, group_index) in order.into_iter().enumerate() {
        let name = piece_name(&part.name, groups.len(), ordinal);
        let (piece, balls) = build_piece(&part, &islands, &groups[group_index], rotation, name);
        pieces.push(piece);
        piece_balls.push(balls);
    }
    (pieces, piece_balls)
}

fn piece_name(base: &str, count: usize, ordinal: usize) -> String {
    if count == 1 {
        base.to_string()
    } else if count == 2 {
        format!("{base}.{}", if ordinal == 0 { "L" } else { "R" })
    } else {
        format!("{base}.{ordinal}")
    }
}

fn island_group_x(islands: &[MeshIsland], group: &[usize]) -> f64 {
    group.iter().map(|index| islands[*index].centroid[0]).sum::<f64>() / group.len() as f64
}

fn mesh_islands(part: &BindPart, rotation: crate::Quat) -> Vec<MeshIsland> {
    let mut parent: Vec<u32> = (0..part.positions.len() as u32).collect();
    for triangle in part.indices.chunks_exact(3) {
        let a = find_vertex(&mut parent, triangle[0]);
        let b = find_vertex(&mut parent, triangle[1]);
        let c = find_vertex(&mut parent, triangle[2]);
        parent[b as usize] = a;
        parent[c as usize] = a;
    }
    let mut grouped: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
    for triangle in part.indices.chunks_exact(3) {
        let root = find_vertex(&mut parent, triangle[0]);
        let entry = grouped.entry(root).or_default();
        entry.extend_from_slice(triangle);
    }
    let mut islands = Vec::new();
    for mut verts in grouped.into_values() {
        verts.sort_unstable();
        verts.dedup();
        if verts.len() < 3 {
            continue;
        }
        let mut sum = [0.0_f64; 3];
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        let mut worlds = Vec::with_capacity(verts.len());
        for vertex in &verts {
            let world = world_vertex(part, part.positions[*vertex as usize], rotation);
            worlds.push(world);
            for axis in 0..3 {
                sum[axis] += world[axis];
                min[axis] = min[axis].min(world[axis] as f32);
                max[axis] = max[axis].max(world[axis] as f32);
            }
        }
        let count = verts.len() as f64;
        let centroid = [sum[0] / count, sum[1] / count, sum[2] / count];
        let mut mean = 0.0;
        let mut radii = Vec::with_capacity(worlds.len());
        for world in &worlds {
            let radius = distance3(*world, centroid);
            radii.push(radius);
            mean += radius;
        }
        mean /= count;
        let variance = radii.iter().map(|radius| (radius - mean) * (radius - mean)).sum::<f64>() / count;
        let cv = if mean > 1.0e-6 { variance.sqrt() / mean } else { 9.0 };
        islands.push(MeshIsland { verts, min, max, centroid, radius: mean, cv });
    }
    islands
}

fn find_vertex(parent: &mut [u32], mut index: u32) -> u32 {
    while parent[index as usize] != index {
        parent[index as usize] = parent[parent[index as usize] as usize];
        index = parent[index as usize];
    }
    index
}

fn cluster_islands(islands: &[MeshIsland]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..islands.len()).collect();
    for left in 0..islands.len() {
        for right in (left + 1)..islands.len() {
            if boxes_touch(islands[left].min, islands[left].max, islands[right].min, islands[right].max, 0.015) {
                let left_root = find_group(&mut parent, left);
                let right_root = find_group(&mut parent, right);
                parent[right_root] = left_root;
            }
        }
    }
    let mut grouped: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for index in 0..islands.len() {
        grouped.entry(find_group(&mut parent, index)).or_default().push(index);
    }
    grouped.into_values().collect()
}

fn find_group(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

fn build_piece(part: &BindPart, islands: &[MeshIsland], group: &[usize], rotation: crate::Quat, name: String) -> (BindPart, Vec<[f64; 3]>) {
    let mut used = Vec::new();
    let mut balls = Vec::new();
    for index in group {
        let island = &islands[*index];
        used.extend_from_slice(&island.verts);
        if island.cv < 0.12 && (0.004..0.04).contains(&island.radius) {
            balls.push(island.centroid);
        }
    }
    used.sort_unstable();
    used.dedup();
    let mut remap = vec![u32::MAX; part.positions.len()];
    let mut positions = Vec::with_capacity(used.len());
    let mut normals = Vec::with_capacity(used.len());
    let mut uvs = Vec::with_capacity(used.len());
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for (next, vertex) in used.iter().enumerate() {
        remap[*vertex as usize] = next as u32;
        let local = part.positions[*vertex as usize];
        positions.push(local);
        normals.push(part.normals.get(*vertex as usize).copied().unwrap_or([0.0, 1.0, 0.0]));
        uvs.push(part.uvs.get(*vertex as usize).copied().unwrap_or([0.0, 0.0]));
        let world = world_vertex(part, local, rotation);
        for axis in 0..3 {
            min[axis] = min[axis].min(world[axis] as f32);
            max[axis] = max[axis].max(world[axis] as f32);
        }
    }
    let keep: std::collections::BTreeSet<u32> = used.iter().copied().collect();
    let mut indices = Vec::new();
    for triangle in part.indices.chunks_exact(3) {
        if triangle.iter().all(|index| keep.contains(index)) {
            indices.extend(triangle.iter().map(|index| remap[*index as usize]));
        }
    }
    let triangles = (indices.len() / 3) as u32;
    let piece = BindPart {
        name,
        source_parent: part.source_parent.clone(),
        local: part.local.clone(),
        world: part.world.clone(),
        bounds_min: min,
        bounds_max: max,
        triangles,
        neighbor_gap_m: 0.0,
        outlier: false,
        socket_world: part.world.translation,
        positions,
        normals,
        uvs,
        indices,
    };
    (piece, balls)
}

fn contact_parents(parts: &[BindPart]) -> Vec<Option<usize>> {
    let pelvis = pelvis_part(parts);
    let mut parent = vec![None; parts.len()];
    let mut seen = vec![false; parts.len()];
    let mut queue = std::collections::VecDeque::new();
    seen[pelvis] = true;
    queue.push_back(pelvis);
    while let Some(current) = queue.pop_front() {
        for other in 0..parts.len() {
            if seen[other] {
                continue;
            }
            if boxes_touch(parts[current].bounds_min, parts[current].bounds_max, parts[other].bounds_min, parts[other].bounds_max, BIND_POSE_CONNECT_M as f32) {
                seen[other] = true;
                parent[other] = Some(current);
                queue.push_back(other);
            }
        }
    }
    parent
}

fn choose_socket(parts: &[BindPart], balls: &[Vec<[f64; 3]>], parents: &[Option<usize>], index: usize) -> [f64; 3] {
    let center = part_center(&parts[index]);
    let Some(parent) = parents[index] else {
        return center;
    };
    if let Some(ball) = nearest_point(&balls[index], part_center(&parts[parent])) {
        return ball;
    }
    if let Some(ball) = nearest_point(&balls[parent], center) {
        if distance3(ball, center) < 0.12 {
            return ball;
        }
    }
    center
}

fn nearest_point(points: &[[f64; 3]], target: [f64; 3]) -> Option<[f64; 3]> {
    points.iter().copied().min_by(|left, right| distance3(*left, target).total_cmp(&distance3(*right, target)))
}

fn rebase_to_socket(part: &mut BindPart, socket: [f64; 3]) {
    let rotation = unit_rotation(part.world.rotation);
    let delta = crate::Vec3::new(socket[0] - part.world.translation[0], socket[1] - part.world.translation[1], socket[2] - part.world.translation[2]);
    let local = rotation.conjugate().rotate(delta);
    let scale = part.world.scale;
    if scale.iter().any(|axis| !axis.is_finite() || *axis == 0.0) {
        part.socket_world = part.world.translation;
        return;
    }
    let offset = [(local.x / scale[0]) as f32, (local.y / scale[1]) as f32, (local.z / scale[2]) as f32];
    for position in &mut part.positions {
        position[0] -= offset[0];
        position[1] -= offset[1];
        position[2] -= offset[2];
    }
    part.socket_world = socket;
}

fn world_vertex(part: &BindPart, local: [f32; 3], rotation: crate::Quat) -> [f64; 3] {
    let scaled = crate::Vec3::new(local[0] as f64 * part.world.scale[0], local[1] as f64 * part.world.scale[1], local[2] as f64 * part.world.scale[2]);
    let rotated = rotation.rotate(scaled);
    [part.world.translation[0] + rotated.x, part.world.translation[1] + rotated.y, part.world.translation[2] + rotated.z]
}

fn unit_rotation(value: [f64; 4]) -> crate::Quat {
    let scale = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2] + value[3] * value[3]).sqrt();
    if !scale.is_finite() || scale < 1.0e-12 {
        crate::Quat::IDENTITY
    } else {
        crate::Quat { x: value[0] / scale, y: value[1] / scale, z: value[2] / scale, w: value[3] / scale }
    }
}

fn distance3(left: [f64; 3], right: [f64; 3]) -> f64 {
    let delta = [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
}

struct PartProbe {
    index: usize,
    name: String,
    source_parent: String,
    local: RigidTransform,
    world: RigidTransform,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    triangles: u32,
}

fn pack_glb(mut json: Vec<u8>, bin: Vec<u8>) -> Vec<u8> {
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let mut glb = Vec::new();
    let total = 12 + 8 + json.len() + 8 + bin.len();
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    glb.extend_from_slice(&json);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    glb.extend_from_slice(&bin);
    glb
}

fn parent_index(nodes: &[Json]) -> std::collections::HashMap<usize, usize> {
    let mut parent = std::collections::HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        for child in node.get("children").and_then(Json::as_array).unwrap_or(&[]) {
            if let Some(child) = child.as_f64() {
                parent.insert(child as usize, index);
            }
        }
    }
    parent
}

fn composed_transforms(nodes: &[Json], document: &Json) -> Result<Vec<RigidTransform>, ImportError> {
    let mut world = vec![identity_trs(); nodes.len()];
    let mut pending = Vec::new();
    if let Some(roots) = scene_nodes(document) {
        for root in roots {
            pending.push((root, identity_trs()));
        }
    }
    let mut guard = nodes.len() + 1;
    while let Some((index, parent)) = pending.pop() {
        guard -= 1;
        if guard == 0 {
            return Err(ImportError("glTF node hierarchy did not resolve".into()));
        }
        let node = nodes.get(index).ok_or_else(|| ImportError(format!("glTF node {index} is missing")))?;
        let local = node_trs(node)?;
        let composed = compose_trs(&parent, &local);
        world[index] = composed.clone();
        for child in node.get("children").and_then(Json::as_array).unwrap_or(&[]) {
            let child = child.as_f64().ok_or_else(|| ImportError("glTF child is not a node index".into()))? as usize;
            pending.push((child, composed.clone()));
        }
    }
    Ok(world)
}

fn identity_trs() -> RigidTransform {
    RigidTransform { translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3] }
}

fn node_trs(node: &Json) -> Result<RigidTransform, ImportError> {
    if node.get("matrix").is_some() {
        return Err(ImportError("bind pose keeps the source node TRS, and this node stores a matrix".into()));
    }
    Ok(RigidTransform {
        translation: read_vec3_or(node, "translation", [0.0; 3])?,
        rotation: read_quat_or(node)?,
        scale: read_vec3_or(node, "scale", [1.0; 3])?,
    })
}

fn read_vec3_or(node: &Json, key: &str, default: [f64; 3]) -> Result<[f64; 3], ImportError> {
    let Some(values) = node.get(key).and_then(Json::as_array) else { return Ok(default) };
    if values.len() != 3 {
        return Err(ImportError(format!("glTF node {key} is not a vec3")));
    }
    let mut out = [0.0; 3];
    for (index, value) in values.iter().enumerate() {
        out[index] = value.as_f64().filter(|number| number.is_finite()).ok_or_else(|| ImportError(format!("glTF node {key} is not finite")))?;
    }
    Ok(out)
}

fn read_quat_or(node: &Json) -> Result<[f64; 4], ImportError> {
    let Some(values) = node.get("rotation").and_then(Json::as_array) else { return Ok([0.0, 0.0, 0.0, 1.0]) };
    if values.len() != 4 {
        return Err(ImportError("glTF node rotation is not a quaternion".into()));
    }
    let mut out = [0.0; 4];
    for (index, value) in values.iter().enumerate() {
        out[index] = value.as_f64().filter(|number| number.is_finite()).ok_or_else(|| ImportError("glTF node rotation is not finite".into()))?;
    }
    Ok(out)
}

fn compose_trs(parent: &RigidTransform, local: &RigidTransform) -> RigidTransform {
    let parent_rotation = quat_from(parent.rotation);
    let scaled = crate::Vec3::new(local.translation[0] * parent.scale[0], local.translation[1] * parent.scale[1], local.translation[2] * parent.scale[2]);
    let translation = crate::Vec3::new(parent.translation[0], parent.translation[1], parent.translation[2]) + parent_rotation.rotate(scaled);
    let rotation = parent_rotation.mul(quat_from(local.rotation));
    RigidTransform {
        translation: [translation.x, translation.y, translation.z],
        rotation: [rotation.x, rotation.y, rotation.z, rotation.w],
        scale: [parent.scale[0] * local.scale[0], parent.scale[1] * local.scale[1], parent.scale[2] * local.scale[2]],
    }
}

fn quat_from(value: [f64; 4]) -> crate::Quat {
    crate::Quat { x: value[0], y: value[1], z: value[2], w: value[3] }
}

fn trs_matrix(trs: &RigidTransform) -> [f32; 16] {
    let translation = [trs.translation[0] as f32, trs.translation[1] as f32, trs.translation[2] as f32];
    let scale = [trs.scale[0] as f32, trs.scale[1] as f32, trs.scale[2] as f32];
    let rotation = [trs.rotation[0] as f32, trs.rotation[1] as f32, trs.rotation[2] as f32, trs.rotation[3] as f32];
    mul_matrix(translation_matrix(translation), mul_matrix(rotation_matrix(rotation), scale_matrix(scale)))
}

fn primitive_triangles(accessors: &[Json], primitive: &Json) -> Result<u32, ImportError> {
    let Some(index) = primitive.get("indices").and_then(Json::as_f64) else {
        return Err(ImportError("glTF primitive has no indices".into()));
    };
    let accessor = accessors.get(index as usize).ok_or_else(|| ImportError("glTF index accessor is missing".into()))?;
    let count = accessor.get("count").and_then(Json::as_f64).unwrap_or(0.0);
    if count < 3.0 || count.fract() != 0.0 || count as u32 % 3 != 0 {
        return Err(ImportError("glTF indices are not a triangle list".into()));
    }
    Ok((count as u32) / 3)
}

fn accessor_bounds(accessors: &[Json], index: usize) -> Result<([f32; 3], [f32; 3]), ImportError> {
    let accessor = accessors.get(index).ok_or_else(|| ImportError(format!("glTF accessor {index} is missing")))?;
    let minimum = json_vec3(accessor.get("min")).ok_or_else(|| ImportError("glTF POSITION accessor has no bounds".into()))?;
    let maximum = json_vec3(accessor.get("max")).ok_or_else(|| ImportError("glTF POSITION accessor has no bounds".into()))?;
    Ok((minimum, maximum))
}

fn json_vec3(json: Option<&Json>) -> Option<[f32; 3]> {
    let values = json?.as_array()?;
    if values.len() != 3 {
        return None;
    }
    Some([values[0].as_f64()? as f32, values[1].as_f64()? as f32, values[2].as_f64()? as f32])
}

fn cluster_ids(parts: &[PartProbe], gap: f32) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..parts.len()).collect();
    for left in 0..parts.len() {
        for right in (left + 1)..parts.len() {
            if boxes_touch(parts[left].bounds_min, parts[left].bounds_max, parts[right].bounds_min, parts[right].bounds_max, gap) {
                let left_root = find_root(&mut parent, left);
                let right_root = find_root(&mut parent, right);
                if left_root != right_root {
                    parent[right_root] = left_root;
                }
            }
        }
    }
    (0..parts.len()).map(|index| find_root(&mut parent, index)).collect()
}

fn find_root(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

fn boxes_touch(a_min: [f32; 3], a_max: [f32; 3], b_min: [f32; 3], b_max: [f32; 3], gap: f32) -> bool {
    (0..3).all(|axis| a_min[axis] <= b_max[axis] + gap && b_min[axis] <= a_max[axis] + gap)
}

fn union_bounds(parts: &[PartProbe], members: &[usize]) -> ([f32; 3], [f32; 3]) {
    let mut low = [f32::MAX; 3];
    let mut high = [f32::MIN; 3];
    for index in members {
        for axis in 0..3 {
            low[axis] = low[axis].min(parts[*index].bounds_min[axis]);
            high[axis] = high[axis].max(parts[*index].bounds_max[axis]);
        }
    }
    (low, high)
}

fn union_part_bounds(parts: &[BindPart], members: &[usize]) -> ([f32; 3], [f32; 3]) {
    let mut low = [f32::MAX; 3];
    let mut high = [f32::MIN; 3];
    for index in members {
        for axis in 0..3 {
            low[axis] = low[axis].min(parts[*index].bounds_min[axis]);
            high[axis] = high[axis].max(parts[*index].bounds_max[axis]);
        }
    }
    (low, high)
}

fn standing_body(low: [f32; 3], high: [f32; 3]) -> bool {
    let size = [high[0] - low[0], high[1] - low[1], high[2] - low[2]];
    (1.40..2.05).contains(&(size[1] as f64)) && (size[2] as f64) < 0.50 && (size[0] as f64) < 0.90 && (low[1] as f64).abs() < 0.05
}

fn read_local_mesh(bin: &[u8], accessors: &[Json], views: &[Json], mesh: &Json) -> Result<(Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>), ImportError> {
    let mut surfaces = Vec::new();
    collect_mesh(bin, accessors, views, mesh, identity(), Some(0), &mut surfaces)?;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for surface in surfaces {
        let base = positions.len() as u32;
        let count = surface.positions.len();
        positions.extend(surface.positions);
        if surface.normals.len() == count {
            normals.extend(surface.normals);
        } else {
            normals.extend(std::iter::repeat([0.0, 1.0, 0.0]).take(count));
        }
        uvs.extend(surface.texcoords);
        indices.extend(surface.indices.into_iter().map(|index| index + base));
    }
    if positions.is_empty() {
        return Err(ImportError("selected part has no material-0 triangles".into()));
    }
    Ok((positions, normals, uvs, indices))
}

fn fill_neighbor_gaps(parts: &mut [BindPart], outliers: &mut [BindPart]) {
    for index in 0..parts.len() {
        let mut nearest = f64::MAX;
        for other in 0..parts.len() {
            if other == index {
                continue;
            }
            nearest = nearest.min(part_gap(&parts[index], &parts[other]));
        }
        parts[index].neighbor_gap_m = nearest;
    }
    for outlier in outliers.iter_mut() {
        outlier.neighbor_gap_m = parts.iter().map(|part| part_gap(outlier, part)).fold(f64::MAX, f64::min);
        outlier.outlier = true;
    }
}

fn part_gap(left: &BindPart, right: &BindPart) -> f64 {
    let mut squared = 0.0;
    for axis in 0..3 {
        let separation = (left.bounds_min[axis] as f64 - right.bounds_max[axis] as f64).max(right.bounds_min[axis] as f64 - left.bounds_max[axis] as f64).max(0.0);
        squared += separation * separation;
    }
    squared.sqrt()
}

fn pelvis_part(parts: &[BindPart]) -> usize {
    let mut best = 0usize;
    let mut best_rank = 3u8;
    for (index, part) in parts.iter().enumerate() {
        let name = part.name.to_ascii_lowercase();
        let rank = if name.contains("hip") { 0 } else if name.contains("pelvis") { 1 } else if name.contains("waist") { 2 } else { 3 };
        if rank < best_rank {
            best_rank = rank;
            best = index;
        }
    }
    best
}

fn symmetry_error(parts: &[BindPart], low: [f32; 3], high: [f32; 3]) -> f64 {
    let mid = ((low[0] + high[0]) * 0.5) as f64;
    let mut worst = 0.0_f64;
    for part in parts {
        let center = part_center(part);
        if (center[0] - mid).abs() <= 0.04 {
            continue;
        }
        let target = [mid * 2.0 - center[0], center[1], center[2]];
        let nearest = parts
            .iter()
            .filter(|other| other.name != part.name)
            .map(|other| {
                let center = part_center(other);
                let delta = [center[0] - target[0], center[1] - target[1], center[2] - target[2]];
                (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
            })
            .fold(f64::MAX, f64::min);
        worst = worst.max(nearest);
    }
    worst
}

fn part_center(part: &BindPart) -> [f64; 3] {
    [
        (part.bounds_min[0] + part.bounds_max[0]) as f64 * 0.5,
        (part.bounds_min[1] + part.bounds_max[1]) as f64 * 0.5,
        (part.bounds_min[2] + part.bounds_max[2]) as f64 * 0.5,
    ]
}

fn first_material(document: &Json) -> ([f32; 4], f32, f32) {
    let Some(material) = document.get("materials").and_then(Json::as_array).and_then(|materials| materials.first()) else {
        return ([0.8, 0.8, 0.8, 1.0], 0.0, 1.0);
    };
    let pbr = material.get("pbrMetallicRoughness");
    let base_color = pbr.and_then(|value| value.get("baseColorFactor")).and_then(json_float4).unwrap_or([0.8, 0.8, 0.8, 1.0]);
    let metallic = pbr.and_then(|value| value.get("metallicFactor")).and_then(Json::as_f64).unwrap_or(1.0) as f32;
    let roughness = pbr.and_then(|value| value.get("roughnessFactor")).and_then(Json::as_f64).unwrap_or(1.0) as f32;
    (base_color, metallic, roughness)
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

    #[test]
    fn a_disconnected_box_is_an_outlier_and_the_standing_pair_is_kept() {
        let positions = [
            [-0.5, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ];
        let normals = [[0.0, 1.0, 0.0]; 8];
        let uvs = [[0.0, 0.0]; 8];
        let indices = [
            0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2, 2, 6, 7, 2, 7, 3, 3, 7, 4, 3, 4, 0,
        ];
        let mut bin = Vec::new();
        let position_view = view_json(push_f32x3(&mut bin, &positions));
        let normal_view = view_json(push_f32x3(&mut bin, &normals));
        let uv_view = view_json(push_f32x2(&mut bin, &uvs));
        let index_view = view_json(push_u32(&mut bin, &indices));
        let json = format!(
            "{{\"asset\":{{\"version\":\"2.0\"}},\"scene\":0,\"scenes\":[{{\"nodes\":[0,1,2]}}],\"nodes\":[{{\"name\":\"body\",\"mesh\":0,\"translation\":[0,0.8,0],\"scale\":[0.4,1.6,0.2]}},{{\"name\":\"arm\",\"mesh\":0,\"translation\":[0.25,0.3,0],\"scale\":[0.2,0.2,0.2]}},{{\"name\":\"stray\",\"mesh\":0,\"translation\":[3,0.8,0],\"scale\":[0.2,0.2,0.2]}}],\"materials\":[{{\"pbrMetallicRoughness\":{{\"baseColorFactor\":[0.8,0.8,0.8,1],\"metallicFactor\":0,\"roughnessFactor\":1}}}}],\"meshes\":[{{\"primitives\":[{{\"attributes\":{{\"POSITION\":0,\"NORMAL\":1,\"TEXCOORD_0\":2}},\"indices\":3,\"material\":0}}]}}],\"buffers\":[{{\"byteLength\":{}}}],\"bufferViews\":[{position_view},{normal_view},{uv_view},{index_view}],\"accessors\":[{{\"bufferView\":0,\"componentType\":5126,\"count\":8,\"type\":\"VEC3\",\"min\":[-0.5,-0.5,-0.5],\"max\":[0.5,0.5,0.5]}},{{\"bufferView\":1,\"componentType\":5126,\"count\":8,\"type\":\"VEC3\"}},{{\"bufferView\":2,\"componentType\":5126,\"count\":8,\"type\":\"VEC2\"}},{{\"bufferView\":3,\"componentType\":5125,\"count\":36,\"type\":\"SCALAR\"}}]}}",
            bin.len()
        );
        let assembly = assemble_rigid_bind_pose(&pack_glb(json.into_bytes(), bin)).expect("assembly");
        let names: Vec<_> = assembly.parts.iter().map(|part| part.name.as_str()).collect();
        assert_eq!(names, vec!["arm", "body"]);
        assert_eq!(assembly.outliers.len(), 1);
        assert_eq!(assembly.outliers[0].name, "stray");
        assert!(assembly.outliers[0].outlier);
        assert!(assembly.parts.iter().all(|part| !part.outlier && part.neighbor_gap_m <= BIND_POSE_CONNECT_M));
        assert!(assembly.feet_y.abs() < 0.02, "{}", assembly.feet_y);
        assert!((1.40..2.05).contains(&assembly.height_m), "{}", assembly.height_m);
        assert!(assembly.parts.iter().all(|part| part.positions.len() == 8));
        assert_eq!(assembly.parts[1].local.scale, [0.4, 1.6, 0.2]);
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
