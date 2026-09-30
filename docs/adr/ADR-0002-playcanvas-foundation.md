# ADR-0002 — PlayCanvas-derived TypeScript foundation

Status: Partially superseded
Date: 2026-09-22

## Context

The founding doc starts TypeScript-first, in the PlayCanvas lineage, and allows Rust, C++, or WASM only behind a measured interface. It also says to record the upstream remote and the merge policy.

## Decision

The engine is TypeScript on Node 22 and pnpm. PlayCanvas Engine (MIT) is the intended open-source foundation. The git remote `upstream-playcanvas` is `https://github.com/playcanvas/engine`.

Phase 0 does **not** vendor or fork that source. The code in this repository is an original skeleton of JARVIG module boundaries. Importing PlayCanvas is a later reviewed change that:

- pins an upstream commit,
- copies the MIT notice into `LICENSES/THIRD_PARTY_NOTICES.md`,
- lands on a branch, and
- does not mix CryEngine or other restricted source into the import.

Upstream merges are deliberate, reviewed, and tested. They are not a cron job. `origin` is the project fork when one exists. This local bootstrap has no `origin`.

`npm create playcanvas` is not run inside this repository. That scaffold is a separate application and would collide with the monorepo.

## Alternatives Considered

- Vendor PlayCanvas in the founding commit. Rejected for Phase 0. It would hide JARVIG's module boundaries under an unreviewed import.
- A C++ engine from day one. Rejected by the founding doc until a profile shows the TypeScript boundary is the problem.
- A clean-room clone that pretends PlayCanvas will never be used. Rejected. The founding strategy is a lineage, not a permanent rewrite.

## Consequences

Until the import lands, behavior is JARVIG's, not PlayCanvas's. Do not claim PlayCanvas compatibility that tests do not show.

## Supersedes

Nothing.

## Superseded By

[ADR-0017](ADR-0017-native-engine-core-and-multi-host-runtime.md) supersedes the decisions that TypeScript is the engine implementation and that PlayCanvas is the runtime foundation. The upstream remote, the MIT notice rule, and the ban on an unreviewed PlayCanvas import remain in force. PlayCanvas is a reference and an optional adapter, not the engine.
