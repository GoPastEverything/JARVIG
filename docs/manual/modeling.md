# Blocks

A Block is the object you authored. The level stores its size in meters, one material, and its transform. When the solid is no longer a plain box it also stores a per-face inset, one bevel, and a short history log. It does not store triangles. The mesh, the clusters, and any debug view are rebuilt from that record.

Object scale stays (1, 1, 1). You change the solid by changing its size, not by scaling the entity. That is why the Scale tool stays disabled.

## Place one

**Create > Block** or the toolbar Block button places a solid.

- The first one in a level sits at scene-local (0, 1, −4) with size (2, 2, 2) m.
- The next ones step 2.5 m on +X.
- The default material is the standard white surface.
- A plain 2 m cube is twelve triangles. Coverage of those triangles matches the box.

**File > New Level** asks for confirmation, then writes an empty level: World Settings only. Replacing the world releases the previous world's render data before the next frame. Save first if you wanted the old level.

## Select a face or the whole solid

Auto, Object, and Face appear in the inspector only while a parametric solid is selected. Edge and Vertex are not offered. The solid does not have those elements as an editing mode yet.

| Mode | A single click |
| --- | --- |
| Auto | The face under the cursor |
| Face | The face under the cursor |
| Object | The whole solid |

Double-click promotes to the whole solid and leaves the mode alone. The whole solid shows Object Tools. A face shows Face Tools.

## Extrude, inset, and bevel

The three tools share one session. The toolbar icon, the Amount field, and one arrow in the viewport are the same number.

1. Select a solid, outside Play, Land, and Character.
2. Click the face you want, unless you are beveling the whole object.
3. Click **Extrude**, **Inset**, or **Bevel**.
4. Pull the arrow, or the face, or type an amount.
5. **Apply** commits. **Cancel** restores the solid to the moment the session opened.

While the session is open:

- The drag plane faces the camera and contains the arrow. Pull along the arrow.
- Inset's arrow points inward. Pulling that arrow increases the inset.
- A drag snaps the change to 0.05 m, then adds it to the amount from when the drag started.
- A typed amount is exact. It is not snapped.
- Extrude may go negative. Inset and bevel stop at zero.
- Bevel and inset also clamp so they stay inside the box, at most 45% of the shortest side.
- Releasing the mouse leaves the preview open. Apply is what writes the level's feature log.
- The Move and Rotate gizmos stay hidden. Choosing Move or Rotate on the toolbar closes the preview without a second gesture. Keys 1–4 do not.
- Escape closes the whole session, including in the middle of a drag, and records no undo entry.
- Clicking a different face of the same solid retargets the face. It does not open a second session on another object.

Apply writes one log line and one editor undo entry. Cancel restores size, insets, bevel, and translation, and it does not append or erase the log.

A face push grows one axis and moves the center by half the change, so the opposite face stays put. Editing the size numbers in the inspector keeps the center where it is.

## Features

The Features list shows the current size and any nonzero bevel or inset. Clicking a row reopens that current amount. It does not replay the solid from scratch.

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

## Collision and materials

Collision is the analytic oriented box of `size`. Half extents are `size / 2`. A bevel or an inset does not shrink the collider to the chamfer. The inspector shows collision as enabled. There is still no physics solver, so nothing falls and nothing is pushed.

A block can take a material. An imported mesh can take a material and does not get Extrude, Inset, or Bevel. A light, a camera, and a player start do not get them either.

## What you should see

A plain cube is a closed box. After a bevel, the silhouette is the chamfer and the status line can report more than twelve triangles. That is the evaluated solid. Diagnostic views under **View > Visualization** recolor the picture. **View > Reset Rendering Debug** returns the ordinary shaded view. Procedural detail stays off for these exact solids. The private rule that would add surface relief is not part of this manual, and exact solids are not submitted to it.

Boolean union, subtract, and intersect are not in this build. Neither are sketch, shell, split, fillet, array, cylinders, ramps, spheres, or wedges. Plane, Ramp, Cylinder, Sphere, and Wedge stay unstarted on the toolbar.
