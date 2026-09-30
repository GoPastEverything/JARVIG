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

The first world command is `SetProperty`. The inspector sends it to `EngineSession`. The engine checks the type registry, then `SceneWorld` validates and stores the value. Name and local translation are the editable inspector fields. UUID and parent are not. The inspector still shows the quaternion read-only. The gizmo may set that same field. The engine accepts a finite, normalizable quaternion and rejects anything else. Repeating the current value does not bump the world revision. Undo is not recorded yet.

A gizmo drag is a session around that command, not a second command type. Begin stores the original local translation and quaternion. Each move sends the absolute desired value. Mouse release commits by keeping it. Escape, focus loss, deactivate, hide, shutdown, or a resize during the drag sends the original again. The world revision moves on a real change. The selection revision does not. A later undo entry can store that original and the final value as one record. There is no undo stack yet. See [gizmos.md](gizmos.md).

Workspace commands are a different list. `ShowPanel`, `ClosePanel`, `DockPanel`, `SetSplitRatio`, `Activate`, `Focus`, and `ResetLayout` change the dock tree only. They are not world commands and they do not belong on the undo stack of the scene.

When commands arrive, gizmo drags still coalesce into one undo record, and runtime state still carries an origin so a play session cannot be saved by accident. See [../architecture/play-in-editor.md](../architecture/play-in-editor.md). JRV-0068 is the play/simulate control. It is not the world-copy state machine by itself.
