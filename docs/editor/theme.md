# JARVIG Editor theme v1

This is the current visual baseline for `JARVIGEditor`. Do not put the shell back on the light Win32 face.

The dark chrome was applied after JRV-0064 and is part of the accepted editor, not a separate ticket. Navigation icons are drawn. The workspace selector is the words Level, Land, and Character. The rest of the toolbar is not a line of text.

```text
charcoal window
    dark panel surfaces
    blue accent on the active tab and the active tool
    compact icon toolbar
    dark Outliner, Inspector, Content, and Output
    the engine viewport stays the 3D image, not more chrome
    status line, including Perspective speed
```

Colors live in `native/jarvig_editor/src/chrome.rs`. Roughly: window `#181818`, panels `#242424`, fields `#161616`, text `#DCDCDC`, accent `#4AB4FF`. The menu bar is owner-drawn in the same charcoal. The 3D clear color is not this theme.

## Toolbar

Icons are the PNGs in `assets/editor/ui/navigation/`. Windows Imaging decodes them. They are editor chrome, not scene content and not the asset database.

Select, Translate, Rotate, and Scale are one tool group. The active tool is a darker tile, a blue bottom edge, and an accent-tinted icon. Inactive tools are gray on charcoal. Hover lightens the tile. Scale is dimmed and cannot become active: the frame has no scale. Translate and Rotate are the tools that drag. See [gizmos.md](gizmos.md).

Level, Land, and Character sit after that group and before Play. They are words on the same tiles. One is active. They choose an editor view of the open level. They do not load a second world. See [workspaces.md](workspaces.md).

Play, pause, stop, and the navigation icons stay in their own colors. They are actions or reminders, not the transform mode. Focus frames the selection. Play is still not a session. A floating tooltip window is not part of v1. The command name is the label.

## Gizmo colors

Axis colors are not the UI accent. X is red, Y is green, Z is blue. The gizmo is an editor overlay, not scene geometry. A click still resolves to an `EntityUuid`. A drag is an authoring command. It does not write a GPU matrix.
