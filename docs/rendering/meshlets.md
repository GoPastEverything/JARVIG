# Meshlets

Meshlets are an offline cluster decomposition of a derived JARVIG mesh. The cooker reads that mesh. It does not read glTF, and it does not rebuild clusters on launch or per frame.

JRV-0028 is accepted. Human visual confirmation on 2026-09-25: with meshlet colors off, drawing from meshlets matches the ordinary imported shop. It is not virtual geometry and it is not Nanite. The render flag `virtualGeometry` stays off. Occlusion, GPU-driven draws, page streaming, and Einstein detail are later tickets. They are not part of this meshlet pass.

The ordinary indexed mesh remains available. `View > Draw From Meshlets` draws the same vertex buffer and the same material through the meshlet index buffer. ADR-0061 leaves that draw on for the selected imported mesh. `View > Show Meshlet Colors` paints one flat color per cluster and stays off until checked. With an imported mesh selected, the status bar shows the meshlet count, source triangles, average and min/max triangles, average and min/max vertices, derived size, build time, and this process's load time. The output log prints the same line when the project meshes load. The TypeScript flag `virtualGeometry` stays off.

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

Meshlets are not a second mesh and not a second scene. Actors still reference the AssetId. The clustered draw uses the same vertex buffer as the ordinary draw, so positions, normals, tangents, UVs, and vertex colors stay the ones in the derived mesh. The ordinary index buffer remains the shaded path for a mesh that is not drawn from meshlets.

`meshlet_coverage` counts canonical triangles, leaf triangles, leaf meshlets, missing triangles, and duplicate triangles. The leaf set passes when missing and duplicate are both zero. `View > Pipeline > Leaf Truth` shows that set: one stable color per leaf, no parent substitution, and no ordinary index draw hiding a hole. `Frustum Only` keeps that leaf draw and drops clusters outside the camera. The hierarchy pipelines draw the parent cut. A visualization recolors that cut and does not change which triangles were submitted, so a hole there is the cut rather than a detail patch. It does not bind a second vertex buffer. When Draw From Meshlets is on for a mesh, its shaded triangles come from the visible leaf cut or from a parent that covers its children. The status line reports those counts as `legacy`, `meshlet`, and `parent`.

This milestone does not include occlusion, GPU-driven visibility, cluster LOD, virtual geometry, page streaming, or procedural microgeometry. `virtualGeometry` stays off. Frustum and occlusion of these bounds are recorded in `docs/rendering/visibility.md`.

## Parametric solids

A Block does not have an asset id, so it does not get a `.jarvigmeshlets` file. When its surface is generated, at create, at a size change, and at load, the same `build_meshlets` runs on that surface. The set lives with the world and is released with it. The level stores `ParametricBlock`. A plain solid stores the size. A solid with inset, bevel, or a history log stores those parameters too, and still stores no triangles. ADR-0064 and ADR-0066. The builder version, the 128 caps, and the 8-to-48 grid are unchanged. A 2 m cube measures 12 meshlets, one triangle each, exact coverage. The two triangles of a face fall in different cells of the minimum 8-cell grid. That grid was not retuned. This generation is not a per-frame rebuild and it does not scan sidecars at launch. An exact solid is classified by Einstein and does not receive relief. ADR-0065.

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
