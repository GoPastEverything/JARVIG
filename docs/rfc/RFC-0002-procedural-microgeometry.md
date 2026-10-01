# RFC-0002 — Procedural microgeometry

Status: **Proposed**.

RFC-0002 evaluates optional procedural surface detail that can add bounded fine geometry only when screen-space error says the base representation no longer carries enough local detail. The ordinary material and base-geometry path remains the fallback.

## Public implementation state

Procedural detail defaults off.

The public source tree intentionally contains only:

- a disabled provider;
- a flat reference provider with zero displacement;
- the public query/provider contract and fallback path.

The active private/local research provider is **not** distributed in this repository. A public clone therefore does not reproduce the private/local RFC-0002 candidate merely because the interface is present.

No private distribution rule, aperiodic construction rule, subdivision strategy, topology builder, cache/addressing method, or equivalent unpublished implementation detail is specified by this RFC.

## Required behavior

Any provider promoted under RFC-0002 must demonstrate all of the following:

1. **Determinism** — the same provider revision, seed, and surface address return the same detail.
2. **Screen-space gating** — detail appears only while projected error justifies it and returns to zero/fallback when it does not.
3. **Base preservation** — the accepted RFC-0001 hierarchy and ordinary base surface remain valid independently of microgeometry.
4. **Bounded generation** — generated vertices, triangles, displacement, and work are constrained by explicit budgets.
5. **No partial publication** — a replacement patch set becomes visible only after a complete renderable result is ready.
6. **Stale-work rejection** — camera or request changes may invalidate in-flight work without publishing stale geometry.
7. **Stable return path** — moving away and back restores the expected detail/fingerprint rather than accumulating or drifting geometry.
8. **Measured cost** — generation, upload, memory, CPU render cost, and GPU cost are reported separately where measurable.
9. **Feature-off equivalence** — disabling the feature returns the ordinary base path without changing authored assets.
10. **No hidden dependency** — acceptance does not depend on page streaming or a second geometry system unless the RFC is explicitly revised.

## Internal acceptance-candidate evidence

The private/local development tree has recorded the following automated harness verdicts:

- `RFC0002_MICRO_PASS`
- `RFC0002_TRANSITION_PASS`
- `RFC0002_ASYNC_PASS`

The local runs exercise generated microgeometry, a moving-camera transition across the detail threshold, deterministic return behavior, and asynchronous generation/publication with stale-work rejection.

These are **internal acceptance-candidate results**. They do not change this RFC to Accepted and they are not represented as reproducible from the public `master` branch.

The private/local work has also been exercised on Intel UHD-class integrated graphics. That hardware fact is useful context, but it is not a performance claim by itself. Public performance claims require stated resolution, frame time or FPS, feature state, comparison baseline, and a reproducible revision/artifact.

## Aperiodic / Einstein-hat research

The mathematical `einstein` concept and the 2023 hat monotile are prior mathematical work associated with David Smith, Joseph Samuel Myers, Craig S. Kaplan, and Chaim Goodman-Strauss. JARVIG does not claim invention of that mathematical result.

For this RFC, **JARVIG Einstein Surface** names the JARVIG engine integration being evaluated, not the mathematical discovery itself.

The JARVIG-specific contribution under evaluation is the real-time engine architecture that combines Einstein-hat-derived aperiodic surface information with:

- projected screen-space relevance;
- bounded connected generated microgeometry;
- the accepted RFC-0001 hierarchy and ordinary material fallback;
- asynchronous generation and chunked upload work;
- stale-build rejection;
- complete-result/frame-boundary publication;
- deterministic reuse and return behavior;
- independent base-geometry and generated-detail accounting.

The intent is generated geometry that participates in the renderer as local surface detail while leaving the authored/base representation valid independently. It is not merely a non-repeating texture pattern and it is not a claim to have invented the Einstein hat.

The public RFC intentionally stops at this architectural boundary. The active private/local provider's exact aperiodic construction, subdivision/refinement method, topology rules, cache/addressing implementation, and placement heuristics remain unpublished.

Public wording must distinguish the mathematical prior work from JARVIG's engine integration. A specific provider may be described as Einstein-hat-derived or Einstein-hat-inspired only when that is accurate for the provider revision being discussed.

## Benchmark and disclosure policy

No private benchmark number should be presented as a public-source result unless the public repository contains enough code/data to reproduce it or a reproducible artifact is published alongside the claim.

A public RFC-0002 benchmark should include, at minimum:

- JARVIG revision or immutable artifact identifier;
- provider/feature state;
- test scene and camera path;
- output resolution;
- CPU and GPU identification;
- base-triangle and generated-microtriangle counts;
- generation and upload cost;
- CPU frame cost and GPU frame cost when available;
- memory/residency cost;
- feature-off comparison;
- visual acceptance evidence;
- hashes/fingerprints for deterministic return tests.

## Promotion

RFC-0002 remains Proposed until a human acceptance record says otherwise.

An internal pass is necessary evidence, not the acceptance act. Promotion must name exactly which provider and revision are accepted, what implementation is public or private, what benchmark evidence supports the decision, and which measured follow-ups remain open.
