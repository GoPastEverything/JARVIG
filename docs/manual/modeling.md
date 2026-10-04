# Blocks

A Block is the object you authored. The level stores its size in meters, one material, and its transform. When the solid is no longer a plain box it also stores a per-face inset, one bevel, and a short history log. After an edge, vertex, or subdivision edit it also stores the faces, edges, and vertices, each with an id that survives later edits. It does not store triangles. The mesh, the clusters, and any debug view are rebuilt from that record.

Object scale stays (1, 1, 1). You change the solid by changing its size, not by scaling the entity. That is why the Scale tool stays disabled.

## Place one

**Create > Block** or the toolbar Block button places a solid.

- The first one in a level sits at scene-local (0, 1, −4) with size (2, 2, 2) m.
- The next ones step 2.5 m on +X.
- The default material is the standard white surface.
- A plain 2 m cube is twelve triangles. Coverage of those triangles matches the box.

**File > New Level** asks for confirmation, then writes an empty level: World Settings only. Replacing the world releases the previous world's render data before the next frame. Save first if you wanted the old level.

## Select a face, an edge, a vertex, or the whole solid

Auto, Object, Face, Edge, and Vertex appear in the inspector only while a parametric solid is selected.

| Mode | A single click | What you see |
| --- | --- | --- |
| Auto | The face under the cursor | That face outlined |
| Face | The face under the cursor | That face outlined |
| Object | The whole solid | The box outline |
| Edge | The edge under the cursor | That edge drawn thick |
| Vertex | The vertex under the cursor | That vertex marked |

Double-click promotes to the whole solid and leaves the mode alone. The whole solid shows Object Tools. A face shows Face Tools: Extrude, Inset, Bevel, Subdivide, and 4×4. An edge shows Edge Tools: Move Edge, Extrude Edge, and Split Edge. A vertex shows Move. The inspector names a stored face as `F:`, an edge as `E:`, and a vertex as `V:`. Those names are the solid's own ids.

## Extrude, inset, and bevel

The three tools share one session. The toolbar icon, the Amount field, and one arrow in the viewport are the same number.

1. Select a solid, outside Play, Land, and Character.
2. Click the face you want, unless you are beveling the whole object.
3. Click **Extrude**, **Inset**, or **Bevel**.
4. Pull the arrow, or the face, or type an amount.
5. Releasing the mouse commits a drag. **Apply** commits a typed amount. **Cancel** restores the solid to the moment the session opened.

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

The Features list shows the current size, any nonzero bevel or inset, and the latest topology edits. Clicking the size, the bevel, or an inset reopens that current amount. A topology row is a label. It does not replay the solid from scratch.

The log keeps the last 24 edits and drops the oldest. An entry cannot be muted, duplicated, or moved earlier. Extrude is already baked into the size. A later edit of the original 2 m cube does not reapply a chain of operations. A regenerating construction history would be a later decision. This manual does not describe Features as that timeline.

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

Move Edge and Extrude Edge are different tools.

**Move Edge** slides the selected edge and keeps it connected to the same faces. Dragging the top edge of a side inward turns that rectangle into a trapezoid. The solid stays closed. A drag that would flatten a face, shrink an edge to nothing, or fold the edge through the solid is refused, and the last valid shape stays.

**Extrude Edge** builds a new edge and a new face between that edge and the one you selected. The faces beside the original edge gain the new vertices, so the solid does not open a crack.

**Move**, on a vertex, moves that one point the same way Move Edge moves two.

**Split Edge** puts a new vertex at the middle of the edge. **Subdivide**, on a four-sided face, replaces that face with a 2 by 2 grid of faces you can select. **4×4** does the same with sixteen faces. The toolbar Subdiv stays 2 by 2. No face selected uses +X, and the editor says so. A face that is not a quad is left alone. The original face id stays on one corner cell. Click another cell to edit that cell. Shift-click adds cells. Ctrl-click removes one.

The same tools are on the toolbar while a solid is selected: **Subdiv**, **Edge**, **Extend**, **Split**, and **Vertex**. Subdiv and Split run when you click them. Edge, Extend, and Vertex arm a drag.

Select the edge or vertex first. With Move active, the red, green, and blue arrows sit on that element. Pull an arrow the way you move a whole object. Releasing the mouse commits. Extend pulls a new face out along that arrow. You can also press in the view and drag, the way Extrude drags its arrow. Escape or a right-click during the drag restores the solid and records nothing. Ctrl+Z undoes the committed drag.

Clear a bevel and any inset before an edge edit. After the solid stores its faces and edges, Inset and Bevel ask you to use Reset Shape to return to the box. Extrude pulls the faces you have selected. One interior cell of a 4×4 top can come up on its own. The cap you pulled, and the new side faces around it, can be selected and extruded again. The solid stays closed. Two faces that meet still share one edge, so moving that edge moves both.

**View > Show Grid** draws every stored edge, including while the whole object is selected. Face, Edge, and Vertex draw those edges on their own. Object mode stays a clean box until you turn the grid on. The lines are the editable cage. They are not the dense surface the renderer may draw later. The cage draws every edge, including after many subdivisions.

Face and Auto on a stored body do not cover the cells with the object Move arrows. Switch to Object to move the whole solid. While an Extrude arrow is up, a click drags that region or does nothing. Escape, then click, selects a different cell. A click that misses the shaded triangle can still select the cell the ray enters.

Loop cut, knife, dissolve, connect, and a separate chamfer are not in this build.

## Collision and materials

Collision is the analytic oriented box of `size`. Half extents are `size / 2`. A bevel, an inset, or a slanted face does not shrink the collider to the chamfer or to the triangles. The inspector shows collision as enabled. There is still no physics solver, so nothing falls and nothing is pushed.

A block can take a material. An imported mesh can take a material and does not get Extrude, Inset, or Bevel. A light, a camera, and a player start do not get them either.

## What you should see

A plain cube is a closed box. After a bevel, the silhouette is the chamfer and the status line can report more than twelve triangles. That is the evaluated solid. Diagnostic views under **View > Visualization** recolor the picture. **View > Reset Rendering Debug** returns the ordinary shaded view. Procedural detail stays off for these exact solids. The private rule that would add surface relief is not part of this manual, and exact solids are not submitted to it.

Boolean union, subtract, and intersect are not in this build. Neither are sketch, shell, fillet, loop cut, knife, array, cylinders, ramps, spheres, or wedges. Plane, Ramp, Cylinder, Sphere, and Wedge stay unstarted on the toolbar. Split Edge and Subdivide Face are the topology tools above.
