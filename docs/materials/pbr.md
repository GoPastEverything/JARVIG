# PBR baseline

Canonical workflow: metallic/roughness, linear light, explicit color space, meaningful normals, recorded tangent space. Advanced models extend this. ADR-0007 and [ADR-0028](../adr/ADR-0028-standard-metal-rough-surface.md).

JRV-0053 is the `StandardMetalRough` surface. JRV-0054 feeds it world lights. The graph still does not contain a light. The shading stage loops the per-view light list and calls this BRDF. See [../rendering/lighting.md](../rendering/lighting.md) and [ADR-0029](../adr/ADR-0029-direct-light-conventions.md).

## Shading models now

| Model | Status |
| --- | --- |
| `Unlit` | Kept. UI-like surfaces, debug, emissive-only, editor visualization. |
| `StandardMetalRough` | Surface properties plus the direct BRDF policy below. |

Both are `Surface` / `Opaque`. Masked, translucent, additive, clearcoat, subsurface, anisotropy, sheen, transmission, and cloth are not implemented.

## Surface contract

These are material outputs. They are not WGSL variables, GPU resources, world components, or light state.

| Field | Default | Range |
| --- | --- | --- |
| Base color | `(1, 1, 1)` | Linear RGB after sampling |
| Metallic | `0` | Clamped to `[0, 1]` |
| Perceptual roughness | `0.5` | Clamped to `[0, 1]`, then the BRDF floor below |
| Tangent-space normal | `(0, 0, 1)` | After decode |
| Ambient occlusion | `1` | Clamped to `[0, 1]` |
| Emissive | `(0, 0, 0)` | Linear radiance. Not clamped to the display |

Opaque opacity stays 1.

Runtime factors on the bootstrap master: `BaseColorFactor` (Float4), `MetallicFactor`, `RoughnessFactor`, `NormalScale`, `OcclusionStrength`, and `EmissiveFactor` (Float4, rgb used). Every uniform slot is 16 bytes. Changing one uploads bytes. It does not recompile.

## Working color space

Linear RGB, sRGB / Rec.709 primaries. All BRDF math is linear. Base color and emissive textures are sRGB storage and decode once in the GPU format (ADR-0027). Metallic, roughness, occlusion, normal, and height are linear `Rgba8Unorm`. The shader does not call a second decode and does not gamma-correct the lighting math.

## Output path

The shader returns linear RGB. On this machine the swapchain format is `Bgra8UnormSrgb` (the first sRGB format the surface offered). The hardware encodes that linear color on present. The shader does not encode again. This is not a tone-mapping or HDR display pipeline. A surface with no sRGB format would fall back to the first listed format, and that case is not what the self-test hit.

The null backend's test target is `Rgba8Unorm` and does not present. Do not treat that test format as the window path.

## Texture semantics

| Semantic | Storage | Sample | Unbound default |
| --- | --- | --- | --- |
| Base color, unlit color | sRGB | Linear | White |
| Emissive | sRGB | Linear | Black |
| Metallic | Linear | Linear | `0` |
| Roughness | Linear | Linear | `128/255` (closest byte to `0.5`) |
| Ambient occlusion | Linear | Linear | `1` |
| Normal | Linear | Linear | `(128, 128, 255)` → `(0, 0, 1)` |
| Height | Linear | Linear | `0`. Not a BRDF input |
| ORM, as a graph choice | Linear | Linear | White, an identity multiplier |
| Data | Linear | Linear | `0` |

`TextureId(0)` is unbound. It uses the default above. It does not use the magenta texture. A nonzero id missing from the library is invalid: color semantics sample the magenta error texture and count it; linear data fails the frame.

The shared 1×1 defaults live once on the texture library. Instances do not each own a copy.

## Packed maps

ORM, RMA, MRA, and custom layouts are graph swizzles. The engine does not assume one packing.

The bootstrap graph, and only that graph, reads one linear texture as R = occlusion, G = roughness, B = metallic, then:

- occlusion = `mix(1, R, OcclusionStrength)`
- roughness = `G * RoughnessFactor`
- metallic = `B * MetallicFactor`

The bound bootstrap texture is linear white, so the factors are the authored values. A different master can swizzle a different layout without an engine change.

## Normals and tangents

Vertex semantics for this model: `Position`, `Normal` (Float3), `Tangent` (Float4), `TexCoord0`. `Color0` remains available for Unlit. `Tangent.w` is handedness. The bootstrap triangle supplies normal `(0, 0, 1)` and tangent `(1, 0, 0, 1)`. Those are authored. There is no MikkTSpace generator. Imported assets must be converted into this basis later, in the cook, not in the shader.

The normal matrix is the inverse-transpose of the object's linear 3×3. The shader builds it from cofactors. It does not call a matrix inverse helper, and it does not use the translation column. Tangents are transformed by the linear 3×3, then:

```text
N = normalize(normalMatrix * geometricNormal)
T = normalize(linear * tangent.xyz)
T = normalize(T - N * dot(T, N))
B = cross(N, T) * tangent.w
normal = normalize(T * nx + B * ny + N * nz)
```

