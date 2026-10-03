# ADR-0066 — A solid keeps its parameters, and the inspector shows what it can do

Status: Accepted
Date: 2026-10-02

## Context

ADR-0062 makes a Block the object. The saved component is `ParametricBlock`. The mesh, the meshlets, and Einstein are derived. ADR-0064 and ADR-0065 then said the level stores that component and the size.

A Block that can only be resized from three inspector numbers is still a primitive actor. The editor needs to change one face, inset a face, chamfer the edges, copy the solid, mirror it, drop it onto the ground, and snap its translation. Those tools have to appear because the entity can do them. A light, a camera, a player spawn, an imported character, and an ordinary imported mesh must not grow a pile of solid-modeling controls because their display name or class was checked.

The same record has to stay parametric. An extrude, an inset, or a bevel must not replace the authored object with a saved triangle soup.

## Decision

`AuthoringCapabilities` is derived from the entity's existing payload inventory. It is not a second entity store, and the inspector does not branch on the display name or on `AuthoringClass::Block` to decide which modeling panels exist. `AuthoringClass::Block` remains the header label.

| Capability | Granted when |
| --- | --- |
| Transformable | the entity has a transform |
| ParametricSolid, FaceEditable, Patternable, Collidable | the entity has a parametric block |
| MaterialAssignable | the entity has a parametric block or a mesh renderer |
| ComponentHost | the entity is not World Settings |
| BooleanOperand | never, in this slice |

A parametric block therefore shows Shape, Modeling, Pattern, Placement, Collision, and Material. Modeling holds the selected face, Reset Shape, Extrude, and Inset. Pattern holds Duplicate, Mirror X, Mirror Y, and Mirror Z. Placement holds Align and Snap. An imported mesh can take a material. It does not advertise ParametricSolid, so it does not get those panels. A light does not either. Components stay the existing Add Component path. Modeling changes the shape. Components are game behavior. This slice does not add a Door, a Hinge, or a Destructible.

The evaluated solid is `size_m`, `inset_m` for the six faces, and one `bevel_m`. Faces are +X, −X, +Y, −Y, +Z, −Z. The derived mesh is rebuilt from those numbers. Inset 0 and bevel 0 use the existing box. A plain record has those zeros and an empty history, and the level writer still emits only size and material. A record that is not plain also writes `inset_m`, `bevel_m`, and `history` inside the same component version and the same level format 6. The file still does not store triangles, meshlets, or Einstein patches.

History is a log of at most 24 edits. Older entries drop off the front. The shown text is the last eight summaries. The mesh is not produced by replaying that log, and an entry cannot be muted. Changing the size rebuilds the surface with the insets and bevel that are already stored, after those features are clamped into the new box. That is non-destructive relative to triangles. It is not a Fusion-style feature tree. Booleans are the next slice, and they are what would make a real operation stack.

A face push grows one axis and moves the center by half the applied change, so the opposite face stays. An inspector edit of Size X, Y, or Z changes the size and leaves the translation. Face drag snaps the outward distance to 0.05 m. The Move gizmo does not. Extrude pushes the selected face 0.25 m, or +X when no face is selected. Inset adds 0.10 m on that face per click, and the six fields set the distances directly. Bevel is one amount, clamped to 45% of the shortest side. Reset Shape returns a 2 m plain cube and keeps the material and the translation.

Duplicate is the existing duplicate command. Mirror makes an independent copy, shifts it so the two solids share a face, swaps the copy's insets on that axis, and logs Mirror on the copy only. It is not a live instance and it is not a reflected quaternion. Align drops the lowest corner onto scene Y = 0. Snap rounds local translation to 0.1 m. The entity origin stays the geometric center, so the Origin field reads Center and there is no separate Center Pivot command.

Face selection and the face handles live in the editor. They are not level fields and they are not scene entities. The handles draw in Select, Move, and Rotate for a FaceEditable selection while the editor is not playing. Gizmo handles win, then face handles, then scene triangles. A handle drag pushes the face. A click on the mesh selects the face and does not start that drag. Escape restores the drag's size, insets, bevel, and translation and does not append or erase the log. Commit appends one entry. The overlay is lines. Dimension text is the inspector fields.

A plain 2 m cube stays 12 triangles, 24 vertices, 36 indices, and 12 meshlets with exact coverage. The 8-cell meshlet grid is not retuned. A featured surface is a different derived mesh. It is not sent through that 12-triangle audit. It stays inside the analytic box. Collision remains that box of `size`, so a bevel or an inset does not shrink the collider. An authored block stays Exact. Einstein, Leaf Truth, visibility, and microgeometry are unchanged. This slice does not add a Geometry Quality cockpit or new Einstein controls.

The type registry version is 9. Origin, history, collision, and collision-enabled are read-only. Collision-enabled is the string Yes, not a checkbox. Bevel and the six insets are editable numbers.

## Alternatives Considered

- `if entity == Block` in the inspector. Rejected. The next solid would copy that branch, and a renamed mesh could grow tools it cannot perform.
- Bake each edit into triangles and store the mesh. Rejected. ADR-0062. The object would stop being a solid.
- Replay history as a feature tree, including mute and reorder. Rejected for this slice. The evaluated numbers are the solid. A tree that can mute an intermediate boolean belongs with booleans, which are not implemented.
- Draw editable dimension text on the cube. Rejected for this slice. The overlay vertex is a line. The numbers are the inspector fields.
- Snap the Move gizmo to 0.1 m. Rejected. The editor drag test moves X by 1 cm and expects that motion. Position snap is the Snap command. Rotation snap, surface snap, and vertex snap are not this slice.
- Shrink the analytic box to the chamfer. Rejected. Collision stays the oriented box of `size`. The visible chamfer is inside it.
- Turn Exact off when a bevel adds triangles. Rejected. This slice does not retune Einstein. An authored block still draws no relief.

## Consequences

Select, Move, and Rotate stay the object tools. Scale stays disabled. A selected block can be edited by face drag and by the Modeling, Pattern, and Placement commands above. A plain cube round-trips as before and still measures 12 meshlets. Boolean union, subtract, and intersect are the next slice. Sketch, then extrude, revolve, sweep, and loft, come after that. The construction library is not in this decision. No JRV ticket is accepted. RFC-0002 stays unstamped. The open editor does not contain the face handles until it is closed and rebuilt.

## Supersedes

These sentences, and only these:

- ADR-0062's "the saved file stores the size, not a triangle list," and its statement that the visible mesh is always a 12-triangle box rebuilt when the size changes. The file stores the parametric record. A plain solid is still that box. A solid with inset or bevel derives a different mesh and still stores no triangle list. ADR-0062 still says the object is the solid.
- ADR-0064's "the level file stores `ParametricBlock` and the size." The level stores the record described above. ADR-0064 still owns derived clusters and the unchanged meshlet builder. A plain 2 m cube is still 12 meshlets.
- ADR-0065's "the level still stores only `ParametricBlock` and the size," and the claim that every size change rebuilds the same 12-triangle surface. That claim holds while inset and bevel stay zero. ADR-0065 still classifies an authored block as Exact and still draws no relief.

Nothing in ADR-0063 is superseded. An imported miss still means zero candidates.
