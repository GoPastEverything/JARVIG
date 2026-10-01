# Surface detail research

Status: research. Promotion is governed by [RFC-0002](../rfc/RFC-0002-procedural-microgeometry.md).

Procedural detail defaults off. The public repository includes only a disabled provider and a flat reference provider. That public reference path is intentionally not the complete private/local research implementation.

## Research question

Can deterministic aperiodic surface information provide useful fine geometric detail without obvious short-period repetition, while remaining compatible with JARVIG's screen-space LOD decisions, ordinary material fallback, caching, and asynchronous editor/runtime work?

The engine-level requirements are:

- deterministic output for the same seed and surface address;
- ordinary materials below the detail threshold;
- bounded generated detail;
- no cracks or missing base geometry when detail appears or disappears;
- no partial publication while a replacement is still being built or uploaded;
- stale work may be discarded safely when the camera or requested patch set changes;
- the accepted RFC-0001 hierarchy remains a separate system.

## Internal acceptance-candidate evidence

The private/local development tree has produced internal harness passes named:

- `RFC0002_MICRO_PASS`
- `RFC0002_TRANSITION_PASS`
- `RFC0002_ASYNC_PASS`

Those runs are evidence about the private/local candidate, not proof that a public clone contains the same provider. RFC-0002 remains Proposed until the human acceptance step is recorded.

The public repository intentionally omits the active provider's exact distribution rule, topology construction, subdivision logic, cache/addressing choices, and other unpublished implementation details.

## Einstein / hat terminology

The mathematical term *einstein* and the 2023 hat monotile are prior mathematical work, not a JARVIG invention. The discovery is associated with David Smith, Joseph Samuel Myers, Craig S. Kaplan, and Chaim Goodman-Strauss.

JARVIG's work is the engine-side research and implementation built around aperiodic procedural detail: deciding when detail is perceptually relevant, generating bounded geometry, composing it with the existing surface, reusing deterministic results, and publishing completed work without destabilizing the base mesh.

Public descriptions should use language such as **"Einstein-hat-derived/inspired aperiodic procedural microgeometry"** only when that wording matches the actual provider being discussed. They should not say that JARVIG invented the Einstein hat or the mathematical einstein concept.

## Disclosure rule

A private benchmark may be described as a private/internal result, but it must not be presented as reproducible from `master` until the matching implementation or a reproducible public artifact is released. Public benchmark claims should record hardware, resolution, feature state, comparison baseline, and the exact revision or artifact that produced the result.
