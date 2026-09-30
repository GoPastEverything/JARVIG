# Rendering hardware interface

JARVIG code above the backend does not depend on wgpu types. wgpu is the first backend implementation, not the public rendering API. ADR: [ADR-0019](../adr/ADR-0019-rhi-hides-backend-types.md). Lifetimes: [resources.md](resources.md) and [ADR-0024](../adr/ADR-0024-gpu-resource-lifetime.md).

```text
JARVIG Renderer
      |
      v
JARVIG RHI interfaces
      |
      v
backend implementation
      |
      v
wgpu
      |
      v
D3D12 / Vulkan / Metal
```

Later, and only if a platform justifies it:

```text
JARVIG RHI
      |
      v
native D3D12 or Vulkan backend
```

Browser, same concepts, different platform edge:

```text
same renderer and RHI
      |
      v
Web/WASM platform backend
      |
      v
WebGPU
```

Do not build those extra native backends now. wgpu already targets D3D12, Vulkan, and Metal. Putting them on the public RHI as well would be two layers that mean the same thing.

## Contract

The authoritative interfaces are the Rust traits in `native/jarvig_rhi`. They are not wgpu types. The crate's manifest test fails if a GPU library is added.

| Area | What the contract is |
| --- | --- |
| Instance and device | `Instance`, adapter enumeration, `request_device` |
| Capabilities | Timestamp queries and max texture size, queried, not assumed |
| Queues | One graphics queue id on the null device |
| Resources | Buffers, textures, views, samplers, shader modules, bind layouts, pipelines |
| Commands | Encoders, render passes, compute passes, command buffers |
| Presentation | Headless surface, swapchain, `present`. Reconfigure drops swapchain views first and does not configure 0×0 |
| Sync | Fences, submit, wait |
| Lifetime | `destroy` rejects a double free |
| Labels | Optional debug label stored and readable |
| Timestamps | Hook exists. The null device reports the capability off and rejects the call |

## Null backend

`create_null_instance()` is the backend for unit tests, headless validation, server-side architecture tests, and any build that must not link a GPU library. It records render-pass order, pipeline binds, draws, and presents. It does not rasterize, it does not open a window, and it does not call wgpu.

`native/jarvig_core` does not depend on this crate. The headless runtime the editor ticks is not a renderer.

## First presented frame

JRV-0042 is that path, and it is implemented:

```text
native/jarvig_editor_host
  -> jarvig_engine::EngineSession::run_frame
  -> jarvig_renderer::Renderer::render(RenderViewId)
  -> jarvig_rhi
  -> jarvig_rhi_wgpu
  -> winit window owned by jarvig_platform
  -> clear JARVIG_CLEAR (0.051, 0.090, 0.141, 1)
  -> present
```

The development window is 1600×900 and resizable, not 1920×1080, so it fits a laptop without changing the proof. A zero drawable size skips the frame instead of panicking. Surface outdated, lost, timeout, and device-loss map to `RhiError` inside the wgpu crate. The null backend still clears and presents in tests, with no window.

Launch:

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host
cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host -- --frames 4
cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host -- --self-test
cargo run --manifest-path native/Cargo.toml -p jarvig_editor -- --self-test
```

`jarvig_editor_host` is the two-view test. `JARVIGEditor.exe` is the product shell and uses one perspective view on a child window. The renderer is the same either way.

`--self-test` presents, drives a zero-size frame, restores, checks that the pipeline was created once and the two geometry buffers were not recreated, and exits. It is the automated check. A continuously open window is the command without `--frames`.

## First triangle

JRV-0043 added one procedural triangle on that path and was visually accepted: the clear plus the interpolated RGB triangle. JRV-0044 keeps that image and replaces the vertex-index generation with persistent buffers. The host still only calls `run_frame`. The renderer owns the geometry, the shader, the pipeline, the pass, and the draw.

```text
vertex buffer + index buffer
  -> set_vertex_buffer
  -> set_index_buffer
  -> draw_indexed(3, 1, 0, 0, 0)
