# JARVIG Editor theme v1

This is the current visual baseline for `JARVIGEditor`. Do not put the shell back on the light Win32 face.

The dark chrome was applied after JRV-0064 and is part of the accepted editor, not a separate ticket. The toolbar draws an icon with a short caption under it.

```text
charcoal window
    dark panel surfaces
    blue accent on the active tab and the active tool
    icon toolbar with a caption under each button
    dark Outliner, Inspector, Content, and Output
    the engine viewport stays the 3D image, not more chrome
    status line, including Perspective speed
```

Colors live in `native/jarvig_editor/src/chrome.rs`. Roughly: window `#181818`, panels `#242424`, fields `#161616`, text `#DCDCDC`, accent `#4AB4FF`. The menu bar is owner-drawn in the same charcoal. The 3D clear color is not this theme.

## Toolbar

Icons are the PNGs in `assets/editor/ui/navigation/`. Windows Imaging decodes them. They are editor chrome, not scene content and not the asset database. The bar is tall enough for the icon and one short word. A floating tooltip is not part of v1. The caption is the label.

Select, Move, Rotate, and Scale are the mesh tools. Move is the caption for Translate. The active tool is a darker tile, a blue bottom edge, and an accent-tinted icon. Inactive tools in that group are gray on charcoal. Hover lightens the tile. Scale is dimmed and cannot become active: the frame has no scale. Select picks. Move and Rotate drag a gizmo. See [gizmos.md](gizmos.md).

Block sits in that group and is not a tool mode. It runs the same Create Block command as the menu and places one parametric solid. It does not stay selected. Extrude, Inset, and Bevel appear after Block, with a gap before Extrude, only while a parametric solid is selected outside Play, Land, and Character. They start one modeling session. ADR-0067.

Level, Land, and Character sit after that group and before Play. Each has an icon and its name. One is active. They choose an editor view of the open level. They do not load a second world. See [workspaces.md](workspaces.md).

Play, Pause, and Stop run the in-editor session. Focus frames the selection. Fly, Pan, Orbit, and Speed are labeled reminders of the mouse and keyboard camera. They do not switch a mode. Snap toggles the Land brush grid. Maximize is labeled and not implemented. The Local button reads World until it is toggled, then Local.

## Gizmo colors

Axis colors are not the UI accent. X is red, Y is green, Z is blue. The gizmo is an editor overlay, not scene geometry. A click still resolves to an `EntityUuid`. A drag is an authoring command. It does not write a GPU matrix.
