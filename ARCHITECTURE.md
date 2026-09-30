# JARVIG architecture

JARVIG is a native multi-platform engine. Browser execution is an export target. Browser technology must never become an implicit dependency of engine core.

This file condenses [`designdoc.html`](designdoc.html) where that document still holds. [ADR-0017](docs/adr/ADR-0017-native-engine-core-and-multi-host-runtime.md) supersedes its TypeScript-first / PlayCanvas-as-runtime plan. [ADR-0018](docs/adr/ADR-0018-engine-owned-capabilities.md) says the editor, CLI, and automation call engine APIs and do not own them. If an accepted ADR disagrees with this file, the ADR wins and this file must be updated in the same change.

## Product

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
          +--------+--------+------------------+
                   |
              ENGINE APIs
                   |
         +---------+---------+
         |                   |
         v                   v
   Native renderer       Web export
 D3D12/Vulkan/WebGPU    WASM + WebGPU
```

The picture below is the same rule drawn as hosts of one core. The editor is not a second engine. The web player is not the core.

```text
JARVIG HUB
    |
    v
JARVIG EDITOR  ---- hosts the real engine
    |
    v
ENGINE CORE (world, simulation, assets, renderer)
    |
    +-------------------+
    |                   |
    v                   v
GAME CLIENT      DEDICATED SERVER
```

Also attached to the engine, not to a private editor copy: cooker, CLI, and the automation host.

How those callers cross into the engine is [the API doctrine](docs/api/README.md). ADR-0022. Internal crates use Rust. The stable boundary is a C ABI of opaque handles. A C++ SDK, if written, wraps that C ABI. Editor commands and the CLI call the same engine capabilities. One type registry feeds the inspector, saves, and later scripts. The bootstrap header `native/jarvig_core/include/jarvig_core.h` is not that SDK yet.

The Editor viewport renders the runtime world. Play-In-Editor snapshots the authoring world, simulates a copy, and by default throws runtime mutations away on Stop. Panels talk to a command/transaction bus. They do not poke arbitrary runtime objects. Game code does not call editor APIs.

Details: [engine runtime](docs/architecture/engine-runtime.md), [editor runtime](docs/architecture/editor-runtime.md), [Play-In-Editor](docs/architecture/play-in-editor.md).

## Dependency direction

```text
math, schemas
    ^
engine core, ecs, world, assets, materials
    ^
render (client/editor only)
    ^
runtime hosts (hub, editor, client, server, cli, samples)
    ^
editor panels and tools
```

`engine/**` must not import `editor/**`, `apps/**`, `hosts/**`, or `tools/**`. Enforced by `pnpm lint`. Rules: [dependency rules](docs/architecture/dependency-rules.md). ADR-0015.

## World

The authoritative world is a tree of reference frames, not a BSP and not one float32 origin.

```text
Universe
└─ Star system
   └─ Planet
      └─ City
         └─ Station
            └─ Ship
               └─ Interior
                  └─ Entity
```

Frame poses are float64 relative to their parent (`FrameTransform64`). Local simulation and rendering use float32 poses near a useful origin (`LocalTransform32`, camera-relative offsets). World cells partition a frame for streaming. The same cells are the future unit of render, physics, AI, audio, and network relevance. Server meshing is not implemented and must not invent a second spatial model.

Details: [world](docs/architecture/world.md), [coordinates](docs/architecture/coordinate-frames.md), [partition](docs/architecture/world-partition.md), [streaming](docs/architecture/streaming.md). ADR-0003, ADR-0004, ADR-0013.

## ECS

Stable UUID identity and hierarchy are separate from component storage. Components are versioned schemas with editor and replication metadata. Phase 0 stores components in maps. Archetype or SoA storage can replace that map later without changing snapshots. Runtime-only state is not saved unless it is promoted. Prefabs are a later template-plus-overrides layer, not a second entity system.

Details: [ecs](docs/architecture/ecs.md), [serialization](docs/architecture/serialization.md). ADR-0009.

## Rendering and materials

WebGPU is the architectural target. WebGL2 may exist later as a compatibility backend and must not force the design back to one draw call per object. The CPU describes scene state and work graphs.

Materials are authored assets. The graph compiles to a canonical Material IR, then to WGSL. Authored materials do not store WGSL as their meaning. Master materials, instances, functions, layers, and parameter collections are the authoring hierarchy. Virtual geometry, microgeometry, and representation virtualization are research and stay flagged off.

Details: [rendering](docs/rendering/overview.md), [materials](docs/materials/overview.md). ADR-0006, ADR-0007, ADR-0008.

## Assets

Source files go through an import graph into a derived-data cache. The cache key is a hash of source bytes, import settings, importer version, engine format version, target platform, and feature flags. Runtime code holds asset handles (`unresolved`, `loading`, `resident`, `evicted`, `failed`), not editor paths. Server cooks omit renderer and audio payloads they do not need.

Details: [assets](docs/assets/asset-system.md). ADR-0012.

## Phase 0 repository shape

`designdoc.html` section 5 is canonical. The bootstrap instruction's alternate folder names were not used where the founding doc already named the folder.

| Bootstrap suggestion | Canonical path | Phase 0 |
| --- | --- | --- |
| `apps/client` | `hosts/game-web`, `hosts/game-desktop` | `hosts/client` until those targets split |
| `apps/server` | `hosts/dedicated-server` | implemented |
| `engine/rendering` | `engine/render` | implemented |
| `engine/networking` | `engine/net` | contract only |
| `engine/platform` | hosts + `engine/core` | no separate package |
| `tools/asset-cooker` | `tools/cooker` | contract only |
| `tools/shader-compiler` | `tools/shaderc` | contract only |

Unimplemented module directories contain a README and no fake runtime. They are not imported by the boot path.

## Frame lifecycle

```text
Host.pollEvents
  -> Input.sample
  -> Clock.advance
       -> FixedSimulationSteps (0..N)
            -> Net.receive/reconcile
            -> scripts / gameplay / AI
            -> Physics.step
            -> transform propagation
       -> VariableUpdate (animation, audio, presentation)
  -> World.streamingUpdate
  -> Render.prepare      (skipped when profile is server)
  -> Render.execute      (skipped when profile is server)
  -> Telemetry.commitFrame
```

Phase 0 implements the clock, the stage hooks, streaming and render hook points, and telemetry counters. Net, physics, animation, and the GPU are not hooked up. The null render device counts prepare/execute so hosts can prove the stage ran.

## What is intentionally absent

No BSP world. No vendored PlayCanvas. No CryEngine code. No Nanite claim. No planet. No server mesh. No full material graph compiler. No dockable editor. No multiplayer protocol. Those have tickets in [`docs/BACKLOG.md`](docs/BACKLOG.md) or RFCs in [`docs/rfc/`](docs/rfc/README.md).
