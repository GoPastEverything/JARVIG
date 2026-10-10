# Selection

JRV-0062. The editor has one `SelectionService`. Panels do not keep their own.

```text
Outliner  ----+
Viewport  ----+--> SelectionService --> Inspector summary
Content later +         |
                        +--> status bar, as an observer
```

The service lives on the editor session, on the editor thread. It is not a process global. It is bound to the active authoring world. A future play-in-editor world or a second document must not share it just because two entities have the same uuid bytes. Changing the active world should reconcile or clear selection. There is no world uuid yet.

## What a selection is

An item is `SelectionItem::Entity(EntityUuid)`.

Not an `EntityHandle`, not an `ObjectId`, not a `RenderInstanceId`, and not a tree item. The world root is not an item. The nil uuid is not persistent and is rejected.

Only the entity variant exists. An asset, a component, a material, or a subobject can be added later without pretending those ids exist today. The content browser lists project assets. Those assets are not selection items until a placement creates an entity.

Selection does not mutate the world, does not bump the world revision, does not compile a material, and is not an authoring command. It is not saved in the scene. JRV-0069 may remember it with the workspace. It is not an undo step. Editor undo exists for world edits. Changing which entities are selected stays off that stack. ADR-0068.

The face of a parametric solid is editor state beside this service. The service item is still the entity. Object selection clears that face.

## Several items, one primary

The vector is explicit selection order. Duplicates are removed. The last item is primary: the most recently explicitly selected item that is still in the set.

`replace` makes one item the whole selection. `add` appends a new item and ignores an item that is already there. `toggle` adds or removes. `remove` keeps the order of the rest. If the primary item goes away, the new last item becomes primary. An empty set has no primary.

A plain outliner click replaces. Ctrl-click toggles. Shift-range is not implemented. It depends on the visible row order, and the service does not know that order.

Clicking `World` clears entity selection, with or without Ctrl. The root is not inserted as an entity.

## Caret is not selection

The tree caret is keyboard focus inside the outliner. A click usually moves both. A programmatic change can move selection and leave the caret where it was. Focus can move to the output log and the selection stays. Reset Layout does not clear it. Hiding the outliner or the inspector does not clear it. Reopening either panel reads the service again.

The tree control only has one caret. JARVIG selection does not. Selected rows are drawn from the service. The control's own highlight is not the authority.

## World changes

When the world revision changes, selection is reconciled against the live uuids.

Rename, reparent, and a material or transform edit keep the same uuid selected. Duplicating an entity does not select the copy. Retiring an entity removes that uuid. If it was primary, the previous explicit survivor becomes primary. A new entity that reuses the slot is not selected.

`revision` is the effective semantic selection, not a mathematical set hash. The operations that exist today change the ordered vector whenever membership, order, or primary changes, and that bumps the revision. A future operation that changes only primary or order, without changing which items are members, must bump it too. Panels remember the last revision they drew. A later event list can replace that poll. Callbacks are not wired into window handles.

`last_source` records which peer made the last effective change: outliner, viewport, inspector, command, or programmatic. It is a diagnostic. It does not make one panel more authoritative than another.

## Focus

F in the Perspective view reads the primary entity and frames it. Navigation does not change this service. RMB look does not select. Several selected entities frame the primary only.

## Picking

JRV-0065. The Perspective click uses the same service. The hit is the `EntityUuid` on the visible render instance. It is not an `ObjectId`.

```text
mouse hit
    -> visible RenderInstance
    -> EntityUuid
    -> SelectionService.replace or toggle
    -> outliner highlight and inspector summary
```

A plain click on a parametric solid selects the hit face in Auto and in Face. On an authored block, a point in a fillet strip selects that fillet and a point in a corner shadow selects that corner. The flat interior stays the planar face. Object mode selects the entity and clears the face. A double-click promotes to the owning entity and does not change the mode. On a stored solid, Face, Edge, and Vertex keep one selection set beside the entity: a plain click replaces it, Shift-click adds an element of the same kind, and Ctrl-click toggles one. A different kind or a different solid replaces the set. Every selected element highlights, and the inspector names one id or a count such as `3 Edges selected`. Loop, Ring, Connected, Boundary, Grow, and Shrink replace that set from the authored solid. They do not select triangles or meshlets, and they do not write the body. Select Faces Using Material, on the Surface card, replaces the face set with the faces that use that material slot. Select Group replaces the face set with the faces that still resolve for that named surface group. Neither writes the body, and neither is an undo entry. A group with no resolved face leaves the selection as it was. A walk that cannot continue says so and keeps the last unambiguous element. If Shift and Ctrl are both held, Ctrl wins. An empty click clears unless Shift or Ctrl is held. In Auto and Object, a Select-mode drag on empty space is a marquee of whole objects: left to right takes objects fully inside, and right to left takes objects the rectangle touches. In Face, Edge, and Vertex, the same drag on empty space or on the one primary solid that has a display body selects that mode's authored faces, edges, or vertices. A press on a different solid stays a pick. The element box does not clear the entity selection, and it does not select render triangles, meshlets, Einstein patches, or microgeometry. Four pixels or less stays a click. Ctrl toggles and wins over Shift. Shift adds. Outliner Shift-range is still not implemented. A right-click that stays under four pixels opens a menu of the existing commands for that mode. It does not select. A right-drag past four pixels is still look. MMB and orbit do not select. A click does not bump the world revision. See [picking.md](picking.md). ADR-0068.

## Inspector

JRV-0063 edits name and local translation through engine commands. It follows this service to a uuid. Empty selection still says `No selection.` Several entities do not open an edit form. See [inspector.md](inspector.md).
