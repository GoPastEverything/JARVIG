# Native editor host

This directory points at the transitional renderer test, not at `JARVIGEditor.exe`.

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host
```

That window is one surface and two render views, `JARVIG.Perspective` and `JARVIG.Alternate`. It does not record passes. It is not the TypeScript dock.

The product shell is:

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor
```

That executable hosts the engine and one perspective view. It does not embed `apps/editor`. See [docs/editor/README.md](../../docs/editor/README.md).
