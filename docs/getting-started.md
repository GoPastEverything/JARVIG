# Make a game with JARVIG

JARVIGEditor is the program you use. It is a Windows application. The editor hosts the same engine a game runs. Import, placement, save, and play go through that engine.

This page is the current authoring path. There is no installer yet. You build the editor from this repository.

## What you need

- Windows
- A Rust toolchain with `cargo` ([rustup](https://rustup.rs/))
- A GPU and driver that can present a Direct3D 12 window

Node.js is not required to run the editor.

From the repository root:

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
```

The first build compiles the native crates. Later launches reuse that build. Leave off `--self-test`. That flag is an automated check and closes the window itself.

The editor opens on the Lighting Lab sample when it can find `samples/lighting-lab/LightingLab.jarvigproject` by walking up from the current directory. The townshop model is not in this repository. GitHub rejects that file because it is 185 MB. Lighting Lab still opens. The shop actor has no mesh until you import one.

## Create a project

1. Choose **File > New Project...**
2. Pick a folder and a name. The file must end in `.jarvigproject`.
3. JARVIG writes the project file, `Content/`, `Config/`, and a startup level at `Content/Levels/<name>.jarviglevel`.

A new level starts with World Settings, a directional light, a blue point light, a warm spot light, and a reflection probe. It does not start with a floor or a character. You add those.

**File > Open Project...** opens an existing `.jarvigproject`. **File > Save** writes the open level. Save after you place anything you want to keep. Importing a mesh does not, by itself, rewrite the level file.

## Move the editor camera

The perspective camera is the editor view. It is not an object in the level. Flying it does not change the saved world.

- Right mouse button looks around
- W A S D flies along that look
- Q moves down, E moves up
- Shift boosts for as long as it is held
- Mouse wheel changes the saved fly speed
- Middle mouse button pans
- Alt and left mouse button orbits
- Alt and the wheel dollies
- F frames the selection

With the viewport focused and the mouse not captured, 1 selects, 2 translates, and 3 rotates. Scale is not available. The gizmo writes translation and rotation through the engine. Select an object in the viewport or the outliner. The inspector edits that object.

## Add a model

The mesh format is glTF 2.0, either `.glb` or `.gltf`. FBX is not an import path.

1. Open a project first. Import refuses to start without one.
2. Stop play if it is running. Import does not write into the play session.
3. Choose **File > Import Mesh...** and pick the file.

What that does:

- The file you picked stays where it is. JARVIG does not modify it.
- A copy is stored at `Content/Meshes/`.
- The engine builds its own mesh under `Intermediate/Meshes/`. The level stores an asset id, not the vertices and not the original path.
- One actor is placed in the level, using the materials that came in with the mesh.
- A file that cannot be imported adds nothing.

Then choose **File > Save**. Without that save, the actor is only in the open session.

The Content Browser lists project assets after its scan finishes. Drag a **Model** onto the perspective view to place another actor where the cursor hits the ground plane. Ctrl+Z, or **Edit > Undo Mesh Placement**, removes that drop. Dragging a level does not spawn it. Dragging a material or a texture does not assign it yet. Those slots are not wired.

Meshes larger than 250,000 triangles can still sit in the level and receive shadows. They are left out of the shadow-map redraw.

## Play

1. **View > Create Camera Actor**.
2. Select that camera.
3. **View > Set Selected as Startup Camera**.
4. **Play > Play In Editor**.

Play runs a runtime copy of the level. The editor camera is not the play camera. If no startup camera is set, the output log says so. Stop returns you to the authored level. Play does not save.

The default pawn is a free-fly camera: W A S D move, the mouse looks, E or Space moves up, Q or Ctrl moves down, Shift sprints, Esc pauses.

## Run it outside the editor

Build the player once, next to the editor:

```powershell
cargo build --manifest-path native/Cargo.toml -p jarvig_game --bin JARVIGGame
```

`cargo run` of the editor and `cargo build` of the player need to use the same target directory so `JARVIGGame.exe` sits beside `JARVIGEditor.exe`. From this repository that is `native/target/debug/`.

Then, with a project open:

- **Play > Run Standalone** starts `JARVIGGame.exe` on that project.
- **Build > Build Project** copies a loose development game into the project's `Build/Windows-x64` folder: `Game.exe`, the project file, and `Content`. There is no pak, no cook step, and no installer. **Build > Build & Run** stages that folder and starts `Game.exe`.

## Limits that are still real

- Windows is the editor you can use today.
- There is no installer in this repository.
- Models are glTF or GLB only.
- Material and texture drops do not assign slots.
- Scale is not an authoring tool.
- The townshop mesh is not in the Git clone.
