# Limits

This is the honest edge of the build the manual describes. A later version can move a line. Until the architecture page says the system exists, do not write a tutorial as if it does.

## You can do this

- Create and open a project from the Hub, and save a level.
- Fly the editor camera, select, move, and rotate.
- Author parametric blocks, including extrude, inset, and bevel, with undo.
- Import glTF or GLB, browse the asset registry, and drag a model into the level.
- Create a flat heightfield and sculpt it in Land.
- Open the joint pose on the sample characters.
- Edit the environment, and edit lights that already exist in a level.
- Play a runtime copy through a startup camera.
- Stage a loose Windows player next to the editor.

## This build stops here

| Area | Where it stands |
| --- | --- |
| Installer | You build from the repository with `cargo`. |
| Operating systems | The editor you run is Windows. |
| Scale | The tool stays disabled. Size is the solid's extents. |
| Mesh import | glTF 2.0 only. |
| More primitives | Plane, Ramp, Cylinder, Sphere, and Wedge are unstarted. |
| Booleans and sketch | Union, subtract, intersect, shell, split, fillet, and array are unstarted. |
| Feature replay | Features shows the current parameters. It does not mute a step and rebuild. |
| Clipboard | Cut, Copy, and Paste are absent. |
| Materials | A block or an imported mesh can take a material. Dropping a material or a texture on a slot does not assign it. |
| New lights | Create does not spawn a light. Lighting Debug isolates lights that are already there. |
| Shadows | Meshes past 250,000 triangles are left out of the shadow-map redraw. |
| Physics | Block collision is the analytic box. Terrain queries read the heightfield. Nothing is solved as a rigid body. |
| Pawns | New projects leave the pawn as `none`. |
| Animation | Joints store a pose. Clips, skinning, IK, and ragdolls are absent. |
| Audio, AI, replication | Not in this build. |
| Scripting | No language is chosen. |
| Streaming | The editor loads the open level. Page streaming is unstarted. |
| Terrain extras | No erosion, water, foliage, voxels, or generated procedural world. |
| Procedural detail | Defaults off. Exact solids are not sent to it. The private surface rule is withheld from this manual. |
| Cook and pak | Build Project copies loose files. |
| Marketplace | Accounts, a plugin store, an updater, and an engine-version manager are later. |

## Samples that stay local

Lighting Lab can omit its shop mesh in a public clone because that file is about 185 MB. Ghost City and the base-character meshes stay on the machine that imported them. Credits for those downloads stay beside the samples. Do not add them to a published game tree from this manual.

## Diagnostics

Rendering debug views, meshlet colors, and the procedural-detail visualizations are for inspecting the renderer. **View > Reset Rendering Debug** is the way back to the shaded picture. A colored overlay is not the game's look. Harness flags such as `--self-test` close themselves when they finish. They are not how you open a project.

## License

JARVIG's own license has not been chosen. Third-party notices live in `docs/legal/`. Absence of a license file is not a grant to relicense the tree.
