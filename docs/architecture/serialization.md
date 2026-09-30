# Serialization and project format

Authoring data stays human-diffable. Cooked runtime data may be compact binary later. Large worlds should shard data (one file per entity or per cell is the founding example) so moving one object does not rewrite the map. Sharding is not implemented.

## Project document

Package: `@jarvig/project-schema`. Id: `jarvig.project/v1`.

```json
{
  "$schema": "jarvig.project/v1",
  "name": "FPSVerticalSlice",
  "engine": "0.1.x",
  "projectId": "uuid",
  "defaultWorld": "assets/worlds/main.world.json",
  "targets": ["webgpu-web", "windows-desktop", "dedicated-server"],
  "settings": {
    "fixedHz": 60,
    "renderBackend": "webgpu",
    "largeWorldCoordinates": true
  }
}
```

`validateProject` checks the schema id, a non-empty name, an engine string, a UUID, a default world path, at least one known target, a positive `fixedHz`, a render backend of `webgpu` or `webgl2`, and a boolean `largeWorldCoordinates`. The CLI command is `jarvig project validate <file>`. Fixture: `tests/fixtures/minimal.project.json`.

## Scene document

Id: `jarvig.scene/v1`. See [ecs.md](ecs.md). Entities carry `id`, `parentId`, and an ordered component array of `{ name, version, data }`. Data values are finite numbers, strings, or booleans.

The native editor writes a different, explicit pair of documents. ADR-0045. A `*.jarvigproject` file is schema `jarvig.project` format version 1. A `*.jarviglevel` file is schema `jarvig.level` format version 1. The level stores `EntityUuid`, name, parent uuid, and component payloads. It does not store `EntityHandle`, `ObjectId`, subsystem ids, or GPU resources. Builtin mesh and material names are temporary references until AssetIds exist. The TypeScript packages above remain the prototype schema. They are not the bytes `JARVIGEditor` saves. See [entity-identity.md](entity-identity.md).

## Migration

```text
load(version N) -> validate -> migrate N to N+1 ... -> current
-> produce a migration report -> never silently downgrade
```

Phase 0 validates and refuses a version mismatch. It does not migrate. That refusal is intentional.

## Material document

Id: `jarvig.material/v1`. See [../materials/overview.md](../materials/overview.md).
