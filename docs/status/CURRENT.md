# JARVIG current state

JARVIG is a native multi-platform engine. This repository is the public source. No open-source license is granted. All rights are reserved until a human lead chooses a license and records it.

Users start at [getting-started.md](../getting-started.md): a new project, glTF or GLB import, save, and play.

The editor cold start is:

```text
cargo run --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

## Public source state

The public tree contains the native editor, renderer, material and lighting work, meshlets, and the accepted RFC-0001 virtual-geometry baseline. RFC-0001's accepted scope is the hierarchy, parent coverage, screen-space LOD selection, persistent sidecar, and composition with frustum culling and conservative occlusion. Its measured performance follow-ups remain open.

Procedural detail defaults off. The RFC-0002 provider included in this public tree is intentionally limited to the public reference path: disabled detail or a flat reference triangle with zero displacement. The public source does **not** contain the private/local experimental provider used by current RFC-0002 research.

RFC-0002 remains **Proposed**. A public clone must not be represented as reproducing private/local RFC-0002 results unless the corresponding implementation or a reproducible public artifact is released.

Page streaming is not part of this tree.

The Lighting Lab mesh `samples/lighting-lab/Content/Meshes/townshop.glb` is not in this repository because it is larger than GitHub's file limit. The rest of the Lighting Lab project is included.

## Private/local research state

The active private/local development tree has progressed beyond the public RFC-0002 reference provider. Internal harnesses have recorded:

- `RFC0002_MICRO_PASS`
- `RFC0002_TRANSITION_PASS`
- `RFC0002_ASYNC_PASS`

Those runs exercise an acceptance-candidate procedural-detail path, including deterministic return behavior, screen-space transition behavior, and asynchronous generation/publication. They are **not** a public-source acceptance result and do not make RFC-0002 Accepted.

The exact provider algorithm, distribution rule, topology construction, and other unpublished implementation details are outside this public repository. Public documentation should describe the boundary and measured behavior without pretending those private details are present in `master`.

See [RFC-0002](../rfc/RFC-0002-procedural-microgeometry.md) and [Surface detail research](../research/aperiodic-detail.md).
