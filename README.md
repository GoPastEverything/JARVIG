# JARVIG

All rights reserved. This repository does not grant an open-source license. See [LICENSES.md](LICENSES.md).

JARVIG is a native game engine. The product you run is **JARVIGEditor**, a Windows program that hosts that engine. A game is a project and a level edited there, then played in the editor or in **JARVIGGame**.

The engine core is the Rust crates under `native/`. PlayCanvas is not linked, not vendored, and not the runtime. An early plan to build on it was dropped. Browser technology is an export target, not the engine.

Start here: [Make a game with JARVIG](docs/getting-started.md). That page covers a new project, bringing in a glTF or GLB model, saving, and play.

## Run the editor

Windows and a Rust toolchain with `cargo`.

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

Node.js is not required for the editor. `pnpm dev:editor` is an old TypeScript dock used while the contracts were being prototyped. It is not JARVIGEditor.

The Lighting Lab sample is `samples/lighting-lab`. Its townshop model is not in this repository because the file is larger than GitHub allows.

Architecture notes, if you are changing the engine rather than making a game, live under [docs/](docs/README.md). Accepted ADRs outrank the founding `designdoc.html` where they name a decision.
