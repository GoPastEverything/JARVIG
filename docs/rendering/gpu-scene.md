# GPU scene

`GPUScene` is a compact table of instances, shared geometries, and meshlet bounds. The CPU fills it from a `RenderSceneSnapshot` and from meshlet records the host already loaded. The renderer uploads the bytes. It does not open a project file, parse glTF, or choose which clusters to draw.

JRV-0025 is this upload. JRV-0024, the render graph, is still unstarted, so the tables ride the existing frame instead of a new pass. JRV-0026 and JRV-0027 read these bounds from the CPU. They do not change this upload. The ordinary indexed draw is still not culled.

## Tables

One view produces three buffers.

- Instances. One row per extracted instance. The row keeps the entity id, an index into the geometry table, a visible flag, and the same camera-relative model matrix the draw already uses. A camera at a million meters does not put that million meters in the matrix.
- Geometries. One row per distinct runtime mesh. Two actors on one mesh share the row. The row is the mesh id plus the range of meshlet descriptors.
- Meshlets. Bounds, sphere, normal cone, and triangle and vertex counts. No indices and no vertices. A builtin mesh contributes a geometry row and zero descriptors.

The ordinary indexed draw is unchanged. These buffers are resident data for the next visibility tickets. They are not a second renderer.

## What you see

With townshop selected, the status line includes `GPU scene inst … geom … meshlets … upload … us`. The meshlet count is the shared descriptor table, not a per-instance copy. The first frame that remembers the shop logs `JRV-0025`. Later frames upload the instance rows again because the camera-relative matrix changes, and they keep the meshlet buffer.

View > Show Meshlet Colors and View > Draw From Meshlets are still the JRV-0028 views. They are not culling.
