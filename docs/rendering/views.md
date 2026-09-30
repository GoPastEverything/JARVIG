# Render views

A render view is a view of engine state. It is not another world, another renderer, or another asset database. ADR-0025.

```text
JARVIG WORLD
    |
shared frames, logical meshes, GPU mesh residency
    |
    +------------------+------------------+
    |                  |                  |
Perspective         Alternate         later preview
camera A            camera B
origin A            origin B
viewport A          viewport B
depth A             depth B
    |                  |                  |
    +------------------+------------------+
                       |
                 RenderTarget
                       |
              acquire once, present once
```

`jarvig_editor_host` is one surface target split left and right. Both halves draw the same extracted snapshot. See [scene-extraction.md](scene-extraction.md). The renderer does not own which objects exist. The right camera is translated and yawed, so the picture is not a copy of the left. Mesh uploads stay at two. The pipeline and the shader stay at one. Each view has its own uniform buffers.

`JARVIGEditor` uses one full perspective view on its own child window. That is a layout choice. The renderer still accepts more than one view. Do not delete the split from the test host to make the editor simpler.

The view's drawable size is the configured surface size, in physical pixels. The editor layout uses DIPs. The child `GetClientRect` is what gets configured. Depth matches that size. Projection aspect is configured width over configured height. A 0×0 request suspends the surface instead of configuring an empty target. Resize waits until the frame boundary and drops swapchain image views first. One queue wait there is conservative and is not the long-term fence. JRV-0070 is accepted. The queue wait stays bootstrap debt.

A view may carry a resolved pose override. Prepare uses it instead of the snapshot camera when it is set. The snapshot is still the world. The override is how the editor flies without revising that world. The bootstrap front and side cameras remain frame cameras for views that do not set an override. A future game Camera can feed a view the same way. The view does not have to be an entity. ADR-0033. Field of view stays on the view. Aspect is the configured drawable, not a cached window size.

## View and target

| Object | Means | Does not mean |
| --- | --- | --- |
| `RenderView` | Camera, origin, viewport, scissor, depth, settings | A world, a swapchain, an RHI id |
| `RenderTarget` | Where pixels go | A window, necessarily |
| `RenderTargetKind::Surface` | The native swapchain | The only kind forever |

`RenderViewId` is index plus generation. Generation 0 is never live. Destroying a view retires its uniforms, bind groups, and depth through the JRV-0048 rules, then the old id fails. A new view may reuse the index. The stale id does not.

Views on one target draw in ascending slot index. That order is stable. It is not hash-map iteration.

The render origin is the resolved pose of that view's camera frame. Camera-relative float32 is computed per view. The billion-meter root stays in the frame graph and out of both GPU packets. An editor overlay for one view is drawn after tone mapping, into the swapchain, in the same camera-relative space, with no depth test. It is not another view and not a scene object. Scene color is `Rgba16Float` and is not the swapchain. See [editor-overlays.md](editor-overlays.md) and [hdr.md](hdr.md).

Settings live on the view (`RenderViewSettings`). The bootstrap shader ignores them. They are not a show-flag system and not renderer globals.

There is no `Renderer.current_camera`.

## One frame

```text
render_target
  -> acquire the surface once
  -> clear the color target once
  -> for each view, in slot order:
       load color
       clear that view's depth
       set viewport and scissor
       draw the shared meshes with that view's uniforms
  -> submit
  -> present once
```

A color `Clear` covers the whole attachment. View passes use `Load` so the second view does not erase the first. Depth is cleared per view. Reversed-Z is unchanged: `Depth32Float`, clear 0, compare greater-or-equal, infinite far.

Each view owns a depth texture. In this bootstrap the texture is the size of the target, not the size of the viewport. The viewport and scissor are what limit the draw. That size match is an implementation detail so the attachments agree. It is not one global depth buffer.

A zero drawable size does not acquire, does not allocate depth, and does not destroy the view ids. Restore recreates depth and keeps the same ids.

## What the host does

The host calls `EngineSession::run_frame`, which calls `Renderer::render_target`. It does not loop views, acquire, or present. Authored cameras still come from the shared world. `JARVIGEditor` also stores the Perspective session pose and pushes it on the view before that call. The transitional host does not. Neither host reads winit for a camera.

The editor, later, maps panels to views. Layout and focus stay in the UI. The UI does not become the renderer.

## Not built

No second world for ordinary views. No offscreen texture target yet. The editor Perspective camera flies. That is session state, not a second world. No viewport picking and no gizmos. No scene-submission list. That is JRV-0050. No public C view API.
