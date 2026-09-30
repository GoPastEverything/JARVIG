# ECS and scene model

Package: `@jarvig/ecs`. Snapshot schema: `@jarvig/scene-schema` (`jarvig.scene/v1`). ADR: [ADR-0009](../adr/ADR-0009-ecs-identity-and-data.md).

PlayCanvas's useful idea is entity, component, and system. JARVIG keeps that and separates stable identity from hot storage.

| Layer | Contract |
| --- | --- |
| Entity ID | Stable UUID. Hierarchy via `parentId`. |
| Component schema | Versioned fields plus editor and replication metadata. |
| Storage | Dense, archetype, or SoA where that proves useful. Phase 0 is a map. |
| System | Queries data and schedules work. Not implemented as a scheduler yet. |
| Prefab | Template graph plus explicit overrides. Not implemented. |
| Runtime-only state | Ephemeral simulation data. Excluded from saves unless promoted. |

## Phase 0 API

- `ComponentRegistry.register` stores `name`, `version`, `replicated`, and field metadata (`kind`, optional `min` / `max`).
- `Scene.createEntity` preserves a caller-supplied UUID or allocates one. Duplicate ids throw. Parents must already exist.
- `setParent` rejects cycles.
- `addComponent` checks the registered schema. Unknown fields, missing fields, and range errors throw.
- `duplicate` allocates a new id, keeps the parent, and copies component data.
- `serialize` / `Scene.fromSnapshot` round-trip `jarvig.scene/v1`. A version mismatch throws `migration required`. There is no silent downgrade and no migrator yet.

The sample schema used in tests is `Health` (`current`, `max`, `replicated: true`). It is not a gameplay system.

Hot archetype storage, prefabs, and the decorator-style authoring syntax in the founding doc (`@component`, `@field`) are future API sugar over this registry. Do not add a parallel component system beside it.

Golden fixture: `tests/golden/health-scene.json`. Ticket JRV-0013.
