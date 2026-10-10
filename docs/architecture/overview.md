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

`JARVIGEditor.exe` is the product editor. It hosts the native engine in-process and presents a Direct3D 12 view on Windows. A launch with no project shows the Hub. Play In Editor runs a runtime copy of the authored level. A loose standalone player can be built beside the editor. The creator manual is [../manual/README.md](../manual/README.md).

The TypeScript packages under `engine/` remain the executable prototype of the contracts. They are not the shipping runtime and they are not the editor. The prototype render module can still boot on a null device. Do not wrap that dock in a browser and call it JARVIGEditor.

In the native editor today: project open and save, the asset registry, glTF import, parametric blocks, round and extrude on an authored seed, translate and rotate, terrain heightfields in Land, joint pose and the Character workspace, and environment lighting. Direct lights and a reflection probe edit in the inspector when the level already contains them. Scale stays disabled. Clip playback, a physics solver, audio, AI, replication, page streaming, and a gameplay scripting language are not in this build. The renderer architecture is an RHI ([rhi.md](../rendering/rhi.md)). The native editor's present path is Direct3D 12. WebGPU via wgpu remains the portable backend described in the RHI pages, and it is not a second editor.

Read [engine-runtime.md](engine-runtime.md) and [dependency-rules.md](dependency-rules.md) before changing boot code.
