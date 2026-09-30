# ADR-0027 — Texture color space is metadata, not a shader guess

Status: Accepted
Date: 2026-09-22

## Context

Base color is stored sRGB so authors see the color they painted. Lighting math needs linear light. Roughness, metallic, ambient occlusion, normals, height, and masks are data. If those take the same sRGB decode as base color, every later PBR material looks wrong in a way that is hard to attribute.

## Decision

Color space lives on the logical texture. The shader does not inspect the image and does not apply a second conversion.

```text
BASE COLOR
sRGB-encoded bytes
    -> GPU texture format Rgba8UnormSrgb
    -> hardware sample conversion
    -> linear RGB
    -> shading

DATA
roughness, metallic, AO, normal, height, masks
    -> GPU texture format Rgba8Unorm
    -> sample stays linear
    -> no sRGB decode
```

`ColorSpace::Srgb` means the bytes are sRGB and the sampled result is linear because the GPU format says so. `ColorSpace::Linear` means the bytes are data and sampling must not decode them. JARVIG maps that choice onto the RHI format. wgpu does not appear in the logical texture.

`TextureId(0)` means the parameter is unbound. That is not an error, and it does not sample the magenta texture. The renderer substitutes the semantic default: white for base color, black for emissive, a flat normal, and the linear defaults in [../materials/pbr.md](../materials/pbr.md). A nonzero id that is not in the library is invalid. Color semantics then use the magenta error texture. Linear data fails the frame. Neither path samples uninitialized memory. JRV-0053 made that split. It does not change the color-space rule above.

## Alternatives Considered

- Decode sRGB in the shader for every sample. Rejected. Data maps would need a special case in every graph, and the GPU already decodes sRGB formats.
- Treat every RGBA8 texture as linear until PBR. Rejected. The first color texture would teach the wrong habit, and JRV-0053 would have to undo it.

## Consequences

JRV-0053 metallic/roughness materials bind base color as sRGB and the data maps as linear. They do not add a shader branch to rediscover that. Compressed formats, file import, and virtual textures can arrive later without changing this rule. Mip count is metadata now. Generating those mips is not this decision.

## Supersedes

Nothing.

## Superseded By

Nothing.
