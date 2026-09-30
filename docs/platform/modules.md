# Modules

Build only what the target needs.

| Module | Native home | Prototype today | Server |
| --- | --- | --- | --- |
| Core, clock, jobs | `native/jarvig_core` (clock only) | `@jarvig/core` | yes |
| Math, frames | later crate | `@jarvig/math`, `@jarvig/world` | yes |
| ECS | later crate | `@jarvig/ecs` | yes |
| Assets | later crate | `@jarvig/assets` | yes, without GPU payloads |
| Materials | later crate | `@jarvig/materials` | schema only if gameplay needs it |
| Renderer | `jarvig_renderer` (RHI only) | `@jarvig/render` null device | no |
| RHI | `jarvig_rhi` plus null backend | none | null only, and the server does not need to link it |
| wgpu backend | `jarvig_rhi_wgpu` only | none | no |
| Window | `jarvig_platform` (`winit`) | none | no |
| Physics, animation, audio, AI, net | reserved directories | not started | physics, AI, net yes; audio and render no |

A game module lives under `game/`, not in `engine/`. The editor loads it. Hot reload is a future reload of that module across the C ABI. Do not design an API that only works if the game is compiled into the editor binary, and do not block the bootstrap on hot reload.

Job-system, SIMD, and allocation budgets are requirements of the native core. They are not implemented. Do not invent frame-time numbers for them.
