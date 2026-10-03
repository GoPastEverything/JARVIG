# Models and content

The mesh format JARVIG imports is glTF 2.0, `.glb` or `.gltf`. You need a project open, and Play must be stopped. Import writes into the project. It does not write into the play session.

## Import

1. **File > Import Mesh...**
2. Pick the file.
3. **File > Save**.

What that does:

- The file you picked stays where it is. JARVIG does not modify it.
- A copy is stored under `Content/Meshes/`.
- The engine builds its own mesh under `Intermediate/Meshes/`. The level stores an asset id, not the vertices and not the original path.
- One actor is placed in the level, using the materials that came in with the mesh.
- A file that cannot be imported adds nothing.

Without **File > Save**, the actor exists only in the open session.

Imported meshes do not become parametric solids. You can move, rotate, and assign a material. You cannot extrude a face of an imported mesh. FBX is not an import path.

## Content browser

The lower-left panel lists game assets of the open project. It reads `Intermediate/AssetRegistry.jarvigregistry` (schema `jarvig.registry`, version 1). It does not walk the disk itself, and it does not decode a source file just to draw a row.

Each row keeps a stable asset id, the project-relative path, a kind, a subtype, the byte size, the modified time, and a triangle count when the catalog already stored one. Kind comes from the file type and the name. A file named `stone_n.png` is a normal map even when it does not sit in a folder called Normal.

The scan runs on the editor's background job queue as Scan Project Content. The splash can say the scan was queued before the panel fills. Opening the same project again reuses rows whose path, size, and modified time still match. It does not reimport the glTF.

Visible roots are the project's content directory, its config directory, and `Source/` when that folder exists. These stay out of the panel: `Intermediate/`, `Saved/`, `DerivedDataCache/`, `target/`, `Build/`, `docs/`, `.git/`, and `catalog.jarvigassets`.

The panel has type filters, a search box, and a metadata line. A texture, or a model base color that is already resident, can show a small thumbnail. The panel does not keep a second full-size copy for itself.

## Place another copy

Drag a **Model** onto the perspective view. JARVIG places one mesh actor where the cursor hits the ground plane. That placement is one Place Mesh transaction. Ctrl+Z undoes it with your other edits.

Dragging a level, a source file, or a config file does not spawn an actor. Dragging a material or a texture does not assign it to a slot. Those assignments are not wired.

## Shadows and size

A mesh can sit in the level and receive shadows above 250,000 triangles. It is left out of the shadow-map redraw past that count. The shadow pass still has its own caster budget. A huge prop in the level is not a promise that it draws a shadow map.

## What is not a project asset

The editor's own icons live under `assets/editor/ui/` in the engine repository. They are chrome, not content in your game project. Samples under `samples/` are projects you can open. Ghost City and the base-character meshes are local sample content. Do not treat them as files to publish with a game.

Lighting Lab is `samples/lighting-lab/LightingLab.jarvigproject`. It is a sample, not the project the Hub opens on an empty launch. The shop mesh in that sample is large. A clone that omitted it still opens the level. The shop actor has no mesh until you import one.
