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

## Editor modes

Level, Land, and Character are workspaces over the same project data. The toolbar shows those three words. View > Land Mode and View > Character Editor select the same modes. The level is not copied.

Level is the authored world. Characters, props, lights, and terrain stay visible. The terrain editing grid is off. Joint helpers are off.

Land is the terrain workspace. The terrain is the editable object. Characters, props, and gameplay actors are hidden until Show Full Level. The conforming grid is on by default, depth-tested, and follows the surface. Minor and major spacing stay independent. Sculpt, smooth, flatten, and paint run only here.

Character is the rig workspace. Terrain and props are hidden. Joints, limits, and pose tools draw here. A character asset opens here. A level with no rig stays in the current mode.

The last mode and its visualization settings are `Saved/Editor/workspace.json`, schema `jarvig.editor-workspace` version 1. That file is editor state, beside `viewport.json`. It is not in the level. A Terrain World template opens in Land only when that file is absent. Land overlays and joint helpers are applied in their own mode. Level does not draw them.

This is not the dock layout. Named layout presets and a saved dock tree are still JRV-0069. Reset Layout does not change the mode.

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
