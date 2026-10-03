# Your first hour

JARVIGEditor is the program you use. It is a Windows application. The editor hosts the same engine a game runs. Creating a project, placing a block, importing a mesh, saving, and playing all go through that engine.

There is no installer yet. You build the editor from this repository.

## What you need

- Windows
- A Rust toolchain with `cargo`
- A GPU and driver that can present a Direct3D 12 window

Node.js is not required to launch the editor. It is only for the older TypeScript prototype.

From the repository root:

```powershell
cargo run --offline --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

The first build compiles the native crates. Later launches reuse that build. Leave off `--self-test`. That flag is an automated check and closes the window itself.

A launch with no project shows the Hub: Recent Projects, New Project, and Open Project. It does not open Lighting Lab. `--project path\to\name.jarvigproject` skips the Hub and opens that project.

## Create a project

1. Choose **File > New Project**, then **Blank**, **Landscape**, **Third Person**, or **First Person**.
2. Pick a parent folder and a name.

JARVIG writes `{folder}/{name}/{name}.jarvigproject`, plus `Content/`, `Config/`, and the startup level `Content/Levels/Main.jarviglevel`.

| Template | What the level contains | Where it opens |
| --- | --- | --- |
| Blank | World Settings and the bootstrap environment | Level |
| Landscape | The same level. No heightfield yet | Land |
| First Person | World Settings and one Player Start | Level |
| Third Person | World Settings and one Player Start | Level |

Blank keeps the environment already stored on World Settings: intensity 0.20 and two hemisphere colors. It does not add a mesh, a directional light, or a character. First Person and Third Person write `Content/Players/DefaultPlayer.jarvigplayer` with an empty character path. They do not invent a camera, a controller, or input. The pawn and the controller stay `none`.

Older template words `empty`, `terrain`, `land`, `fps`, and `third-person` still select these four.

**File > Open Project...** opens an existing `.jarvigproject`. **File > Save** writes the open level. Save after you place anything you want to keep.

## Move the view

The perspective camera is the editor view. It is not an object in the level. Flying it does not change the saved world.

| Input | Result |
| --- | --- |
| Right mouse | Look |
| W A S D | Fly along that look |
| Q / E | Down / up |
| Shift | Faster, while held |
| Mouse wheel | Change fly speed |
| Middle mouse | Pan |
| Alt + left mouse | Orbit |
| Alt + wheel | Dolly |
| F | Frame the selection |

With the viewport focused and the mouse not captured, `1` selects, `2` moves, and `3` rotates. `4` reports that scale is unavailable. Those keys do not steal W A S D Q E.

## Build a room

This is a graybox. It uses blocks, which are parametric solids. The level stores the size of each solid. The triangles are rebuilt from that record.

1. Stay on a Blank project, in Level.
2. Choose **Create > Block**, or the Block button on the toolbar. The first solid is a 2 m cube at scene-local (0, 1, −4). Another block steps 2.5 m along +X.
3. Click a face. **Extrude** appears on the toolbar. Click it, pull the arrow, then **Apply**. A typed amount in the inspector is exact. A drag steps by 0.05 m.
4. Double-click the solid to select the whole object. **Edit > Duplicate** (Ctrl+D) makes another. Press `2` and drag the move gizmo.
5. Repeat until you have a floor and four walls. **File > Save**.

Collision for a block is the analytic box of its size. There is no physics solver in this build, so the room does not push a character around. [Blocks](manual/modeling.md) is the full modeling page. [The editor](manual/editor.md) covers selection and undo.

## Look through a camera

A new project does not grow a pawn. Play will not fly the editor camera for you.

1. **View > Create Camera Actor**.
2. Move that camera with the gizmo until it looks through the room. Select it in the viewport or the outliner.
3. **View > Set Selected as Startup Camera**.
4. **Play > Play In Editor**.

Play shows the runtime copy through that camera. The inspector is read-only. **Stop** (toolbar or Play) drops the runtime copy and returns to the authored level. Play does not save. Undo refuses while Play is running.

If the output log says play has no startup camera, the editor camera was left unused. Set the startup camera and press Play again.

## Save, close, reopen

**File > Save** writes `Content/Levels/Main.jarviglevel`. Closing the editor does not write that file for you. The recent list for the Hub is `%LOCALAPPDATA%\JARVIG\Hub\recent-projects.json`. It is not inside the game project.

Next: [import a model](manual/content.md), [sculpt terrain](manual/worlds.md), or [run the game outside the editor](manual/play.md).
