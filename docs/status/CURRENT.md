# JARVIG current state

JARVIG is a native multi-platform engine. This repository is the public source. No open-source license is granted. All rights are reserved until a human lead chooses a license and records it.

Procedural detail defaults off. The included provider draws nothing, or one flat reference triangle with no displacement. The private surface rule is not in this repository.

The editor toolbar has three workspaces over one level: Level, Land, and Character. Level is the authored world. Land creates and sculpts one heightfield. Character isolates a rig when the level has one. Those are editor views of the same project. They are not a second copy of the world.

A Block is a parametric solid. Extrude, Inset, and Bevel share one session: a toolbar icon, an Amount field, and one viewport arrow. Dragging the arrow, or the session face, changes the amount in 0.05 m steps. A typed amount is exact. Releasing the mouse commits that drag. Apply remains for a typed amount. Escape or a right-click during the drag restores the solid. The Move and Rotate gizmo stays hidden during Extrude, Inset, and Bevel. Choosing Move or Rotate on the toolbar closes that preview.

Selection on a parametric solid is Auto, Object, Face, Edge, and Vertex. A single click selects the current element. A double-click selects the owning object and leaves the mode alone. Shift-click adds. Ctrl-click toggles. In Select mode, a drag on empty space draws a marquee: left to right selects objects fully inside, and right to left selects objects the rectangle touches. Editor undo and redo are Edit menu commands, Ctrl+Z and Ctrl+Y. One drag is one undo. The feature list shows the current size, any nonzero bevel or inset, and topology edits as labels. It does not replay earlier steps.

While a block is selected, the toolbar also shows Subdiv, Edge, Extend, Split, and Vertex. Subdiv and Split run on the click. Edge, Extend, and Vertex arm a drag. With Move active, the same translate gizmo sits on the selected edge or vertex. Dragging an axis edits that element. Extend extrudes a new face along the axis. A press that misses the gizmo drags on the view plane. Releasing the mouse commits.

Extrude on a stored body pulls the selected cells and adds their side faces. View > Show Grid draws every stored edge. Face, Edge, and Vertex draw that cage on their own. Face and Auto on a stored body leave the object Move arrows off the cells. Switch to Object to move the whole solid. While an Extrude arrow is up, a click drags that region or does nothing. The toolbar paints from a memory bitmap. Einstein surface builds use one dedicated worker, separate from parent-mesh, content, and terrain jobs. Exact solids still draw no relief. The status line times face, edge, and vertex picks, validation, mesh rebuild, inspector refresh, and cage draw.

Users start at [getting-started.md](../getting-started.md): a new project, a room of blocks, glTF or GLB import, save, and play. The creator manual is [manual/README.md](../manual/README.md). The rendered site is `docs/site/index.html`. Rebuild it with `pwsh -File scripts\build-docs.ps1`. It is a reader, not a second authority.

The editor cold start is:

```text
cargo run --offline --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

Page streaming is not part of this tree.

The Lighting Lab mesh `samples/lighting-lab/Content/Meshes/townshop.glb` is not in this repository because it is larger than GitHub's file limit. The rest of the Lighting Lab project is included. Base character meshes and Ghost City stay out of this repository.
