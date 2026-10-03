# JARVIG current state

JARVIG is a native multi-platform engine. This repository is the public source. No open-source license is granted. All rights are reserved until a human lead chooses a license and records it.

Procedural detail defaults off. The included provider draws nothing, or one flat reference triangle with no displacement. The private surface rule is not in this repository.

The editor toolbar has three workspaces over one level: Level, Land, and Character. Level is the authored world. Land creates and sculpts one heightfield. Character isolates a rig when the level has one. Those are editor views of the same project. They are not a second copy of the world.

A Block is a parametric solid. Extrude, Inset, and Bevel share one session: a toolbar icon, an Amount field, and one viewport arrow. Dragging the arrow, or the session face, changes the amount in 0.05 m steps. A typed amount is exact. Apply stores one editor undo entry. Cancel restores the solid from when the session opened. The Move and Rotate gizmo stays hidden while that session is open. Choosing Move or Rotate on the toolbar closes the preview.

Selection on a parametric solid is Auto, Object, or Face. A single click selects the face in Auto and in Face, and the whole object in Object. A double-click selects the owning object and leaves the mode alone. Shift-click adds. Ctrl-click toggles. In Select mode, a drag on empty space draws a marquee: left to right selects objects fully inside, and right to left selects objects the rectangle touches. Editor undo and redo are Edit menu commands, Ctrl+Z and Ctrl+Y. One drag is one undo. The feature list shows the current size and any nonzero bevel or inset. It does not replay earlier steps.

Users start at [getting-started.md](../getting-started.md): a new project, a room of blocks, glTF or GLB import, save, and play. The creator manual is [manual/README.md](../manual/README.md). Build the local documentation site with `pwsh -File scripts\build-docs.ps1` and open `docs/site/index.html`. That folder is generated and gitignored. It is a reader, not a second authority.

The editor cold start is:

```text
cargo run --offline --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

Page streaming is not part of this tree.

The Lighting Lab mesh `samples/lighting-lab/Content/Meshes/townshop.glb` is not in this repository because it is larger than GitHub's file limit. The rest of the Lighting Lab project is included. Base character meshes and Ghost City stay out of this repository.
