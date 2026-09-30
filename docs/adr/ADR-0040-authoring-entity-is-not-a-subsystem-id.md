# ADR-0040 — An authorable world object has one EntityUuid

Status: Accepted
Date: 2026-09-23

## Context

Meshes already had an `EntityUuid`. Lights, the reflection probe, and the environment sky then grew their own handles: `LightId`, `ProbeId`, and the environment record. The outliner still listed only the two triangles. A second editor list for each subsystem would make `LightId` or `ProbeId` the thing the user selects. Those ids are runtime handles. They are not save identity.

ADR-0009 already separates a persistent id from storage. ADR-0033 already says the Perspective camera is not a scene entity. This decision says how the editor, the world, and the renderer share that split once more than meshes are authorable.

## Decision

Every independently placeable world object has one persistent `EntityUuid`. That is the editor, save, and load identity. Selection uses it. The outliner row uses it.

These layers stay different:

```text
EntityUuid                         persistent authoring identity
EntityHandle                       slot index + generation, not serialized
LightId, ProbeId, MeshId,
MaterialInstanceId, FrameId,
ObjectId                           subsystem implementation identity
RenderInstanceId                   extracted rendering identity
```

An authorable entity has a name, a parent uuid, a transform when it is spatial, and component membership. The registry does not store the component payload. `SceneWorld` classifies a handle by which subsystem record owns it. Light data stays in the light list. Probe data stays in the probe list. Mesh data stays on the object. The environment stays the world environment record.

The inspector is a view of the existing type registry (`TypeId`, `TypeInfo`, `FieldInfo`, `PropertyValue`). It is not a second schema. Property edits go through `EngineSession::execute_authoring`. The inspector does not write subsystem structs.

The World Outliner lists authorable entities, not components. The selected entity's components are the inspector sections. Bootstrap rows, in registry order, are Near Triangle, Far Triangle, Directional Light, Blue Point Light, Warm Spot Light, Reflection Probe, and World Settings. The visible `World` root is still not an entity.

World Settings is the owner of the global environment. That environment is not a `LightKind` and it has no transform. It is not a point light and not a directional light.

The Perspective editor camera and the side editor camera stay session state. They are not entities. An authored gameplay `Camera` component can exist later. It is a different object. ADR-0033 stands.

Component types the registry can describe now: Transform, Mesh Renderer, Directional Light, Point Light, Spot Light, Sphere Reflection Probe, and Environment. Camera, Collider, RigidBody, AudioSource, and Script are named as future components. They are not implemented.

`AddComponent`, `RemoveComponent`, `Reparent`, and `CreateEntity` are still later. Duplicate and delete are not registry-only. ADR-0041 makes those two operations keep the subsystem record with the entity.

Extraction still reads `SceneWorld` only. The renderer does not query the entity registry. It receives the same snapshot: meshes, materials, lights, and probes. A dedicated server can own the same entities with no editor, no swapchain, and no GPU.

Moving a light or a probe moves the next extract. A static reflection probe does not recapture because its entity moved or a property changed. Capture stays the renderer policy from ADR-0039.

## Alternatives Considered

- Select `LightId` and `ProbeId` directly. Rejected. The editor would grow a parallel identity for every subsystem.
- Store every component in one struct on the entity. Rejected. Lighting and probes already have their own storage. Membership is enough.
- Put each component in the outliner as its own actor. Rejected. The outliner would stop being a list of placeable objects.
- Make the global environment a directional light. Rejected. It is a hemisphere, not an emitter.
- Make the Perspective camera an entity so it appears under Cameras. Rejected. ADR-0033. Navigation would revise the authoring world.

## Consequences

`docs/architecture/entity-identity.md` is the identity map. `docs/editor/outliner.md` is the row map. The inspector shows the component that belongs to the selected uuid. There are no bitmap icons. The mesh section is two read-only strings, not a material editor. Viewport picking is still mesh-only. A light is selected from the outliner.

Do not turn `RenderInstanceId` into an actor id. Do not expose a GPU handle to the editor. Do not build a second scene graph for the outliner.

## Supersedes

Nothing. ADR-0009 and ADR-0033 still stand. This decision says which id the editor uses once lights and probes are placeable.

## Superseded By

ADR-0041, for the duplicate and delete sentence only. The four identity layers still stand.
