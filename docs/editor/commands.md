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

The shell does not mutate a snapshot or a GPU buffer from a panel. File > Exit closes the window. Help > About opens a message box. Select, Translate, Rotate, and the Local toggle are real. Scale reports that it is unavailable. Play > Play In Editor runs a runtime copy of the level. See [../architecture/play-in-editor.md](../architecture/play-in-editor.md). Snap on the toolbar toggles the Land brush grid. Maximize is labeled and has no behavior yet.

The outliner lists the entity registry. A rename is an inspector edit. JRV-0061's self-test can still call `SceneWorld` directly so a view can be proved without a panel writing the world.

Changing selection is not one of those commands. It does not belong on the scene undo stack. A later editor may remember it as navigation history. That is separate from a world edit.

`SetProperty` is the command the inspector sends to `EngineSession`. The engine checks the type registry, then `SceneWorld` validates and stores the value. Name and local translation are the editable inspector fields. UUID and parent are not. The inspector still shows the quaternion read-only. The gizmo may set that same field. The engine accepts a finite, normalizable quaternion and rejects anything else. Repeating the current value does not bump the world revision. A committed inspector edit is one editor transaction. ADR-0068.

A gizmo drag is a session around that command, not a second command type. Begin stores the original local translation and quaternion. Each move sends the absolute desired value. Mouse release keeps the last value and commits one editor transaction. Escape, focus loss, deactivate, hide, shutdown, or a resize during the drag restores the original and pushes nothing. The world revision moves on a real change. The selection revision does not. See [gizmos.md](gizmos.md).

Editor undo is that stack. It is capped at 64 and drops the oldest. Ctrl+Z undoes. Ctrl+Y and Ctrl+Shift+Z redo. The Edit menu names the open gesture or the top entry. Undo and Redo are not toolbar icons. Create, delete, duplicate, mirror, align, snap, reset shape, a committed modeling Apply, inspector edits, name and parent changes, materials, a terrain stroke, Place Mesh, and Place Prefab are transactions. A numeric field and a viewport drag are one entry from begin to commit. Cancel pushes nothing. Play does not record ticks, and undo refuses during Play. Loading a level clears the stacks. Selection stays off the stack. Edit also has Duplicate, Delete, Select All, Deselect All, and Invert Selection. Those selection commands are not undo entries. There is no clipboard. ADR-0068.

Workspace commands are a different list. `ShowPanel`, `ClosePanel`, `DockPanel`, `SetSplitRatio`, `Activate`, `Focus`, and `ResetLayout` change the dock tree only. They are not world commands and they do not belong on the undo stack of the scene.

Gizmo drags coalesce into one undo record, and runtime state still carries an origin so a play session cannot be saved by accident. See [../architecture/play-in-editor.md](../architecture/play-in-editor.md). JRV-0068 is the play/simulate control. It is not the world-copy state machine by itself.
