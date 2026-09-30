# Textures and samplers

A texture is a logical engine resource. It is not an image file, not a GPU texture, and not a material instance. JRV-0052. ADR-0027.

```text
programmatic RGBA8 bytes
    -> Texture in TextureLibrary
         dimension, mip_count, color space, revision
    -> renderer residency
         GpuTexture
         TextureView
    -> JARVIG RHI
    -> backend texture and view
```

The bootstrap images are 8×8 checkers generated in memory. There is no PNG, JPEG, or other decoder. Rows in the logical texture are tightly packed. The backend pads a copy to its own alignment. Callers do not pad.

## Color space

| Kind | Stored bytes | GPU format | Sample result |
| --- | --- | --- | --- |
| Base color and other color | sRGB | `Rgba8UnormSrgb` | Linear, by the hardware format |
| Roughness, metallic, AO, normal, height, masks | Linear data | `Rgba8Unorm` | Linear, no decode |

The generated shader samples with `textureSample` and does not call a decode. The format is the conversion. A data texture must not use the sRGB format.

## What is separate

| Thing | Is not |
| --- | --- |
| Source bytes | The logical texture |
| Logical texture | The GPU texture |
| Texture | Its view |
| Sampler | A texture |
| Material texture parameter | A shader compile or a pipeline |
| Material instance | The owner of a texture |

`TextureDimension::Texture2D` is the only dimension implemented. Array, cube, and 3D are named as future kinds, not as code. `mip_count` is stored. The first textures use 1. A mip generator, compression, UDIM, and virtual pages are not built. Residency can still drop the GPU texture later without deleting the logical one. JRV-0048.

## Sampler

A sampler is `min`, `mag`, and `mip` filter plus address modes U, V, and W. The bootstrap sampler is linear and repeat, so UVs outside 0..1 show another copy of the checker. Identical sampler state shares one GPU sampler. Changing the sampler on an instance rebuilds that instance's bind group. It does not recompile the master.

## Material binding

Group 0 binding 0 is still the view and object transform.

Group 1 binding 0 is the material uniform. Later bindings are whatever resources that master compiled, in graph order. The standard master binds base color, the shared sampler, the packed map, the normal, and emissive. Those indices are compiler output. They are not an authoring API.

`TextureId(0)` is unbound. The renderer substitutes the semantic default from [pbr.md](pbr.md). That is not the magenta error texture. A nonzero id missing from the library is invalid. Color semantics (`BaseColor`, `Emissive`, `UnlitColor`) then sample the magenta error texture and increment `missing_texture_uses`. Linear semantics fail the frame with `UnknownTexture`. Optional and broken are different.

A mesh that lacks a semantic the master requires is rejected. The standard master requires `TexCoord0`, `Normal`, and `Tangent`. Zero UVs are not invented. Normal and tangent are not inferred from garbage.

Both views share the uploaded textures and the sampler. Two cameras do not upload the checker twice. Hiding a render instance does not destroy the texture. The server does not create GPU textures, samplers, or shaders.

## Not in this ticket

File import, block compression, streaming, virtual textures, and UDIM. Normal maps and the metallic/roughness surface are [pbr.md](pbr.md). They use this texture split. They do not replace it.
