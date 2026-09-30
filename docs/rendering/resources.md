# GPU resources

The logical mesh has no wgpu type and no RHI type. Residency is a renderer decision. A handle is not the backend object. ADR-0024.

The HDR scene target is the same kind of residency. It is created with the drawable size, sampled by the output pass, and retired on resize and shutdown. It is not a mesh and not a material texture.

```text
JARVIG WORLD

    Mesh
      |
      logical resource
      |
      v
Renderer residency
      |
      +---------------+
      |               |
   Resident        Evicted
      |
      v
    GpuMesh
      |
      +--------+---------+
      |        |         |
      v        v         v
  BufferId  BufferId   future pages
      |        |
      +---+----+
          |
          v
     JARVIG RHI
          |
          v
   backend resources
```

A sampled texture uses the same ladder. The logical checker lives in `TextureLibrary`. The renderer uploads it once into a GPU texture and a full-resource view. Two views share that residency. The depth target is still a renderer-owned recreatable texture, not a texture asset. See [../materials/textures.md](../materials/textures.md).

```text
texture asset          logical, no wgpu type, no RHI type
    |
    v
residency              Resident | Evicted | Requested (later)
    |
    v
GpuTexture             TextureId
    |
    v
JARVIG RHI
    |
    v
backend texture
```

Virtual geometry, when it exists, is the same choice at a finer grain. It is not a second resource system. See [virtual-geometry.md](virtual-geometry.md). Do not build it in this ticket.

```text
Mesh
├── cluster page 0001    resident
├── cluster page 0002    resident
├── cluster page 0003    evicted
├── cluster page 0004    requested
└── ...
```

`Requested` is a future state. The code today is only `Resident` and `Evicted`.

## Who owns what

| Layer | Owns | Does not own |
| --- | --- | --- |
| `jarvig_material` | Graph, IR, master, instance, compiled program | GPU handles, wgpu, the RHI |
| `jarvig_core` | Logical mesh, bounds, submesh slot index, snapshot binding ids | A material object, GPU handles, wgpu |
| `jarvig_engine` | The in-memory material library for profiles that present | Compilation on the dedicated server |
| `jarvig_renderer` | Residency: `GpuMesh`, `GpuMasterMaterial`, `GpuMaterialInstance`, depth | Backend objects, the world |
| `jarvig_rhi` | Generational handles, validation. Bind groups 0 and 1 | Material graphs |
| `jarvig_rhi_wgpu` | Backend objects and the physical free | Logical meshes and logical materials |

A compiled master becomes one `GpuMasterMaterial`: shader module, material bind-group layout, and one pipeline. Each instance becomes a `GpuMaterialInstance`: a parameter buffer and a bind group. Instances share the master. A view's destruction drops that view's transform bindings only. It does not drop the shared material. Shutdown retires bind groups before the buffers and layouts they reference, and the pipeline before its shader, under the same rules as a mesh.

The pipeline is not keyed by material instance. The identity that exists today is the master, the compiled program key, and the vertex layout, plus the renderer's one color format and reversed-Z depth state. A later cache also includes target formats, raster state, pass compatibility, and static permutations.

`BufferId`, `TextureId`, `TextureViewId`, `SamplerId`, `ShaderModuleId`, `BindGroupLayoutId`, `BindGroupId`, and `PipelineId` are an index plus a generation. Generation 0 is null. A reused slot bumps the generation, so a stale handle cannot write the new occupant. Double release is `InvalidHandle`. Adapter, queue, command, fence, query, surface, and swapchain ids stay plain `u64` until those objects join this table.

These ids are in-process. They are not `JarvigBufferId` and they are not in `jarvig_core.h`. ADR-0022.

## Lifetime

```text
Alive  -- release -->  Retired  -- GPU completion -->  Destroyed
```

Release kills the handle immediately. The backend object stays until the submission that last used it has completed. Nothing is collected by waiting "three frames." There is no garbage collector and no public `Arc`.

The token is a submission serial.

- Null: submit completes at once, because there is no GPU. A retired object that was never submitted (`last_used == 0`) is freed on the next collect.
- wgpu: one queue. `flush` is `device.poll(Maintain::Wait)`, then collect. That wait means every command buffer already submitted on the queue is done. Deletion does not overlap the GPU. This is conservative on purpose.

Callers retire a resource only when no unsubmitted encoder still names it. `flush` does not search open encoders.

Deferred, not a change to this model: wgpu `flush` waits for the whole queue. That is correct while one frame is in flight. Multiple frames, streaming, and async uploads should retire from the submission serial or a fence instead of stalling. Do not reopen JRV-0048 to build that.

Swapchain color views are frame-transient. Present retires the view. `flush` drops it. They are not the persistent registry's story. The null backend's fake frame texture follows the same retire-on-present rule so tests do not grow a texture per frame.

## Dependencies

The caller retires dependents first. The backend's private reference count is not the rule.

| Still alive | Blocks release of |
| --- | --- |
| Bind group | Its uniform buffers |
| Pipeline | Its shader and bind-group layout |
| Texture view | Its texture |

A mesh vertex buffer is not kept alive by a bind group. Evicting a `GpuMesh` retires those buffers and leaves the logical mesh in the library. The next draw uploads again.

## Classes

| Class | Examples today | Later |
| --- | --- | --- |
| Recreatable | Mesh GPU buffers, depth target | Pipelines after device loss |
| Transient | — | Render-graph resources |
| External | Swapchain image | — |

States in code: alive and retired, which the renderer surfaces as Resident and Evicted for a mesh. Named for later, not built: PendingUpload, Requested, Lost, Recreating. Upload is still create-with-data. A staging ring is a later ticket.

Device loss is not recovered. A lost device would recreate Recreatable objects and drop Transient ones. External objects are recreated with the surface.

## Thread and shutdown

GPU mutation runs on the thread that calls `render`. Today that is the host thread. There is no job system.

Shutdown does not follow struct field order:

```text
stop frames
  -> flush
  -> retire bind groups
  -> retire uniform buffers
  -> retire mesh GPU buffers (logical meshes stay)
  -> retire pipeline, then shader, then layout
  -> drop swapchain image views immediately, then reconfigure
  -> retire depth view, then depth texture
  -> flush
  -> drop the device
  -> drop the window
```

`Renderer` does this on drop. The host drops the renderer before the window.

## What this is not

No asset manager, material, virtual texture, render graph, transient allocator, upload ring, GPU scene, meshlets, or VRAM budget. Counts (`alive_*`, `retired`, estimated bytes, labels) are for tests and a future inspector. They are not a UI.
