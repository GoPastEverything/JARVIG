# RFC-0006 — Advanced global illumination

Status: Proposed
Date: 2026-09-22

Hypothesis: A GI or reflection technique beyond the PBR baseline improves a fixed interior and exterior scene enough to justify its GPU cost and its fallback.

Baseline: The production PBR direct lighting plus image-based lighting from milestone P5, same cameras, same exposure.

Feature flag: `advancedGi` (default off).

Test scenes: One indoor room with a known light leak risk, and one outdoor range from the vertical slice. Both need a moving light or a moving occluder so a baked-only cheat fails.

Hardware: Record GPU and WebGPU adapter. Golden images need a tolerance, not a zero diff.

Image-quality metric: Image error against a path-traced or artist-approved reference, plus a note on light leaks and contact shadows.

Frame-time metric: GPU milliseconds added versus the baseline at the target resolution.

VRAM / storage / bandwidth metric: Probe, voxel, or cache residency and update cost when the camera or a light moves.

Expected failure cases: Leaks, view-dependent popping, a cost that blows the frame budget, and no fallback to the baseline lighting.

Raw result location: `docs/benchmarks/rfc-0006/`. None recorded.

Promotion threshold: A documented visual improvement inside a stated GPU budget on both scenes, with the baseline path still available. Otherwise continue or reject.

Conclusion: not run.
