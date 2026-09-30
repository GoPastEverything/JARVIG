# ADR-0031 — The native editor shell is not the engine

Status: Accepted
Date: 2026-09-22

## Context

The renderer ladder through adapter policy is in place. The product that hosts it is `JARVIGEditor`, not another renderer ticket and not the TypeScript dock. That dock can stay as a transitional tool. It must not become the owner of rendering, world state, asset import, material compilation, scene editing, or engine execution.

The shell has to be a Windows executable with menus, a toolbar, trees, property text, a log, and a custom GPU viewport. Docking, tabs, and floating windows are required later. A toolkit chosen only because it demos quickly would own the wrong things.

`pnpm lint` already confines wgpu to `jarvig_rhi_wgpu` and winit to `jarvig_platform`. The viewport has to be the existing `RenderView` on the existing device.

## Decision

`JARVIGEditor.exe` is a native Win32 process. It hosts the engine in-process. The editor UI is not the engine.

```text
JARVIGEditor.exe
      |
      +-- Win32 frame, menu, toolbar, panel children
      |
      +-- EngineSession
              |
              +-- SceneWorld
              |
              +-- one RenderView on the viewport child HWND
                      |
                      +-- Renderer
                              |
                              +-- RHI
```

The center child is the only surface. `attach_window` receives that HWND. The renderer creates one `JARVIG.Perspective` view with `NormalizedRect::FULL`. There is no `EditorRenderer`, `EditorWorld`, or `EditorMaterialSystem`.

Panel widgets in this milestone are ordinary Win32 controls: a list, read-only edits, and static text. They do not draw the world. Docking, tabbing, and floating windows are a later ticket. They may replace how panels are arranged. They may not take the swapchain.

Authoring mutations, when they exist, become engine commands (ADR-0022). A panel does not write a snapshot or a GPU buffer. Play does not enter Play-In-Editor. The editor is not the player. The dedicated server does not link this shell and does not enumerate a GPU.

The same engine remains the engine for `JARVIGEditor`, a future `JARVIGPlayer`, and `JARVIGServer`. In-process hosting is the v1 editor model. Command and service boundaries stay on the engine side so a later cooker or remote tool can call them without this window.

## Alternatives Considered

- egui, or Dear ImGui, as the editor surface. Rejected for this shell. The practical Rust bindings render through wgpu and would either own the swapchain or create a second device. That fights the backend boundary and the rule that the viewport is an engine `RenderView`. Either toolkit can be reconsidered later for panel widgets only, and only if it does not own the device.
- Qt as a C++ shell around the engine. Rejected for this milestone. The docking is real, and the engine could still sit behind a stable interface, but it splits the product into C++ and an unresolved license. The engine does not need that split to gain a window.
- WebView, Electron, or a browser DOM around `apps/editor`. Rejected. That is the transitional dock pretending to be the product. PlayCanvas is not the editor runtime either.
- Keep `jarvig_editor_host` and call the two-view bootstrap the editor. Rejected. That host is a renderer test. It stays. The product layout is one perspective viewport plus panels.

## Consequences

A later UI toolkit is allowed to replace Win32 controls. The engine path above has to survive that replacement. Do not amend the wgpu or winit boundary so a toolkit can own the viewport.

This decision does not build docking, selection, reflection widgets, an asset database, Play-In-Editor, or the Hub. The fixed child layout is the first shell, not the workspace system.

## Supersedes

Nothing. ADR-0017 still separates hosts from the engine. ADR-0018 still makes the editor a caller of engine capabilities. ADR-0025 still says a render view is not a world.
