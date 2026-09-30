# RFC-0003 — Representation virtualization

Status: Proposed
Date: 2026-09-22

Hypothesis: Switching among proxy, cluster HLOD, triangle meshlets, procedural microgeometry, and optional Gaussian or surfel data under one error and cost model beats any single representation across far-to-near camera motion.

Baseline: RFC-0001's virtualized triangles alone, on the same paths.

Feature flag: `representationVirtualization` (default off). Depends on a recorded RFC-0001 baseline. Do not start this as the first renderer.

Test scenes: The RFC-0001 scenes plus one soft captured surface if a splat path is claimed.

Hardware: Same recording rules as RFC-0001.

Image-quality metric: Error against the chosen reference, plus temporal stability at the switch distances.

Frame-time metric: GPU and CPU time across the whole path, not only at the distance where the new representation looks good.

VRAM / storage / bandwidth metric: Peak resident memory and transition bandwidth.

Expected failure cases: Pops at representation changes, a far proxy that costs more than the cluster it replaces, and a splat path that cannot fall back.

Raw result location: `docs/benchmarks/rfc-0003/`. None recorded.

Promotion threshold: A measured cost advantage at matched stability on the full path, with a graceful fallback to triangles. Otherwise reject or continue.

Conclusion: not run.
