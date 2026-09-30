# Scripting

Not implemented. `engine/scripting/` is reserved.

Gameplay scripting starts as TypeScript ES modules. Performance-sensitive or ecosystem modules may later use WASM or native boundaries with an explicit ABI. Raw internal object pointers are not a plugin ABI.

Scripts subscribe to the engine fixed step or variable update through a public API. They do not import editor modules. A script that needs an editor panel ships a separate editor entry. See [../plugins/overview.md](../plugins/overview.md).

The founding example manifest:

```json
{
  "id": "com.example.vehicle-tools",
  "version": "0.1.0",
  "engine": "^0.4.0",
  "entryRuntime": "./dist/runtime.mjs",
  "entryEditor": "./dist/editor.mjs",
  "permissions": ["asset.read", "editor.panel.register"]
}
```

Capabilities are explicit so a future collaboration or marketplace story is possible. The marketplace itself is an early non-goal.
