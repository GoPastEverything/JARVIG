# ADR-0014 — Codename JARVIG

Status: Accepted
Date: 2026-09-22

## Context

`designdoc.html` v0.2 leaves the working codename TBD. The founding build instruction names the repository and the product JARVIG.

## Decision

The project codename is JARVIG. Package scope is `@jarvig/*`. Schema ids use the prefix `jarvig.` (`jarvig.project/v1`, `jarvig.scene/v1`, `jarvig.material/v1`). The CLI binary is `jarvig`.

## Alternatives Considered

- Keep the codename TBD in code and docs. Rejected. Hosts, packages, and schemas need a stable name.
- Treat the HTML title "AAA Engine Platform" as the product name. Rejected. That is the document title, and the document says the codename is TBD.

## Consequences

Renaming later is an ADR because it changes schema ids and package names. This ADR does not choose a license or a company name.

## Supersedes

The sentence in `designdoc.html` that the working codename is TBD. It does not supersede any technical decision in that file.

## Superseded By

Nothing.
