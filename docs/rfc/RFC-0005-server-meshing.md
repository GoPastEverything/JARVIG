# RFC-0005 — Server meshing

Status: Proposed
Date: 2026-09-22

Hypothesis: Authoritative simulation can be split across servers by assigning ownership of existing world cells, with clients seeing one replicated world.

Baseline: One dedicated server simulating the vertical-slice vehicle scene with relevance filtering. That baseline does not exist yet. This RFC does not start before P9's single-server slice.

Feature flag: `serverMeshing` (default off).

Test scenes: The vertical slice with the vehicle crossing a cell boundary that is also an ownership boundary, plus a second client.

Hardware: Two server processes and two clients. Record versions and machine counts.

Image-quality metric: Not the primary metric. Visual disagreement between clients is a failure if it exceeds the normal interpolation error.

Frame-time metric: Server tick time per owned cell, and client correction magnitude at the boundary.

VRAM / storage / bandwidth metric: Bytes replicated per client versus the single-server baseline.

Expected failure cases: Double simulation, lost authority, momentum errors at the boundary, and a spatial structure that is not `WorldCell`.

Raw result location: `docs/benchmarks/rfc-0005/`. None recorded.

Promotion threshold: The boundary crossing stays authoritative, both clients agree within a stated tolerance, and deleting the mesh code returns the single-server behavior. Otherwise do not promote.

Conclusion: not run.
