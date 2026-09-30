# ADR-0033 — The editor camera is not a scene entity

Status: Accepted
Date: 2026-09-23

## Context

ADR-0025 says a render view is not a world. The bootstrap scene still stores two cameras as frames inside `SceneWorld`. The Perspective viewport was using the front one. If fly, orbit, and look wrote that frame, every mouse move would revise the authoring world, dirty the outliner, and pretend the editor had a Camera entity. Gameplay cameras and a later Play-in-Editor view need the opposite: an authored camera that the editor must not overwrite.

## Decision

The Perspective camera is editor session state. It is not an `EntityUuid`, not a component, and not an authoring command.

```text
EditorCameraController
        |
        v
RenderView pose override
        |
        v
camera-relative float32 for that view only
```

`EditorCameraController` holds navigation: yaw, pitch, binary64 root position, speed, orbit pivot. The renderer stores an optional resolved pose on the view. When the override is set, prepare uses it instead of `snapshot.camera(frame)`. When it is absent, the view still uses the authored frame. That is how a future game Camera can feed a view without every view becoming an entity.

The editor does not write the reference frame while navigating. The pose is resolved in the root frame. Field of view and near plane stay on the view camera. Projection keeps infinite reversed-Z and the configured drawable aspect.

Dock layout and this camera are separate. Reset Layout does not reset the pose. The pose is not written to disk here.

## Alternatives Considered

- Put a Camera entity in the bootstrap world and fly it with `SetProperty`. Rejected. Navigation would revise `SceneWorld` and serialize into the game.
- Store the editor position as `Vec3<f32>` in one global space. Rejected. ADR-0004. The billion-meter root would jitter.
- Teach the renderer about Win32 keys. Rejected. The controller has no window handle. The RHI has no navigation.

## Consequences

`docs/editor/camera.md` is the map. Viewport picking and the transform gizmo are still later. They read selection. They do not become the camera.

## Supersedes

Nothing. ADR-0025 still stands. This decision says who owns the Perspective pose.

## Superseded By
