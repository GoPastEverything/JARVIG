# Renderer

The renderer sits behind the JARVIG RHI. It does not call DOM, browser, or wgpu types. The first GPU backend is wgpu, and `native/jarvig_rhi_wgpu` is the only crate where those types may appear. D3D12, Vulkan, and Metal are what that backend targets, not extra public backends, until a platform forces a direct one. See [rhi.md](rhi.md) and [ADR-0019](../adr/ADR-0019-rhi-hides-backend-types.md).

The presented path is two depth-tested meshes (JRV-0047) with engine-owned GPU lifetimes (JRV-0048), drawn by two views of one world (JRV-0049). The world is extracted once into a snapshot (JRV-0050). The renderer does not query that world while it draws. Surface color comes from a material graph compiled to IR and then to one shared pipeline (JRV-0051, JRV-0052). JRV-0053 draws that through one `StandardMetalRough` master. JRV-0054 lights it from world lights extracted once and converted per view. See [lighting.md](lighting.md), [../materials/pbr.md](../materials/pbr.md), and [ADR-0029](../adr/ADR-0029-direct-light-conventions.md). A mesh does not own that material or those textures. Color space is texture metadata. ADR-0027. A mesh is local geometry plus submeshes and local bounds. It is not an asset, an entity, or a GPU buffer. The logical mesh has no wgpu type and no RHI type. Vertices are object-local meters. Each view turns that into its own camera-relative float32. Perspective depth is reversed-Z with no finite far plane. A view is not a world and not a swapchain. See [views.md](views.md). The snapshot is not a second world. See [scene-extraction.md](scene-extraction.md). The ladder through the standard surface, the first direct lights, and graphics-adapter policy is recorded in the backlog. That numbered renderer ladder stops at JRV-0055, which is accepted. The product milestone is the native editor, JRV-0057. The first shell is JRV-0058. Adapter policy is [../platform/graphics-device-selection.md](../platform/graphics-device-selection.md). Adapter policy is not part of the editor tickets. GPU-driven culling and virtual geometry stay behind it. The residency model they will share is [resources.md](resources.md).

WebGL2 can remain a compatibility backend later. It must not force the CPU to submit tens of thousands of individual draws. ADR-0006 still holds as "do not design around one draw per object." Its rejection of a native graphics API is superseded.

```text
World extraction
  -> GPU scene database
  -> frustum / layer filtering
  -> hierarchical-Z occlusion
  -> instance and cluster selection (compute)
  -> streaming feedback
  -> indirect command generation
  -> depth / visibility / GBuffer
  -> lighting / shadows / GI
  -> transparency / VFX
  -> post
  -> UI / debug
```

| Subsystem | Purpose | Phase 0 |
| --- | --- | --- |
| RenderDevice | Buffers, textures, pipelines, queues, sync | Interface plus a null device |
| RenderGraph | Pass dependencies and transient lifetimes | Not started (JRV-0024) |
| GPUScene | Compact GPU-readable transforms, materials, instances | Not started (JRV-0025) |
| Visibility | Frustum and occlusion of meshlet bounds | Frustum accepted (JRV-0026). JRV-0027 is a held CPU occlusion reference, not a GPU depth pyramid |
| Material graph | Authored graph compiled to shader variants | Unlit and StandardMetalRough. Lights are world state, not graph nodes. See [lighting.md](lighting.md). |
| Virtual resources | Geometry and texture page residency | Model only. [resources.md](resources.md). Not streamed |

Package `@jarvig/render` registers module id `render`, requires `world`, and counts prepare/execute hooks. The default device is `{ backend: 'null', label: 'null-device' }`. Server profiles never call those hooks, and the server package does not import this module.

## Promotion sequence

Do not skip ahead.

1. Correct PBR baseline and render graph.
2. GPU scene and indirect rendering.
3. Hierarchical-Z and compute visibility.
4. Meshlet / cluster hierarchy.
5. Virtual geometry paging.
6. Virtual textures.
7. Advanced GI and reflections.
8. Experimental representation virtualization.

Each experimental step needs benchmarks: frame time, GPU time, CPU time, memory, VRAM, streaming bandwidth, storage size, shader cost, visual error, pop-in, and generation cost. Never fabricate them.

## Research flags

`virtualGeometry`, `proceduralMicrogeometry`, `representationVirtualization`, and `advancedGi` default off. Pages: [webgpu.md](webgpu.md), [gpu-scene.md](gpu-scene.md), [visibility.md](visibility.md), [meshlets.md](meshlets.md), [virtual-geometry.md](virtual-geometry.md), [microgeometry.md](microgeometry.md), [representation-virtualization.md](representation-virtualization.md), [lighting.md](lighting.md).
