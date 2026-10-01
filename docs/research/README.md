# Research

Research notes are not architecture. Promotion goes through an RFC measurement and then an ADR.

| Note | RFC | Flag |
| --- | --- | --- |
| [Aperiodic detail](aperiodic-detail.md) | [RFC-0002](../rfc/RFC-0002-procedural-microgeometry.md) | `proceduralMicrogeometry` |
| Virtual geometry sequence | [RFC-0001](../rfc/RFC-0001-virtual-geometry-baseline.md) | `virtualGeometry` |
| Representation switching | [RFC-0003](../rfc/RFC-0003-representation-virtualization.md) | `representationVirtualization` |
| Planets | [RFC-0004](../rfc/RFC-0004-planet-scale-terrain.md) | none until the RFC says so |
| Server meshing | [RFC-0005](../rfc/RFC-0005-server-meshing.md) | `serverMeshing` |
| Advanced GI | [RFC-0006](../rfc/RFC-0006-advanced-global-illumination.md) | `advancedGi` |

Do not put experimental code on the default host boot path. Benchmarks go in [../benchmarks/README.md](../benchmarks/README.md) only when a run produced them.

## Public/private research boundary

The public repository may expose a research interface or reference provider without publishing the active private/local experimental provider behind that interface.

When that happens:

- the public docs must say that the provider is intentionally withheld rather than implying the feature is absent everywhere;
- private/local harness results must be labeled private/internal until a matching implementation or reproducible artifact is published;
- a private result does not make an RFC Accepted;
- public benchmark claims must name the hardware, resolution, revision/artifact, comparison baseline, and feature state;
- unpublished algorithmic details should not be reconstructed in public documentation merely to make a progress claim sound complete.

RFC-0002 currently uses this boundary. Its public provider is a deliberately limited reference path; the active private/local research provider is not distributed in this tree.
