# ADR-0018 — Engine-owned capabilities

Status: Accepted
Date: 2026-09-22

## Context

Engines rot when the editor grows a private copy of import, cooking, materials, navigation, or builds. The CLI and the build farm then either shell the editor or drift. ADR-0011 already required a CLI for critical editor operations. That is necessary and not sufficient: the operation itself has to live in the engine, with the editor and the CLI as callers.

## Decision

The editor never owns engine functionality.

```text
                 ENGINE CAPABILITY
                       ^
                       |
        +--------------+--------------+
        |              |              |
      Editor          CLI        Automation
```

There is no `EditorMaterialImporter` that only the editor understands. There is an engine API, conceptually `ImportMaterial`, and the editor, the CLI, and later a build farm call that same API.

The same rule covers cooking, shaders, worlds, materials, physics setup, navmesh generation, importing, builds, and profiling. A panel may own layout, selection highlight, and other view state. It may not own the meaning of an asset or a simulation step.

UI technology is not engine technology. An HTML, native, or GPU panel is legal. A viewport that does not run the engine renderer is not the shipping viewport.

Game-specific rules do not go in `engine/`. A game module consumes JARVIG. Hot reload, when it exists, reloads that module across the C ABI from ADR-0017. APIs must not store process-global editor state inside engine objects in a way that makes a reload or a second host impossible. Hot reload is not required for the bootstrap.

## Alternatives Considered

- Editor implements the feature and the CLI automates the editor UI. Rejected. It needs a desktop session and it splits behavior.
- Duplicate the feature in the CLI "for now." Rejected. That is the drift this ADR exists to stop.
- Put every call in the editor process and pretend it is the engine. Rejected. The dedicated server and the player cannot link the editor.

## Consequences

- New tickets name the engine API first and the editor panel second.
- The temporary editor host's Tick and layout commands already follow this: layout is view state, tick calls `Engine.tick`.
- Material schema, derived-data keys, scene snapshots, and project validation stay in engine or schema packages. The CLI calls them. The editor must call those same packages when those panels exist, not a private editor copy.
- A review that finds an editor-only importer, cooker, or shader compiler sends the work back.

## Supersedes

Nothing. It tightens ADR-0011. It does not replace it.

## Superseded By

Nothing.
