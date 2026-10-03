# Project hub

ADR-0060. The hub is the host above the editor. Level, Land, and Character stay where they are.

## Startup

```text
JARVIG or JARVIGEditor, no project and no harness flag
        |
        v
JARVIG Hub
        |
        +-- close --> exit
        |
        +-- .jarvigproject
                |
                v
        read the manifest
        splash, for work the editor actually does
        startup level
        restore the workspace
        Level, unless the template or workspace says otherwise
```

`JARVIGEditor --project path.jarvigproject` skips the hub.

These flags also skip the hub, and they do not open a sample in its place:

```text
--self-test
--frames
--lod-capture
--rfc0001
--rfc0001-runtime
--content-check
--bind-pose-shots
```

`--self-test` is still the two-triangle bootstrap. A harness that needs Lighting Lab passes `--project samples/lighting-lab/LightingLab.jarvigproject`. `--content-check` needs that sample because it looks for townshop. No development sample is the fallback project.

## What the hub loads

Enough of `.jarvigproject` to show the name, the path, the engine version, the startup level, and the last-opened time. A thumbnail path may be stored. This slice does not decode it.

The recent list is `%LOCALAPPDATA%\JARVIG\Hub\recent-projects.json`, schema `jarvig.hub-recent` version 1. If `LOCALAPPDATA` is missing, the file is `Saved/Hub/recent-projects.json` next to the process. It is not inside a game project. A missing path stays missing until the user removes it or locates the file. The hub does not scan the disk.

## Templates

New Project asks for a template, a name, and a folder. The project file is `{folder}/{name}/{name}.jarvigproject`. The writer is `create_project_at` in `jarvig_core`. File > New Project uses that same writer. The menu words are Blank, Landscape, Third Person, and First Person.

| Template | Level | Workspace when `workspace.json` is absent |
| --- | --- | --- |
| Blank | World Settings and the bootstrap environment | Level |
| Landscape | The same level. No heightfield yet | Land |
| First Person | World Settings and one Player Start | Level |
| Third Person | World Settings and one Player Start | Level |

The startup level file is `Content/Levels/Main.jarviglevel`. Blank keeps the environment already stored on World Settings: intensity 0.20 and the two hemisphere colors. It does not add a light, a probe, a mesh, a triangle, Base, Base Male, or townshop. Land can create terrain after that.

First Person and Third Person write `Content/Players/DefaultPlayer.jarvigplayer` with an empty character path. Empty means no character has been chosen. They do not copy a body, and they do not invent a controller, a camera, or input. The editor log says so. Older words `empty`, `terrain`, `land`, `fps`, and `third-person` still select these templates. The saved word is `blank`, `landscape`, `first-person`, or `third-person`.

Settings are `jarvig.settings` version 1 with the controller and the pawn left `none`.

## Splash

The splash is the editor, after a project is chosen. The lines name work that is happening:

```text
Reading project manifest
Loading startup level
Loading meshes
Loading materials
Initializing renderer
Restoring editor workspace
Scanning asset registry
```

The registry line means the scan was queued on the existing job queue. The content browser fills when that job finishes. The splash does not mark it done early.

## Later, not this slice

Marketplace, accounts, an updater, a plugin store, and an engine-version manager can sit in the hub later. A Windows file association for `.jarvigproject` can come later. `JARVIG.exe` already forwards a project path to a sibling `JARVIGEditor.exe`.
