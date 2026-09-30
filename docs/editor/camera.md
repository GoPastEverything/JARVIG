# Perspective editor camera

JRV-0064. The Perspective camera is editor session state. It is not an authored entity.

```text
Editor navigation
        |
        v
Perspective RenderView
        |
        v
view pose, binary64, resolved in the root frame
        |
        v
object and light position - that origin
        |
        v
small float32 GPU values
```

`SceneWorld` stays where it is while you fly:

```text
SceneWorld
    Near Triangle
    Far Triangle
    future authored entities
```

Moving the view does not mint an `EntityUuid`, does not write a component, does not call `SetProperty`, and does not bump the world revision. A future gameplay Camera is a different object. Play-in-editor may show that camera later. It must not overwrite this one. Nothing here is saved to disk. JRV-0069 can persist the plain fields (position, orientation, speed, field of view, orbit pivot) without a new camera model.

ADR-0033.

## Who owns what

The editor owns intent: yaw, pitch, fly speed, boost, orbit pivot, and whether the mouse is captured. `EditorCameraController` has no window handle.

The render view owns the pose the frame consumes. The editor calls `Renderer::update_view` with that pose before `run_frame`. Win32 does not write a shader uniform.

The view still remembers the bootstrap front-camera `FrameId` as its reference frame. Navigation does not write that frame. There is no automatic switch between planet, ship, and interior. The pose is already resolved in the root frame, so a later ticket can re-express it in another frame without inventing a float32 universe.

## Pose and projection

Translation is `Vec3` meters, binary64. Orientation is one quaternion built from yaw about reference +Y and pitch about local +X. Roll stays 0. Pitch is clamped to `PITCH_LIMIT_RADIANS` (±89°). Three Euler fields are not the engine camera.

Coordinates stay +X right, +Y up, −Z forward. Camera forward is local −Z. Mouse right yaws toward +X.

Projection is the accepted one: right-handed, 0..1 clip, infinite reversed-Z, near 0.1 m. Vertical field of view is the bootstrap camera's 60°. It lives on the view, not in the shader. Every projection uses the configured drawable aspect from JRV-0070. The 1600×900 window size is not cached.

The same viewport HWND and `RenderViewId` survive resize, splitters, and Reset Layout. Reset Layout does not reset the camera. `reset_to` is a separate operation for tests and a future Reset View.

## Gestures

| Input | Effect |
| --- | --- |
| RMB drag | Look. Yaw and pitch. Captures the cursor. |
| W / S | Fly along camera forward, including pitch. |
| A / D | Strafe on camera right / left. |
| E / Q | Up / down on reference +Y, not camera up. |
| Shift | Temporary ×4. Base speed stays put. |
| Wheel | Speed × 1.25 per notch. Clamped to 0.05 .. 10,000 m/s. |
| Alt + wheel | Dolly. Orbit distance × 1.1 per notch, inverted, clamped to 0.2 .. 10,000,000 m. |
| MMB drag | Pan on the view plane. Camera and pivot move together. |
| Alt + LMB drag | Orbit the pivot. Distance stays constant. |
| F | Frame the primary selection. |

Speed is meters per second. Distance is `speed * min(dt, 0.1)`. The 0.1 s cap is `MAX_DELTA_SECONDS`. A debugger pause must not throw the camera across the scene. The clock is the editor frame `Instant`, not a second Win32 timer. Keys are edge state. Windows key-repeat does not set the speed.

Pan scale is `max(orbit distance, 1) * 0.002` meters per pixel. The default pivot is 5 m in front of the camera. Focus replaces it.

F reads `SelectionService`. It does not change the selection. One selected entity with a mesh is framed from its transformed bounds, vertical field of view, and a 1.25 margin. The camera keeps its yaw and pitch and backs up along its own forward until it is looking at that origin. A spatial entity with no mesh uses a 1 m radius. An entity with no frame logs a line and does nothing. Several selected entities frame the primary only. Combined bounds are later.

RMB does not select. A left click does. See [picking.md](picking.md).

There is no collision, gravity, or smoothing. The camera flies through the triangles.

## Input routing

Navigation runs only when the Perspective panel is focused, or while it holds a capture, and no text field has the caret, and the view is visible.

W in an Inspector edit, the Output log, or a menu does not fly. RMB in the viewport focuses Perspective and clears text focus. Escape releases capture and does not clear selection. If an Inspector field has focus, Escape still cancels that edit.

Capture uses recenter, not raw input. On press, the screen point is stored. Each move reads `GetCursorPos`, adds the delta, and `SetCursorPos` puts the cursor back. The screen edge does not stop the turn. The cursor is hidden with a balanced `ShowCursor` count and restored to the press point on release.

Capture ends on RMB/MMB/Alt+LMB release, Escape, focus loss, deactivate, hide, and shutdown. A normal button release leaves WASD held if Perspective still has focus. Deactivate and focus loss clear stuck keys so Alt-Tab cannot leave W down.

Perspective cannot be closed. Hiding it is not a dock command. The same release path runs if the view is not visible. The pose is kept.

The order in a frame is: Win32 updates the key and gesture state, the controller steps with dt, the view pose is pushed, then the engine extracts and renders. The camera is not applied after the picture.

The status bar shows `Perspective | N.N m/s` next to the selection. That is not a viewport overlay.

The toolbar draws the navigation icons. Select, Translate, Rotate, and Scale are the tool group. Scale stays disabled. Translate and Rotate drag a gizmo. Focus frames the selection. Play is still not a session. The shell is Editor theme v1. See [theme.md](theme.md) and [gizmos.md](gizmos.md). Do not put the light Win32 face back.

Tool keys do not take W, A, S, D, Q, or E. Those stay fly and vertical movement. With Perspective focused, no text caret, and no mouse capture, `1` is Select, `2` is Translate, `3` is Rotate, and `4` reports that scale is unavailable. During RMB, MMB, or orbit capture, Q and E stay camera down and up. A gizmo drag owns the mouse until release, Escape, focus loss, deactivate, hide, or shutdown. The camera does not look, pan, or orbit during that drag.

## What does not happen

A camera move does not rebuild meshes, materials, or textures. Extraction still runs every presented frame. That is bootstrap. A later dirty extract must treat a view change as not a world change.

World light poses stay. The per-view light packet is rebuilt from the new origin. JRV-0054.

## Source icons

Editor chrome icons live in `assets/editor/ui/navigation/`. The toolbar decodes those PNGs. They are source art, not cooked content, and not the asset database. See that folder's README and [theme.md](theme.md).
