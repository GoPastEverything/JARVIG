# Content browser

The lower-left panel lists the open project's game assets. It reads the asset registry. It does not walk the disk itself, and it does not decode a source file just to draw a row.

`jarvig_core` writes `Intermediate/AssetRegistry.jarvigregistry` (schema `jarvig.registry`, version 1). The record keeps a stable `AssetId`, the project-relative path, kind, subtype, byte size, modified time, and any triangle count already stored in `Content/Assets/catalog.jarvigassets`. A mesh that is already in that catalog keeps the catalog id, so the level and the browser name the same asset. A second open reuses those rows when the path, size, and modified time still match. It does not reimport the glTF.

Visible roots are the project's content directory, its config directory, and `Source/` when that folder exists. `Intermediate/`, `Saved/`, `DerivedDataCache/`, `target/`, `Build/`, `docs/`, and `.git/` stay out. `catalog.jarvigassets` stays out. Files that are not a texture, model, material, level, prefab, audio, shader, source file, or config file stay out. Kind comes from the file type and the name, not only the folder. `stone_n.png` is a normal map even if it does not sit in a Normal folder.

The panel has type filters, a search box, and a metadata line. A texture or an already-resident model base color is reduced to a 128-pixel thumbnail with `thumbnail_rgba`. The full image is not kept as a browser copy and is not uploaded for the sake of the panel. Lighting Lab's loose `Content/Textures` folder is empty; the shop row uses the base color that the mesh library already holds after project open.

Dragging a model into the viewport places one mesh actor through `place_existing_mesh` and records that actor for Ctrl+Z or Edit > Undo Mesh Placement. Levels, source files, and config files do not spawn. Material and texture assignment onto a slot is not in this slice.

The scan runs on the existing background job queue as `Scan Project Content`. `--content-check` opens Lighting Lab, checks the list, places the townshop mesh, undoes that placement, and prints `CONTENT_BROWSER_PASS`.
