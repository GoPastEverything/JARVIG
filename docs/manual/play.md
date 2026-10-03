# Play and build

Play In Editor is a second world. The authored level stays where it is. Play captures it, instantiates a runtime copy, and points the perspective view at the runtime camera. Stop discards that copy and restores the editor camera. Runtime edits are not saved. The outliner shows Runtime during play. The inspector is read-only. Modeling tools hide. Undo refuses.

Pause freezes simulation time and keeps drawing that world. Step, eject, possess, and Apply Runtime Changes are not in this build.

## A camera on a new project

Templates leave the pawn and the controller as `none`. Play does not invent a pawn, and it does not fall back to the editor camera.

1. **View > Create Camera Actor** while you are editing.
2. Place it.
3. **View > Set Selected as Startup Camera**. You can also use **Set as Startup Camera** in the inspector when that button is on the camera.
4. **Play > Play In Editor**, or the Play toolbar button.

The view uses that camera's pose. If the camera is orthographic, the log says so and Play keeps the perspective projection. If no startup camera is set, the log says the editor camera was not used.

**View > Pilot Selected Camera** is the editor control for looking through a camera while you author. It is not the play session.

## When a project has the free-fly pawn

If the project pawn is `JARVIG.DefaultFreeFlyPawn` and the startup camera is the pawn, Play places that pawn at the editor view before the first tick. The runtime camera is the pawn's camera.

| Input | Result |
| --- | --- |
| W A S D | Move |
| Mouse | Look, while captured |
| E or Space | Up |
| Q or Ctrl | Down |
| Shift | Sprint |
| Escape | Release the mouse |
| Click | Capture the mouse again |

A click in the viewport captures. Toolbar Stop returns to the authored level.

Standalone play does not use the editor view as the spawn. The default spawn for that path is local (0, 1.6, 8), yaw 0, looking toward −Z, when the pawn is the free-fly pawn. A project that does not select a pawn does not grow one there either.

New templates do not switch the pawn on. There is no menu in this build that turns a Blank project into that free-fly pawn. Expect a static startup camera until a project file has selected the pawn.

## Run it outside the editor

Build the player once, next to the editor, so both executables share a target directory:

```powershell
cargo build --offline --manifest-path native/Cargo.toml -p jarvig_game --bin JARVIGGame
```

From this repository the executables land in `native/target/debug/`. `JARVIGGame.exe` has to sit beside `JARVIGEditor.exe`.

With a project open:

| Command | Result |
| --- | --- |
| Play > Run Standalone | Starts `JARVIGGame.exe` on that project. |
| Build > Build Project | Copies a loose development game into the project's `Build/Windows-x64` folder: `Game.exe`, the project file, and `Content`. |
| Build > Build & Run | Stages that folder and starts `Game.exe`. |

There is no pak, no cook step, and no installer. If the log says `JARVIGGame.exe` is not next to the editor, build the `jarvig_game` package and try again.

The dedicated server is the same world without the editor and without a rendered view. `--server` on the game host is that path. A server build must not gain a GPU dependency. This manual's standalone button is the windowed player.

## What Play will not do yet

- It will not save the runtime world back over the level.
- It will not simulate a character controller, a weapon, or a physics stack.
- It will not replicate to a second client.
- It will not stream cells in and out.

A walking person inside the graybox room needs the pawn, animation, and physics work that is still ahead. Play today shows the level through the camera you set, and it proves the runtime copy is separate from the file you save.
