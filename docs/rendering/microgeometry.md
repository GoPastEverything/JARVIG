# Procedural detail

Procedural detail is an optional rendering input and is off unless a host enables it.

## Public contract

The public engine contract is intentionally conservative:

- detail is queried from stable surface information plus a deterministic seed;
- screen-space error decides whether the ordinary material path is sufficient;
- ordinary materials remain the fallback;
- disabling procedural detail must return the unmodified ordinary surface;
- detail generation must not mutate the accepted RFC-0001 hierarchy;
- an incomplete or stale generated result must not replace the last complete renderable result.

The provider shipped in this public tree is deliberately limited to disabled detail or a flat reference triangle with zero displacement. It exists to keep the interface, fallback, and tests public without publishing the active private/local research provider.

The public code therefore should not be read as the complete state of JARVIG's procedural-detail research.

## RFC-0002 boundary

RFC-0002 remains **Proposed**. Private/local acceptance-candidate work has exercised deterministic detail, screen-space transitions, and asynchronous build/publication behavior, but that provider implementation is not distributed in this repository.

No private distribution rule, topology construction rule, subdivision strategy, cache/addressing scheme, or equivalent implementation detail is specified here.

See [RFC-0002](../rfc/RFC-0002-procedural-microgeometry.md) and [Surface detail research](../research/aperiodic-detail.md).

## Attribution and JARVIG-specific contribution

JARVIG does not claim invention of the mathematical `einstein` concept or the 2023 hat monotile. Those are prior mathematical work associated with David Smith, Joseph Samuel Myers, Craig S. Kaplan, and Chaim Goodman-Strauss.

The JARVIG-specific work under RFC-0002 is the **engine architecture that applies Einstein-hat-derived aperiodic structure to real-time procedural microgeometry**. At the architecture level, the private/local candidate evaluates all of the following as one system:

- deterministic surface-detail addressing from stable surface information and a seed;
- projected screen-space error as the gate that decides whether generated detail is warranted;
- bounded connected local microgeometry layered onto the accepted base surface rather than replacing that base representation;
- preservation of the RFC-0001 hierarchy and ordinary material path as an independent fallback;
- background generation and upload work that does not own the editor frame loop;
- stale-work rejection when a newer camera/detail request supersedes an in-flight build;
- complete-result publication at a frame boundary rather than exposing partially generated geometry;
- deterministic reuse and return behavior so revisiting the same surface state can recover the same detail result;
- separate accounting for base geometry and generated microgeometry.

This engine integration is distinct from the underlying mathematical discovery. The documentation may call the private/local integration **JARVIG Einstein Surface** for clarity, but that name does not assert ownership of the Einstein/hat mathematics.

The public repository intentionally does not publish the active provider's exact aperiodic construction, topology/subdivision rules, cache/addressing scheme, placement heuristics, or equivalent unpublished implementation details.

Any public claim about a specific private provider must distinguish the underlying mathematical work from JARVIG's engine integration and must be backed by a reproducible artifact before it is presented as a public-source result.
