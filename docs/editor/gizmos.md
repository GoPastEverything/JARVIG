# Transform gizmo

JRV-0065. The gizmo is editor overlay state. It is not a scene entity, not an `EntityUuid`, not an `ObjectId`, not a gameplay object, and not something the world saves.

```text
tool mode + primary selection
        |
        v
gizmo pose and handle
        |
        v
EditorOverlay vertices, camera-relative float32
        |
        v
overlay pass after the scene
```

A drag does not write a GPU matrix. It sends an absolute authoring command. The next extract moves the object.

## Tool mode

`EditorToolMode` is the editor session. It is not `SceneWorld`.

| Tool | What it does |
| --- | --- |
| Select | Picking only. No gizmo. |
| Translate | Red X, green Y, blue Z axes. |
| Rotate | Rings on those axes. The value is a quaternion. |
| Scale | Visible, labeled, and disabled. A spatial frame stores translation and a quaternion, not scale. |

The toolbar caption under Translate is Move. Scale stays on that toolbar and stays dim.

Scale does not invent an editor-only scale. The button stays dim. Choosing it logs that scale is unavailable. Bootstrap object scale is not an entity property.

World and Local are real. World axes are the reference frame. Local axes are the selected entity's orientation. The toolbar Local button toggles them and is accented in Local. The high-precision root is not a float32 global space.

With more than one entity selected, the gizmo sits on the primary and the drag changes only the primary. It does not pretend the others move.

No spatial frame, or no selection, draws no gizmo.

## Screen size and depth

The drawn length is about 90 pixels. It comes from camera distance, vertical field of view, and viewport height, and it is clamped. That scale is visual. It is not entity scale.

The overlay pass has no depth test, so the gizmo stays visible inside geometry. Object materials are not edited to draw it. Reversed-Z is unchanged for the scene pass.

Axis colors are the usual ones: X red, Y green, Z blue. Hover brightens a handle. The active drag is drawn stronger. The other handles stay visible. Plane handles (XY, XZ, YZ) are not in this pass.

## Hit order

When Translate or Rotate is active, the ray tests gizmo handles before scene triangles. A press on a handle starts a manipulation. A miss falls through to picking. Select mode does not hit-test the gizmo. Once a body is stored, Auto and Face do not draw the object Move or Rotate gizmo over the cells. Object mode still draws that gizmo. Edge and Vertex still carry the element gizmo. During an Extrude, Inset, or Bevel session the operation arrow is tested after the gizmo and before scene triangles. A region-extrude press that misses the arrow does not change the selected faces.

## Face outline

A parametric block outlines the face under the cursor, and the face already selected, while that entity is selected and the editor is not playing. Land mode and the character workspace do not draw it. Axis colors match the gizmo: X red, Y green, Z blue. Hover blends toward white. The vertices are camera-relative lines in the same overlay that draws the gizmo. They are not scene entities, they are not saved, and they carry no dimension text. The live number is the inspector Amount field.

A click on the mesh selects the face and does not start a drag. A double-click selects the whole block. Opening Extrude, Inset, or Bevel draws one arrow on the session face. Inset's arrow points inward. Dragging that arrow, or pressing the session face and dragging, snaps the delta to 0.05 m and previews from the session baseline. The drag plane faces the camera and contains the arrow, so pulling along the arrow changes the amount. A typed amount is not snapped. Mouse-up commits that drag, writes one feature-log line and one editor transaction, and closes the session. The Move and Rotate gizmo stays hidden during Extrude, Inset, and Bevel. Choosing Move or Rotate on the toolbar cancels the preview, and that gizmo is the drag handle again. Apply remains for a typed amount that was not dragged. Cancel, Escape, and a right-click during the drag restore the session baseline and push no undo entry. A selection change away from the solid, and entering Play, do the same. A resize or a focus loss during the drag restores that baseline and leaves the session open. Keys 1 through 4 stay Select, Translate, Rotate, and scale-unavailable. WASD and Q E are not taken. ADR-0067. ADR-0069.

