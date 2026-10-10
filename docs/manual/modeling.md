# Blocks

A Block is the object you authored. **Create > Block** writes an authored seed: the 2 m cube, one seed operation, and no stored body. Later Round, Extrude, and Split append to that tape. Save writes the tape. The file keeps no body and no triangles. The mesh on screen is rebuilt from the tape.

A solid that already stores its own shape is the other kind. That includes an older size box, **Create > Plane**, and any block whose file already has faces, edges, and vertices. The level stores its size in meters, one material, and its transform. When that solid is no longer a plain box it also stores a per-face inset, one bevel, and a short history log. After an edge, vertex, or subdivision edit it also stores the faces, edges, and vertices, each with an id that survives later edits. It does not store triangles. The mesh, the clusters, and any debug view are rebuilt from that record. Selected faces can use another material slot. Slot 0 stays the object material. In Face mode the Surface card names that slot. Make Unique copies it onto the selected faces, and a color edit of a shared slot does that before it changes the color. Assign Existing uses a slot that is already there. Select Faces Using Material finds that region again. Create Group names the selected faces, for example Upper Rim, and keeps that name across a subdivide and a save. Select Group finds the faces that still resolve. Delete Group removes the name and leaves the faces.

Object scale stays (1, 1, 1). You change the solid by changing its size, not by scaling the entity. That is why the Scale tool stays disabled.

## Place one

**Create > Block** or the toolbar Block button places an authored block. **Create > Plane** places a stored solid, 2 m by 2 m and 0.05 m thick, so the sheet stays closed. The toolbar does not have a Plane button.

- The first one in a level sits at scene-local (0, 1, −4). A block is (2, 2, 2) m. A plane is (2, 0.05, 2) m.
- The next solid, block or plane, steps 2.5 m on +X.
- The default material is the standard white surface.
- A plain 2 m cube is twelve triangles. Coverage of those triangles matches the box.

**File > New Level** asks for confirmation, then writes an empty level: World Settings only. Replacing the world releases the previous world's render data before the next frame. Save first if you wanted the old level.

## An authored block

Create > Block is this object. Switch Select to Face and click a face. The outline is that face. The inspector names it `F:seed/0` through `F:seed/5`. Auto does the same. Orbiting leaves the selection on that face. The short segments that draw a curve are not edges you can select.

**Extrude** appends one extrude of the faces you have selected. Pull the arrow and release. The cap keeps its face name. Each new side is `F:side(...)`. A new outer edge is `E:outer(...)`. Click a side and extrude it again. The tape grows by one entry, and the level still has no body. Adjacent faces that share an edge come out together. Two faces that only touch at a corner stay put, and the log says "Those faces do not form one region."

**Round** writes the curve. In Edge mode, select one edge, or Ctrl-click several, and click Round. Object mode on an authored block opens Round on every sharp edge. The card shows Selection, Resolved semantic edges, and Status: Valid, Clamped, or Conflict. A handle on each selected fillet arc drags one shared radius. The Radius field follows. Releasing the mouse leaves the preview. **Apply** writes that set as one undo entry. The log says `Applied Round on {tokens}.` Cancel, Escape, and a right-click record nothing.

One edge rounds by itself. Two edges that share a vertex close together. Three equal-radius edges that meet at one convex corner close as one corner patch. The twelve edges of a fresh box, at one radius, close as eight of those patches. A corner the radius cannot close stays sharp. The note names the edges, Apply writes nothing, and the log says `Round was not applied: {note}.` More than three fillets at one vertex is that refusal.

The fillet face is `F:fillet(...)`. Its tangent boundaries are `E:fillet-a(...)` and `E:fillet-b(...)`. The corner patch is `F:corner(...)`. A click in the fillet strip selects the fillet. A click in the corner shadow selects the corner. The flat middle of a face stays that face. The outline of a selected fillet is the radius-wide surface. Selecting that fillet and opening Round reopens the round's edges at the saved radius.

Extrude a face that is still flat. The rounds stay, and they follow the new silhouette. The old edge stays the flat seam at the base of a wall. Extrude of a fillet face or a corner patch is refused. The log says `UnsupportedOperation: Extrude does not edit a curved face.` and the shape stays.

These stay off on an authored block, and each button names the reason: Inset, Bevel, Subdivide, Move Edge, Extrude Edge, Move Vertex, Reset Shape, and Mirror. The sentence is `{Tool} is not available on an authored block.` Split of an edge that already carries a round is refused and writes nothing. Split of an edge that does not is one more tape entry, and the piece stays in the same object. Turning that piece into its own object is not in this build.

Round on a solid that stores its own shape says `Round edits an authored block. This object stores its own shape.` Round on a saved size box says `This block is a saved size box. Create Block makes an authored block.` Extrude on a stored shape still uses the tools in the next sections.

Collision stays the analytic box of the size. A round does not shrink that box. Sphere, Cylinder, Torus, Capsule, Cone, Revolve, and Sweep are not in this build.

## Select a face, an edge, a vertex, or the whole solid

Auto, Object, Face, Edge, and Vertex appear in the inspector only while a parametric solid is selected.

