# JARVIG current state

JARVIG is a native multi-platform engine. This repository is the public source. No open-source license is granted. All rights are reserved until a human lead chooses a license and records it.

Procedural detail defaults off. The included provider draws nothing, or one flat reference triangle with no displacement.

The editor cold start is:

```text
cargo run --offline --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

Page streaming is not part of this tree.

The Lighting Lab mesh `samples/lighting-lab/Content/Meshes/townshop.glb` is not in this repository because it is larger than GitHub's file limit. The rest of the Lighting Lab project is included.
