# ADR-0045 — A project and a level are documents, not GPU state

Status: Accepted
Date: 2026-09-23

## Context

The editor could author a world, but that world existed because startup code built it. Closing the process discarded every move. Saving a render snapshot, a `MeshId`, or a cubemap would freeze process-local handles and make the renderer part of the game.

ADR-0040 already says `EntityUuid` is the save identity. ADR-0026 says a render snapshot is not a world. ADR-0033 says the editor camera is not a scene entity. This decision says what file those rules produce.

## Decision

A project file (`*.jarvigproject`, schema `jarvig.project`, format version 1) names the project and the relative locations of content, config, saved data, and the startup level. Those paths are not asset identities. Absolute paths are rejected.

A level file (`*.jarviglevel`, schema `jarvig.level`, format version 1) is the authored world. It stores entity uuid, name, parent uuid, local binary64 transform, and component payloads: Transform, MeshRenderer, DirectionalLight, PointLight, SpotLight, ReflectionProbe, and WorldSettings. The loader turns those records into the existing `SceneWorld` subsystem records. It does not introduce a second entity store.

The file does not store `EntityHandle`, `ObjectId`, `RenderInstanceId`, `FrameId`, `LightId`, `ProbeId`, `MeshId`, `MaterialInstanceId`, GPU handles, or captured cubemaps. Those are recreated after load. A newer `format_version` is refused. There is no silent migration.

Mesh and material references use `scheme: jarvig.builtin` and a name until an asset database exists. `scheme: jarvig.asset` is reserved for a future AssetId and is rejected rather than guessed. That is the migration path for JRV-0066. It is not the asset database.

Saves write a temporary file, fsync it, copy the previous file to `Saved/Backup`, then replace. A failed save leaves the previous level intact. Autosave writes `Saved/Autosaves` and does not overwrite the authored level. Editor camera and dock layout are not level truth. A viewport pose may live in `Saved/Editor`.

`jarvig_core` parses and instantiates a level with no window and no GPU. The editor, a future player, and the dedicated server load the same document. The server still does not extract or enumerate a GPU.

## Consequences

The Lighting Lab sample at `samples/lighting-lab` is the first real level, the serialization regression, and the permanent renderer torture-test. Do not regenerate or replace that file when a rendering feature lands. New lighting, materials, shadows, and later geometry are judged on the saved world. `--self-test` still builds the two-triangle bootstrap in memory and does not open that project.

The outliner, inspector, and gizmos keep editing `SceneWorld`. The renderer keeps consuming snapshots. Neither reads the JSON.

## Alternatives considered

- Deriving a binary blob from the live registry. Rejected. A refactor of slot layout would invalidate every save.
- Saving the TypeScript `jarvig.scene/v1` package as the native file. Rejected for this milestone. That package remains the prototype schema. The native document is explicit version 1 and does not pretend to be that package.
- Putting the editor camera in the level so the view restores. Rejected. ADR-0033. Optional editor state is a different file.