| Mode | A single click | What you see |
| --- | --- | --- |
| Auto | The face under the cursor | Every selected face outlined |
| Face | The face under the cursor | Every selected face outlined |
| Object | The whole solid | The box outline |
| Edge | The edge under the cursor | Every selected edge drawn thick |
| Vertex | The vertex under the cursor | Every selected vertex marked |

Double-click promotes to the whole solid and leaves the mode alone. The inspector Select row is Auto, Object, Face, Edge, and Vert, then Loop, Ring, Connected, Boundary, Grow, and Shrink. Model changes with that mode. Object shows Bevel, Reset Shape, Duplicate, Mirror, Align, and Snap. A face shows Extrude, Inset, Bevel, Subdivide, and 4×4. An edge shows Move Edge, Extrude Edge, Split Edge, and Bevel. A plain click replaces the current faces, edges, or vertices. Shift-click adds another of the same kind on the same solid. Ctrl-click toggles one, and Ctrl wins. Loop and Ring start from the selected edges and stop where the next edge is not unique. Connected selects the whole piece. Boundary selects the outer edges of the selected faces. Grow adds the next adjacent faces, edges, or vertices, and Shrink takes that layer back off. Bevel still uses every selected edge as one operation. A vertex shows Move. One selected face shows its id, area, and normal above those tiles. One edge shows its id and length. Several selected elements all highlight, and the inspector counts them, for example `3 Edges selected`, instead of one id with its length or area. Move Edge, Extrude Edge, and Split Edge still ask for one edge. Move still asks for one vertex. Geometry lists faces, edges, vertices, and whether the solid is closed. Show Grid is in Geometry once the solid has stored faces, and it is the same switch as View > Show Grid. The inspector names a stored face as `F:`, an edge as `E:`, and a vertex as `V:`. Those names are the solid's own ids. On an authored block the names come from the tape: `F:seed/0` through `F:seed/5`, `E:seed-edge/0` through `E:seed-edge/11`, `F:side(...)`, `F:fillet(...)`, and `F:corner(...)`.

## Extrude, inset, and bevel

The three tools below are for a solid that stores its own shape. On an authored block, Inset and Bevel say they are not available, and Extrude is the authored-block section above. The three tools share one session. The toolbar icon, the Amount field, and one arrow in the viewport are the same number.

1. Select a solid, outside Play, Land, and Character.
2. Click the face you want, unless you are beveling the whole object.
3. Click **Extrude**, **Inset**, or **Bevel**.
4. Pull the arrow, or the face, or type an amount.
5. Releasing the mouse commits a drag. **Done** commits a typed amount. **Cancel** restores the solid to the moment the session opened. The amount, Cancel, and Done are the Active Tool card. That card closes when the operation finishes.

While the session is open:

- The drag plane faces the camera and contains the arrow. Pull along the arrow.
- Inset's arrow points inward. Pulling that arrow increases the inset.
- A drag snaps the change to 0.05 m, then adds it to the amount from when the drag started.
- A typed amount is exact. It is not snapped.
- Extrude may go negative. Inset and bevel stop at zero.
- Bevel and inset also clamp so they stay inside the box, at most 45% of the shortest side.
- Releasing the mouse commits the drag and writes one line in the feature log. Apply does that for a typed amount.
- The Move and Rotate gizmos stay hidden. Choosing Move or Rotate on the toolbar cancels the preview. Keys 1–4 do not.
- Escape, and a right-click during the drag, close the whole session and record no undo entry.
- Clicking a different face of the same solid retargets the face. It does not open a second session on another object.

A committed drag writes one log line and one editor undo entry. Apply does the same for a typed amount. Cancel restores size, insets, bevel, and translation, and it does not append or erase the log.

A face push grows one axis and moves the center by half the change, so the opposite face stays put. Editing the size numbers in the inspector keeps the center where it is.

## Features

On a stored body, the Features list shows the current size, any nonzero bevel or inset, and the latest topology edits. An authored block keeps its construction in the saved tape. This list does not replace that tape. Clicking the size, the bevel, or an inset reopens that current amount. A topology row is a label. It does not replay the solid from scratch.

On a stored body, the log keeps the last 24 edits and drops the oldest. An entry cannot be muted, duplicated, or moved earlier. Extrude is already baked into the size. A later edit of the original 2 m cube does not reapply a chain of operations. A regenerating construction history would be a later decision. This manual does not describe Features as that timeline.

## Object tools

With the whole solid selected:

| Tool | Result |
| --- | --- |
| Bevel | The same session, on the solid |
| Reset Shape | Returns a plain box |
| Duplicate | An independent copy. Ctrl+D |
| Mirror | An independent copy that shares a face with the original, on X, Y, or Z |
| Align | Drops the lowest corner onto scene Y = 0 |
| Snap | Rounds local translation to 0.1 m. The Move gizmo itself does not snap |

## Edges, vertices, and subdivision

These tools edit a solid that stores its own shape. On an authored block they say they are not available. Move Edge and Extrude Edge are different tools.

