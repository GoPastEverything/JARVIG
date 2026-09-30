# Entity identity

An entity has two ids. They are not interchangeable.

```text
EntityUuid          128-bit persistent id
                    survives save, load, and an editor restart
                    references and serialization use this

EntityHandle        slot index + generation
                    process-local
                    stale after the slot is reused
                    not serialized

EntityRegistry
    EntityUuid  ↔  EntityHandle
```

`EntityUuid` is the same text form as the TypeScript entity id (`8-4-4-4-12`). `ObjectId` is still the bootstrap drawable slot. It is not either of these.

The component stack is membership over those records, not a new id. ADR-0048. A mesh actor's stack names Transform and Mesh Renderer and still carries that object's `ObjectId` and `MeshId`. The Perspective camera is not on any stack.

The other ids are not authoring identity either. ADR-0040:

```text
EntityUuid                         persistent authoring identity
EntityHandle                       slot + generation, not serialized
LightId, ProbeId, MeshId,
MaterialInstanceId, FrameId,
ObjectId                           subsystem implementation identity
RenderInstanceId                   extracted rendering identity
```

Hot lookups use the handle. Resolving a handle is one slot read. A wrong generation returns stale. It does not scan UUIDs. The UUID is how the editor, a save file, and a reference name the same entity.

The registry record has a name and a parent handle. It still does not store component payloads. Membership is `SceneWorld`'s classification of the handle: which mesh, light, probe, or world-settings record owns it. `frame_of(handle)` reaches a mesh, a light, a probe, or a frame anchor. World Settings has no frame. A future component store would hang off this id. It is not a second registry, and it is not a second outliner.

A duplicate mints a new UUID and a new handle. It copies the parent, the name, and the subsystem record that owns the source: a new object that shares the mesh, a new `LightId`, or a new `ProbeId`. Retiring an authorable entity removes that record too. The old handle does not resolve, even if a new entity reuses the slot. ADR-0041.

The render snapshot copies the UUID so a frame can name its source. It does not copy the handle into a file format. The id is not in the C header.

Parse accepts the canonical text, versions 1 through 8, variant 8, 9, a, or b. The nil id is version 0 and is rejected. A second insert of the same uuid is rejected. New ids are version 4. The outliner did not change that.

A `.jarviglevel` file stores `EntityUuid`, name, parent uuid, and components. It does not store `EntityHandle` or `ObjectId`. ADR-0045.

Deleting a parent reparents each child to no parent before the slot is retired. `entity_parent` then returns `Ok(None)`, not a stale handle. A scene file can store that. World Settings cannot be duplicated, deleted, or reparented.

The outliner is a view of this registry. `World` is not an entity. Row identity is the uuid. The label is the name, with a class suffix (Mesh, Light, Probe, World) that is not a second id. Components are inspector sections, not outliner rows. Selection is JRV-0062. The inspector is JRV-0063 and reads the same uuid. The Perspective camera is still not an entity. ADR-0033.

Later viewport picking should follow the snapshot uuid, not the bootstrap slot:

```text
pixel or primitive hit
    -> RenderInstance
    -> EntityUuid
    -> SelectionService
    -> outliner and inspector
```

The reverse uuid-to-handle lookup still scans. Hot runtime code does not use it. An index can replace that scan later without changing either id.
