# ADR-0008 — Research stays behind flags

Status: Accepted
Date: 2026-09-22

## Context

Virtual geometry, procedural microgeometry, representation virtualization, server meshing, and advanced GI are part of the long-term program. None of them has a measured win. Treating them as default architecture would lock the engine to untested designs.

## Decision

Those five systems default off via `FeatureFlags`. Enabling a flag is an experiment. Promotion into the default path requires an ADR that cites recorded benchmarks and a fallback. RFCs define the success and failure criteria before the experiment is treated as a milestone exit.

"Better than Nanite" is not an assumption.

## Alternatives Considered

- Build the experimental renderer first because it is the interesting part. Rejected. The founding promotion sequence starts with a correct ordinary renderer.
- Hide experiments behind comments instead of flags. Rejected. Dead code paths get enabled by accident.

## Consequences

Derived-data keys include feature flags, so experimental products do not alias production products. Benchmarks live in `docs/benchmarks/` and are never invented.

## Supersedes

Nothing.

## Superseded By

Nothing.
