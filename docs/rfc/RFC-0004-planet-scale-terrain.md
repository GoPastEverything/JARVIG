# RFC-0004 — Planet-scale terrain

Status: Proposed
Date: 2026-09-22

Hypothesis: Clipmap or quadtree terrain, virtual texture pages, and data-driven scatter can represent a planet by using the existing frame and cell contracts, without a second world system.

Baseline: A large terrestrial test world that already streams cells and keeps a moving local frame stable. The founding sequence says to validate that before a full planet.

Feature flag: none yet. Do not add `planet` as a secret mode. If an experiment needs a flag, this RFC must be revised first.

Test scenes: A terrestrial streaming map with a moving vehicle, then one planetary scene that reuses the same frame API.

Hardware: Not applicable until the terrestrial baseline exists.

Image-quality metric: Terrain LOD error and texture page error at the camera.

Frame-time metric: No frame-blocking IO during ordinary traversal. Frame time recorded, not asserted from the P0 budgets.

VRAM / storage / bandwidth metric: Resident cells and texture pages versus the authored set.

Expected failure cases: A second coordinate system, precision failure at altitude, and scatter that cannot become entities when gameplay touches it.

Raw result location: `docs/benchmarks/rfc-0004/`. None recorded.

Promotion threshold: The planetary scene runs on `FrameGraph` and world cells, passes the precision tests at altitude, and streams within a stated budget. A parallel "planet mode" is a failure even if it looks right.

Conclusion: not run.
