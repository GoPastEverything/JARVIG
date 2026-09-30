# ADR-0032 — The dock tree is editor state, not a window tree

Status: Accepted
Date: 2026-09-22

## Context

ADR-0031 says `JARVIGEditor` is a Win32 shell around the engine, and that docking must not take the swapchain. The first shell placed panels with fixed pixel rectangles. That is not a workspace. A later toolkit, a saved layout, or a floating window has to be able to replace the Win32 controls without rewriting what a layout means.

## Decision

The editor owns a `DockWorkspace`. The engine does not.

```text
DockWorkspace
    root: DockNode
    DockNode
        Split { axis, ratio, first, second }
        Stack { tabs: PanelId[], active }
```

`PanelId` is the panel identity. An HWND is not. Node relationships are ids. The tree contains no window handles, no engine objects, and no GPU ids.

Ratios are resolution-independent. DIPs are the editor layout unit. The renderer still receives the viewport's drawable pixel size. Those are different numbers.

Application chrome stays outside the tree: the menu bar, the toolbar, and the status bar. The workspace is the outliner, the viewport, the inspector, content, and output.

Showing, hiding, docking, and resetting the layout are workspace commands. They do not mutate the world. Engine commands such as `SetTransform` stay on the other side of that line.

A host is `Main` today. The tree does not assume that every panel's only possible parent is the main frame. A floating window would be another host with its own tree. It is not implemented.

The Win32 layer realizes the tree: it creates each built-in panel HWND once, then moves, shows, and hides those handles. Ordinary split, tab, and reset operations do not destroy the perspective viewport HWND or its `RenderView`.

The default preset is the only one built. The model can name another preset later. Workspace schema version is 1, for the persistence ticket. Nothing is written to disk here.

## Alternatives Considered

- Keep storing panel positions as the Win32 child tree. Rejected. Saving an HWND, or switching toolkits, would throw the layout away.
- Put the dock types in the engine. Rejected. The dedicated server and the player must not link a workspace.
- Treat layout edits as world commands. Rejected. Resetting the layout must not reset the scene.

## Consequences

Plugin panels later register a `PanelId` and metadata. They do not get a new match arm inside every split. Dragging a tab docks that id. Closing a singleton hides it. It does not destroy world data.

The panel move and the surface reconfiguration are separate. JRV-0070 applies the child client size on the next frame. That is a realization detail, not a change to this decision. Do not "fix" a resize by giving the editor a second device.

## Supersedes

Nothing. ADR-0031 still chooses Win32 and still forbids the editor from owning the renderer.
