# ADR-0019 — The RHI hides backend types

Status: Accepted
Date: 2026-09-22

## Context

ADR-0017 says the first GPU backend is wgpu, and drew wgpu as if it were a sibling of D3D12 on the public RHI. That diagram can be implemented by importing `wgpu` types in the renderer. wgpu already sits on D3D12, Vulkan, and Metal. A second public split that repeats those APIs is two names for one thing, and it leaks a backend into engine code.

## Decision

JARVIG code above the backend does not depend on wgpu types. wgpu is the first backend implementation, not the public rendering API.

```text
JARVIG Renderer
      |
      v
JARVIG RHI interfaces          native/jarvig_rhi
      |
      v
backend implementation         a separate crate, not written yet
      |
      v
wgpu
      |
      v
D3D12 / Vulkan / Metal
```

A later explicit D3D12 or Vulkan backend is allowed only when a platform or a tool needs it. It is not started now. The browser uses the same renderer and RHI concepts, then a Web/WASM platform backend, then WebGPU. It does not get a second renderer.

The RHI contract covers instance and device creation, adapter and device capabilities, queues, buffers, textures, samplers, shader modules, bind layouts, pipelines, command encoders and buffers, render passes, compute passes, surfaces, swapchains and presentation, synchronization, resource lifetime, debug labels, and timestamp-query hooks. The null backend implements that contract for tests, headless checks, and any build that must not link a GPU library. `jarvig_core` does not depend on `jarvig_rhi`. The future wgpu crate is the one a server link must be able to drop.

The first native graphics milestone is a clear color presented through this stack:

```text
native editor host
  -> engine
  -> renderer
  -> JARVIG RHI
  -> wgpu backend
  -> native window surface
  -> clear
  -> present
```

Then triangle, indexed mesh, camera, depth, material, and scene, in that order. Not before the contract exists, and not inside the editor. The editor viewport consumes an engine render surface. It does not record passes or name wgpu types.

## Alternatives Considered

- Call wgpu directly from the renderer and treat that as the RHI. Rejected. The public API would be wgpu's, and a later D3D12 backend would fork the renderer.
- Implement D3D12, Vulkan, and wgpu backends together. Rejected. wgpu already covers those APIs for the first desktop path.
- Put the first triangle in the editor page. Rejected. That is an editor-owned renderer.

## Consequences

- `native/jarvig_rhi` must not gain a wgpu dependency. A unit test reads its manifest.
- The TypeScript null device in `@jarvig/render` remains the prototype host device. It is not a second RHI. New GPU work goes through `jarvig_rhi`.
- No window is opened in this decision. Presentation is the next graphics milestone, JRV-0042, and it is not done.

## Supersedes

The backend diagram in ADR-0017 that lists wgpu, D3D12, Vulkan, and browser WebGPU as peer RHI backends. wgpu remains the first backend to write. It is private to that backend.

## Superseded By

Nothing.
