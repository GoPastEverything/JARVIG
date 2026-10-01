# Meshlets

Meshlets are an offline cluster decomposition of a derived JARVIG mesh. The cooker reads that mesh. It does not read glTF, and it does not rebuild clusters on launch or per frame.

JRV-0028 is accepted. Human visual confirmation on 2026-09-25: with meshlet colors off, drawing from meshlets matches the ordinary imported shop. It is not virtual geometry and it is not Nanite. The render flag `virtualGeometry` stays off. Occlusion, GPU-driven draws, page streaming, and Einstein detail are later tickets. They are not part of this meshlet pass.

The ordinary indexed mesh remains the shaded reference. `View > Draw From Meshlets` draws the same vertex buffer and the same material through the meshlet index buffer. `View > Show Meshlet Colors` paints one flat color per cluster over that surface. Both are off unless the menu is checked. With an imported mesh selected, the status bar shows the meshlet count, source triangles, average and min/max triangles, average and min/max vertices, derived size, build time, and this process's load time. The output log prints the same line when the project meshes load.

## What one cluster stores

Each cluster stays inside one submesh and one spatial cell. The record is:

- a vertex range into the packed parent-vertex ids
- a local index range (`u8`, because the vertex limit is below 256)
- the source submesh and material slot
- an axis-aligned box and a bounding sphere
- a normal cone: the average face normal, and the smallest dot of a face normal with that axis

The pack limits for this measurement are 128 vertices and 128 triangles. The spatial grid is `clamp(cbrt(triangles / 128), 8, 48)` cells on each axis of the mesh bounds. A cluster does not cross a cell. These are measured parameters, not a permanent architecture constant. On townshop the measured average is 99.22 triangles and 108.80 vertices. Triangles inside one cell stay in source order, which is what makes that reuse stable.

## File

`Intermediate/Meshes/<AssetId>.jarvigmeshlets`

Magic `JARVMLET`, format version 2. The header stores the builder version, the importer version, the source vertex count, the source fingerprint, the triangle count, and the pack limits. A load uses the sidecar only when all of those match. A version mismatch, a fingerprint change, a corrupt file, or a limit change rebuilds the clusters and writes the file again. The sidecar is not appended to `.jarvigmesh`, because that file already carries the pick BVH. Selection, the inspector, and the gizmo do not read it. Turning a debug view off keeps the uploaded buffers, so the next toggle of the same actor does not expand the clusters again.

Meshlets are not a second mesh and not a second scene. Actors still reference the AssetId. The clustered draw uses the same vertex buffer as the ordinary draw, so positions, normals, tangents, UVs, and vertex colors stay the ones in the derived mesh. The ordinary index buffer remains the shaded fallback.

This milestone does not include occlusion, GPU-driven visibility, cluster LOD, virtual geometry, page streaming, or procedural microgeometry. `virtualGeometry` stays off. Frustum and occlusion of these bounds are recorded in `docs/rendering/visibility.md`.

## townshop, 2026-09-25

Asset `affe7d8d-e3e1-448a-a008-76c6e6266867`. The release check was:

```text
cargo test --offline --release --manifest-path native/Cargo.toml -p jarvig_core --lib townshop_meshlets -- --ignored --nocapture
```

| Measure | Value |
| --- | --- |
| Source triangles | 6,122,214 |
| Source vertices | 3,419,738 |
| Meshlets | 61,702 |
| Average triangles | 99.22 |
| Average vertices | 108.80 |
| Min / max triangles | 1 / 128 |
| Min / max vertices | 3 / 128 |
| Derived bytes | 50,155,678 |
| Build / sidecar write | 1967.8 ms / 216.6 ms |
| Sidecar decode on the next load | 20.5 ms in the release test, 701 ms in the debug editor, file not rewritten |
| Full asset reload around that decode | 2212.0 ms, including the canonical mesh |
| Cluster-index expand | 50.3 ms |
| Grid / builder / importer | 37 / 1 / 1 |
| Source fingerprint | `f55372e5a6527a5a` |
| Mean / max cluster radius | 0.0256 m / 0.0651 m |
| Mesh bounding radius | 1.4247 m |
| Triangle cover | exact, winding included |
| Debug upload, debug editor, Intel UHD | 5942.7 ms once, then 0.00–0.03 ms |

The minimum of one triangle is a sparse cell, not a hole. The clusters are much smaller than the mesh. The shaded fallback is still the original index buffer.

The intended later baseline, tracked as [RFC-0001](../rfc/RFC-0001-virtual-geometry-baseline.md), adds a hierarchical geometric-error metric, compressed bounded pages, GPU LOD selection, a resident parent when a child page is missing, and streaming feedback. Meshlets without those pieces must not be described as virtualized geometry.
