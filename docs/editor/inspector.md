# Inspector

The inspector is a view of engine inspection data. It is not a second copy of the entity.

```text
SelectionService
    -> EntityUuid
    -> SceneWorld::inspect_entity
    -> InspectorModel
    -> Win32 edits
    -> EngineSession::execute_authoring
    -> SceneWorld
```

Nothing selected shows `No selection.` Several entities show the count and the primary name, and the form stays closed. One entity shows Entity, then Transform when that entity has a frame, then the one component the world says it owns, then a read-only Components section when the stack is not empty. The Components line is the stack in order, such as `Transform, Mesh Renderer`. It does not edit the world. Subsystem ids are not printed there.

- Entity: name (editable), uuid (read-only), parent (read-only, `World` for a root)
- Transform: local location in meters (editable, binary64), local rotation as a quaternion (read-only in this panel)
- The component section from the type registry. A point light shows color, intensity in candela, range, and enabled. A reflection probe shows radius, priority, intensity, enabled, and the read-only capture fields. World Settings shows the environment and has no transform. A mesh shows a read-only surface and material binding. That is not a material editor.

Vector rows use the field name and its registry units. They are not all labeled as location. A number commits as `Float64`. `true` / `false` (also `1`, `0`, `on`, `off`) commits as `Bool`. Unchanged text does not send a command.

Scale is not a frame property, so it is not shown. Bootstrap drawable scale is not an entity field. The Rotate gizmo may still `SetProperty` the quaternion. The inspector does not grow an Euler editor for that. After a drag, this panel writes the new numbers into the existing edits. The gizmo does not write the edit boxes itself. A change to the selection, the component list, or which rows are open rebuilds the controls. A pose change does not. A light or a probe can be selected from the outliner and moved with the gizmo, because those entities have frames. World Settings does not. Viewport picking is still meshes only.

Enter or leaving the field commits one absolute command. Escape puts the text back. A half-typed number such as `-` does not change the world. A non-finite value is rejected and the authoritative text returns. The window does not write `SceneWorld` fields itself.

Hiding the panel does not destroy it, so uncommitted text survives a hide. A structural world change rebuilds the controls from the committed world and drops uncommitted text. A value change updates controls that do not have focus. Multi-object editing, reparenting, and material assignment are later.

See [selection.md](selection.md) and [../api/reflection.md](../api/reflection.md).

Do not grow a hand-built form for every component. The long-term path is the one type registry from ADR-0022:

```text
selected object
    |
    v
TypeId / TypeInfo / FieldInfo
    |
    v
inspector widgets
    |
    v
engine command
```

JRV-0063 is the panel bound to selection. The SDK and the reflection implementation stay deferred with JRV-0056. A bootstrap inspector may show a few known fields before that registry exists. Those fields still go through commands. They are not a second schema.
