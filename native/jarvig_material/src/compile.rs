//! First material compiler. WGSL is the backend artifact, not the IR.

use crate::ir::{IrOp, MaterialIr, ResourceClass, VertexSemantic};
use crate::{BlendMode, CullMode, MaterialDomain, MaterialError, ShadingModel, ValueType};

/// Compiler target. Only WGSL is implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderTarget {
    Wgsl,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledParameter {
    pub name: String,
    pub ty: ValueType,
    pub offset: u32,
    pub size: u32,
    pub default: [f32; 4],
}

/// A texture or sampler binding. The index is group 1. It is not an authoring contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledResource {
    pub name: String,
    pub class: ResourceClass,
    pub binding: u32,
    pub semantic: Option<crate::TextureSemantic>,
}

/// Program identity after compilation. The artifact is one backend. The rest is JARVIG data.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledMaterial {
    pub domain: MaterialDomain,
    pub shading: ShadingModel,
    pub blend: BlendMode,
    pub cull: CullMode,
    pub two_sided: bool,
    pub parameters: Vec<CompiledParameter>,
    pub resources: Vec<CompiledResource>,
    pub vertex: Vec<VertexSemantic>,
    pub target: ShaderTarget,
    pub artifact: String,
    /// Program key. Not a material-instance id. Static changes belong here later.
    pub key: u64,
}

pub fn compile(ir: &MaterialIr) -> Result<CompiledMaterial, MaterialError> {
    if ir.version() != crate::MATERIAL_IR_VERSION {
        return Err(MaterialError::UnsupportedIr);
    }
    let mut parameters = Vec::new();
    let mut offset = 0u32;
    let mut fields = String::new();
    for parameter in ir.parameters() {
        let size = 16;
        let ident = wgsl_ident(&parameter.name)?;
        fields.push_str(&format!("    {ident}: vec4<f32>,\n"));
        parameters.push(CompiledParameter {
            name: parameter.name.clone(),
            ty: parameter.ty,
            offset,
            size,
            default: parameter.default,
        });
        offset += size;
    }
    if fields.is_empty() {
        fields.push_str("    unused: vec4<f32>,\n");
    }
    let mut body = String::new();
    for op in ir.ops() {
        match op {
            IrOp::LoadVertexColor { dest } => {
                body.push_str(&format!("    let v{dest} = vec4<f32>(in.color, 1.0);\n"));
            }
            IrOp::LoadParameter { dest, name } => {
                let ident = wgsl_ident(name)?;
                let ty = ir.parameters().iter().find(|parameter| parameter.name == *name).map(|parameter| parameter.ty);
                let expr = match ty {
                    Some(ValueType::Float) => format!("material.{ident}.x"),
                    Some(ValueType::Float2) => format!("material.{ident}.xy"),
                    Some(ValueType::Float3) => format!("material.{ident}.xyz"),
                    _ => format!("material.{ident}"),
                };
                body.push_str(&format!("    let v{dest} = {expr};\n"));
            }
            IrOp::Constant { dest, ty, value } => {
                let expr = match ty {
                    ValueType::Float => format!("{:?}", value[0]),
                    ValueType::Float3 => format!("vec3<f32>({:?}, {:?}, {:?})", value[0], value[1], value[2]),
                    _ => format!("vec4<f32>({:?}, {:?}, {:?}, {:?})", value[0], value[1], value[2], value[3]),
                };
                body.push_str(&format!("    let v{dest} = {expr};\n"));
            }
            IrOp::Multiply { dest, left, right } => {
                body.push_str(&format!("    let v{dest} = v{left} * v{right};\n"));
            }
            IrOp::LoadTexCoord0 { dest } => {
                body.push_str(&format!("    let v{dest} = in.uv;\n"));
            }
            IrOp::ScaleTexCoord { dest, scale } => {
                body.push_str(&format!("    let v{dest} = in.uv * v{scale};\n"));
            }
            IrOp::SampleTexture2D { dest, texture, sampler, uv } => {
                let texture = wgsl_ident(texture)?;
                let sampler = wgsl_ident(sampler)?;
                body.push_str(&format!("    let v{dest} = textureSample({texture}, {sampler}, v{uv});\n"));
            }
            IrOp::Swizzle { dest, source, channels, width } => {
                body.push_str(&format!("    let v{dest} = {};\n", swizzle_expr(*source, channels, *width)));
            }
            IrOp::NormalDecode { dest, sample, scale } => {
                body.push_str(&format!(
                    "    let raw{dest} = v{sample}.xyz * 2.0 - vec3<f32>(1.0);\n    let v{dest} = normalize(vec3<f32>(raw{dest}.xy * v{scale}, raw{dest}.z));\n"
                ));
            }
            IrOp::Lerp { dest, from, to, factor } => {
                body.push_str(&format!("    let v{dest} = mix(v{from}, v{to}, v{factor});\n"));
            }
            IrOp::WriteSurfaceColor { value } => {
                body.push_str(&format!("    return v{value};\n"));
            }
            IrOp::WriteBaseColor { value } => body.push_str(&format!("    surface.base_color = v{value};\n")),
            IrOp::WriteMetallic { value } => body.push_str(&format!("    surface.metallic = v{value};\n")),
            IrOp::WriteRoughness { value } => body.push_str(&format!("    surface.perceptual_roughness = v{value};\n")),
            IrOp::WriteNormal { value } => body.push_str(&format!("    surface.normal_ts = v{value};\n")),
            IrOp::WriteAmbientOcclusion { value } => body.push_str(&format!("    surface.ambient_occlusion = v{value};\n")),
            IrOp::WriteEmissive { value } => body.push_str(&format!("    surface.emissive = v{value};\n")),
        }
    }
    let mut resources = Vec::new();
    let mut resource_wgsl = String::new();
    let mut binding = 1u32;
    for resource in ir.resources() {
        let ident = wgsl_ident(&resource.name)?;
        let ty = match resource.class {
            ResourceClass::Texture2D => "texture_2d<f32>",
            ResourceClass::Sampler => "sampler",
        };
        resource_wgsl.push_str(&format!("@group(1) @binding({binding}) var {ident}: {ty};\n"));
        resources.push(CompiledResource {
            name: resource.name.clone(),
            class: resource.class,
            binding,
            semantic: resource.semantic,
        });
        binding += 1;
    }
    let vertex_io = vertex_io(ir.vertex());
    let normal_matrix = if ir.vertex().contains(&VertexSemantic::Normal) {
        "fn jarvig_normal_matrix(m: mat3x3<f32>) -> mat3x3<f32> {\n    let c0 = m[0];\n    let c1 = m[1];\n    let c2 = m[2];\n    let r0 = cross(c1, c2);\n    let inv_det = 1.0 / max(dot(c0, r0), 1e-8);\n    return mat3x3<f32>(r0, cross(c2, c0), cross(c0, c1)) * inv_det;\n}\n"
    } else {
        ""
    };
    let fragment = if ir.shading == crate::ShadingModel::StandardMetalRough {
        standard_fragment(&body, ir.two_sided)
    } else {
        format!("@fragment\nfn fs(in: VertexOut) -> @location(0) vec4<f32> {{\n{body}}}\n")
    };
    let artifact = format!(
        r#"// Generated from JARVIG Material IR version {version}. Not an authored shader.
// Group 0 binding 0 is the view and object transform.
// Group 1 binding 0 is the material uniform. Later bindings are textures and samplers.
// Group 2, when this master is StandardMetalRough, is the direct-light list, one environment packet, one reflection-probe packet with its cubemap, and direct-light visibility. Unlit does not declare it.
// Sampling returns linear values. sRGB versus linear is a texture format, not a shader guess.
// color-space is a texture format. This shader does not decode sRGB.
struct RenderUniforms {{
    projection: mat4x4<f32>,
    view: mat4x4<f32>,
    model: mat4x4<f32>,
}}
struct MaterialParameters {{
{fields}}}
@group(0) @binding(0) var<uniform> render: RenderUniforms;
@group(1) @binding(0) var<uniform> material: MaterialParameters;
{resource_wgsl}{normal_matrix}struct VertexOut {{
    @builtin(position) clip: vec4<f32>,
{outputs}}}
@vertex
fn vs({inputs}) -> VertexOut {{
    var out: VertexOut;
    let world = render.model * vec4<f32>(position, 1.0);
    let eye = render.view * world;
    out.clip = render.projection * eye;
{assigns}    return out;
}}
{fragment}"#,
        version = ir.version(),
        fields = fields,
        resource_wgsl = resource_wgsl,
        normal_matrix = normal_matrix,
        outputs = vertex_io.outputs,
        inputs = vertex_io.inputs,
        assigns = vertex_io.assigns,
        fragment = fragment,
    );
    Ok(CompiledMaterial {
        domain: ir.domain,
        shading: ir.shading,
        blend: ir.blend,
        cull: ir.cull,
        two_sided: ir.two_sided,
        parameters,
        resources,
        vertex: ir.vertex().to_vec(),
        target: ShaderTarget::Wgsl,
        key: fnv(&artifact),
        artifact,
    })
}

