# ADR-0015 — Dependency direction

Status: Accepted
Date: 2026-09-22

## Context

Editor convenience imports are how engines become unable to ship a headless server.

## Decision

Dependencies point toward the engine, never out of it. The enforced table is `tools/validation/check-boundaries.mjs`, documented in `docs/architecture/dependency-rules.md`. `pnpm lint` fails on a violation. The dedicated server may not import the render package or the editor. Render may depend on world. World may not depend on render.

## Alternatives Considered

- Convention without a test. Rejected. The founding ticket JRV-0001 asks CI to reject the forbidden import.
- A runtime check inside every module. Rejected as the primary gate. The import must fail at lint time, before the module graph loads.

## Consequences

New packages update the checker and the doc together. Cook-time exclusion of unused `node_modules` is still future work. Source-level exclusion is the Phase 0 gate.

## Supersedes

Nothing.

## Superseded By

Nothing.
