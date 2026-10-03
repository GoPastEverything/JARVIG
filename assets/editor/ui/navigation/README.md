# Editor navigation icons

Source art for editor chrome. Not scene content, not a cooked package, and not an `EntityUuid`.

The asset database is JRV-0066 and does not exist yet. Do not import these files through a mesh, a material, or a content browser and call that the database. `docs/assets/asset-system.md` is the future handle system.

`JARVIGEditor` decodes the PNGs with Windows Imaging. The toolbar draws each one at about 26 pixels with a short caption underneath. That is current, not a future plan. Select, Move, Rotate, and Scale are the mesh tools. Move is the caption on the Translate button. The active icon is accent-tinted. The other three in that group are gray. Scale stays dimmed because a spatial frame has no scale. Select picks. Translate and Rotate drag the gizmo. Block runs Create Block and does not stay selected. Focus frames the current selection. Play, Pause, and Stop run the in-editor session. The dark shell is Editor theme v1. Do not treat these files as unused.

`manifest.json` lists the navigation set. The wired toolbar icons are bold silhouettes at 128×128, PNG and SVG together. Backgrounds are transparent. Colors are `#D8E5F2`, `#4AB4FF`, and `#7F93A8`. `frame_all` is still the older line drawing and is not on the toolbar. Subdiv, Edge, Extend, Split, and Vertex sit with Extrude, Inset, and Bevel and appear only while a parametric solid is selected.

Extra PNGs in this folder (gear, undo, redo, eye, trash, save, lock, folder, copy, hierarchy) are reserved chrome from the same pack. They are not wired.

Fly, Pan, Orbit, and Speed are reminders. Flying the view is RMB and WASD. Pan is the middle mouse button. Orbit is Alt and the left mouse button. The mouse wheel changes speed. Those buttons do not switch a camera mode.
