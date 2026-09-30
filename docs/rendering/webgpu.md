# WebGPU

WebGPU is a graphics API, not a browser. Native code reaches it through wgpu (Vulkan, D3D12, Metal). The browser reaches the same shading target through the browser's WebGPU implementation. Neither path is allowed to pull DOM types into engine core. Engine simulation code does not call `navigator.gpu`.

The prototype `RenderDevice.backend` is `'webgpu' | 'webgl2' | 'null'`. Hosts boot with `'null'` so tests do not need a GPU. `null` is not a third production backend. A real device, when it exists, implements the RHI. See [rhi.md](rhi.md).

A later WebGL2 backend is a compatibility path. It is not an excuse to keep per-object submission as the core design.

Project setting `renderBackend` accepts `webgpu` or `webgl2`. The founding default is `webgpu`. The null device is a host concern, not a project setting.

Shader output for materials is WGSL after Material IR. See [../materials/material-ir.md](../materials/material-ir.md). Authored assets must not embed WGSL as their source of meaning.
