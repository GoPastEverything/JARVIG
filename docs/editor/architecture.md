# Editor architecture

The editor hosts the engine. The engine does not know about panels.

```text
                      JARVIG.exe
                     project hub
                           |
                           | a chosen .jarvigproject
                           v
                    JARVIGEditor.exe
                           |
          +----------------+----------------+
          |                                 |
          v                                 v
    Native Editor UI                  JARVIG Engine
          |                                 |
          |                         SceneWorld / Runtime
          |                                 |
          |                           RenderSceneSnapshot
          |                                 |
          +------------ RenderView <--------+
                           |
                           v
                        Renderer
                           |
                           v
                          RHI
```

`native/jarvig_editor` is the editor executable. It links `jarvig_engine`, `jarvig_renderer`, `jarvig_rhi`, and `jarvig_rhi_wgpu`. It does not name wgpu. It does not depend on winit. The window is Win32. The viewport HWND is the only handle passed to `attach_window`.

`native/jarvig_hub` is the project browser. A launch with no `--project` and no harness flag shows that window before `EngineSession` is constructed. See [hub.md](hub.md) and [ADR-0060](../adr/ADR-0060-the-hub-chooses-a-project-before-the-editor.md).

The dock tree lives in this crate (`DockWorkspace`, `PanelId`). No engine crate names it. `pnpm lint` checks that. See [workspaces.md](workspaces.md) and [ADR-0032](../adr/ADR-0032-editor-dock-workspace.md).

## Process

The engine runs in this process. That is the v1 model: the viewport, input, and debugging stay on one device thread. The editor does not invent an IPC boundary just to draw a triangle.

Ownership still sits on the engine. A later cooker, build worker, or remote tool should call command and service APIs. It should not need this window, and it should not reach into renderer snapshots.

## Worlds

`SceneWorld` is the authoring world for this shell. It is also the bootstrap world. That is temporary.

Later, Play-In-Editor is a second world. See [../architecture/play-in-editor.md](../architecture/play-in-editor.md). The Play menu does not copy the world and does not flip the profile. Runtime mutations must not be saved back by accident. That machine is not implemented.

## What this shell is not

- Not `JARVIGPlayer.exe`. The player ships the game without these panels. It does not exist yet. It will link the same engine.
- Not `JARVIGServer.exe`. The server stays headless. Smoke still reports `graphics=none`. It does not enumerate adapters.
- Not the project hub. `JARVIG.exe` chooses the project. This shell opens the one it was given. Do not treat `pnpm dev:hub` as that product.
- Not a second project format. `*.jarvigproject` is `jarvig.project` version 1. The hub does not invent another one.

## Identity

The outliner is a derived view of `EntityRegistry`. `World` is an editor root, not an entity. Each entity row is keyed by `EntityUuid`. The label is the name, plus a class suffix. Components are not rows. Bootstrap lights, the reflection probe, and World Settings are entities. `LightId` and `ProbeId` are not. `EntityHandle` is the runtime lookup and is not shown. `ObjectId` remains the bootstrap drawable slot. Editor selection is one `SelectionService` on the editor session, also keyed by `EntityUuid`. The tree caret is not that selection. The inspector reads that selection, asks the engine for a property snapshot, and sends edits back as authoring commands. It does not keep its own transform. The Perspective camera is editor state: a view pose, not an entity and not a selection. The transform gizmo is the same kind of state. It is an overlay on the view, not an entity. A viewport click feeds the selection service. A drag feeds `SetProperty`. The shell around it is Editor theme v1: dark panels and a real icon toolbar. Do not put that chrome back on the light Windows face. See [selection.md](selection.md), [inspector.md](inspector.md), [camera.md](camera.md), [picking.md](picking.md), [gizmos.md](gizmos.md), and [theme.md](theme.md).

## Log

The output panel is an in-process string copied into a read-only edit. That is a temporary bridge. JRV-0067 is an engine-owned diagnostics stream. Categories can later include Engine, Renderer, RHI, Assets, Materials, Physics, Network, and Editor. Do not scrape stdout as the permanent log.
