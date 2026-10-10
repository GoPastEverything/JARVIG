# Viewport picking

JRV-0065. A click in the Perspective view asks which visible renderable was under the cursor. The answer is an `EntityUuid`. It is not an `ObjectId`, a mesh id, a render-instance id, a buffer id, or a tree row.

```text
Perspective child client pixel
        |
        v
analytical ray from the editor camera
        |
        v
visible RenderInstance bounds, then mesh triangles
        |
        v
EntityUuid
        |
        v
SelectionService
```

The snapshot is only the candidate list. It already carries the entity id and the resolved pose. It does not become `SelectionService`. Hidden instances are skipped. An entity that is not in the current view is not a hit. Non-renderable outliner rows stay selectable from the outliner. They have no triangles.

## Pixels

The mouse position is the Perspective child client pixel from that HWND. JRV-0070 already makes those pixels match the configured surface, the depth target, and the projection aspect. Dock DIP coordinates and the old 1600×900 cache are not the ray.

## Ray

The ray is built from the editor camera's binary64 pose, the vertical field of view, and the configured width and height. It does not read the depth buffer, so reversed-Z does not affect the hit. Drawing still uses right-handed, 0..1 clip, infinite reversed-Z.

Pixel (0, 0) is the top-left. The sample is the pixel center (`pixel + 0.5`). Window Y grows down and is flipped into JARVIG +Y. +X is right. Camera forward is local −Z. The origin is the camera position. The direction is the view basis plus the field-of-view offset. Both stay `f64`. Nothing downcasts the origin to `f32` before the intersection.

## Intersection

CPU only. There is no GPU object-id buffer.

```text
ray
  -> sphere bounds of each visible instance
  -> that mesh's triangle BVH, built when the mesh was created
  -> smallest positive distance
  -> EntityUuid
```

The outliner does not use this path. It already has the `EntityUuid` and only updates `SelectionService`, the inspector, and the gizmo. A click does not rebuild the mesh, recompute normals, upload buffers, recapture probes, or invalidate shadow maps.

The BVH is local to the mesh. The ray is transformed into that space, boxes prune the walk, and only the surviving leaves are tested. Selecting or deselecting does not scale with the mesh's triangle count. There is no second copy of the mesh for a selection highlight.

A closer triangle wins. The two bootstrap triangles are distinguishable: the center pixel hits Near Triangle, and a ray that passes beside it hits Far Triangle.

## Gestures

One owner at a time.

| Gesture | Result |
| --- | --- |
| LMB on a visible entity | Replace. On a parametric solid, Auto and Face select the hit face. Object selects the entity. Edge selects the edge. Vertex selects the vertex. |
| Double-click | Promote to the owning entity. The mode stays. |
| Shift+LMB | Add. |
| Ctrl+LMB | Toggle. Ctrl wins over Shift. |
| LMB on empty view | Clear. |
| Shift or Ctrl on empty view | No change. |
| Select-mode drag on empty view | Marquee. Left to right selects objects fully enclosed. Right to left selects objects the rectangle touches. Shift adds. Ctrl toggles. In Auto and Object the result is whole objects. In Face, Edge, and Vertex the same drag selects authored elements of the primary solid. |
| RMB click under four pixels | Opens the command menu for the mode. It does not select. A right-drag past four pixels is still look. |
| RMB look, MMB pan, Alt+LMB orbit | No selection change. Escape does one action and still consumes the key. |

Auto, Object, Face, Edge, and Vertex are a Selection section on a parametric solid. They are not toolbar buttons. Edge selects one edge. Vertex selects one vertex. ADR-0069. On an authored block the hit is semantic. A point in a fillet strip selects `F:fillet(...)`. A point in a corner shadow selects `F:corner(...)`. The flat interior stays the planar face. The short segments used to draw an arc are not selectable edges. A movement of four pixels or less is still the click in the first rows. The marquee does not hide or recenter the cursor. World Settings and hidden Land actors are skipped. A terrain chunk selects the terrain actor once. The hit is the projected box of the object. Face, Edge, and Vertex use the projected vertices of that solid's display body instead. ADR-0068.

While a transform handle is under the cursor, the gizmo takes the press and the scene behind it is not picked. An active camera capture does not pick. A plain click does not bump the world revision. It bumps the selection revision only when the ordered selection changes.

The dedicated server does not call this. The math lives in `jarvig_core` and has no window and no GPU type. It is not in the C ABI.

See [gizmos.md](gizmos.md), [selection.md](selection.md), and [ADR-0034](../adr/ADR-0034-gizmo-is-editor-overlay.md).
