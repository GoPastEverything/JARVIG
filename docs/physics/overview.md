# Physics

Not implemented. Directory `engine/physics/` is reserved and is not a package.

Physics sits behind an engine interface so the backend can move from a browser or WASM library to a native library without rewriting gameplay. No backend is chosen. Choosing one requires an ADR, a license entry, and a dependency-rule update.

The world contract physics must honor:

```text
Physics universe
  planet / ground scene
  vehicle or ship exterior scene
  ship interior local scene
  local interaction islands

Cross-frame transfer:
  detect boundary
  -> capture world velocity
  -> transform into the destination frame
  -> insert body
  -> preserve momentum within tested tolerances
```

Also required before the vertical slice: fixed timestep plus interpolation, a character controller independent of render FPS, continuous collision for fast bodies, constraints, contact and island debug, a server-authoritative path, and replayable tests.

Do not step physics in render time. The engine clock already produces fixed steps. A future physics module subscribes to `onFixedStep`.
