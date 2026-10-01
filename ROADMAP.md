# JARVIG roadmap

Source: `designdoc.html` sections 28 and 29. Tickets: [`docs/BACKLOG.md`](docs/BACKLOG.md). This file is the milestone sequence, not a promise that later phases are designed in more detail than the founding doc.

A milestone is done when its exit criterion is true, not when a directory exists.

| ID | Milestone | Exit |
| --- | --- | --- |
| P0 | Foundation / governance | CI builds and the minimal sample runs |
| P1 | Editor kernel | Shell, runtime viewport, outliner, inspector, transform tools, save/load, undo/redo. Author a scene without hand-editing raw files |
| P2 | Play-In-Editor | Authoring/runtime isolation, snapshot or copy-on-write, pause, step, eject, possess. Stop restores the authoring state |
| P3 | Large-world core | Hierarchical coordinates, partition, async cell loading, debug view. Traverse more authored content than resident memory |
| P4 | Asset / cook pipeline | Hash derived-data cache, import graph, cooked formats, validation, target builds. A clean checkout reproduces a packaged sample |
| P4M | Material / PBR authoring | Canonical PBR inputs, material graph and IR, masters, instances, functions, texture-set import, library browser. The CLI reproduces the result |
| P5 | GPU-driven renderer | Render graph, GPU scene, compute culling, hierarchical Z, indirect submission. Stress scene shows bounded CPU submission cost |
| P6 | Virtual geometry baseline | Meshlet hierarchy, error metric, paging, parent fallback. Stable LOD with no missing-geometry holes |
| P7 | FPS gameplay | Character, weapon, physics interaction, animation, audio, AI. A real playable loop |
| P8 | Moving frames / vehicles | Vehicle or ship local frame and interior traversal without precision failure |
| P9 | Networked vertical slice | Authority, prediction, interpolation, relevance, dedicated server. Repeatable multiplayer traversal and combat |
| R1 | Microgeometry research | Aperiodic/stochastic detail. Promote only measured wins |
| R2 | Representation virtualization | Mesh, splat, proxy, and procedural switching under one cost/error model |

R1 and R2 are research milestones. They do not block P7–P9 and they are not production features until an ADR says so.

## First vertical slice

The slice is an architecture torture test, not a content demo. From the founding doc:

1. Player spawns in a streamed outdoor range.
2. Walks into a drivable spacecraft or large vehicle.
3. The vehicle moves quickly while the player walks inside.
4. The player fires a weapon and moves physics objects inside.
5. The vehicle crosses partition boundaries.
6. A second client sees the same scenario through a dedicated server.
7. The player exits into a different local frame.
8. The editor can eject and inspect during Play-In-Editor.

Passing that slice requires: no obvious transform jitter at the validated scale, no blocking content IO during normal traversal, credible momentum across a frame transition, a server build with no graphics or editor dependency, no client-authoritative teleport or self-awarded state, a replay of a scripted traversal, and a profiler that shows CPU, GPU, streaming, and network cost.

## Explicit non-goals for the early phases

No marketplace. No finished cinematic suite. No full server mesh. No attempt to duplicate every Unreal feature. No claim that experimental microgeometry is superior to Nanite before objective testing.

## Where the repository is

This roadmap is **not** a linear progress meter. The repository now contains accepted work across foundation, editor, materials/lighting, renderer, meshlets, and virtual-geometry research while other tickets in those same milestone groups remain open.

RFC-0001, the virtual-geometry baseline, is Accepted. Its measured performance follow-ups remain open.

RFC-0002, procedural microgeometry, is still Proposed. The public tree contains only the public reference provider. Private/local acceptance-candidate research exists beyond that public provider, but it is intentionally not represented as public-source completion.

Use [`docs/status/CURRENT.md`](docs/status/CURRENT.md) for the public/private state boundary and [`docs/BACKLOG.md`](docs/BACKLOG.md) for ticket-level status. Do not infer current project status from the old P0-only repository snapshot.