When the primary selection is a parametric block and no face of that block is selected, the overlay draws the analytic box in `[0.75, 0.88, 1.0]`. View > Show Grid draws the stored edges instead, including in Object mode. Face mode, and Auto once a body is stored, draw those edges without the toggle. The cage draws every stored edge, including after many subdivisions. Each line is one edge of the solid, and each enclosed region is one face. A modeling session for analytic Extrude, Inset, or Bevel suppresses that box and keeps the session face and one arrow. Extrude on a stored body draws the current body, highlights the selected faces, and draws one arrow along their shared outward normal. Dragging that arrow, or pressing one of those faces, moves only that region. Edge mode draws the solid's edges, and the selected edge is thick. Vertex mode draws those edges faint and marks the vertices. With Move active, a selected edge or vertex carries the translate gizmo. Dragging an axis moves that element and commits on release. Extend uses the same gizmo and extrudes along the axis. A press that misses the gizmo, while that session is open, drags on the view plane. Imported meshes and other actors have no silhouette in this pass. ADR-0069.

Select mode arms a marquee on a press that misses the gizmo, the operation handle, a character joint pivot, and the scene. The drag does not hide or recenter the cursor. Four pixels or less is still a click. Left to right requires every projected corner in front of the camera and inside the rectangle. Right to left selects when the on-screen corner box touches the rectangle. Shift adds. Ctrl toggles. The marquee selects whole objects. World Settings and hidden Land actors are skipped. A terrain chunk selects the terrain actor once. ADR-0068.

## Translate

The constraint is the axis line and a plane through that axis that faces the camera. The drag stores the original local translation. Each move intersects the plane, takes the signed distance along the axis, and writes:

```text
new local = original local + inverse(parent rotation) * world delta
```

The delta is not added to the billion-meter origin and then subtracted back. Parent rotation is included, so a world +X drag on a yawed parent does not land on local X. The value sent is the absolute local translation, not a stream of `Move +0.001` commands.

## Rotate

Rings write the authoritative quaternion. Euler degrees are not engine truth. World mode composes the delta in front of the old world orientation and converts through the parent. Local mode composes the delta on the local axis after the old local quaternion. The result is normalized. The engine rejects a non-finite or near-zero quaternion. The inspector field stays read-only. The command is still allowed for the gizmo.

An edge-on ring is hit as a tube, and its drag plane faces the camera when the ring plane is parallel to the view. The angle is measured after projecting off the axis.

## Edit session

The editor owns the drag. That drag is one transaction on the editor undo stack. A Move or Rotate begun during an open modeling preview stays inside that preview. ADR-0068.

| Step | Behavior |
| --- | --- |
| Begin | Store the original local translation and quaternion. Open one Move or Rotate transaction when no other gesture is open. Capture the mouse. The cursor stays visible and is not recentered. |
| Update | `SetProperty` of the absolute desired value. The same value does not bump the world. A real change does. Intermediate values are not undo entries. |
| Commit | Mouse release. Keep the last value. Commit the open Move or Rotate. Clear the session. |
| Cancel | Escape, focus loss, deactivate, viewport hide, shutdown, or a resize during the drag. Restore the before-stamp and push nothing. |

Resize while not dragging leaves the drag alone. Camera look, pan, and orbit do not start during a drag, and the camera does not tick. An active camera capture does not start a drag. Selection revision stays put. The world revision moves because the transform changed.

Gizmo vertices are camera-relative float32. They are not billion-meter coordinates. The mesh, material, and texture uploads do not change. The entity uuid does not change. The device is not recreated.

Each drag update is an absolute `SetProperty` before the next presented frame. That frame extracts the new translation and quaternion, rebuilds the instance matrix, and shades with the current normal and tangent. Lighting is not deferred until mouse release. Escape writes the original quaternion, so the next frame's normals match the start. The gizmo is drawn after tone mapping, so exposure does not recolor the axes. A face that turns away from the direct lights goes dark because there is no environment light yet. That is [the lighting contract](../rendering/lighting.md), not a stale matrix. Do not add a fake ambient term.

## Keys

Accepted fly navigation still owns W A S D, and Q down / E up, whenever Perspective navigation is open. Tool keys do not take those.

| When | Keys |
| --- | --- |
| RMB, MMB, or orbit capture | Q and E stay camera down and up. WASD flies. Tool keys do not fire. |
| Perspective focused, no text field, no capture | `1` Select, `2` Translate, `3` Rotate, `4` reports that scale is unavailable. WASD and Q E still fly. |
| Inspector or any text field | Neither fly nor tool keys. |

A floating tooltip window is not part of this. The button name is the label.

The dedicated server does not see tool modes, the overlay, or the gizmo. Nothing here is in the C ABI.

See [picking.md](picking.md), [commands.md](commands.md), and [../rendering/editor-overlays.md](../rendering/editor-overlays.md).
