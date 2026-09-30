# Editor overlays

JRV-0065. An editor overlay is presentation data for one render view. It is not a scene object.

```text
jarvig_editor
    tool, selection, gizmo pose, hover, active handle
        |
        v
EditorOverlay
    RenderViewId + camera-relative float32 vertices
        |
        v
jarvig_renderer
    one overlay pass after the scene
        |
        v
jarvig_rhi
    ordinary buffers, a pipeline, a pass
```

The renderer does not know Win32 buttons, `SelectionService`, or the inspector. The RHI does not know a gizmo. The world does not contain a gizmo mesh, a gizmo material, or a gizmo light.

## Draw

The pass runs after the scene passes on the same color target. Load is the existing color. There is no depth attachment, so the overlay is not tested against reversed-Z and it is not written into the object depth. The scene pass is unchanged.

The uniform is the same camera block the materials use: infinite reversed-Z projection, and a view matrix that is only the camera rotation. Positions are already camera-relative, so there is no model translation and no billion-meter float32. The vertex buffer is recreated when the bytes change. `write_buffer` is for the uniform, not for vertex memory.

The pipeline and the shader stay cached. Clearing the selection or leaving Translate and Rotate drops the packet. Shutdown destroys the overlay shader, pipeline, uniform, vertex buffer, and bind group before the shared camera layout.

A view that is not the overlay's `RenderViewId` does not draw it.

See [../editor/gizmos.md](../editor/gizmos.md) and [ADR-0034](../adr/ADR-0034-gizmo-is-editor-overlay.md).