struct VertexIo {
    inputs: String,
    outputs: String,
    assigns: String,
}

fn vertex_io(vertex: &[VertexSemantic]) -> VertexIo {
    let mut inputs = String::from("@location(0) position: vec3<f32>");
    let mut outputs = String::new();
    let mut assigns = String::new();
    if vertex.contains(&VertexSemantic::Color0) {
        inputs.push_str(", @location(1) color: vec3<f32>");
        outputs.push_str("    @location(0) color: vec3<f32>,\n");
        assigns.push_str("    out.color = color;\n");
    }
    if vertex.contains(&VertexSemantic::TexCoord0) {
        inputs.push_str(", @location(2) uv: vec2<f32>");
        outputs.push_str("    @location(1) uv: vec2<f32>,\n");
        assigns.push_str("    out.uv = uv;\n");
    }
    if vertex.contains(&VertexSemantic::Normal) {
        inputs.push_str(", @location(3) normal: vec3<f32>");
        outputs.push_str("    @location(2) normal: vec3<f32>,\n");
        outputs.push_str("    @location(4) render_position: vec3<f32>,\n");
        assigns.push_str(
            "    let model3 = mat3x3<f32>(render.model[0].xyz, render.model[1].xyz, render.model[2].xyz);\n    let view3 = mat3x3<f32>(render.view[0].xyz, render.view[1].xyz, render.view[2].xyz);\n    out.normal = normalize(view3 * jarvig_normal_matrix(model3) * normal);\n    out.render_position = eye.xyz;\n",
        );
    }
    if vertex.contains(&VertexSemantic::Tangent) {
        inputs.push_str(", @location(4) tangent: vec4<f32>");
        outputs.push_str("    @location(3) tangent: vec4<f32>,\n");
        assigns.push_str(
            "    let tangent_model = mat3x3<f32>(render.model[0].xyz, render.model[1].xyz, render.model[2].xyz);\n    let tangent_view = mat3x3<f32>(render.view[0].xyz, render.view[1].xyz, render.view[2].xyz);\n    out.tangent = vec4<f32>(normalize(tangent_view * tangent_model * tangent.xyz), tangent.w);\n",
        );
    }
    VertexIo { inputs, outputs, assigns }
}

