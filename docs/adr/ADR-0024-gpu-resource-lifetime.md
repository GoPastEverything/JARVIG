# ADR-0024 — GPU resource lifetime

Status: Accepted
Date: 2026-09-22

## Context

JRV-0047 separated a logical mesh from the buffers that draw it. The handles that name those buffers were still plain numbers, and destroying one dropped the backend object immediately. That is enough for a triangle. It is not enough for resize, streaming, or a mesh that stays loaded while its GPU copy is gone.

The same split has to hold for textures, and later for virtual-geometry cluster pages. Those systems are not built here.

## Decision

A logical engine resource has no wgpu type and no RHI type. The renderer decides whether a GPU representation exists. The RHI names that representation with a generational handle. Only the backend owns the backend object.

```text
JARVIG WORLD
    Mesh                         logical resource
      |
      v
Renderer residency               Resident | Evicted
      |
      v
GpuMesh                          BufferId, later cluster pages
      |
      v
JARVIG RHI
      |
      v
backend resources
```

A texture follows the same ladder when one exists: texture asset, then residency, then `TextureId`, then the backend texture. A cluster page follows it again: resident, evicted, or requested. Requested is not implemented.

Identity for buffers, textures, texture views, samplers, shaders, bind group layouts, bind groups, and pipelines is an index plus a generation. Generation 0 is never live. A stale handle must not alias a reused slot. Double release is `InvalidHandle`.

Lifetime is Alive, then Retired, then Destroyed. Release retires the handle immediately. The backend object is dropped only after the submission that last used it has completed. This ticket does not guess a frame count and does not use `Arc` or a garbage collector as the contract.

The safety token is a submission serial. The null backend completes on submit because it has no GPU. The wgpu backend has one queue. `flush` calls `poll(Maintain::Wait)` and then collects every retired object, because that wait finishes the work already submitted on that queue. It does not overlap deletion with the GPU. A caller retires a resource only when no unsubmitted encoder still references it.

A live bind group keeps its buffers. A live pipeline keeps its shader and layout. A live texture view keeps its texture. The caller retires the dependent first. wgpu's internal reference count is not the public rule.

Destroying or evicting a `GpuMesh` does not destroy the logical mesh. The same is true later for a texture asset and for a cluster page.

GPU mutation stays on the thread that owns the device. Today that is the host thread that calls `render`. This decision does not add a job system.

Shutdown is explicit: stop frames, flush, retire renderer resources in dependency order, collect, drop the device, then the window. Struct field order is not the order.

## Alternatives Considered

- Drop the backend object inside `destroy`. Rejected. A resize or a present can still be using it.
- Wait three frames and call that safe. Rejected. It is not a completion signal.
- Keep resources alive with `Arc` until wgpu drops them. Rejected. That hides the lifetime in the backend.
- Put `BufferId` or `MeshId` in the C header. Rejected. ADR-0022. An internal handle is not a stable ABI handle.

## Consequences

`docs/rendering/resources.md` is the map. The wgpu path completes the queue inside `flush` before it destroys retired objects. That stall is safe for bootstrap. It is not the final overlap model. Later multi-frame, streaming, and async upload work should retire with the submission serial or a fence, and should not turn this decision into a synchronization project. Streaming, virtual textures, a render graph, an upload ring, and device-loss recovery stay later tickets. The states they will need — pending upload, requested, lost, recreating — are named there and not implemented. Recreatable resources (mesh GPU buffers, the depth target) can be released and created again. Transient resources belong to a future render graph. External resources, such as the swapchain image, are not recreated from engine data.

## Supersedes

Nothing. It does not replace ADR-0019 or ADR-0022.

## Superseded By

Nothing.
