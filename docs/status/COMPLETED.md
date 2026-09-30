# Completed

Verified on 2026-09-22 by `pnpm lint`, `pnpm typecheck`, `pnpm test` (25 passed), `pnpm build`, and `pnpm smoke`.

| Ticket | What was shown |
| --- | --- |
| JRV-0001 | Boundary checker fails engine→editor and server→render imports |
| JRV-0002 | `upstream-playcanvas` remote and ADR-0002 merge policy. Nothing vendored |
| JRV-0003 | `Engine` lifecycle on test and client profiles |
| JRV-0004 | Fixed-step clock under variable deltas, with a tested step cap |
| JRV-0005 | Entity UUIDs survive save/load; duplicates get new ids |
| JRV-0006 | `Health` schema metadata serializes and can be inspected |
| JRV-0012 | `jarvig project validate` accepts `tests/fixtures/minimal.project.json` |
| JRV-0013 | `jarvig.scene/v1` matches `tests/golden/health-scene.json` |
| JRV-0016 | Frame composition keeps 0.25 m at 8,000,000 m; float32 absolute does not |
| JRV-0018 | Uniform grid assignment is deterministic, including negative coordinates |
| JRV-0021 | Derived key changes when source, settings, tool version, platform, or flags change |
| JRV-0029 | Dedicated server boots `world` only, graphics none |
| JRV-0030 | `AGENTS.md` and this status directory |
| JRV-0031 | `jarvig.material/v1` round-trips and rejects an unknown shading model |
| JRV-0007 | Dock, command bus, viewport pane bound to the engine camera. HTTP host is temporary, not the native editor |

| JRV-0041 | Headless native runtime called by the editor and the server. RHI contract and null backend. No wgpu dependency above the backend. |
| JRV-0042 | Native window clears and presents through the engine, renderer, RHI, and private wgpu backend. Server stays headless. |
| JRV-0043 | One procedural triangle. Visually accepted: clear plus the interpolated RGB triangle. |
| JRV-0044 | The same triangle from a vertex buffer, an index buffer, and `draw_indexed`. Visually confirmed over 275 frames. Buffers are created once. Accepted. |
| JRV-0045 | The triangle moves through the frame tree and a camera. GPU data is camera-relative float32. A billion-meter origin does not become the world matrix. Accepted. |
| JRV-0046 | Two triangles. The nearer one wins even though it is drawn first. Reversed-Z, infinite far, depth target per view. Implementation and architecture accepted. Visual confirmation still open. |
| JRV-0047 | Two local meshes, one submesh each, uploaded through RHI buffers. Not a public C type. Implementation and architecture accepted. Visual confirmation optional. |
| JRV-0048 | Generational GPU handles. A logical mesh can be evicted from the GPU and uploaded again. Shutdown is explicit. ADR-0024. Accepted. wgpu flush is conservative on purpose. |
| JRV-0049 | Two views of one world, one surface acquire, one present. Shared GPU meshes. ADR-0025. Accepted. |
| JRV-0050 | One render snapshot per presented frame. The renderer does not query the world. ADR-0026. Accepted. |
| JRV-0051 | First material. Graph, IR, one unlit master, two instances, one pipeline. Accepted. |
| JRV-0052 | Logical Texture2D, sampler, UV0, and a material sample. sRGB versus linear is ADR-0027. Accepted. Human look confirmed. |
| JRV-0056 | API doctrine. ADR-0022 accepted. Implementation deferred. ADR-0023 adds struct size, calling-convention macros, and the bootstrap-header boundary. |

JRV-0053 (standard metallic/roughness surface) and JRV-0054 (first direct lights, human look confirmed) are accepted. JRV-0055 (graphics adapter policy) is accepted. On this Lenovo laptop the only hardware adapter is Intel UHD, and the high-performance fallback to it is correct. JRV-0056 stays the API doctrine. JRV-0057 is the native editor epic. JRV-0058 (the shell), JRV-0059 (the dock workspace), JRV-0060 (entity uuid plus generational handle), JRV-0061 (the world outliner), JRV-0062 (editor selection), JRV-0063 (the reflected inspector), JRV-0064 (the Perspective editor camera, human look and fly confirmed), and JRV-0070 (child viewport surface resize) are accepted. The editor shell after that acceptance is theme v1: dark chrome and the navigation icon toolbar. JRV-0008's native view is that shell, not a second viewport. JRV-0065 (viewport picking and the transform gizmo) is accepted. Human input confirmed Escape during a drag and RMB flight after release. Scale stays unavailable until a real scale model exists. JRV-0071 (HDR scene color, exposure, and tone mapping) is accepted. Human visual confirmed highlight rolloff, Exposure − recovering detail, Exposure + and Reset, and gizmo colors staying display-space. JRV-0072 (environment diffuse, including two-sided reverse-face lighting) is accepted. Human visual confirmed coherent lighting on both sides of the bootstrap cards. The missing metal reflection was not part of that acceptance. JRV-0087 (presentation and lighting stability) is accepted. Human visual confirmed on 2026-09-24 that the diagonal floor artifact is quantization, not a shadow failure. The Lighting Lab file was not rewritten for that look. Remaining 8-bit steps, colored dither, and the 64² mirror were left open on purpose.

JRV-0017 is not complete. The camera-relative helper exists and is tested. The render stress exit does not.
