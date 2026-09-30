# Editor navigation icons

Source art for editor chrome. Not scene content, not a cooked package, and not an `EntityUuid`.

The asset database is JRV-0066 and does not exist yet. Do not import these files through a mesh, a material, or a content browser and call that the database. `docs/assets/asset-system.md` is the future handle system.

`JARVIGEditor` decodes the PNGs with Windows Imaging and draws them on the toolbar at about 20 pixels. That is current, not a future plan. Select, Translate, Rotate, and Scale are one tool group: the active icon is accent-tinted, the other three are gray. They do not move actors. That movement is JRV-0065 and is not started. Focus frames the current selection. Play, pause, and stop are visible and not a play session. The dark shell is Editor theme v1. Do not treat these files as unused.

`manifest.json` lists the navigation set: select, translate, rotate, scale, play, pause, stop, fly, pan, orbit, focus, frame all, grid snap, local/world, speed, maximize. SVG is 96×96. PNG is 128×128. Backgrounds are transparent. Colors are `#D8E5F2`, `#4AB4FF`, and `#7F93A8`.

Extra PNGs in this folder (gear, undo, redo, eye, trash, save, lock, folder, copy, hierarchy) are reserved chrome from the same pack. They are not wired.

Translate, rotate, and scale are stored for JRV-0065. The Perspective camera does not use them. Flying the view is RMB and WASD, not those icons.