Normal texture, +Y / OpenGL green, linear:

```text
rgb * 2 - 1
xy *= NormalScale
normalize
```

No sRGB conversion. A DirectX / −Y source is flipped at import. The shader does not carry both conventions.

Shading space is camera-relative. The CPU subtracts the view origin in f64, then uploads float32. The GPU camera is at the render origin, so `view = normalize(-renderPosition)`. Each `RenderView` does that for itself. Normals are directions and do not carry the billion-meter translation. Reversed-Z is unchanged (ADR-0021).

## BRDF policy

CPU reference in `jarvig_material`. The generated shader uses the same constants (`MIN_PERCEPTUAL_ROUGHNESS`, `DIELECTRIC_F0`, `MIN_LIGHT_DISTANCE_M`, pi) and the same GGX, Smith, Schlick, and Lambert equations. Direct illuminance is lux for a directional light and `cd / d²` for a point or spot, times the spot cone when the kind is a spot. The result is that illuminance times the BRDF. Emissive is added once. AO is not a factor on that direct term. It scales environment diffuse only.

| Term | Choice |
| --- | --- |
| Model | Cook-Torrance microfacet |
| NDF | GGX / Trowbridge-Reitz |
| Visibility | Separable Smith GGX, Walter G1(`NdotV`) * G1(`NdotL`) |
| Fresnel | Schlick |
| Diffuse | Lambert, `base * (1 - metallic) * NdotL / π` |
| F0 | `mix(vec3(0.04), baseColor, metallic)` |
| Roughness | Perceptual. `alpha = roughness²` after the floor |
| Floor | `MIN_PERCEPTUAL_ROUGHNESS = 0.045`. One constant |

`NdotL <= 0` or `NdotV <= 0` contributes no direct light. Emissive is unchanged in that case. The BRDF is a density and may exceed 1 at a smooth peak. The tests check that the values stay finite, that a rough head-on sample stays modest, that Fresnel rises toward grazing, and that a back-facing light adds nothing.

Ambient occlusion is stored on the surface and ignored by `evaluate_direct`. `environment_diffuse` multiplies by it. The graph's occlusion strength is already in that surface value: strength 0 keeps AO at 1, strength 1 uses the sampled channel. Emissive is linear radiance added after the direct term and is not part of the environment term. It is not multiplied by `NdotL`. `EmissiveFactor` may be greater than 1.

Height stays linear data for parallax, displacement, or microgeometry. `feeds_standard_brdf` is false for height.

## What the window shows

The fragment adds emissive, then each direct light, then environment diffuse, then environment specular. There is no constant ambient and no shadow. Diffuse environment light does not reflect other triangles. Specular environment light is the analytical sky, and inside the local probe it is a captured cubemap of the opaque scene. A rough dielectric turned away from the lights keeps the hemisphere for the visible-side normal, plus a small Fresnel reflection. A metal drops the diffuse term and keeps the reflection: `F0` is its base color. The global term uses roughness² to blend the mirror ray toward the hemisphere the normal faces. The local term uses that same roughness to pick a cubemap mip. Values above 1 stay in the HDR target and are tone-mapped. The head-on directional is intentionally dim (0.35 lux). The bootstrap point and spot can still reach display white at 0 EV. That is exposure, not a second BRDF. AO does not scale the reflection.

Opaque materials cull back faces. Counter-clockwise is front. A two-sided material is an opt-in: no face is culled, and a back-facing fragment negates the geometric normal and the tangent handedness before `jarvig_shade_normal`. The bootstrap cards use that opt-in so both sides can be inspected. A closed mesh should not. See [../rendering/face-culling.md](../rendering/face-culling.md).

The bootstrap instances share one master, one compile, and one pipeline:

| Instance | Base color | Metallic | Roughness | Emissive factor |
| --- | --- | --- | --- | --- |
| Near | Red / yellow sRGB checker | `0` | `0.85` | `0` |
| Far | Blue / green sRGB checker | `1` | `0.2` | `(0.35, 0.12, 0.02)` |

The far warm bias is the emissive factor on a white emissive texture. It is not a specular highlight. Vertex color is still in the mesh and is not wired into this graph. The flat normal does not tilt the preview.

## Long-term schema

The names below are the direction. Only the rows implemented above exist in native code.

| Semantic | Rules |
| --- | --- |
| Base color / albedo | sRGB authored color. No baked lighting. |
| Normal | Linear. +Y recorded here. Green inversion is import metadata. |
| Roughness | Linear 0–1 perceptual. Gloss converts at import. |
| Metallic | Linear 0–1. |
| Ambient occlusion | Linear. Indirect only. |
| Emissive | Linear, HDR values allowed. |
| Height / displacement | Linear. Not part of this BRDF. |
| Clearcoat / anisotropy / SSS | Later shading models. |

Image-based lighting, shadows, and a tone map are not in this ticket. Golden reference spheres are not claimed.
