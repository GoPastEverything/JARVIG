# Native core

This is the authoritative runtime's home. It is not a second engine beside the TypeScript packages.

```text
jarvig_core          headless runtime and the reference-frame math, no GPU, no window
jarvig_engine        frame order: clock, then an optional render callback
jarvig_rhi           public GPU interface and the null backend
jarvig_renderer      clear plus one indexed triangle through the camera path
jarvig_platform      winit window and event loop
jarvig_rhi_wgpu      the only crate that names wgpu
jarvig_editor_host   transitional two-view renderer test
jarvig_editor        JARVIGEditor.exe, the native editor shell
```

`pnpm lint` fails if another crate depends on `wgpu` or `winit`, or if Rust outside `jarvig_rhi_wgpu` names `wgpu::`.

```powershell
cargo test --manifest-path native/Cargo.toml --workspace
cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host -- --self-test
cargo run --manifest-path native/Cargo.toml -p jarvig_editor -- --self-test
```

```powershell
cargo test --manifest-path native/Cargo.toml
```

See [ADR-0017](../docs/adr/ADR-0017-native-engine-core-and-multi-host-runtime.md).
