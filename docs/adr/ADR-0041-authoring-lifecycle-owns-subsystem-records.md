# ADR-0041 — Duplicating or deleting an entity owns its subsystem record

Status: Accepted
Date: 2026-09-23

## Context

ADR-0040 gave every placeable object one `EntityUuid`, and the outliner lists meshes, lights, the reflection probe, and World Settings as actors. Duplicate and delete still changed only the registry. A deleted light could keep its `LightId` and keep lighting the scene. A duplicated probe was a new uuid with no probe. That is not a coherent authoring world.

The registry also left a child's parent handle stale when the parent was retired. The outline hid that by failing the lookup. A future save would have written the stale handle.

## Decision

`EntityUuid` stays the authoring identity. Subsystem ids stay subsystem ids. Lifecycle is the operation that keeps them together.

`SceneWorld::entity_ownership` is the one composition query. It reports transform, mesh renderer, directional/point/spot light, reflection probe, and world settings, plus the `ObjectId`, `MeshId`, `LightId`, or `ProbeId` when that record owns the entity. The outliner class, the inspector section, duplicate, and delete all use it. Payloads stay in the subsystem lists. This is not a second entity store.

Duplicate and delete go through `EngineSession::execute_authoring`:

```text
DuplicateEntity { target }
DestroyEntity   { target }
```

Failures are `NotFound`, `Unsupported`, `ProtectedEntity`, and `InvalidOperation`.

Duplicate mints a new `EntityUuid` and a new `EntityHandle`. It copies the local pose into a new frame.

- A mesh shares `MeshId` and the bound material-instance ids. It gets a new object record. The transform is independent.
- A light gets a new `LightId` and copied kind, color, intensity, range, cones, and enabled flag.
- A reflection probe gets a new `ProbeId` and copied radius, priority, intensity, and enabled flag. It does not receive the source probe's cubemap. The renderer captures that probe later, into its own cube.
- An empty entity copies name and parent only.
- More than one payload on the same entity is `Unsupported`. The bootstrap actors have one.

Delete removes the subsystem record and then retires the entity. The mesh asset and the material instances stay. Extract no longer emits the deleted object, light, or probe. The editor does not destroy a GPU object. The next snapshot is missing that probe id, and the renderer destroys that probe's cube before any bind group can keep a view of it. One probe does not share a cube with another.

World Settings is a singleton authoring entity. It is selectable, inspectable, and persistent. It is not duplicable, not deletable, and not reparentable. It has no transform and no gizmo. Those commands return `ProtectedEntity`. The visible `World` row remains an editor root, not an entity.

Deleting a parent does not delete the children. Each surviving child is reparented to World in the same operation: the stored parent handle becomes none before the parent slot is retired. A reused slot cannot become that parent.

The Perspective camera is still not an entity. ADR-0033. The renderer still does not receive the entity registry. ADR-0026. A dedicated server owns the same entities and does not gain a GPU.

## Alternatives Considered

- Leave duplicate and delete as registry-only until a scene file exists. Rejected. The outliner already presents these rows as actors.
- Cascade-delete children. Rejected for this milestone. Reparenting to World keeps the child's uuid and does not surprise the user by deleting a light that was parented under a mesh.
- Clone the source probe's cubemap into the copy. Rejected. That shares mutable capture residency and clones a GPU resource the editor should not name.
- Make World Settings a non-entity global. Rejected. ADR-0040 already gave it an `EntityUuid`. The singleton rules are the protection, not a second identity.

## Consequences

Delete and Ctrl+D work from the outliner and from the perspective view. They do not fire while an inspector field has focus. Neither selects the copy. A protected entity reports the failure in the output log. Frames of deleted actors are not recycled. They are not extracted.

`docs/editor/outliner.md` and `docs/architecture/entity-identity.md` record the parent rule. The open debt "Normalize a retired parent's children" is closed here.

## Supersedes

The ADR-0040 sentence that duplicate and retire touch only the registry. The identity split in ADR-0040 still stands.

## Superseded By
