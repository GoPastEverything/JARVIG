# ADR-0060 — The hub chooses a project before the editor starts

Status: Accepted
Date: 2026-10-01

## Context

Launching JARVIGEditor with no project opened Lighting Lab. That sample was useful while the renderer was the thing under test. It is the wrong product: a person who starts the engine has not asked for a lighting scene, a townshop mesh, or any other development sample.

The editor already hosts the engine in-process. ADR-0018 keeps import, cook, and project files as engine APIs. ADR-0045 keeps `.jarvigproject` as the project document and refuses absolute paths inside that file. ADR-0058 keeps an empty world as World Settings. ADR-0059 keeps a player definition separate from a character asset. None of those decisions named the screen that should appear before a project exists.

## Decision

`JARVIG.exe` is the project hub. `JARVIGEditor.exe` is the editor. They may share code. The hub does not construct an engine world, scan project content, build meshlets, or initialize a renderer in order to draw itself.

A launch with no project and no harness flag shows the hub: Recent Projects, New Project, and Open Project. Closing it exits without starting the editor. `JARVIGEditor --project <path.jarvigproject>` skips the hub and opens that file. `--self-test` still uses the two-triangle bootstrap and does not open a project. Other harness flags do not silently open a sample. Lighting Lab opens only when its `.jarvigproject` is chosen.

`.jarvigproject` stays schema `jarvig.project` version 1. This decision does not add Editor or Game fields to that file. The startup level path is the default map. The workspace stays in `Saved/Editor/template.txt` and `Saved/Editor/workspace.json`. The player definition stays in the level's Player Start and the `.jarvigplayer` file.

Recent projects are a hub list, schema `jarvig.hub-recent` version 1, outside any game project. On Windows the file is `%LOCALAPPDATA%\JARVIG\Hub\recent-projects.json`. A path that is not a file stays in the list as missing. The hub does not scan the disk and does not substitute another project.

New Project asks for a template, a name, and a location, then writes `{location}/{name}/{name}.jarvigproject`. The templates in this foundation are Blank, First Person, Third Person, and Landscape. Each one is ordinary project files plus new ids. The editor's File menu calls the same writer. Older template words `empty`, `terrain`, `land`, `fps`, and `third-person` still parse. The file stores `blank`, `landscape`, `first-person`, or `third-person`.

Blank is World Settings and the bootstrap environment already stored there: intensity 0.20 and the two hemisphere colors. It adds no light, probe, mesh, triangle, character, or townshop. It opens in Level. Land can create terrain afterward. Landscape is that same level with the word `landscape`, so Land opens when `workspace.json` is absent. It does not prebuild a heightfield.

First Person and Third Person add one Player Start and `Content/Players/DefaultPlayer.jarvigplayer`. The character path may be empty, which means no character has been chosen. That is a clarification of `jarvig.player` version 1, not a version bump. Absolute paths still fail. These templates do not copy Base or Base Male, and they do not add a controller, a camera, or input. The editor says so when the project opens. They open in Level.

The startup level path for these templates is `Content/Levels/Main.jarviglevel`. Settings are `jarvig.settings` version 1 with the controller and pawn left `none`.

After a project is chosen, the editor's existing progress reports the work it actually does: reading the manifest, loading the startup level, loading meshes, loading materials, initializing the renderer, and restoring the workspace. The asset registry scan is queued on the existing job queue. The splash does not call that scan finished while it is still running. The splash is not shown on the hub.

Marketplace, accounts, an updater, a plugin store, and an engine-version manager are not in this slice. The hub is a separate host so they can be added there later. A Windows association for `.jarvigproject` is later. `JARVIG.exe` can already start the editor with `--project` when a sibling `JARVIGEditor.exe` is present.

## Consequences

A cold `JARVIGEditor` launch shows the hub. It does not open Lighting Lab, Base Characters, or an in-memory lab. Samples stay samples. Level, Land, and Character are unchanged. ADR-0058's empty world is the Blank template. ADR-0059's player file may store an empty character path until one is chosen.

No JRV ticket is accepted by this decision. The hub window has not been looked at on a GPU, because the hub does not present one. No new C ABI was added.
