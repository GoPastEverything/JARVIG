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

Play captures the authored level, instantiates a runtime world, and points the Perspective view at the runtime camera. Pause freezes simulation time and keeps drawing that world. Stop drops the runtime world and restores the editor camera pose. The outliner shows `Runtime` during play. The inspector is read only. Runtime edits are not saved.

When the project pawn is `JARVIG.DefaultFreeFlyPawn` and `startup_camera` is `pawn`, Play In Editor places that pawn at the editor view before the first tick. The runtime camera is the pawn's Camera. It is not the Perspective editor camera. WASD moves, the mouse looks, E and Space rise, Q and Ctrl descend, and Shift sprints. Escape releases the mouse. Click captures it again. Toolbar Stop returns to the authored level. Standalone play and `PlayControl::attach` keep the default spawn, local (0, 1.6, 8), yaw 0, looking toward −Z. A project that does not select a pawn does not grow one, and Play does not fall back to the editor camera.

Not in this slice: step, eject, possess, and Apply Runtime Changes. Undo is still not the boundary between the two worlds.
