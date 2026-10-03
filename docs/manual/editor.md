# The editor

`JARVIGEditor.exe` is one window on one authored world. The menu, the toolbar, and the status bar are chrome. The dock between them holds the panels. The engine never sees the dock.

The default layout is the world outliner on the left, the perspective view in the center, the inspector on the right, and the content browser and output log along the bottom. **View** shows or hides each panel. **View > Reset Layout** restores that arrangement. Reset Layout does not move the camera and does not change the level.

Perspective cannot be closed. It is one engine render view, not a second world.

## Toolbar

Each button is an icon with a short caption. The active mesh tool is a darker tile with a blue edge.

| Button | What it does |
| --- | --- |
| Select | Picks. Drag on empty space draws a marquee. |
| Move | Translate gizmo. Red X, green Y, blue Z. |
| Rotate | Rotation rings. The stored value is a quaternion. |
| Scale | Drawn and disabled. A spatial frame has no scale. |
| Block | Places one parametric solid. It does not stay selected as a mode. |
| Extrude, Inset, Bevel | Shown while a solid is selected, outside Play, Land, and Character. One modeling session. |
| Level, Land, Character | Three views of the work you have open. |
| Play, Pause, Stop | The in-editor session. |
| Focus | Frames the current selection. Same as F. |
| Local | Toggles world axes and the selected entity's axes. |
| Snap | Toggles the Land brush grid. It does not snap the Move gizmo. |

Fly, Pan, Orbit, and Speed are reminders of the mouse and keyboard camera. They do not switch a mode. Maximize is labeled and has no behavior yet.

## Selection

The editor has one selection. The outliner, the viewport, and the inspector read it. Selection is not saved in the level, and it is not an undo step.

| Gesture | Result |
| --- | --- |
| Click a mesh | Selects that entity. On a parametric solid, Auto and Face select the face you hit. Object selects the whole solid. |
| Double-click a face | Selects the whole owning solid. The mode stays where it was. |
| Click empty space | Clears the selection, unless Shift or Ctrl is held. |
| Shift+click | Adds. |
| Ctrl+click | Toggles. If both modifiers are held, Ctrl wins. |
| Drag on empty space in Select | Marquee. Left to right selects objects fully inside the rectangle. Right to left selects objects the rectangle touches. Four pixels or less stays a click. |

A whole solid draws a light-blue outline of its analytic box. A face draws that face. The box outline stays off while a modeling session is open. An imported mesh can be selected. It does not get that solid outline in this build. A marquee tests the projected box, so a thin mesh can be selected when the rectangle misses the triangles and still touches the box.

Right mouse, middle mouse, and orbit do not change the selection. Viewport picking hits meshes. Lights and probes are selected from the outliner.

**Edit > Select All** (Ctrl+A), **Deselect All** (Ctrl+Shift+A), and **Invert Selection** change the set. They are not undo entries. Outliner shift-range is not implemented.

Escape does one thing. It cancels an open modeling session, or drops the marquee, or ends a gizmo or sculpt drag, or clears the selection when none of those are active. It does not cancel a tool and clear the selection in the same keypress.

## Move and rotate

Press `2` or choose Move. Drag an axis. The gizmo sends the absolute translation through the engine. Release keeps the value and records one undo entry. Escape during the drag puts the original transform back and records nothing.

Press `3` or choose Rotate. The rings edit the quaternion. The inspector shows that quaternion and does not offer a text field for it. Location and name are editable. The entity id and the parent are read-only.

World axes are the reference frame. Local axes follow the selected entity. With several entities selected, the gizmo sits on the primary, the last one you selected, and the drag changes that one.

Keys `1` through `4` do not cancel an open Extrude, Inset, or Bevel session. Choosing Move or Rotate on the toolbar does close that preview.

## Undo

**Edit > Undo** is Ctrl+Z. **Edit > Redo** is Ctrl+Y or Ctrl+Shift+Z. The menu names the entry. Undo and Redo are not toolbar buttons.

The stack holds 64 transactions and drops the oldest. One user intent is one entry: a gizmo drag from press to release, a numeric field from focus to commit, a sculpt stroke, a committed modeling Apply, Create Block, duplicate, delete, mirror, align, snap, reset shape, a material change, a terrain stroke, Place Mesh, and Place Prefab.

Cancel records nothing. Loading a level clears the stacks. Play does not record ticks, and undo refuses during Play. There is no Cut, Copy, or Paste.

The block also keeps a feature log of at most 24 edits. That log is not this undo stack. Undo puts the solid back. The log is the record of Apply. [Blocks](modeling.md) separates the two.

## What the panels show

The outliner is the entity list. Suffixes include Mesh, Light, Probe, World, and Spawn. In Land it also lists the terrain tools. In Character it lists joints.

The inspector edits the selected entity through engine commands. During Play it is read-only.

The content browser lists the open project's assets from the asset registry. It does not walk the disk on its own. [Models and content](content.md) is the import path.

The output log is text from the editor. A line there is a report, not a second undo history.

The status bar shows the perspective speed and the selection. Diagnostic counters that appear beside it belong to rendering views. Leave those views on the ordinary shaded picture while you are building.

## Workspace file

Level, Land, or Character, and the Land overlay settings, are remembered in `Saved/Editor/workspace.json` inside the project. That file is editor state. It is not in the level. Deleting it makes the next open follow the template again: Blank, First Person, and Third Person open in Level; Landscape opens in Land.
