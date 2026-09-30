# Engine runtime

JARVIG is a native multi-platform engine. Browser execution is an export target. Browser technology must never become an implicit dependency of engine core.

The shipping runtime is `native/jarvig_core` and the crates that will sit beside it. The TypeScript `Engine` in `@jarvig/core` is the prototype hosts still boot. It must not import Node or the DOM. `isDirectRun` lives in `@jarvig/node-host` because detecting a Node entrypoint is a host concern.

Profiles are `editor`, `client`, `server`, and `test`. A server profile skips render stages. A native server link must be able to omit the renderer the same way.

## Who drives a frame

A host drives the process. The engine defines the order. The editor does not.

```text
Platform poll
  -> EngineSession::run_frame
       -> clock tick
       -> render callback, only when the profile presents
  -> host waits for the next event
```

`native/jarvig_engine` implements that order and does not link a GPU crate. When the profile presents, it extracts one `RenderSceneSnapshot` and the callback is `Renderer::render_target` with that snapshot, the mesh library, the material library, and the texture library. Profiles that present build one standard master, two instances, and the bootstrap textures before the first frame. The server does not extract, compile materials, or upload textures. Its libraries stay empty. The host does not walk views, encode passes, acquire a surface, or name wgpu. It does pass a graphics-device config. Adapter scoring stays in the RHI. See [../platform/graphics-device-selection.md](../platform/graphics-device-selection.md). The dedicated server never reaches that config. One target is acquired and presented once per callback. A server session does not call the callback. GPU calls stay on the thread that owns the device. Today that is the host thread. A render thread can wrap that later. This is not a job system. UI code must not call the backend. Property edits are `EngineSession::execute_authoring`. They are not writes into a render snapshot. The server can link the core type registry. It does not link the inspector.

`JARVIGEditor` hosts that engine in-process and creates one perspective view. `jarvig_editor_host` hosts it the same way and still creates two views. `JARVIGGame` hosts it with no editor panels: it loads a `.jarvigproject`, instantiates a `RuntimeWorld`, and presents that world's startup camera. `--server` loads the same project and does not create a device. The server hosts it with no renderer. The editor does not get a private copy of the world, the materials, or the device. See [editor-runtime.md](editor-runtime.md).

Shutdown of a presented frame is not struct-field order. Stop frames, flush the queue, retire renderer GPU objects, collect, drop the device, then drop the window. Logical meshes survive that. See [../rendering/resources.md](../rendering/resources.md).

The TypeScript prototype still has its own `Engine.tick` for the temporary dock. That dock is not the frame authority for the native window.

## Lifecycle

The founding frame order is the contract. The TypeScript prototype implements it as hook lists on `Engine.tick`:

```text
clock advance
  -> fixed steps (0..N, cap 8 by default)
  -> variable update
  -> streaming
  -> render prepare     skipped when profile is server
  -> render execute     skipped when profile is server
  -> telemetry
```

Input sampling, net receive, scripts, physics, and animation are the fixed-step and variable-update subscribers of later milestones. They are not stubbed as fake systems. A module that does not exist is not registered.

`SimulationClock` accumulates variable frame deltas into a fixed step (default 60 Hz). A frame that would need more than `maxStepsPerFrame` steps runs the cap and **drops** the leftover time. The tick reports `clamped: true`. Negative deltas count as zero. Non-finite deltas throw.

## Scheduling lanes

| Lane | Use |
| --- | --- |
| Main | Host events, transaction commit, final presentation state |
| Workers | Asset decode, partition build, navigation, compression, heavy CPU preprocessing |
| GPU compute | Visibility, skinning, particles, cluster selection, procedural detail |
| Server workers | AI batches, persistence serialization, cell simulation where ownership allows |

Phase 0 runs on the main thread only. Worker and GPU lanes are not invented ahead of the systems that need them.

## Modules

An `EngineModule` has an id, optional requirements, `initialize`, and `shutdown`. The engine topologically sorts modules and rejects duplicates, missing requirements, and cycles. Shutdown runs in reverse order. A failed initialize shuts down modules that already started.

Registered in Phase 0:

| Id | Package | Hosts |
| --- | --- | --- |
| `world` | `@jarvig/world` | editor, client, server |
| `render` | `@jarvig/render` | editor, client. Requires `world`. Not registered on the server |

## Feature flags

Defaults are off and tested:

- `virtualGeometry`
- `proceduralMicrogeometry`
- `representationVirtualization`
- `serverMeshing`
- `advancedGi`

Turning one on is an explicit host choice for an experiment. It is not promotion. See [ADR-0008](../adr/ADR-0008-research-feature-flags.md).

## Telemetry

`Engine.telemetry` stores named counters. Phase 0 increments `engine.start`, `frame.tick`, `frame.fixedSteps`, `frame.renderSkipped`, and `engine.shutdown`. Milliseconds are recorded only when a caller passes a measured value. The engine does not invent timings.

## Budgets

Initial targets from the founding doc, to force measurement later, not as Phase 0 pass/fail gates:

| Area | Initial target |
| --- | --- |
| Client frame | 16.67 ms at 60 Hz |
| Fixed simulation | ≤ 4 ms typical client slice |
| Main-thread JS | ≤ 6 ms typical |
| Render CPU submit | ≤ 2 ms long-term, via the GPU-driven path |
| Streaming | no visible-frame blocking IO in normal traversal |
| Editor interaction | < 100 ms perceived response for common operations |
| World save | incremental; never rewrite a giant world for one actor move |

No Phase 0 benchmark claims these numbers. See [../benchmarks/README.md](../benchmarks/README.md).
