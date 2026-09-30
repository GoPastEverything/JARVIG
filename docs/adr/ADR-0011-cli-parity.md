# ADR-0011 — CLI parity for critical editor operations

Status: Accepted
Date: 2026-09-22

## Context

Build farms and agents cannot click a panel. The founding doc requires build, import, cook, validate, package, and critical world and material operations to be automatable.

## Decision

A critical editor operation is not done until a CLI or stable API performs the same operation. Command names in the founding doc are provisional. Renaming them updates `docs/cli/overview.md` in the same change. Dropping the capability requires an ADR.

Phase 0 implements `info`, `version`, and `project validate` because those are the operations the bootstrap actually has. Cook and material commands stay listed as not implemented rather than as fake success stubs.

## Alternatives Considered

- Editor-only authoring until the UI is pleasant. Rejected. Parity arrives with the feature, not after it.
- Generate a CLI by shelling the editor GUI. Rejected. It is not deterministic and not headless.

## Consequences

New editor panels name their CLI equivalent in the ticket before implementation starts.

## Supersedes

Nothing.

## Superseded By

Nothing.
