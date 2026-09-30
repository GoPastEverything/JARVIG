# ADR-0017 — Native engine core and multi-host runtime

Status: Accepted
Date: 2026-09-22

## Context

The bootstrap implemented the module contracts in TypeScript because that was the fastest way to make them executable, and because `designdoc.html` started from a PlayCanvas-derived, TypeScript-first plan. That plan is now the wrong architectural owner.

JARVIG has to be a standalone native engine and editor in the same product sense as Unreal, CryEngine, id Tech, and a Unity native player. A JavaScript engine wrapped in a desktop shell is not that product. The browser is one deployment target. It must not become an implicit dependency of engine core.

ADR-0002 made TypeScript the engine and PlayCanvas the foundation. ADR-0006 rejected a native graphics API as the first backend and can be misread as "the renderer is a browser." Both of those points are superseded here. Everything else those ADRs still get right (PlayCanvas license and upstream hygiene, WebGPU as a real graphics API, no per-draw design) stays.

## Decision

JARVIG is a native engine with optional web deployment. Web technology is a target and a tooling option, not the engine's architectural foundation.

```text
                         JARVIG
                            |
                    NATIVE ENGINE CORE
                            |
          +-----------------+------------------+
          |                 |                  |
          v                 v                  v
     NATIVE EDITOR      NATIVE GAME       DEDICATED SERVER
          |                 |                  |
          +-----------------+------------------+
                            |
                       ENGINE APIs
                            |
             +--------------+---------------+
             |                              |
             v                              v
       Native renderer                  Web export
    D3D12 / Vulkan / WebGPU          WASM + WebGPU
                                             |
                                             v
                                          Browser
```

A shipped Windows game must be able to boot `platform -> engine -> renderer -> world -> game` without Electron, Chromium, Node.js, a local HTTP server, browser APIs, DOM, or a JavaScript runtime, unless that game deliberately opts into one of those. The dedicated server must be able to omit the renderer, the editor, and graphics libraries. A web build is the same engine compiled for WASM plus a browser platform layer. It is not a second engine.

The TypeScript packages under `engine/` are the current executable prototype of the contracts (schemas, clock behavior, hosts, tests). They are not the shipping runtime. They must not import Node, DOM, or browser globals. New engine behavior is added as an engine API a host can call, never as an editor-only or browser-only implementation. See [ADR-0018](ADR-0018-engine-owned-capabilities.md).

### Core language

Primary language of the native core: **Rust**.

C and C++ are first-class at the edges, not optional afterthoughts:

- RHI backends that wrap D3D12, Vulkan, Metal, or a vendor SDK
- Console and OS platform modules
- Middleware that is only shipped as C or C++
- Plugins and game modules, which cross a **stable C ABI** so they can be written in Rust or C++ and can be reloaded

Rust's own ABI is not a plugin ABI. It is unstable across compiler versions.

This was scored against the other serious option, C++20 as the language of every system. Scores are qualitative (strong / mixed / weak) for this product, not a benchmark.

| Criterion | Rust as core, C/C++ at the edge | C++20 as the whole core |
| --- | --- | --- |
| Runtime performance | Strong. No GC. Predictable if allocations stay off the frame. | Strong. The proven AAA baseline. |
| Compile times | Mixed. Good if crates stay small. A monolith would be a mistake. | Mixed. Slow without unity builds and careful headers. |
| Tooling | Strong for build and test (`cargo`). Weaker IDE story than Visual Studio. | Strong on Windows. `cl` is not installed on the current dev machine. |
| Debugging | Mixed. CPU debugging is behind MSVC. GPU debugging stays on the C++ RHI side (PIX, RenderDoc). | Strong. |
| Ecosystem | Mixed. Excellent for wgpu, WASM, and new systems. Thin for console SDKs and classic middleware. | Strong. PhysX, D3D12, Vulkan, console SDKs. |
| Graphics access | Strong via wgpu, and via C++ backends behind the RHI. | Strong. Direct. |
| C ABI | Strong. `extern "C"` in both directions. C++ interop through that ABI or a narrow wrapper. | Strong. It is the ABI. |
| WASM | Strong. A first-class target. | Mixed. Emscripten works and is a second toolchain. |
| Multithreading | Strong. `Send`/`Sync` make a job system reviewable. | Mixed. Fast, and data races are on the programmer. |
| Memory safety | Strong. The reason to accept the debugging cost. | Weak, even with RAII. |
| Plugin ABI | Strong if, and only if, plugins use C ABI. | Strong. Game.dll reload is a known pattern. |
| Long-term maintainability | Strong for data-oriented code (indices, arenas). Weak if we fight the borrow checker with shared mutable graphs. | Mixed. Familiar, and easy to rot. |
| Consoles | Mixed. The core can stay Rust only if platform SDKs sit behind the C++/C ABI. Rust-on-console is not a plan. | Strong. |
| Build size | Strong, if unused modules are not linked. | Strong, same condition. |
| Iteration speed | Mixed. Fast tests. Hot reload of a Rust ABI is not allowed; reload is a C ABI game module. | Mixed. Hot reload of Game.dll is mature. Compile times hurt. |

