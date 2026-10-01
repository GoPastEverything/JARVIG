# Viewport

The editor viewport is a `RenderView` into the engine. It is not an editor-owned renderer.

```text
Editor viewport child HWND
        |
        v
RenderViewId  "JARVIG.Perspective"
        |
        v
the same RenderSceneSnapshot the engine just extracted
        |
        v
the same Renderer
        |
        v
the same RHI device
```

The perspective panel is one stable child HWND of the frame. The view uses `NormalizedRect::FULL` of that child, the editor front camera, and default view settings. One acquire and one present cover that single view. Split, tab, and reset move or hide the HWND. They do not destroy it and they do not destroy the `RenderView`.

Dock layout records the child client size in physical pixels (`GetClientRect` on the perspective HWND). It does not reconfigure the swapchain inside `WM_SIZE`. The next frame, after the previous swapchain texture view has been dropped, configures the existing surface and recreates depth to that size. A minimized or hidden view is suspended at 0×0 and does not configure an empty surface. Projection aspect is the configured width divided by the configured height. The HWND and `RenderViewId` stay. JRV-0070. The transitional host still calls `resize` on its own top-level window.

Releasing the renderer drops an acquired swapchain image, and the views and buffers that still reference it, before the surface and the device. wgpu panics if that image is discarded after the surface is gone, and a Win32 window procedure cannot unwind from that panic. A failed frame prints `EDITOR_FRAME` and then `JARVIG_FAIL` with the same text. It does not abort the process.

The bootstrap left/right split stays in `jarvig_editor_host`. It proved that two views can share one snapshot. The product editor does not show that split. Multi-view is not deleted. Perspective, Top, Front, Side, and Game can each be another `RenderView` later. They are not extra worlds.

The Perspective camera is editor session state on that view. It is not a scene entity. Fly, look, orbit, pan, and focus update a view pose. They do not write a GPU matrix and they do not revise `SceneWorld`. Projection uses the configured drawable aspect above, the bootstrap 60° vertical field of view, and infinite reversed-Z. See [camera.md](camera.md) and [ADR-0033](../adr/ADR-0033-editor-camera-is-not-a-scene-entity.md).

Viewport picking is JRV-0065. The cursor is a Perspective child client pixel. The ray uses the configured drawable size, which matches that client, the surface, and the depth target. The hit is an `EntityUuid` into the same `SelectionService` the outliner uses. It is not an `ObjectId`. See [picking.md](picking.md) and [selection.md](selection.md).

The transform gizmo is an editor overlay on this view, not a scene object. It is drawn after tone mapping so exposure does not recolor the axes. Select picks. Translate and Rotate drag through engine commands. Scale is drawn and disabled. View > Exposure +, Exposure -, and Reset Exposure change this view's stops. They do not revise the world. View > Environment Light toggles the scene environment. That does revise the world, and the axis colors stay the same. View > Lighting Debug isolates direct, environment, probe, and emissive terms on this view only. Those switches do not revise the world and do not change light intensity. The shell is Editor theme v1. See [gizmos.md](gizmos.md), [theme.md](theme.md), [../rendering/hdr.md](../rendering/hdr.md), [../rendering/environment-lighting.md](../rendering/environment-lighting.md), and [../rendering/editor-overlays.md](../rendering/editor-overlays.md). Do not restore the light Win32 face. The viewport image is the tone-mapped engine view, plus that overlay.

The triangles in the view are the bootstrap scene. They are allowed until a camera and a gizmo exist. After that, replace them with reusable primitives (a plane, a cube, a sphere) for lighting checks. Do not spend time decorating the triangles. They are temporarily two-sided cards so the reverse side can be inspected. The near triangle is a rough dielectric and should keep environment fill for whichever side faces you. The far triangle is metal. Away from the lamps it should still reflect the sky, more sharply when roughness is low. It will not reflect the near triangle. That is not a missing ambient term and not a culled back face. Do not weaken the BRDF to hide either one. See [../rendering/face-culling.md](../rendering/face-culling.md).

On minimize, or when the drawable size is zero, the frame is skipped and the surface is suspended. The next nonzero client size configures it again.
