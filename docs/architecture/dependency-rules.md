# Dependency rules

ADR: [ADR-0015](../adr/ADR-0015-dependency-direction.md).

```text
schemas and math
    ^
engine subsystems
    ^
runtime hosts
    ^
editor UI and tools
```

The engine must not depend on the editor. Game and server code must not import editor-only modules. A dedicated-server build must be able to exclude rendering and editor modules. Phase 0 enforces that by source imports. Cook-time tree shaking of `node_modules` is JRV-0023, not done.

## Enforced edges

`pnpm lint` runs `tools/validation/check-boundaries.mjs`. The table below is that file. If they disagree, fix both in the same change.

Engine packages (`engine/**`) must not import `node:` or `playcanvas`, and must not reference `document`, `window`, `HTMLElement`, `localStorage`, or `navigator`. The checker fails the build if they do. Node-only helpers belong in `@jarvig/node-host` or in a host under `apps/` and `hosts/`. The native core in `native/` is not part of this TypeScript graph. It must not depend on the editor, and the editor must not grow a private copy of a native system.

| Package root | May import |
| --- | --- |
| `engine/math` | nothing outside the package |
| `engine/core` | nothing outside the package |
| `engine/ecs` | `@jarvig/scene-schema` |
| `engine/world` | `@jarvig/core`, `@jarvig/math` |
| `engine/assets` | nothing outside the package (`node:` allowed) |
| `engine/materials` | nothing outside the package |
| `engine/render` | `@jarvig/core`, `@jarvig/world` |
| `editor/shell` | `@jarvig/core`, `@jarvig/ecs`, `@jarvig/render`, `@jarvig/world` |
| `apps/hub` | `@jarvig/core`, `@jarvig/node-host` |
| `apps/editor` | `@jarvig/core`, `@jarvig/editor-shell`, `@jarvig/node-host` |
| `hosts/client` | `@jarvig/core`, `@jarvig/materials`, `@jarvig/node-host`, `@jarvig/render`, `@jarvig/world` |
| `hosts/dedicated-server` | `@jarvig/assets`, `@jarvig/core`, `@jarvig/node-host`, `@jarvig/world` |
| `tools/cli` | `@jarvig/core`, `@jarvig/node-host`, `@jarvig/project-schema` |
| `packages/scene-schema` | nothing |
| `packages/project-schema` | nothing |
| `packages/node-host` | `koffi` only, plus `node:` (host helper; loads the native core library; not an engine module) |
| `samples/minimal` | `@jarvig/client`, `@jarvig/core`, `@jarvig/node-host` |

`node:` built-ins are allowed. A relative import that resolves outside its package root is a violation. The dedicated server cannot import `@jarvig/render`. Engine packages cannot import `@jarvig/editor-shell`.

Tests live outside the package roots and may import any public package. Do not move production code into `tests/` to dodge the checker.

## Adding an edge

Add the dependency to the package's `package.json`, the TypeScript project reference, this table, and `allowed` in the checker. Add a test if the new edge is easy to violate in the direction that matters (engine to editor, server to render).
