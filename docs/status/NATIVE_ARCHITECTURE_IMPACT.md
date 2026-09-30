# Native architecture impact report

Date: 2026-09-22. Decisions: ADR-0017, ADR-0018.

## What was already right

- One engine, several hosts. The dedicated server does not import `@jarvig/render`.
- World, ECS, materials, and assets are packages the hosts call. There is no editor-owned importer.
- The material document is not WGSL. The renderer interface has no DOM types.
- Research flags default off. No BSP. Frame transforms are float64 parented frames.
- `designdoc.html` was not rewritten. The native ADR names the sections it supersedes.

## What was browser- or Node-authoritative

| Location | Problem | What happened |
| --- | --- | --- |
| ADR-0002 | Said the engine is TypeScript and PlayCanvas is the foundation | Partially superseded by ADR-0017. Upstream and license rules kept. |
| ADR-0006 | Could be read as "renderer = browser WebGPU", and rejected a native graphics API | Partially superseded. WebGPU is the first RHI backend, including native wgpu. |
| `engine/core` `isDirectRun` | Engine core imported `node:url` to detect a process entry | Moved to `@jarvig/node-host`. Core no longer imports Node. |
| `engine/assets` derived keys | SHA-256 and base64 came from Node (`node:crypto`, `btoa`) | Replaced with engine-local code. A test checks it against Node's hash so the key did not silently change. |
| `apps/editor` HTTP server | The only editor host was a Node web server | Kept as a **temporary panel host** for the dock shell. Documented as not `JARVIGEditor.exe` and not a WebView wrapper around a JS engine. |
| TypeScript `Engine` class | Hosts still boot this prototype | Kept on purpose. Deleting it would throw away the tested contracts. It is not the shipping runtime. |

No engine source imported `playcanvas`, `document`, or `window`.

## Repository shape added

- `native/jarvig_core` — Rust crate, fixed-step clock, C ABI in `include/jarvig_core.h`. This is the start of the native core, not a second world.
- `engine/rhi/README.md` — RHI contract. No fake backend.
- `apps/player/README.md`, `game/README.md` — player and game module do not exist yet. Game logic does not go in `engine/`.
- `docs/platform/` — hosts, modules, plugin ABI.
- `docs/rendering/rhi.md`.

## Not done, on purpose

- No Rust port of ECS, world, or the renderer.
- No `JARVIGEditor.exe`, no `JARVIGRuntime.dll`, no `Game.dll`.
- Hosts still link the TypeScript prototype.
- wgpu / D3D12 / Vulkan are not initialized. The render module is still a null device.
- PlayCanvas was not vendored and was not deleted as a reference.

## Migration still required

1. Each prototype system moves into `native/jarvig_core` (or a sibling crate) only when the same tests pass.
2. The editor viewport must present the native renderer (JRV-0008). A DOM canvas does not satisfy that ticket.
3. The editor process itself becomes a native host. HTML may remain for panels.
4. Import, cook, and shader compilation, when built, are engine APIs. CLI and editor both call them.
5. A linker flag or crate feature must be able to build the server without the renderer. The TypeScript boundary test is the prototype of that rule, not the native link line.
