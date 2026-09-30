# Editor

The product editor is [README.md](README.md). This page is only the transitional TypeScript dock.

See [../architecture/editor-runtime.md](../architecture/editor-runtime.md) and [../architecture/play-in-editor.md](../architecture/play-in-editor.md).

Temporary panel host, not `JARVIGEditor.exe`:

```powershell
pwsh -File scripts\dev-editor.ps1
```

Equivalent: `pnpm dev:editor`. A dock is served at `http://127.0.0.1:4780` until Ctrl+C. `--once` boots and exits. The page is a view. Commands call the engine. Do not wrap this page in Electron and call it JARVIGEditor.

JRV-0007 (dock, commands, viewport pane) is met by `EditorShell`. The native view is JRV-0058, not a canvas this page owns. Outliner selection, inspector edits, transform undo, and gizmos are still open. Do not wrap this page and call it the native editor.