C++ as the language of every system was rejected as the default because the safety, WASM, and module-boundary gains matter more than IDE familiarity, and C++ remains available exactly where those gains do not exist (graphics backends, consoles, middleware). Rust was not chosen because it is newer. A pure-Rust graphics and console stack was rejected because it would cut the engine off from D3D12/Vulkan tooling and platform SDKs.

TypeScript was rejected as the runtime owner. It stays for the prototype contracts, the CLI orchestration, schemas, and a future scripting host. Gameplay scripting is a host on top of the native API, not the implementation of the world.

### Rendering

The renderer owns an RHI. Backends are D3D12, Vulkan, native WebGPU (wgpu), browser WebGPU, and Metal later if a platform needs it. WebGPU is not "the browser."

The first backend to implement is **native WebGPU via wgpu**, because one implementation covers desktop Vulkan, D3D12, and Metal and matches the browser WebGPU target. That choice is a schedule decision, not a permanent sole backend. Consoles and PIX-level D3D12 work will need an explicit backend. Do not design the RHI around DOM, canvases, or `navigator.gpu`.

### What is not decided here

- Which UI toolkit the native editor uses. HTML panels are allowed. They are not the engine, and they are not the viewport.
- A date when the TypeScript prototype is deleted. A module moves when the native one passes the same tests. Until then, hosts may keep calling the prototype. They may not grow a second world model.
- A claim that the native crate already is a game runtime. `native/jarvig_core` currently contains the fixed-step clock and a C ABI for that clock. See the impact report.

## Alternatives Considered

- Keep TypeScript as the engine and ship it inside a WebView. Rejected. That is a browser engine in a window.
- Fork PlayCanvas and call the fork native later. Rejected as the foundation. PlayCanvas remains a reference, a web-host experiment, an import/export idea, and an optional adapter. The native core must not link it.
- C++ for all systems. Rejected as the default. Scored above. Still required at the platform and RHI edge.
- Freeze no language until a benchmark. Rejected. An unfrozen core is how the next session makes the engine a Node server again.

## Consequences

- `engine/**` TypeScript must not import `node:` or touch DOM globals. The boundary checker enforces that.
- Editor, CLI, and automation call engine APIs (ADR-0018). The temporary HTTP editor is a panel host, not `JARVIGEditor.exe`.
- `native/jarvig_core` is the root of the native core. New hot-path systems land there, or behind the C ABI, rather than as new browser assumptions in the TypeScript prototype.
- Server builds stay renderer-free. That rule already held and still holds.
- PlayCanvas is not vendored. ADR-0002's upstream and license rules still apply.

## Supersedes

- ADR-0002, the sentences that make TypeScript the engine and PlayCanvas the runtime foundation.
- ADR-0006, the rejection of a native graphics backend, and any reading that the renderer is a browser API.
- The founding document's "TypeScript-first / PlayCanvas-derived runtime" implementation plan (`designdoc.html` sections 5 and 31) as the architectural owner of the engine. The rest of that document (world model, materials, no BSP, research gates, CLI parity, milestones) remains in force unless a later ADR names it.

## Superseded By

The peer-backend diagram in this ADR is clarified by [ADR-0019](ADR-0019-rhi-hides-backend-types.md). wgpu stays the first GPU backend, and it stays private to that backend. The rest of this decision stands.
