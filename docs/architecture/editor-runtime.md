# Editor runtime

The editor is a host of the engine. It is not a second engine, not a browser, and not the owner of engine features.

JARVIG is a native multi-platform engine. The product editor is `JARVIGEditor.exe` (`native/jarvig_editor`). It links the engine and draws Win32 panels. The viewport is one engine `RenderView` on a child window. A page that draws its own scene is not the viewport. ADR-0031.

`RenderViewId` is not a world and not a swapchain. The editor creates `JARVIG.Perspective` at full size of that child. `jarvig_editor_host` still draws `JARVIG.Perspective` and `JARVIG.Alternate` as a renderer test. They share one extracted snapshot. The editor does not build that snapshot and does not encode passes. See [../editor/viewport.md](../editor/viewport.md) and [../rendering/views.md](../rendering/views.md).

Three hosts matter:

- `native/jarvig_editor` is the product shell. In-process engine, one perspective view, and a dock workspace that the engine does not know about. ADR-0032.
- `native/jarvig_editor_host` is the transitional two-view renderer test. It is not the dock and not the product editor.
- `apps/editor` is the temporary TypeScript dock. It ticks a headless native runtime and does not draw. Do not wrap it in a WebView.

The dock must not grow into the engine. The shell must not grow into a second renderer. See [ADR-0018](../adr/ADR-0018-engine-owned-capabilities.md).

```text
Editor UI (outliner, inspector, graphs)
  -> command / transaction bus (undo, redo, multi-edit)
  -> authoring world
  -> the same runtime the player and the server use
```

Rules from the founding doc:

- Every editor mutation is a command or transaction.
- Panels never mutate arbitrary runtime objects directly.
- Gizmo drags coalesce into one logical undo record.
- Runtime state carries origin tags so accidental save-back is impossible.
- Editor features call public engine APIs. The game does not call editor APIs.
- The player does not ship these panels. The server does not link them.

`SceneWorld` is the authoring world for the first shell. Play-In-Editor is a different world and is not entered by the Play menu. See [play-in-editor.md](play-in-editor.md).

## Current shell

Launch:

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor
```

The frame is Win32. Menu, toolbar, and status are chrome. The dock tree places the outliner, the perspective viewport, the inspector, content, and output. The viewport HWND is stable. The Perspective camera is editor session state pushed onto that view. It is not a scene entity. ADR-0033. The window around it is Editor theme v1. Do not restore the light shell. See [../editor/theme.md](../editor/theme.md). The outliner reads `EntityRegistry` through one hierarchy snapshot and shows names under a non-entity `World` root. One `SelectionService` on the editor session is the selection. The tree caret is not that selection. The inspector builds `InspectorModel` from an engine inspection snapshot and submits `SetProperty` commands. Win32 control ids are not field ids. The content panel is still a placeholder. The output panel is an in-process string, not the engine log. See [../editor/README.md](../editor/README.md), [../editor/outliner.md](../editor/outliner.md), [../editor/selection.md](../editor/selection.md), and [../editor/workspaces.md](../editor/workspaces.md). The server does not link the outliner or the selection service.

`pnpm dev:editor` still serves the temporary panel host on `127.0.0.1:4780`. `node apps/editor/dist/main.js --once` boots and exits. Smoke uses the once path. That process is not `JARVIGEditor.exe`.

## Later editor packages

Directories under `editor/` other than `shell/` are reserved names from the founding layout: viewport, inspector, outliner, content browser, material editor, material library, graph, world partition, profiler, transactions. They have no implementation. Do not import them from the engine. The native shell does not move those TypeScript packages into the executable.
