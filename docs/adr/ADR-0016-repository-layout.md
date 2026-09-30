# ADR-0016 — Repository layout

Status: Accepted
Date: 2026-09-22

## Context

The founding document (section 5) and the bootstrap instruction name slightly different trees. Two layouts would split imports.

## Decision

`designdoc.html` section 5 is canonical:

- `engine/render`, not `engine/rendering`
- `engine/net`, not `engine/networking`
- `hosts/dedicated-server` for the server
- `tools/cooker`, `tools/shaderc`, `tools/materialc`
- editor panel names as listed in that section

Phase 0 exceptions, which are staging and not a second architecture:

- One Node host at `hosts/client` stands in for `hosts/game-web` and `hosts/game-desktop` until those targets diverge. READMEs in the unused directories say so.
- There is no `engine/platform` package. Platform code is the host plus `engine/core`.
- Unimplemented modules are READMEs, not empty packages, so they cannot be imported by accident.

## Alternatives Considered

- Follow the bootstrap instruction's folder names and ignore section 5. Rejected. The founding doc is the architectural authority, and it already chose names.
- Create a compilable package for every future subsystem. Rejected. Empty exports become import magnets.

## Consequences

`ARCHITECTURE.md` keeps the mapping table. Promoting `hosts/client` into separate web and desktop hosts is a later change that preserves the engine API.

## Supersedes

Nothing in the founding technical design. It records how the bootstrap instruction was reconciled with section 5.

## Superseded By

Nothing.
