# ADR-0028 — Standard metallic/roughness surface

Status: Accepted
Date: 2026-09-22

## Context

ADR-0007 already says the baseline shading model is metallic/roughness PBR, and that a material's meaning is the IR, not a shader string. JRV-0051 implemented only Unlit. JRV-0052 added color and data textures. The next step is the surface those textures feed.

If the material graph also contained a directional light, every later light type would force the authored graph to be rebuilt. If the shader decoded sRGB again, ADR-0027 would be undone. If normals were transformed like positions, non-uniform scale and large-world translation would bend the lighting.

## Decision

`ShadingModel::StandardMetalRough` sits beside `Unlit`. Unlit stays. The graph writes a `MaterialSurface`. A later shading stage consumes that surface plus light inputs. JRV-0054 owns the lights. This decision does not.

```text
Material graph
    -> Material IR
    -> CompiledMaterial
    -> evaluate_material(...) -> MaterialSurface
    -> standard shading stage
    -> light inputs          (JRV-0054, not this decision)
    -> linear color
```

The surface fields are base color, metallic, perceptual roughness, tangent-space normal, ambient occlusion, and emissive. Opaque opacity stays 1. Defaults are white, metallic 0, roughness 0.5, normal `(0, 0, 1)`, occlusion 1, and emissive black.

Working space is linear RGB with sRGB / Rec.709 primaries. Base color and emissive may be sRGB textures and are sampled to linear by the GPU format. Metallic, roughness, occlusion, normal, and height are linear data. The shader does not decode sRGB. Height is not a BRDF input.

The direct BRDF, when a light exists, is Cook-Torrance: GGX / Trowbridge-Reitz, separable Smith GGX (Walter G1), Schlick Fresnel, and Lambert diffuse divided by pi. Dielectric F0 is 0.04. `F0 = mix(0.04, baseColor, metallic)`. Diffuse is scaled by `1 - metallic`. Perceptual roughness is clamped to at least 0.045 in one place, then `alpha = roughness²`. Ambient occlusion does not scale direct punctual light. Emissive is added and is not clamped to the display range.

Normals use the inverse-transpose of the object's linear 3×3. Translation does not enter. Tangents use that linear 3×3. `tangent.w` is handedness. The normal map is +Y / OpenGL green: `normalize((rgb * 2 - 1)` with `NormalScale` on XY only). Importers convert DirectX maps before they reach the shader. Authored tangents are MikkTSpace-compatible. This decision does not include a tangent generator.

Shading positions are camera-relative float32 after the f64 origin subtract. The camera is at the render origin for that view. `view = normalize(-renderPosition)`. Each view does its own conversion.

Packed channels (ORM, RMA, MRA, or anything else) are a graph mapping. The engine does not assume one packing. The bootstrap graph chooses R = occlusion, G = roughness, B = metallic, and scales those channels by runtime factors.

The live frame may show `baseColor + emissive` until a light input exists. That preview is not this BRDF and must not be described as lit PBR.

## Alternatives Considered

- Compile a hardcoded directional light into the graph or the renderer. Rejected. The same surface has to accept directional, point, spot, image-based, and later clustered or ray-traced lights without a new graph.
- Drop Unlit. Rejected. UI, debug, and emissive-only surfaces still need it.
- Apply occlusion to direct light. Rejected. Occlusion is for indirect lighting that does not exist yet.
- Put height into the BRDF. Rejected. Height is later displacement or parallax data.
- Guess OpenGL versus DirectX normal green in every shader. Rejected. The import path flips, the shader keeps one convention.

## Consequences

`jarvig_material` holds the surface, the BRDF reference, and the IR. It still does not depend on wgpu, winit, or the RHI. The RHI does not learn metallic, roughness, or a BRDF. The server still does not compile or upload. Pipeline identity stays the compiled program plus vertex layout and targets, not the material instance.

The bootstrap scene uses one standard master and two instances. Changing a factor or a texture does not recompile. A real light is JRV-0054. Tone mapping, IBL, shadows, and clearcoat are not this decision.

## Supersedes

Nothing. ADR-0007 still says the IR is the material. ADR-0027 still says color space is texture metadata.
