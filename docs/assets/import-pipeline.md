# Import pipeline

```text
External source
  C:\tmp\townshop.glb
        |
        v
Project source
  Content/Meshes/townshop.glb
        |
        v
Importer
  persistent AssetId, catalog metadata, JARVIG derived mesh
        |
        v
Mesh Renderer
  scheme jarvig.asset, AssetId
        |
        v
Level, Play, and JARVIGGame.exe
```

JRV-0022 imports one static mesh. The external file is not modified and is not the identity. The project copy lives under `Content/Meshes`. The catalog stores that relative path, the external path, the importer version, a fingerprint, triangle and submesh counts, and local bounds. `Intermediate/Meshes/<AssetId>.jarvigmesh` is the canonical mesh: positions, vertex color, UV0, normals, tangents, indices, material-slot ranges, and bounds. `Intermediate/Meshes/<AssetId>.jarvigmeshlets` is the optional JRV-0028 cluster sidecar built from that mesh, not from the glTF. Node transforms are baked into the vertices. Nodes do not become actors. The `.jarviglevel` stores the asset id, not the path and not the vertices. The renderer draws an extracted snapshot and does not parse glTF.

The actor starts on the glTF material: base color, metallic-roughness packed into ORM (R is AO 1, G is roughness, B is metallic), and the normal map with green flipped to the engine's DirectX convention. `doubleSided` and `alphaMode` are read with the material. Townshop's only material is `doubleSided: true` and `alphaMode: OPAQUE`. The shaded draw still uses the shared standard master, which is two-sided for every standard surface. Alpha mode is stored and does not change the opaque shader. Images are JPEG, capped at a 2048 long side. A staged set such as Tiles101 is only used when someone assigns it. Meshes over 250,000 triangles receive shadows and are not redrawn into shadow maps. Meshlets, virtual geometry, and procedural microgeometry stay unstarted. A file that fails validation adds no catalog entry and no project copy.

The production fixture is `C:\tmp\townshop.glb`, about 6.1 million triangles and one material slot. A small generated mesh still proves two material slots.

Import rules that already bind the design:

- Show auto-detected conventions and allow an override.
- Preserve color space, channel meaning, physical scale, and texel density.
- Record tool version and settings so the derived key changes when they change.
- Importers validate sizes and types and run in workers or sandboxed processes where practical (security section of the founding doc).
- Do not execute project plugins or imported scripts as part of opening a file.

The glTF importer must not become a hidden second scene format. One file becomes one mesh asset. Placement is a separate command that creates one actor.
