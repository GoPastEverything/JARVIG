# Workspaces

JRV-0059 replaces the fixed pixel rectangles with a dock tree. The tree is editor state. It is not the Win32 child list, and the engine never sees it. ADR-0032.

```text
DockWorkspace
    schema version 1
    preset: Default
    host: Main
    root
        Split vertical, ratio 0.78
            Split horizontal, ratio 0.18
                Stack [World Outliner]
                Split horizontal, ratio 0.78
                    Stack [Perspective]
                    Stack [Inspector]
            Split horizontal, ratio 0.62
                Stack [Content Browser]
                Stack [Output Log]
```

A ratio is the first child's share of the free space. It is not a pixel width from the 1600×900 shell. Minimum panel size is 96 DIP when the region is large enough. The splitter is 6 DIP. A tab strip is 22 DIP.

Layout converts DIPs to physical pixels with the window DPI (`dip * dpi / 96`). The perspective drawable is still that panel's client size in pixels. Those two units are not the same number.

## Chrome

The menu bar, toolbar, and status bar are application chrome. They are not dock nodes. The dock host fills the client area between the toolbar and the status bar.

## Tabs and splits

A stack has one active tab. Inactive panels are hidden, not destroyed. Clicking a tab activates it. Dragging a tab and dropping on another stack docks that panel: the edges mean left, right, top, or bottom, and the tab strip means a new tab. A splitter drag writes a new ratio.

Empty stacks are removed. A split with one remaining child collapses, and that child takes the space. Ratios are clamped. An active index that no longer exists selects a remaining tab.

## Show, hide, reset

View menu:

- World Outliner
- Inspector
- Content Browser
- Output Log
- Reset Layout

A check mark means the panel is in the tree. Choosing it again closes it. Choosing it after that reopens the same singleton. Closing a panel does not destroy world data, meshes, or materials. The perspective panel is not closable in this milestone.

Reset Layout rebuilds the Default preset. It does not touch the world, the camera, materials, or the project.

## What is not built

Named presets other than Default. Level Editing, Materials, Animation, and Debugging are names for later. They are not layouts yet.

Floating windows are a future host with its own tree. They are not created. No layout is written to disk. That is JRV-0069. Schema version 1 is reserved for that file.

Keyboard navigation of tabs is not implemented. Tabs can already be selected by id, which is the hook for it.

## Realization limit

Moving the perspective HWND is safe. The dock records the child client size. JRV-0070 reconfigures the surface on the next frame, after swapchain views are dropped, and recreates depth to those pixels. Do not reconfigure inside `WM_SIZE`, and do not paper over a failure with a second device.