```

`BufferId` is the buffer handle. `BufferUsage` names Vertex, Index, Uniform, Storage, CopySrc, and CopyDst. Only vertex and index buffers are created on the GPU today. The others are the planned set, and the wgpu backend returns `Unsupported` for them. `IndexFormat` is `Uint16` or `Uint32`. This triangle uses `Uint16` and indices `0, 1, 2`.

Each vertex is `float3` position and `float3` color, stride 24. Those vertices live in two `jarvig_core` meshes, each with one submesh, Uint16 indices, and an object-local AABB. The renderer uploads each mesh once to RHI buffers and draws `submesh 0`. `MeshId` is an in-process id, not a C handle. `jarvig_core.h` is still the bootstrap clock header, not a mesh SDK. The near triangle is `(-0.6, -0.5, 0)`, `(0.6, -0.5, 0)`, and `(0, 0.6, 0)` meters, two meters in front of the camera. A larger gold mesh sits five meters in front and is drawn second. Depth keeps the near triangle visible. The root translation is one billion meters and is not in the uniform. ADR-0004 and ADR-0020 own that split. ADR-0021 owns the depth direction: infinite reversed-Z, clear 0, compare greater-or-equal, format `Depth32Float`. The depth target belongs to the render view and is recreated when the drawable size changes. A zero size does not allocate one.

The buffers and the pipeline are created on the first drawable frame and kept. Nothing is uploaded again on later frames unless a `GpuMesh` was evicted. Release retires the handle immediately. The backend object drops on `flush`, after the queue is idle. See [resources.md](resources.md). The wgpu backend may round a buffer allocation up to a multiple of 4 bytes. The JARVIG size stays the logical size.

A clear is not its own RHI operation. It is the color attachment of a render pass:

```text
RenderPassDesc
  ColorAttachment
    target
    load = Clear(JARVIG_CLEAR)
    store = Store
```

`LoadOp` and `StoreOp` are JARVIG types. The wgpu backend maps them privately. Ordinary `draw` remains for a pass that does not index.

`Viewport` and `ScissorRect` are JARVIG types too. A pass sets them with `set_viewport` and `set_scissor`. They are rectangles inside the current color attachment, origin at the top-left. The null backend rejects a rectangle that is empty or outside that attachment. A color clear still covers the whole attachment. Viewports do not make a clear local. See [views.md](views.md).

The fragment program is generated from Material IR. `ShaderSource::Wgsl` is still how that artifact crosses the RHI. It is not a decision that the JARVIG RHI is a WGSL API, and it is not Material IR. ADR-0007 still stands. Group 0 binding 0 is the view and object transform, visible to the vertex stage. Group 1 binding 0 is the material parameter uniform. Bindings after that, for the current master, are the base-color texture view and the sampler, visible to the fragment stage. See [../materials/textures.md](../materials/textures.md). `@location(0)` is position and `@location(1)` is color. The fixed vertex transform is compiler boilerplate around the generated surface code. No `wgpu::ShaderModule`, `wgpu::Buffer`, or `wgpu::BindGroup` crosses this boundary. Handles are `ShaderModuleId`, `PipelineId`, `BufferId`, `BindGroupLayoutId`, and `BindGroupId`.

`BufferUsage::Uniform` and read-only `BufferUsage::Storage` can be written. Copy-source and copy-destination usages are still unsupported. Bind groups 0, 1, and 2 are implemented. Group 2 is the per-view light list. Index 3 is accepted by the backend and unused. Index 4 and above is rejected. The transform uniform is created once per view and instance and written again only when that packet changes. The material uniform is created once per material instance and written again only when that instance's parameter revision changes. A live bind group blocks release of its buffer. The caller retires the group first.

Adapter selection is JARVIG policy, not a wgpu power hint. See [../platform/graphics-device-selection.md](../platform/graphics-device-selection.md) and [ADR-0030](../adr/ADR-0030-graphics-adapter-policy.md). The host passes `GraphicsDeviceConfig`. The backend enumerates. The RHI scores by device class. No vendor name is a score.

## What is not built

No mesh asset and no material texture. Textures are JRV-0052. The scene the renderer draws is a snapshot, not a scene graph. Two views already share one surface. See [views.md](views.md). No second native GPU backend. The editor viewport only consumes the engine's surface. It does not record those passes. Streaming, virtual textures, and device-loss recovery are not built. The residency split they will use is [resources.md](resources.md).

The TypeScript `RenderDevice` in `@jarvig/render` is the prototype stand-in the current hosts boot. Do not grow it into a second RHI and do not give it DOM types.

Shader source that crosses the RHI today is bootstrap WGSL text. The lasting contract is a JARVIG shader description, then Material IR, then a backend target. See [../materials/material-ir.md](../materials/material-ir.md).