**Move Edge** slides the selected edge and keeps it connected to the same faces. Dragging the top edge of a side inward turns that rectangle into a trapezoid. The solid stays closed. A drag that would flatten a face, shrink an edge to nothing, or fold the edge through the solid is refused, and the last valid shape stays.

**Extrude Edge** builds a new edge and a new face between that edge and the one you selected. The faces beside the original edge gain the new vertices, so the solid does not open a crack.

**Move**, on a vertex, moves that one point the same way Move Edge moves two.

**Split Edge** puts a new vertex at the middle of the edge. **Subdivide** replaces a rectangular face with a 2 by 2 grid of faces you can select. **4×4** does the same with sixteen faces. A rectangle whose edges were already split by a neighbor is still a quad: those vertices stay, and the grid is fine enough to include them. The toolbar Subdiv stays 2 by 2. A selected face is the one that is divided. A selected edge or vertex uses a rectangular face of that element, the one facing the camera. No element selected uses +X, and the editor says so. A face that is not a rectangle is left alone, and the log names its corner count. The original face id stays on one corner cell. From Edge, Vertex, or Object, Subdivide switches to Face so the next click selects a face. Click another cell to edit that cell. Shift-click adds cells. Ctrl-click removes one.

The same tools are on the toolbar while a solid is selected: **Subdiv**, **Edge**, **Extend**, **Split**, and **Vertex**. Subdiv and Split run when you click them. Edge, Extend, and Vertex arm a drag.

Select one edge or one vertex first. With Move active, the red, green, and blue arrows sit on that element. They stay off while more than one edge or vertex is selected. Pull an arrow the way you move a whole object. Releasing the mouse commits. Extend pulls a new face out along that arrow. You can also press in the view and drag, the way Extrude drags its arrow. Escape or a right-click during the drag restores the solid and records nothing. Ctrl+Z undoes the committed drag.

Clear a uniform bevel and any inset before an edge edit. After the solid stores its faces and edges, Inset and a bevel of every edge ask you to use Reset Shape to return to the box. Bevel in Edge mode is one operation on every selected edge. One edge becomes a new strip, and the faces beside it change so the solid stays closed. A straight run that was already split is beveled together, and the log says so. Edges that share a vertex are solved together, so a corner joins instead of two strips crossing. Two edges miter. Three or more that meet at one vertex share the point where their chamfer planes meet. Pull the arrow, or type a Width. The drag snaps by 0.05 m. A typed width is exact. If the solid cannot hold that width but a smaller inset still closes, the inset stops there, and Width shows that distance. A set that cannot close at any width does not open the arrow, and the solid you had stays. One edge logs "Cannot bevel this edge." Several log "Cannot bevel these edges." Releasing the mouse commits one undo entry for the whole set. Cancel, Escape, and a right-click restore the solid and record nothing. A width of zero commits nothing. Extrude pulls the faces you have selected as one region. Adjacent cells that share an edge come out together: the seam between them stays a seam, and the new walls are only the outside boundary. Two cells that only touch at a corner do not extrude, and the log says "Those faces do not form one region." One interior cell of a 4×4 top can still come up on its own. The cap you pulled, and the new side faces around it, can be selected and extruded again. The solid stays closed. Two faces that meet still share one edge, so moving that edge moves both.

**View > Show Grid** draws every stored edge, including while the whole object is selected. Face, Edge, and Vertex draw those edges on their own. Object mode stays a clean box until you turn the grid on. The lines are the editable cage. They are not the dense surface the renderer may draw later. The cage draws every edge, including after many subdivisions.

Face and Auto on a stored body do not cover the cells with the object Move arrows. Switch to Object to move the whole solid. While an Extrude arrow is up, a click drags that region or does nothing. Escape, then click, selects a different cell. A click that misses the shaded triangle can still select the cell the ray enters.

Loop cut, knife, dissolve, connect, and a separate chamfer are not in this build.

## Collision and materials

Collision is the analytic oriented box of `size`. Half extents are `size / 2`. A bevel, an inset, or a slanted face does not shrink the collider to the chamfer or to the triangles. The inspector shows collision as enabled. There is still no physics solver, so nothing falls and nothing is pushed.

A block can take a material. An imported mesh can take a material and does not get Extrude, Inset, or Bevel. A light, a camera, and a player start do not get them either.

## What you should see

A plain cube is a closed box. After a bevel, the silhouette is the chamfer and the status line can report more than twelve triangles. That is the evaluated solid. Diagnostic views under **View > Visualization** recolor the picture. **View > Reset Rendering Debug** returns the ordinary shaded view. Procedural detail stays off for these exact solids. The private rule that would add surface relief is not part of this manual, and exact solids are not submitted to it.

Boolean union, subtract, and intersect are not in this build. Neither are sketch, shell, loop cut, knife, array, cylinders, ramps, spheres, or wedges. Ramp, Cylinder, Sphere, and Wedge stay unstarted. Plane is Create > Plane, on a stored solid. Round is the curve on an authored block, in the section above. Split Edge and Subdivide Face are the topology tools for a stored body.
