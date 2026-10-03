# Editor

The product editor is `JARVIGEditor.exe`. It hosts the engine. The creator manual starts at [../manual/README.md](../manual/README.md). The window, the camera, and the panels are [README.md](README.md).

The TypeScript dock is still available for the old command-bus tests. It is transitional. Do not wrap it in a WebView and call that the editor.

```powershell
pwsh -File scripts\dev-editor.ps1
```

Equivalent: `pnpm dev:editor`. A dock is served at `http://127.0.0.1:4780` until Ctrl+C. `--once` boots and exits. That page is a view of the prototype. It is not `JARVIGEditor.exe`.

See [../architecture/editor-runtime.md](../architecture/editor-runtime.md) and [../architecture/play-in-editor.md](../architecture/play-in-editor.md).
