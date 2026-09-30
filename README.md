# JARVIG

All rights reserved. This repository does not grant an open-source license. See [LICENSES.md](LICENSES.md).

JARVIG is a native multi-platform game engine. The browser is one export target, not the owner of the engine. A shipped game must be able to boot as a native executable without Node, Electron, or a DOM. The editor hosts that same engine. It does not carry a private copy of import, cooking, materials, or simulation.

The TypeScript packages in this repository are the current executable prototype of the contracts. The native core starts at `native/jarvig_core`. See [ADR-0017](docs/adr/ADR-0017-native-engine-core-and-multi-host-runtime.md) and [ADR-0018](docs/adr/ADR-0018-engine-owned-capabilities.md).

The founding specification is [`designdoc.html`](designdoc.html) (AAA Engine Platform — Project Start & Technical Framework v0.2, 2026-09-22). Do not delete or replace that file. Maintainable docs extracted from it live under [`docs/`](docs/README.md). Accepted ADRs outrank the founding document only for the decision they name.

The working codename in the founding document was left TBD. This repository adopts **JARVIG**. See [ADR-0014](docs/adr/ADR-0014-codename-jarvig.md).

## Phase

Current phase: **P0 — Foundation / Governance**. The tree compiles, the engine lifecycle boots inside the editor, client, and headless server hosts, and research systems stay off. It is not a playable editor yet.

Status: [`docs/status/CURRENT.md`](docs/status/CURRENT.md).

## Requirements

- Windows is the primary development OS. PowerShell scripts are first-class.
- Node.js 22 or newer.
- pnpm 10.15.1, pinned in `package.json` and activated with Corepack.

## Bootstrap

```powershell
pwsh -File scripts\bootstrap.ps1
```

That installs dependencies, runs the boundary check, typecheck, unit/integration tests, the TypeScript build, and the smoke commands.

## Everyday commands

| Action | Command |
| --- | --- |
| Launch JARVIGEditor | `cargo run --manifest-path native/Cargo.toml -p jarvig_editor` |
| Launch the Phase 0 TypeScript dock | `pwsh -File scripts\dev-editor.ps1` |
| Same transitional dock, via pnpm | `pnpm dev:editor` |
| Boot the client once | `pwsh -File scripts\dev-client.ps1` |
| Boot the dedicated server once | `pnpm dev:server` |
| Run tests | `pwsh -File scripts\test.ps1` |
| Build | `pwsh -File scripts\build.ps1` |
| Smoke the built hosts | `pnpm smoke` |
| CLI project info | `pnpm cli info` |

`cargo run --manifest-path native/Cargo.toml -p jarvig_editor` is `JARVIGEditor.exe`. It hosts the engine and presents one view. `pnpm dev:editor` still serves the transitional dock on `http://127.0.0.1:4780` until Ctrl+C. That page is not the product editor. Do not wrap it in a WebView.

## Layout

The canonical layout is the one in `designdoc.html` section 5, with the Phase 0 naming notes in [ARCHITECTURE.md](ARCHITECTURE.md). Engine packages live in `engine/`. The native editor is `native/jarvig_editor`. Older hosts live in `apps/` and `hosts/`. The engine does not import the editor. The dedicated server does not import the renderer.

## Authority

1. Accepted ADRs
2. Current architecture docs
3. `designdoc.html`
4. Accepted RFCs
5. Source contracts and tests
6. Temporary notes
7. Chat or model suggestions

## License

JARVIG's own license has not been chosen. See [LICENSES.md](LICENSES.md). PlayCanvas is the intended open-source foundation and is not vendored yet. CryEngine source is not allowed in this repository.
