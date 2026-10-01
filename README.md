# JARVIG

All rights reserved. This repository does not grant an open-source license. See [LICENSES.md](LICENSES.md).

JARVIG is a native game engine. The product you run is **JARVIGEditor**, a Windows program that hosts that engine. A game is a project and a level edited there, then played in the editor or in **JARVIGGame**.

The engine core is the Rust crates under `native/`. PlayCanvas is not linked, not vendored, and not the runtime. An early plan to build on it was dropped. Browser technology is an export target, not the engine.

Start here: [Make a game with JARVIG](docs/getting-started.md). That page covers a new project, bringing in a glTF or GLB model, saving, and play.

## Public source and research boundary

This repository is the **public source tree**, not a promise that every active research provider or experimental implementation is published here.

The public tree contains the engine contracts and the implementations intentionally selected for public release. Experimental research may be developed and measured in a private/local tree before the corresponding implementation is published. When that happens, the public documentation must say so explicitly rather than making a public clone appear to contain code that it does not.

In particular, RFC-0002 procedural microgeometry remains **Proposed** in this public tree. Its public provider is deliberately limited and defaults off. Private/local RFC-0002 experiments and acceptance-candidate runs are tracked separately; their implementation details are not granted for reuse by this repository.

A public clone should be judged only by behavior reproducible from the public source and public benchmark artifacts. See [RFC-0002](docs/rfc/RFC-0002-procedural-microgeometry.md) and [the surface-detail research note](docs/research/aperiodic-detail.md) for the current boundary.

## Run the editor

Windows and a Rust toolchain with `cargo`.

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

Node.js is not required for the editor. `pnpm dev:editor` is an old TypeScript dock used while the contracts were being prototyped. It is not JARVIGEditor.

The Lighting Lab sample is `samples/lighting-lab`. Its townshop model is not in this repository because the file is larger than GitHub allows.

Architecture notes, if you are changing the engine rather than making a game, live under [docs/](docs/README.md). Accepted ADRs outrank the founding `designdoc.html` where they name a decision.
