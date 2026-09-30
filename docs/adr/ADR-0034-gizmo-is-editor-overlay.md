# ADR-0034 — The transform gizmo is editor overlay state

Status: Accepted
Date: 2026-09-23

## Context

The Perspective view can now select what it shows and manipulate a transform. If that manipulator were a scene entity, creating it would revise `SceneWorld`, give it an `EntityUuid`, and invite a serializer to save it. If a click returned `ObjectId` or a GPU id, selection would depend on a bootstrap slot or a buffer. A GPU object-id pass would also couple picking to the current depth convention. Scale is not a field on the spatial frame. Inventing one in the editor would make a value the engine does not own.

ADR-0025 says a view is not a world. ADR-0026 says a snapshot is not a world. ADR-0033 says the editor camera is not an entity. This decision says the same thing about the gizmo, and it says what a viewport hit is allowed to be.

## Decision

The gizmo is editor session state drawn through a generic overlay packet.

```text
Editor tool + SelectionService
        |
        v
EditorOverlay (camera-relative vertices)
        |
        v
overlay pass on the Perspective view
```

Picking returns `EntityUuid`. The ray is analytical, from the editor camera's binary64 pose and the vertical field of view, in Perspective client pixels. It does not read the depth buffer. CPU bounds, then mesh triangles. Hidden instances are not hits.

A drag sends absolute `SetProperty` values through engine authoring. Translate writes local translation. Rotate writes the local quaternion. Begin stores the original. Commit keeps the last value. Cancel writes the original back. That session is not an undo stack. Scale stays disabled until a real scale-bearing property exists. The overlay is not published through the C ABI.

## Alternatives Considered

- A gizmo entity, mesh, and material in `SceneWorld`. Rejected. The manipulator would be authoring data.
- A GPU object-id buffer. Rejected for this foundation. The bootstrap scene is a handful of triangles, and the hit must not become a render id.
- `mouse delta * constant` as the transform. Rejected. The axis or the ring is the constraint.
- An editor-only scale vector. Rejected. The frame does not store scale.
- Q W E R as the tool keys. Rejected. JRV-0064 already uses W A S D to fly and Q / E for vertical movement. Tool keys are `1` `2` `3` `4` when no camera capture and no text field owns the keyboard.

## Consequences

`docs/editor/picking.md`, `docs/editor/gizmos.md`, and `docs/rendering/editor-overlays.md` are the map. The renderer learns a view-scoped overlay. It does not learn the editor. A later undo stack can coalesce one drag into one record because every update was an absolute value and the original was kept. Plane handles and a scale gizmo wait until they have a real model.

## Supersedes

Nothing. ADR-0026 and ADR-0033 still stand.

## Superseded By
