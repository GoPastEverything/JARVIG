# ADR-0010 — Documentation authority order

Status: Accepted
Date: 2026-09-22

## Context

The founding HTML, architecture pages, ADRs, RFCs, and agent sessions will disagree unless one order is law.

## Decision

1. Accepted ADRs
2. Current architecture documentation
3. `designdoc.html`
4. Accepted RFCs
5. Source-code contracts and tests
6. Temporary implementation notes
7. Chat or model suggestions

An ADR supersedes the founding document only for the decision it names, and it must say what it supersedes and why. `designdoc.html` is not deleted, overwritten, or replaced by a summary. Code that matches none of the accepted documents is drift and gets fixed or documented.

## Alternatives Considered

- The HTML file always wins, so ADRs are notes. Rejected. The founding doc itself says a later accepted ADR supersedes it for that decision.
- Tests always win. Rejected. A buggy test would become the architecture.

## Consequences

Sessions update the architecture page in the same change as the code. CURRENT.md is a status file, not an ADR.

## Supersedes

Nothing. It records section 30 and the closing authority rule of `designdoc.html`.

## Superseded By

Nothing.
