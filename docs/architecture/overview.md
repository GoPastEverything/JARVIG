# Architecture overview

JARVIG is a native multi-platform engine. Browser execution is an export target. Browser technology must never become an implicit dependency of engine core.

The editor, the native player, the dedicated server, and the web export are hosts. They call engine APIs. The editor does not own import, cooking, materials, simulation, or rendering. Success for the platform, from the founding charter and ADR-0017, is:

- Create and open a project in the Hub and author it in the Editor.
- Play, pause, step, eject, possess, and stop Play-In-Editor without losing the authoring world.
- Build a client and a headless dedicated server from the same project.
- Traverse a world larger than resident memory without blocking content IO.
- Walk inside a moving ship or vehicle with stable, FPS-precise physics.
- Network that scenario with server authority and relevance-based replication.
- Reproduce all derived content with deterministic command-line tools.
- Benchmark experimental rendering against fixed baselines before promotion.

Early non-goals: a marketplace, a finished cinematic suite, a full server mesh, a clone of every Unreal feature, and any claim that experimental microgeometry beats Nanite before measurement.

## Principles

| Rule | Meaning |
| --- | --- |
| Dependency direction | Editor and tools may depend on engine modules. Engine modules never depend on editor modules. |
| One world contract | Editor, PIE, client, server, and tests share scene and component semantics. |
| Data oriented | Explicit component data and systems, not deep object inheritance. |
| CLI parity | Build, import, cook, validate, package, and critical world operations are automatable. |
| Derived data is reproducible | Baked content has traceable source, settings, and tool versions. |
| Streaming is universal | Geometry, textures, entities, nav, audio, physics proxies, and network state expose residency and relevance. |
| Research stays research | Novel systems stay feature-flagged until measurements justify production. |
| Debuggability over cleverness | Capture, replay, deterministic seeds, and named profiler scopes are first-class. |

## What runs today

The lifecycle, module graph, frame tree, cell ids, scene snapshot, material document, and derived-data key exist and are tested in the TypeScript prototype. That prototype is not the shipping runtime. `native/jarvig_core` has the same fixed-step clock behind a C ABI. GPU execution, physics, animation, audio, AI, and replication do not exist yet. The prototype render module uses a null device so hosts can boot without a GPU. The renderer architecture is an RHI ([rhi.md](../rendering/rhi.md)), not a browser canvas. WebGPU via wgpu is the first backend to build, on native and in the browser.

Read [engine-runtime.md](engine-runtime.md) and [dependency-rules.md](dependency-rules.md) before changing boot code.
