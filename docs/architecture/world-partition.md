# World partition

A `WorldCell` belongs to one frame. Identity is `${frameId}:${ix}:${iy}:${iz}` from `floor(position / cellSize)`. Negative coordinates use `Math.floor`, so the boundary is deterministic. `assignEntities` sorts cell ids and entity ids, so input order does not change the result.

The founding cell also names, for later tickets:

- `renderPages`, `texturePages`
- `physicsProxy`
- `navChunks`
- `audioZones`
- `gameplayTags`
- `dependencies`
- `residentCostEstimate`

Phase 0 stores entities, dependencies, and the cost estimate. It does not allocate the page arrays as empty stand-ins that callers might treat as a streaming system.

Cells are the spatial model for future render relevance, physics relevance, AI relevance, audio relevance, and network relevance. Server meshing, if it is ever built, has to own cells rather than a private map. See [ADR-0013](../adr/ADR-0013-multiplayer-ready-world-identity.md) and [../rfc/RFC-0005-server-meshing.md](../rfc/RFC-0005-server-meshing.md).

Async loading is JRV-0019. The streaming debug overlay is JRV-0020. Neither exists.
