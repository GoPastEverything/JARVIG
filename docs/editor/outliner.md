# World Outliner

JRV-0061. The outliner is a view of the authoritative world. It does not own the world.

```text
SceneWorld
    |
    v
EntityRegistry
    |
    v
EntityOutlineInfo          one hierarchy pass
    |
    v
WorldOutlinerModel         editor view, no HWND
    |
    v
Win32 TreeView             realization only
```

`WorldOutlinerModel` stores entity rows, roots, expansion, and a caret. `HTREEITEM` is not identity. Destroying the control does not change an entity.

## World root

The visible root is `World`. That row is `OutlinerNodeId::WorldRoot`. It is not an entity. It has no `EntityUuid`. The nil UUID is not used as a stand-in: parse rejects version 0, which includes the nil id.

## Identity

An entity row is keyed by `EntityUuid`.

Not by `EntityHandle`, `ObjectId`, a tree item, an array index, or `RenderInstanceId`. The handle is resolved only when a core call needs it. A recycled slot must not reparent or resurrect a row.

Names are the label. They do not have to be unique. An empty name is shown as `Unnamed`. The tree paints a class suffix after the name when the entity has one: `Mesh`, `Light`, `Probe`, or `World`. An entity with no component has no suffix. The suffix is not the row identity. There are no bitmap icons.

A pose edit or a play tick revises the world and does not rebuild the tree. The Win32 items stay. The tree is realized again when an entity is added, removed, renamed, reparented, or changes class, and when Play starts or stops. During play the root label is `Runtime`.

The bootstrap scene, in registry insertion order, is:

```text
World
├── Near Triangle          Mesh
├── Far Triangle           Mesh
├── Directional Light      Light
├── Blue Point Light       Light
├── Warm Spot Light        Light
├── Reflection Probe       Probe
└── World Settings         World
```

`World` is still the editor root, not an entity. World Settings is an entity. It owns the global environment. It is not a light actor and it has no transform. The three lights and the reflection probe are entities. `LightId` and `ProbeId` stay on the subsystem records. The Perspective camera is not a row. ADR-0033 and ADR-0040.

The uuid is not the normal label. The output log prints it once at startup. The tree tooltip shows it. The inspector shows it as a field. That is JRV-0063.

## Hierarchy

Parents come from the registry. The outline reports the parent's uuid when that handle still resolves.

Sibling order is registry insertion order among that parent. It is not hash-map order and not alphabetical. Drag reorder is later.

Deleting a parent does not delete its children. In the same operation each child is reparented to `World`: the stored parent becomes none before the parent slot is retired. A reused slot does not become that parent. ADR-0041. The raw registry retire, used only by registry tests, still leaves a stale handle. `SceneWorld` destroy does not.

A parent cycle is still rejected by the registry. The outliner does not bypass that.

Components are not rows. Selecting Blue Point Light opens that entity in the inspector. The inspector shows the transform and the point-light fields. The outliner does not list Transform or Point Light as children. An authored gameplay camera can be an entity later. The editor Perspective camera is not that camera.

## What a row does not mean

Membership is the registry, not `RenderSceneSnapshot`. An entity with no mesh and no `ObjectId` is still a row. `entity_count` is the registry length. It is not `mesh_count`, not the draw count, and not the snapshot's instance count. Those numbers match only while every entity happens to be a bootstrap drawable.

The render snapshot still copies `EntityUuid` on each instance, and still keeps `ObjectId` as the bootstrap source. Future picking should return the uuid into JRV-0062. It should not return `ObjectId`.

## Editor state

Expansion is a set of `EntityUuid` plus whether `World` is expanded. It is not a component and it is not saved with the scene. JRV-0069 may persist it later. New rows start expanded. A rebuild keeps expansion and the caret when the uuid still exists. A missing caret is cleared. It is not moved onto a different entity.

The tree caret is Win32 focus inside the panel. It is not editor selection. A plain click replaces `SelectionService` with that entity. Ctrl-click toggles it. Clicking `World` clears entity selection. The highlight is drawn from the service, not from the control's single caret. See [selection.md](selection.md). Shift-range is not implemented.

Filtering, when it exists, will hide rows. It will not disable or delete entities. Search is not implemented.

## Refresh

The panel rebuilds when `SceneWorld` revision changes. It does not rebuild every frame. That revision also changes for transforms, visibility, lights, and material bindings, so a non-hierarchy edit can refresh the tree. A separate hierarchy revision is later debt, not a reason to walk the registry every frame.

The refresh does not change `PanelId`, dock position, or the perspective `RenderView`.

## Commands

Delete and Ctrl+D call `EngineSession::execute_authoring` from the outliner and from the perspective view when it has the keyboard. Delete is `DestroyEntity`. Ctrl+D is `DuplicateEntity`. The copy is not selected. World Settings refuses both and says so in the output log. Typing in the inspector does not fire them. The window does not remove a light or a probe from a subsystem list itself. ADR-0041.

## Future save

There is no scene save yet. When one exists, persist `EntityUuid`, name, parent uuid, and components. Do not persist `EntityHandle`. Do not persist `ObjectId` as authoring identity.

## Not this ticket

Selection, the inspector, and the editor camera landed in later tickets and are accepted. Viewport picking and the transform gizmo are JRV-0065. They call this view's selection service. They do not rewrite rows. Drag-reparent, rename editing, context menus, undo, prefabs, and a component-framework rewrite are still later.
