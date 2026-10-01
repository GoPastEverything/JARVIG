# JARVIG current state

JARVIG is a native multi-platform engine. This repository is the public source. No open-source license is granted. All rights are reserved until a human lead chooses a license and records it.

Procedural detail defaults off. The included provider draws nothing, or one flat reference triangle with no displacement. The private surface rule is not in this repository.

The editor toolbar has three workspaces over one level: Level, Land, and Character. Level is the authored world. Land creates and sculpts one heightfield. Character isolates a rig when the level has one. Those are editor views of the same project. They are not a second copy of the world.

Users start at [getting-started.md](../getting-started.md): a new project, a terrain, glTF or GLB import, save, and play.

The editor cold start is:

```text
cargo run --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

Page streaming is not part of this tree.

The Lighting Lab mesh `samples/lighting-lab/Content/Meshes/townshop.glb` is not in this repository because it is larger than GitHub's file limit. The rest of the Lighting Lab project is included. Base character meshes and Ghost City stay out of this repository.
