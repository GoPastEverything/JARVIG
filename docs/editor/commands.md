# Editor commands

ADR-0022 already says authoring operations are commands. The editor calls the capability. It does not reimplement it, and it does not undo a runtime tick.

```text
Editor action
    |
    v
Engine command
    |
    v
World or resource mutation
```

Examples that do not exist yet: `CreateObject`, `DeleteObject`, `SetTransform`, `SetProperty`, `AssignMaterial`, `ImportAsset`.

JRV-0058 did not add those world commands. The shell still does not mutate a snapshot or a GPU buffer. File > Exit closes the window. Play > Play appends one line: play is not implemented, the editor is not the player, and there is no play-in-editor session. Help > About opens a message box. Select, Translate, Rotate, and the Local toggle are real. Scale reports that it is unavailable. Play, snap, and maximize still only log.

The world outliner is read-only. It does not rename, reparent, create, or delete from the window. Those future edits are engine commands, not tree-control side effects. JRV-0061's self-test calls `SceneWorld` directly so the view can be proved without inventing a UI mutation path.

Changing selection is not one of those commands. It does not belong on the scene undo stack. A later editor may remember it as navigation history. That is separate from a world edit.

The first world command is `SetProperty`. The inspector sends it to `EngineSession`. The engine checks the type registry, then `SceneWorld` validates and stores the value. Name and local translation are the editable inspector fields. UUID and parent are not. The inspector still shows the quaternion read-only. The gizmo may set that same field. The engine accepts a finite, normalizable quaternion and rejects anything else. Repeating the current value does not bump the world revision. A committed inspector edit is one editor transaction. ADR-0068.

A gizmo drag is a session around that command, not a second command type. Begin stores the original local translation and quaternion. Each move sends the absolute desired value. Mouse release keeps the last value and commits one editor transaction. Escape, focus loss, deactivate, hide, shutdown, or a resize during the drag restores the original and pushes nothing. The world revision moves on a real change. The selection revision does not. See [gizmos.md](gizmos.md).

Editor undo is that stack. It is capped at 64 and drops the oldest. Ctrl+Z undoes. Ctrl+Y and Ctrl+Shift+Z redo. The Edit menu names the open gesture or the top entry. Undo and Redo are not toolbar icons. Create, delete, duplicate, mirror, align, snap, reset shape, a committed modeling Apply, inspector edits, name and parent changes, materials, a terrain stroke, Place Mesh, and Place Prefab are transactions. A numeric field and a viewport drag are one entry from begin to commit. Cancel pushes nothing. Play does not record ticks, and undo refuses during Play. Loading a level clears the stacks. Selection stays off the stack. Edit also has Duplicate, Delete, Select All, Deselect All, and Invert Selection. Those selection commands are not undo entries. There is no clipboard. ADR-0068.

Workspace commands are a different list. `ShowPanel`, `ClosePanel`, `DockPanel`, `SetSplitRatio`, `Activate`, `Focus`, and `ResetLayout` change the dock tree only. They are not world commands and they do not belong on the undo stack of the scene.

Gizmo drags coalesce into one undo record, and runtime state still carries an origin so a play session cannot be saved by accident. See [../architecture/play-in-editor.md](../architecture/play-in-editor.md). JRV-0068 is the play/simulate control. It is not the world-copy state machine by itself.