fn standard_fragment(body: &str, two_sided: bool) -> String {
    let (fragment_params, facing) = if two_sided {
        (
            ", @builtin(front_facing) front_facing: bool",
            "    var geometric = in.normal;\n    var tangent = in.tangent;\n    if (!front_facing) {\n        geometric = -geometric;\n        tangent.w = -tangent.w;\n    }\n",
        )
    } else {
        ("", "    let geometric = in.normal;\n    let tangent = in.tangent;\n")
    };
    format!(
        r#"const JARVIG_PI: f32 = {pi:.8};
const JARVIG_MIN_ROUGHNESS: f32 = {rough:.8};
const JARVIG_DIELECTRIC_F0: f32 = {f0:.8};
const JARVIG_MIN_LIGHT_DISTANCE: f32 = {distance:.8};
struct MaterialSurface {{
    base_color: vec4<f32>,
    metallic: f32,
    perceptual_roughness: f32,
    normal_ts: vec3<f32>,
    ambient_occlusion: f32,
    emissive: vec3<f32>,
}}
struct LightHeader {{
    count: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}}
struct GpuLight {{
    kind: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    position: vec3<f32>,
    pad3: f32,
    direction: vec3<f32>,
    pad4: f32,
    color: vec3<f32>,
    intensity: f32,
    range_m: f32,
    cos_inner: f32,
    cos_outer: f32,
    pad5: f32,
}}
struct Incident {{
    l: vec3<f32>,
    illuminance: f32,
}}
struct EnvironmentPacket {{
    upper: vec4<f32>,
    lower: vec4<f32>,
}}
struct ReflectionProbePacket {{
    center_radius: vec4<f32>,
    params: vec4<f32>,
}}
struct GpuShadow {{
    clip: mat4x4<f32>,
    atlas_origin: vec2<f32>,
    atlas_scale: vec2<f32>,
    kind: f32,
    enabled: f32,
    bias: f32,
    far_m: f32,
    near_m: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
    clip1: mat4x4<f32>,
    clip2: mat4x4<f32>,
    clip3: mat4x4<f32>,
    splits: vec4<f32>,
    half_extents: vec4<f32>,
    extra: vec4<f32>,
}}
@group(2) @binding(0) var<uniform> light_header: LightHeader;
@group(2) @binding(1) var<storage, read> lights: array<GpuLight>;
@group(2) @binding(2) var<uniform> environment_light: EnvironmentPacket;
@group(2) @binding(3) var<uniform> reflection_probe: ReflectionProbePacket;
@group(2) @binding(4) var reflection_cube: texture_cube<f32>;
@group(2) @binding(5) var reflection_sampler: sampler;
@group(2) @binding(6) var<storage, read> shadows: array<GpuShadow>;
@group(2) @binding(7) var directional_shadow: texture_2d<f32>;
@group(2) @binding(8) var spot_shadow: texture_2d<f32>;
@group(2) @binding(9) var point_shadow: texture_cube<f32>;
@group(2) @binding(10) var shadow_sampler: sampler;
@group(2) @binding(11) var irradiance_cube: texture_cube<f32>;
@group(2) @binding(12) var contact_depth: texture_2d<f32>;
fn evaluate_material(in: VertexOut) -> MaterialSurface {{
    var surface: MaterialSurface;
    surface.base_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    surface.metallic = 0.0;
    surface.perceptual_roughness = 0.5;
    surface.normal_ts = vec3<f32>(0.0, 0.0, 1.0);
    surface.ambient_occlusion = 1.0;
    surface.emissive = vec3<f32>(0.0, 0.0, 0.0);
{body}    return surface;
}}
fn jarvig_view_direction(render_position: vec3<f32>) -> vec3<f32> {{
    return normalize(-render_position);
}}
fn jarvig_shade_normal(geometric: vec3<f32>, tangent: vec4<f32>, normal_ts: vec3<f32>) -> vec3<f32> {{
    let n = normalize(geometric);
    let t_raw = normalize(tangent.xyz);
    let t = normalize(t_raw - n * dot(t_raw, n));
    let b = cross(n, t) * tangent.w;
    return normalize(t * normal_ts.x + b * normal_ts.y + n * normal_ts.z);
}}
fn jarvig_punctual_illuminance(intensity_cd: f32, distance_m: f32, range_m: f32) -> f32 {{
    if (range_m > 0.0 && distance_m > range_m) {{ return 0.0; }}
    let d = max(distance_m, JARVIG_MIN_LIGHT_DISTANCE);
    return intensity_cd / (d * d);
}}
fn jarvig_spot_angle(cos_theta: f32, cos_inner: f32, cos_outer: f32) -> f32 {{
    if (cos_theta >= cos_inner) {{ return 1.0; }}
    if (cos_theta <= cos_outer) {{ return 0.0; }}
    let t = clamp((cos_theta - cos_outer) / max(cos_inner - cos_outer, 1e-6), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}}
fn jarvig_incident(light: GpuLight, frag: vec3<f32>) -> Incident {{
    if (light.kind == 0u) {{
        return Incident(normalize(-light.direction), light.intensity);
    }}
    let to_light = light.position - frag;
    let distance = length(to_light);
    let l = to_light / max(distance, 1e-8);
    var illuminance = jarvig_punctual_illuminance(light.intensity, distance, light.range_m);
    if (light.kind == 2u) {{
        let cos_theta = dot(-l, normalize(light.direction));
        illuminance = illuminance * jarvig_spot_angle(cos_theta, light.cos_inner, light.cos_outer);
    }}
    return Incident(l, illuminance);
}}
fn jarvig_specular_roughness(normal: vec3<f32>, authored: f32) -> f32 {{
    // Geometric specular AA. Derivatives are taken once per fragment, before the light loop.
    // The rise is capped so a smooth 0.045 mirror stays a mirror. Edges may dull slightly.
    let perceptual = clamp(authored, JARVIG_MIN_ROUGHNESS, 1.0);
    let dx = dpdx(normal);
    let dy = dpdy(normal);
    let kernel = min((dot(dx, dx) + dot(dy, dy)) * 0.15, 0.02);
    let alpha = perceptual * perceptual;
    let filtered = sqrt(sqrt(clamp(alpha * alpha + kernel, 0.0, 1.0)));
    return clamp(min(filtered, perceptual + 0.02), JARVIG_MIN_ROUGHNESS, 1.0);
}}
fn jarvig_direct(surface: MaterialSurface, n: vec3<f32>, v: vec3<f32>, l: vec3<f32>) -> vec3<f32> {{
    let normal = normalize(n);
    let view = normalize(v);
    let light = normalize(l);
    let n_dot_l = dot(normal, light);
    let n_dot_v = dot(normal, view);
    if (n_dot_l <= 0.0 || n_dot_v <= 0.0) {{ return vec3<f32>(0.0); }}
    let h = normalize(view + light);
    let n_dot_h = max(dot(normal, h), 0.0);
    let v_dot_h = max(dot(view, h), 0.0);
    let roughness = clamp(surface.perceptual_roughness, JARVIG_MIN_ROUGHNESS, 1.0);
    let alpha = roughness * roughness;
    let a2 = alpha * alpha;
    let denom_d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    let d = a2 / max(JARVIG_PI * denom_d * denom_d, 1e-8);
    let g1 = 2.0 * max(n_dot_v, 1e-4) / max(max(n_dot_v, 1e-4) + sqrt(a2 + (1.0 - a2) * max(n_dot_v, 1e-4) * max(n_dot_v, 1e-4)), 1e-8);
    let g2 = 2.0 * n_dot_l / max(n_dot_l + sqrt(a2 + (1.0 - a2) * n_dot_l * n_dot_l), 1e-8);
    let f0 = mix(vec3<f32>(JARVIG_DIELECTRIC_F0), surface.base_color.rgb, clamp(surface.metallic, 0.0, 1.0));
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - clamp(v_dot_h, 0.0, 1.0), 5.0);
    let spec = fresnel * (d * g1 * g2 / max(4.0 * max(n_dot_v, 1e-4) * n_dot_l, 1e-6)) * n_dot_l;
    let diffuse = surface.base_color.rgb * (1.0 - clamp(surface.metallic, 0.0, 1.0)) * n_dot_l / JARVIG_PI;
    return diffuse + spec;
}}
fn jarvig_hemisphere(direction: vec3<f32>) -> vec3<f32> {{
    let sky = clamp(direction.y, -1.0, 1.0) * 0.5 + 0.5;
    return mix(environment_light.lower.xyz, environment_light.upper.xyz, sky);
}}
fn jarvig_term_on(bit: u32) -> bool {{
    let mask = light_header.pad0;
    if (mask == 0u) {{ return true; }}
    return (mask & bit) != 0u;
}}
fn jarvig_shadows_on() -> bool {{
    let mask = light_header.pad0;
    if (mask == 0u) {{ return true; }}
    return (mask & 32u) != 0u;
}}
fn jarvig_shadow_ndc_perspective(distance_m: f32, near_m: f32, far_m: f32) -> f32 {{
    if (distance_m <= 0.0 || far_m <= near_m || near_m <= 0.0) {{ return 0.0; }}
    let ndc = near_m * (far_m - distance_m) / ((far_m - near_m) * distance_m);
    return clamp(ndc, 0.0, 1.0);
}}
fn jarvig_shadow_bias(shadow: GpuShadow, n_dot_l: f32, distance_m: f32, span_m: f32) -> f32 {{
    let slope = clamp(1.0 - max(n_dot_l, 0.0), 0.0, 1.0);
    let meters = max(shadow.bias + shadow.pad0 * slope, 0.0);
    let span = max(span_m, 0.001);
    if (shadow.kind == 1.0) {{
        return meters / span;
    }}
    let d = max(distance_m, max(shadow.near_m, 0.001));
    return shadow.near_m * shadow.far_m * meters / (span * d * d);
}}
fn jarvig_cascade_clip(shadow: GpuShadow, index: i32, world_position: vec3<f32>) -> vec4<f32> {{
    let point = vec4<f32>(world_position, 1.0);
    if (index <= 0) {{ return shadow.clip * point; }}
    if (index == 1) {{ return shadow.clip1 * point; }}
    if (index == 2) {{ return shadow.clip2 * point; }}
    return shadow.clip3 * point;
}}
fn jarvig_cascade_half(shadow: GpuShadow, index: i32) -> f32 {{
    if (index <= 0) {{ return shadow.half_extents.x; }}
    if (index == 1) {{ return shadow.half_extents.y; }}
    if (index == 2) {{ return shadow.half_extents.z; }}
    return shadow.half_extents.w;
}}
fn jarvig_cascade_end(shadow: GpuShadow, index: i32) -> f32 {{
    if (index <= 0) {{ return shadow.splits.x; }}
    if (index == 1) {{ return shadow.splits.y; }}
    if (index == 2) {{ return shadow.splits.z; }}
    return shadow.splits.w;
}}
fn jarvig_quadrant(index: i32) -> vec4<f32> {{
    let col = f32(index % 2);
    let row = f32(index / 2);
    return vec4<f32>(col * 0.5, row * 0.5, 0.5, 0.5);
}}
fn jarvig_shadow_depth(sample: vec4<f32>) -> f32 {{
    // Pair of the shadow-pass pack. .r alone is an fp16 step of ~0.001 near 1.
    return sample.r + sample.g / 1024.0;
}}
fn jarvig_disk_texel(index: i32, radius: f32, spin: f32) -> vec2<f32> {{
    let i = f32(index);
    let r = sqrt((i + 0.5) / 16.0) * radius;
    let theta = i * 2.39996323 + spin;
    return vec2<f32>(cos(theta), sin(theta)) * r;
}}
fn jarvig_disk_weight(offset: vec2<f32>, radius: f32) -> f32 {{
    return 1.0 - smoothstep(radius * 0.25, max(radius, 0.5), length(offset));
}}
fn jarvig_shadow_spin(uv: vec2<f32>) -> f32 {{
    // One slow turn across the map. Stable while texel snap holds this UV. Not screen noise.
    return (uv.x + uv.y) * 6.2831853;
}}
fn jarvig_shadow_pcf(map: texture_2d<f32>, uv: vec2<f32>, receiver: f32, bias: f32, radius: f32, bounds: vec4<f32>) -> f32 {{
    let texel = 1.0 / vec2<f32>(textureDimensions(map));
    let inset = texel * 0.5;
    let reach = clamp(radius, 0.5, 4.0);
    let spin = jarvig_shadow_spin(uv);
    var lit = 0.0;
    var weight = 0.0;
    for (var i: i32 = 0; i < 16; i = i + 1) {{
        let offset = jarvig_disk_texel(i, reach, spin);
        let w = jarvig_disk_weight(offset, reach);
        let sample_uv = clamp(uv + offset * texel, bounds.xy + inset, bounds.xy + bounds.zw - inset);
        let stored = jarvig_shadow_depth(textureSampleLevel(map, shadow_sampler, sample_uv, 0.0));
        if (!(receiver + bias < stored)) {{
            lit = lit + w;
        }}
        weight = weight + w;
    }}
    return lit / max(weight, 1e-4);
}}
fn jarvig_blocker_average(map: texture_2d<f32>, uv: vec2<f32>, receiver: f32, bias: f32, bounds: vec4<f32>, search: f32) -> vec2<f32> {{
    let texel = 1.0 / vec2<f32>(textureDimensions(map));
    let inset = texel * 0.5;
    let spin = jarvig_shadow_spin(uv);
    let reach = clamp(search, 1.0, 4.0);
    var sum = 0.0;
    var count = 0.0;
    for (var i: i32 = 0; i < 16; i = i + 1) {{
        let offset = jarvig_disk_texel(i, reach, spin);
        let sample_uv = clamp(uv + offset * texel, bounds.xy + inset, bounds.xy + bounds.zw - inset);
        let stored = jarvig_shadow_depth(textureSampleLevel(map, shadow_sampler, sample_uv, 0.0));
        if (stored > receiver + bias && stored < 0.999) {{
            sum = sum + stored;
            count = count + 1.0;
        }}
    }}
    return vec2<f32>(sum, count);
}}
fn jarvig_penumbra_radius(blocker: vec2<f32>, penumbra_m: f32, texel_m: f32, max_radius: f32) -> f32 {{
    // A few blockers ease into the physical width. A hard count switch leaves a ring.
    let found = clamp(blocker.y / 4.0, 0.0, 1.0);
    let wide = clamp(penumbra_m / max(texel_m, 1e-4), 0.75, max_radius);
    return mix(0.75, wide, found);
}}
fn jarvig_pcss_radius(shadow: GpuShadow, receiver: f32, blocker: vec2<f32>, half_m: f32, span_m: f32, max_radius: f32) -> f32 {{
    if (shadow.extra.z < 0.5) {{ return max_radius; }}
    let average = blocker.x / max(blocker.y, 1.0);
    let receiver_m = (1.0 - receiver) * span_m;
    let blocker_m = (1.0 - average) * span_m;
    let gap = max(receiver_m - blocker_m, 0.0);
    let penumbra = gap * max(shadow.extra.w, 0.0);
    let texel_m = max(half_m * 2.0 / f32(textureDimensions(directional_shadow).x) * 2.0, 1e-4);
    return jarvig_penumbra_radius(blocker, penumbra, texel_m, max_radius);
}}
fn jarvig_shadow_projective(map: texture_2d<f32>, uv: vec2<f32>, receiver: f32, bias: f32, radius: f32, bounds: vec4<f32>) -> f32 {{
    if (uv.x < bounds.x || uv.y < bounds.y || uv.x > bounds.x + bounds.z || uv.y > bounds.y + bounds.w) {{
        return 1.0;
    }}
    return jarvig_shadow_pcf(map, uv, receiver, bias, radius, bounds);
}}
fn jarvig_directional_visibility(shadow: GpuShadow, world_position: vec3<f32>, eye_z: f32, n_dot_l: f32) -> f32 {{
    let count = clamp(i32(shadow.extra.x), 1, 4);
    var index = 0;
    if (eye_z > shadow.splits.x) {{ index = 1; }}
    if (eye_z > shadow.splits.y) {{ index = 2; }}
    if (eye_z > shadow.splits.z) {{ index = 3; }}
    index = min(index, count - 1);
    let max_radius = clamp(shadow.pad1, 1.0, 4.0);
    let vis = jarvig_one_cascade(shadow, index, world_position, n_dot_l, max_radius);
    let end = jarvig_cascade_end(shadow, index);
    let start = select(0.05, jarvig_cascade_end(shadow, index - 1), index > 0);
    let width = max((end - start) * shadow.extra.y, 0.05);
    if (index + 1 < count && eye_z > end - width) {{
        let next = jarvig_one_cascade(shadow, index + 1, world_position, n_dot_l, max_radius);
        let t = clamp((eye_z - (end - width)) / width, 0.0, 1.0);
        let s = t * t * (3.0 - 2.0 * t);
        return mix(vis, next, s);
    }}
    return vis;
}}
fn jarvig_one_cascade(shadow: GpuShadow, index: i32, world_position: vec3<f32>, n_dot_l: f32, max_radius: f32) -> f32 {{
    let half = max(jarvig_cascade_half(shadow, index), 0.25);
    let span = half * 2.0 + 1.0 - 0.05;
    let bias = jarvig_shadow_bias(shadow, n_dot_l, 1.0, span);
    let clip = jarvig_cascade_clip(shadow, index, world_position);
    if (clip.w <= 0.0) {{ return 1.0; }}
    let ndc = clip.xyz / clip.w;
    if (ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 || ndc.z < 0.0 || ndc.z > 1.0) {{
        return 1.0;
    }}
    let bounds = jarvig_quadrant(index);
    let uv = bounds.xy + bounds.zw * vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    var radius = max_radius;
    if (shadow.extra.z > 0.5) {{
        let blocker = jarvig_blocker_average(directional_shadow, uv, ndc.z, bias, bounds, max_radius);
        radius = jarvig_pcss_radius(shadow, ndc.z, blocker, half, span, max_radius);
    }}
    return jarvig_shadow_projective(directional_shadow, uv, ndc.z, bias, radius, bounds);
}}
fn jarvig_spot_visibility(shadow: GpuShadow, world_position: vec3<f32>, n_dot_l: f32, distance_m: f32) -> f32 {{
    let bias = jarvig_shadow_bias(shadow, n_dot_l, distance_m, max(shadow.far_m - shadow.near_m, 0.001));
    let clip = shadow.clip * vec4<f32>(world_position, 1.0);
    if (clip.w <= 0.0) {{ return 1.0; }}
    let ndc = clip.xyz / clip.w;
    if (ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 || ndc.z < 0.0 || ndc.z > 1.0) {{
        return 1.0;
    }}
    let uv = shadow.atlas_origin + shadow.atlas_scale * vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    let bounds = vec4<f32>(shadow.atlas_origin, shadow.atlas_scale);
    let max_radius = clamp(shadow.pad1, 1.0, 4.0);
    var radius = max_radius;
    if (shadow.extra.z > 0.5) {{
        let blocker = jarvig_blocker_average(spot_shadow, uv, ndc.z, bias, bounds, max_radius);
        let average = blocker.x / max(blocker.y, 1.0);
        let receiver_d = jarvig_shadow_distance(ndc.z, shadow.near_m, shadow.far_m);
        let blocker_d = jarvig_shadow_distance(average, shadow.near_m, shadow.far_m);
        let penumbra = max(receiver_d - blocker_d, 0.0) / max(blocker_d, 0.05) * max(shadow.extra.w, 0.0);
        let texel_m = max(receiver_d, 0.05) * 0.002;
        radius = jarvig_penumbra_radius(blocker, penumbra, texel_m, max_radius);
    }}
    return jarvig_shadow_projective(spot_shadow, uv, ndc.z, bias, radius, bounds);
}}
fn jarvig_shadow_distance(ndc: f32, near_m: f32, far_m: f32) -> f32 {{
    let span = max(far_m - near_m, 0.001);
    return near_m * far_m / max(ndc * span + near_m, 1e-4);
}}
fn jarvig_point_filter(dir: vec3<f32>, receiver: f32, bias: f32, radius: f32) -> f32 {{
    let face = max(f32(textureDimensions(point_shadow).x), 1.0);
    let texel = 2.0 / face;
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(dir.y) > 0.95);
    let tangent = normalize(cross(up, dir));
    let bitangent = cross(dir, tangent);
    let reach = clamp(radius, 0.5, 4.0);
    let spin = (dir.x + dir.y) * 1.5707963;
    var lit = 0.0;
    var weight = 0.0;
    for (var i: i32 = 0; i < 16; i = i + 1) {{
        let disk = jarvig_disk_texel(i, reach, spin);
        let w = jarvig_disk_weight(disk, reach);
        let offset = (tangent * disk.x + bitangent * disk.y) * texel;
        let stored = jarvig_shadow_depth(textureSampleLevel(point_shadow, shadow_sampler, normalize(dir + offset), 0.0));
        if (!(receiver + bias < stored)) {{ lit = lit + w; }}
        weight = weight + w;
    }}
    return lit / max(weight, 1e-4);
}}
fn jarvig_screen_contact(eye: vec3<f32>, toward_light: vec3<f32>, n_dot_l: f32) -> f32 {{
    let max_dist = bitcast<f32>(light_header.pad1);
    if (max_dist <= 0.001 || n_dot_l <= 0.05) {{ return 1.0; }}
    // A shallow ray skims the floor and the hard gap test paints bands. Fade that case out.
    let grazing_fade = smoothstep(0.15, 0.40, n_dot_l);
    let steps = 6;
    let step = max_dist / f32(steps);
    let jitter = fract(dot(eye, vec3<f32>(17.13, 47.77, 13.31)));
    var darkened = 0.0;
    for (var i = 0; i < steps; i = i + 1) {{
        let dist = step * (f32(i) + jitter * 0.65);
        let pos = eye + toward_light * dist;
        let clip = render.projection * vec4<f32>(pos, 1.0);
        if (clip.w <= 0.0) {{ continue; }}
        let ndc = clip.xy / clip.w;
        let uv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
        if (uv.x <= 0.0 || uv.x >= 1.0 || uv.y <= 0.0 || uv.y >= 1.0) {{ continue; }}
        let stored = textureSampleLevel(contact_depth, shadow_sampler, uv, 0.0).r;
        if (stored <= 0.0) {{ continue; }}
        let expected = max(-pos.z, 0.0);
        let gap = expected - stored;
        let near = smoothstep(0.008, 0.02, gap);
        let far = 1.0 - smoothstep(0.05, 0.09, gap);
        darkened = max(darkened, near * far * (1.0 - dist / max_dist));
    }}
    return 1.0 - darkened * grazing_fade;
}}
fn jarvig_cascade_tint(world_position: vec3<f32>, eye: vec3<f32>) -> vec3<f32> {{
    let count = min(light_header.count, arrayLength(&shadows));
    for (var i = 0u; i < count; i = i + 1u) {{
        if (shadows[i].kind == 1.0 && shadows[i].enabled > 0.5) {{
            let eye_z = max(-eye.z, 0.0);
            var index = 0;
            if (eye_z > shadows[i].splits.x) {{ index = 1; }}
            if (eye_z > shadows[i].splits.y) {{ index = 2; }}
            if (eye_z > shadows[i].splits.z) {{ index = 3; }}
            let colors = array<vec3<f32>, 4>(
                vec3<f32>(1.0, 0.35, 0.30),
                vec3<f32>(0.35, 0.85, 0.40),
                vec3<f32>(0.35, 0.55, 1.0),
                vec3<f32>(1.0, 0.85, 0.30)
            );
            let _unused = world_position.x;
            return colors[index];
        }}
    }}
    return vec3<f32>(0.5);
}}
/// The only material seam for direct-light visibility. Cascade selection stays in this body.
fn jarvig_direct_visibility(index: u32, world_position: vec3<f32>, eye: vec3<f32>, light: GpuLight, n: vec3<f32>, l: vec3<f32>) -> f32 {{
    if (!jarvig_shadows_on()) {{ return 1.0; }}
    if (index >= arrayLength(&shadows)) {{ return 1.0; }}
    let shadow = shadows[index];
    let n_dot_l = dot(n, l);
    let contact = jarvig_screen_contact(eye, l, n_dot_l);
    if (shadow.enabled < 0.5) {{ return contact; }}
    let view_rotation = mat3x3<f32>(render.view[0].xyz, render.view[1].xyz, render.view[2].xyz);
    let world_n = normalize(transpose(view_rotation) * n);
    let grazing = clamp(1.0 - max(n_dot_l, 0.0), 0.0, 1.0);
    let sample_position = world_position + world_n * shadow.pad2 * grazing;
    if (shadow.kind == 1.0) {{
        return jarvig_directional_visibility(shadow, sample_position, max(-eye.z, 0.0), n_dot_l) * contact;
    }}
    if (shadow.kind == 2.0) {{
        return jarvig_spot_visibility(shadow, sample_position, n_dot_l, length(light.position - eye)) * contact;
    }}
    if (shadow.kind == 3.0) {{
        let light_world = transpose(view_rotation) * light.position;
        let offset = sample_position - light_world;
        let distance = length(offset);
        if (distance <= shadow.near_m || distance >= shadow.far_m) {{ return contact; }}
        let receiver = jarvig_shadow_ndc_perspective(distance, shadow.near_m, shadow.far_m);
        let bias = jarvig_shadow_bias(shadow, n_dot_l, distance, max(shadow.far_m - shadow.near_m, 0.001));
        let dir = offset / max(distance, 1e-8);
        return jarvig_point_filter(dir, receiver, bias, max(shadow.pad1, 1.0)) * contact;
    }}
    return contact;
}}
fn jarvig_environment_diffuse(surface: MaterialSurface, world_normal: vec3<f32>) -> vec3<f32> {{
    if (!jarvig_term_on(2u)) {{ return vec3<f32>(0.0); }}
    if (environment_light.lower.w < 0.5) {{ return vec3<f32>(0.0); }}
    let n = normalize(world_normal);
    let metal = clamp(surface.metallic, 0.0, 1.0);
    // AO scales environment diffuse only. jarvig_direct does not read it.
    let ao = clamp(surface.ambient_occlusion, 0.0, 1.0);
    return surface.base_color.rgb * (1.0 - metal) * ao * environment_light.upper.w * jarvig_hemisphere(n);
}}
fn jarvig_env_brdf(roughness: f32, n_dot_v: f32) -> vec2<f32> {{
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * n_dot_v)) * r.x + r.y;
    return vec2<f32>(-1.04, 1.04) * a004 + r.zw;
}}
fn jarvig_probe_weight(world_position: vec3<f32>) -> f32 {{
    if (reflection_probe.params.y < 0.5 || reflection_probe.params.x <= 0.0 || reflection_probe.center_radius.w <= 0.0) {{
        return 0.0;
    }}
    let distance = length(world_position - reflection_probe.center_radius.xyz);
    let t = clamp(1.0 - distance / reflection_probe.center_radius.w, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}}
fn jarvig_probe_lod(roughness: f32) -> f32 {{
    let min_r = JARVIG_MIN_ROUGHNESS;
    let t = clamp((roughness - min_r) / max(1.0 - min_r, 1e-4), 0.0, 1.0);
    return t * max(reflection_probe.params.z, 0.0);
}}
fn jarvig_reflection_lod(reflection: vec3<f32>, roughness: f32) -> f32 {{
    // Footprint lod only when one pixel covers more than one cube texel.
    // A close 0.045 mirror stays on mip 0. The rise is capped at two mips.
    let rough_lod = jarvig_probe_lod(roughness);
    let footprint = max(length(dpdx(reflection)), length(dpdy(reflection)));
    let face = max(f32(textureDimensions(reflection_cube).x), 1.0);
    let covered = footprint / (1.5707963 / face);
    let extra = select(0.0, log2(max(covered, 1.0)), covered > 1.0);
    return min(max(rough_lod, extra), rough_lod + 2.0);
}}
fn jarvig_probe_sample(dir: vec3<f32>, lod_in: f32) -> vec3<f32> {{
    let max_lod = max(reflection_probe.params.z, 0.0);
    let level = clamp(lod_in, 0.0, max_lod);
    let base = floor(level);
    let next = min(base + 1.0, max_lod);
    let a = textureSampleLevel(reflection_cube, reflection_sampler, dir, base).rgb;
    let t = level - base;
    if (t < 0.001 || next <= base) {{ return a; }}
    let b = textureSampleLevel(reflection_cube, reflection_sampler, dir, next).rgb;
    return mix(a, b, t);
}}
fn jarvig_environment_specular(surface: MaterialSurface, world_normal: vec3<f32>, world_view: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {{
    let n = normalize(world_normal);
    let v = normalize(world_view);
    let n_dot_v = dot(n, v);
    let nov = max(n_dot_v, 1e-4);
    let roughness = clamp(surface.perceptual_roughness, JARVIG_MIN_ROUGHNESS, 1.0);
    let alpha = roughness * roughness;
    let reflection = normalize(2.0 * nov * n - v);
    let lod = jarvig_reflection_lod(reflection, roughness);
    if (n_dot_v <= 0.0) {{ return vec3<f32>(0.0); }}
    let allow_global = jarvig_term_on(4u);
    let allow_probe = jarvig_term_on(8u);
    if (!allow_global && !allow_probe) {{ return vec3<f32>(0.0); }}
    var prefiltered = vec3<f32>(0.0);
    if (allow_global && environment_light.lower.w >= 0.5) {{
        prefiltered = mix(jarvig_hemisphere(reflection), jarvig_hemisphere(n), alpha) * environment_light.upper.w;
    }}
    var weight = 0.0;
    if (allow_probe) {{ weight = jarvig_probe_weight(world_position); }}
    if (weight > 0.0) {{
        let local = jarvig_probe_sample(reflection, lod) * reflection_probe.params.x;
        prefiltered = mix(prefiltered, local, weight);
    }}
    if (environment_light.lower.w < 0.5 && weight <= 0.0) {{ return vec3<f32>(0.0); }}
    let f0 = mix(vec3<f32>(JARVIG_DIELECTRIC_F0), surface.base_color.rgb, clamp(surface.metallic, 0.0, 1.0));
    let ab = jarvig_env_brdf(roughness, nov);
    return prefiltered * (f0 * ab.x + vec3<f32>(ab.y));
}}
fn jarvig_indirect_diffuse(surface: MaterialSurface, world_normal: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {{
    if (!jarvig_term_on(64u)) {{ return vec3<f32>(0.0); }}
    let weight = jarvig_probe_weight(world_position);
    if (weight <= 0.0) {{ return vec3<f32>(0.0); }}
    let n = normalize(world_normal);
    let metal = clamp(surface.metallic, 0.0, 1.0);
    let ao = clamp(surface.ambient_occlusion, 0.0, 1.0);
    let irradiance = textureSampleLevel(irradiance_cube, reflection_sampler, n, 0.0).rgb;
    return surface.base_color.rgb * (1.0 - metal) * ao * irradiance * weight * reflection_probe.params.x;
}}
@fragment
fn fs(in: VertexOut{fragment_params}) -> @location(0) vec4<f32> {{
    var surface = evaluate_material(in);
    let view = jarvig_view_direction(in.render_position);
{facing}    let shaded = jarvig_shade_normal(geometric, tangent, surface.normal_ts);
    surface.perceptual_roughness = jarvig_specular_roughness(shaded, surface.perceptual_roughness);
    let view_rotation = mat3x3<f32>(render.view[0].xyz, render.view[1].xyz, render.view[2].xyz);
    let world_position = transpose(view_rotation) * in.render_position;
    var radiance = vec3<f32>(0.0);
    if (jarvig_term_on(16u)) {{ radiance = surface.emissive; }}
    if (jarvig_term_on(1u)) {{
        let count = min(light_header.count, arrayLength(&lights));
        for (var i = 0u; i < count; i = i + 1u) {{
            let incident = jarvig_incident(lights[i], in.render_position);
            let visibility = jarvig_direct_visibility(i, world_position, in.render_position, lights[i], shaded, incident.l);
            radiance = radiance + jarvig_direct(surface, shaded, view, incident.l) * lights[i].color * incident.illuminance * visibility;
        }}
    }}
    let world_normal = normalize(transpose(view_rotation) * shaded);
    let world_view = normalize(transpose(view_rotation) * view);
    let sky = jarvig_environment_diffuse(surface, world_normal);
    let bounce = jarvig_indirect_diffuse(surface, world_normal, world_position);
    if (jarvig_term_on(2u) && jarvig_term_on(64u)) {{
        let cover = jarvig_probe_weight(world_position);
        radiance = radiance + sky * (1.0 - cover) + bounce;
    }} else {{
        radiance = radiance + sky + bounce;
    }}
    radiance = radiance + jarvig_environment_specular(surface, world_normal, world_view, world_position);
    if (light_header.pad2 != 0u) {{
        radiance = mix(radiance, jarvig_cascade_tint(world_position, in.render_position), 0.55);
    }}
    // View-only channel isolation. Bits 20..22 of the light-header mask. Not a level value.
    let viz = (light_header.pad0 >> 20u) & 7u;
    if (viz == 1u) {{ return vec4<f32>(surface.base_color.rgb, 1.0); }}
    if (viz == 2u) {{ return vec4<f32>(world_normal * 0.5 + vec3<f32>(0.5), 1.0); }}
    if (viz == 3u) {{ return vec4<f32>(vec3<f32>(surface.perceptual_roughness), 1.0); }}
    if (viz == 4u) {{ return vec4<f32>(vec3<f32>(surface.ambient_occlusion), 1.0); }}
    if (viz == 5u) {{ return vec4<f32>(vec3<f32>(clamp(surface.metallic, 0.0, 1.0)), 1.0); }}
    return vec4<f32>(radiance, 1.0);
}}
"#,
        pi = std::f32::consts::PI,
        rough = crate::MIN_PERCEPTUAL_ROUGHNESS,
        f0 = crate::DIELECTRIC_F0,
        distance = crate::MIN_LIGHT_DISTANCE_M,
        body = body,
        fragment_params = fragment_params,
        facing = facing,
    )
}

fn swizzle_expr(source: u32, channels: &[u8; 4], width: u8) -> String {
    let name = ['x', 'y', 'z', 'w'];
    let parts: Vec<String> = (0..width).map(|index| format!("v{source}.{}", name[channels[index as usize] as usize])).collect();
    if width == 1 {
        parts[0].clone()
    } else {
        format!("vec{width}<f32>({})", parts.join(", "))
    }
}

fn wgsl_ident(name: &str) -> Result<String, MaterialError> {
    let mut ident = String::new();
    for (index, ch) in name.chars().enumerate() {
        let ok = ch.is_ascii_alphanumeric() || ch == '_';
        if !ok || (index == 0 && ch.is_ascii_digit()) {
            return Err(MaterialError::BadParameterName(name.into()));
        }
        ident.push(ch.to_ascii_lowercase());
    }
    if ident.is_empty() {
        return Err(MaterialError::BadParameterName(name.into()));
    }
    Ok(ident)
}

fn fnv(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
