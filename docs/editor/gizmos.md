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
| Scale | Visible and disabled. A spatial frame stores translation and a quaternion, not scale. |

Scale does not invent an editor-only scale. The button stays dim. Choosing it logs that scale is unavailable. Bootstrap object scale is not an entity property.

World and Local are real. World axes are the reference frame. Local axes are the selected entity's orientation. The toolbar Local button toggles them and is accented in Local. The high-precision root is not a float32 global space.

With more than one entity selected, the gizmo sits on the primary and the drag changes only the primary. It does not pretend the others move.

No spatial frame, or no selection, draws no gizmo.

## Screen size and depth

The drawn length is about 90 pixels. It comes from camera distance, vertical field of view, and viewport height, and it is clamped. That scale is visual. It is not entity scale.

The overlay pass has no depth test, so the gizmo stays visible inside geometry. Object materials are not edited to draw it. Reversed-Z is unchanged for the scene pass.

Axis colors are the usual ones: X red, Y green, Z blue. Hover brightens a handle. The active drag is drawn stronger. The other handles stay visible. Plane handles (XY, XZ, YZ) are not in this pass.

## Hit order

When Translate or Rotate is active, the ray tests gizmo handles before scene triangles. A press on a handle starts a manipulation. A miss falls through to picking. Select mode does not hit-test the gizmo.

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

The editor owns the session. It is not an undo stack.

| Step | Behavior |
| --- | --- |
| Begin | Store the original local translation and quaternion. Capture the mouse. The cursor stays visible and is not recentered. |
| Update | `SetProperty` of the absolute desired value. The same value does not bump the world. A real change does. |
| Commit | Mouse release. Keep the last value. Clear the session. |
| Cancel | Escape, focus loss, deactivate, viewport hide, shutdown, or a resize during the drag. Write the originals back. |

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
