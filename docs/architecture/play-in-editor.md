# Play-In-Editor

PIE is a state machine over two worlds, not a flag on the authoring scene.

```text
EDITING
  -> Play
       -> snapshot or copy-on-write of authoring state
       -> SIMULATION WORLD
            -> RUNNING
                 <-> pause / step / eject / possess
            -> Stop
                 -> discard runtime mutations (default)
                 -> optionally apply explicitly selected runtime changes
  -> EDITING
```

`JARVIGEditor` Play enters this split. It does not toggle `engine.profile` on the authoring world.

Play captures the authored level, instantiates a runtime world, and points the Perspective view at `startup_camera`. Pause freezes simulation time and keeps drawing that world. Stop drops the runtime world and restores the editor camera pose. The outliner shows `Runtime` during play. The inspector is read only. Runtime edits are not saved.

Not in this slice: step, eject, possess, and Apply Runtime Changes. Undo is still not the boundary between the two worlds.
