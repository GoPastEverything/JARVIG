# World model

Package: `@jarvig/world`. The world is entity and object based, dynamically editable, streamable, partitionable, and persistent. It has to support enormous worlds, moving environments, nested reference frames, FPS gameplay, spacecraft, planets, interiors inside moving ships, and later multiplayer authority.

It is not a Quake-style BSP. See [ADR-0003](../adr/ADR-0003-no-bsp-world-model.md).

## What exists

- `FrameGraph` — parented float64 frames. `rootPosition` composes a local point out to the root. See [coordinate-frames.md](coordinate-frames.md).
- `jarvig_core::space` — the same frame contract on the native runtime: binary64 poses, camera-relative float32 for the GPU, no float32 world matrix.
- `jarvig_core::scene` — the bootstrap world the engine owns, and `RenderSceneSnapshot`, the copy extracted for one render frame. The renderer does not query the world. Camera-relative float32 is applied per view after that copy. See [../rendering/scene-extraction.md](../rendering/scene-extraction.md).
- `jarvig_core::entity` — `EntityUuid` for persistence and `EntityHandle` (slot plus generation) for lookup. The registry stores name and parent. It does not store components. See [entity-identity.md](entity-identity.md).
- The native outliner reads that registry. It is editor-only. Bootstrap lights, the reflection probe, and World Settings are entities. The Perspective camera is not. A non-renderable entity is still an entity. See [ADR-0040](../adr/ADR-0040-authoring-entity-is-not-a-subsystem-id.md).
- `jarvig_core::reflect` is schema version 2 of the type registry: entity, spatial frame, the three light components, the sphere reflection probe, the environment, and the mesh renderer. Editable fields go through an engine command. UUID, parent, the local quaternion, and the probe capture fields are visible and not editable. A frame is still translation plus a unit quaternion. Scale is not a frame field. See [../api/reflection.md](../api/reflection.md).
- A terrain actor is one chunked heightfield in terrain-local meters on the existing scene frame. That heightfield owns collision and navigation height. Derived chunk meshes are visuals and do not cast shadows. Einstein detail is a stored record on the actor and does not write the height. Page streaming is still absent. See [ADR-0058](../adr/ADR-0058-terrain-is-a-local-heightfield.md) and [../terrain/foundation.md](../terrain/foundation.md).
- `jarvig_core::light` — directional, point, and spot lights on that same world. Each light has a frame. Emission is local -Z. The snapshot copies enabled lights once. See [../rendering/lighting.md](../rendering/lighting.md) and [ADR-0029](../adr/ADR-0029-direct-light-conventions.md).
- `assignCell` / `assignEntities` — deterministic uniform grid inside one frame. See [world-partition.md](world-partition.md).
- `streamingPriority` — initial heuristic. See [streaming.md](streaming.md).
- `createWorldModule` — engine module id `world`. Counts streaming hook calls. Does not load cells from disk.

## What does not exist

Async cell IO, portals, resident budgets, physics proxies, nav chunks, audio zones, and network relevance filters. The cell record in the founding doc names those fields so later systems attach to the same cell instead of inventing another grid. Phase 0's `WorldCell` includes id, frame id, bounds, entity ids, dependency ids, and a cost estimate. Page arrays are not filled with placeholders.

## One world contract

Editor, Play-In-Editor, client, server, and tests must share this scene and frame meaning. A planetary mode is not a second world system. It has to exercise these contracts. See [../rfc/RFC-0004-planet-scale-terrain.md](../rfc/RFC-0004-planet-scale-terrain.md).
