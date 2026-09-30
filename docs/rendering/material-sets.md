# Staging PBR sets

The folders under `assets/materials` are source sets, not level data. A `.jarviglevel` stores `scheme: jarvig.material` and a set name such as `Tiles101`. It does not store PNG bytes. `jarvig.asset` is still rejected.

Each set inspected on 2026-09-24 is an AmbientCG `_4K-PNG` folder:

| File suffix | Role | Color space |
| --- | --- | --- |
| `_Color.png` | Base color | sRGB |
| `_Roughness.png` | Roughness | linear |
| `_AmbientOcclusion.png` | Ambient occlusion | linear |
| `_Metalness.png` | Metallic, only when the file exists | linear |
| `_NormalDX.png` | Normal, DirectX, green is −V | linear |
| `_NormalGL.png` | Normal, OpenGL, green is +V | linear |
| `_Displacement.png` | Height. Recognized. Not sampled. | linear |
| `_Emission.png` | Emissive, only Facade020B | sRGB |
| `{Set}.png` | Preview. Not shaded. | |
| `.mtlx`, `.blend`, `.tres`, `.usdc` | Sidecars. Not shaded. | |

Tiles, gravel, grass, and carpet have no metalness map. Ingest packs metallic as 0. A missing roughness map packs 0.5. A missing AO map packs 1.

The shader bitangent is `cross(normal, tangent) * tangent.w`. It does not flip green. On the floor, handedness is +1, the tangent is +X, and that bitangent is −Z, opposite texture +V. A positive green channel therefore tilts toward −V. That is the DirectX convention, and it comes from the tangent frame, not from DX12 or wgpu. The smooth sphere stores handedness −1 so its bitangent is also opposite texture +V. The cube's top face uses the same frame as the floor.

`_NormalDX.png` is sampled with no green flip. `_NormalGL.png` is not sampled while the DirectX file is present. If a set had only the OpenGL file, ingest would negate green once. The Tiles101 files were checked directly: on a stride of texels, DirectX green is `255 -` OpenGL green, and red and blue match. Shading a real bump with DirectX, and the same texel after negating the OpenGL green, produces the same world normal. Leaving the OpenGL green alone does not. JRV-0088 stays open until that direction is confirmed by eye.

The Lighting Lab floor references `Tiles101` at UV scale 1, so one repeat covers the 6 m floor. Tile Plane, Tile Cube, and Tile Sphere use the same set and the same saved factors, so a flat face, an edge, and a curve can be compared. Gravel Plane references `Gravel035` through the same path. The shader does not name either set. UV scale, roughness multiplier, metallic multiplier, and normal strength are inspector uniforms. Roughness multiplier is accepted through 2 and the BRDF still clamps the product to the valid range. Normal strength 0 is the geometric normal. Strength 1 is the authored map. Neither edit recompiles a shader or rewrites a PNG.

View > Lighting Debug > Material isolates base color, world normal, roughness, AO, or metallic. That choice is view state. It is not saved and it does not change the level. AO multiplies environment diffuse and indirect diffuse only. Direct light does not read it, and environment specular does not either. Shadow maps are drawn from positions, so a normal map does not move the silhouette or the shadow-map triangles. The receiver's few millimeters of slope bias do use the shaded normal.

Ingest keeps the long side at or below 2048 and builds the mip chain. Color mips are averaged in linear light. Normal mips are renormalized. The repeating material sampler asks for anisotropy 8, capped at 16, which the Intel UHD baseline can ignore if it has to. Shadow and probe samplers stay isotropic. Displacement is still recognized and not sampled. The level stores the set name, the convention, and the instance factors. It does not store pixels.
