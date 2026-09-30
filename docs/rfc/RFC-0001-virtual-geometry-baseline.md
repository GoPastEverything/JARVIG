# RFC-0001 — Virtual geometry baseline

Status: Accepted
Date: 2026-09-22
Accepted: 2026-09-27

Hypothesis: A meshlet cluster hierarchy with geometric error, compressed pages, GPU selection, and a resident parent fallback can render dense authored meshes with bounded CPU submission and no missing-geometry holes.

Baseline: The ordinary PBR renderer and GPU scene from milestones P5, on the same camera paths, before any cluster selection.

Feature flag: `virtualGeometry` (default off).

Test scenes: A dense prop courtyard and a hero mesh with known triangle count. Both must include a camera that moves from far to extreme near and back.

Hardware: Record the GPU name, driver, and whether the device is WebGPU. Do not compare two machines in one table without saying so.

Image-quality metric: Screen-space geometric error against the full-resolution mesh, plus a stated pop-in threshold.

Frame-time metric: GPU frame time and CPU submit time at matched quality. The P5 budget cites 2 ms CPU submit as the long-term target, not as this RFC's pass line. This RFC passes only if submit stays bounded as triangle count grows, relative to the one-draw-per-object baseline on the same scene.

VRAM / storage / bandwidth metric: Resident VRAM, cooked size, and bytes streamed per second on the camera path.

Expected failure cases: Holes when a child page is missing, temporal flicker in error selection, cook times that make iteration unusable, and a fallback that is more expensive than the baseline.

Raw result location: [docs/benchmarks/rfc-0001/2026-09-27](../benchmarks/rfc-0001/2026-09-27/notes.md).

Promotion threshold: Repeatable win on frame time or memory at matched error, with a parent fallback that never leaves a hole, on at least two recorded runs. Otherwise CONTINUE RESEARCH or REJECT.

Conclusion: Accepted 2026-09-27 by the human. The acceptance is the hierarchy, parent coverage, screen-space LOD selection, the persistent sidecar, and composition with frustum culling and conservative occlusion. It is not a production speed target and it is not a frame-time win over another engine.

`JARVIGEditor --rfc0001` printed `RFC0001_PASS`. At 1 px: close 2.40 m is 6122214/6122214 triangles, medium 13.59 m is 2450164 (-60.0%), far 24.15 m is 1963494 (-67.9%). Far at 4 px is 1171846 (-80.9%). Root cut 699898 triangles. Empty parents 0. Child-center misses 0. Hole ratio 0. Cache hit.

`JARVIGEditor --rfc0001-runtime` printed `RFC0001_RUNTIME_PASS`. Forty-nine dolly frames covered 61702 leaves each. Selection median 20110 µs, maximum 32506 µs. Hole ratio 0, including with frustum and occlusion on. Cold usable time 426236 ms. Rebuild false. Recorded build 677692.5 ms.

Follow-ups that stay open, measured and not retargeted: selection 20.1 ms median / 32.5 ms max; parent index upload about 2.7 s when the cut changes; parent GPU vertices 2351284320 bytes; `.jarvigparents` 2325318884 bytes; cache reload 426 s; GPU timestamps unimplemented; peak build memory unmeasured; JRV-0088 open. The frozen hashes are in [the 2026-09-27 record](../benchmarks/rfc-0001/2026-09-27/notes.md).
