# Plugins

A plugin is a native module the engine loads through the C ABI. It exports a versioned table. It does not inherit a Rust trait across the DLL.

Each category gets the capabilities it needs and no others.

| Category | May do | May not do |
| --- | --- | --- |
| Runtime plugin | Subscribe to world events, register components | Draw, import files, edit the project |
| Editor plugin | Submit commands, add panels | Own import, cook, or simulation |
| Game module | The lifecycle in [game-module.md](game-module.md) | Link the editor |
| Asset importer | Turn bytes into an engine asset through `AssetService` | Write a private file format only the editor understands |
| Asset processor | Cook or derive through the engine | Shell the editor UI |
| Physics backend | Step bodies the physics service owns | See materials or the GPU device |
| Audio backend | Play sounds the audio service submits | Mutate the world |
| Renderer extension | Add a pass the render service calls | Take `wgpu` types or replace the RHI |
| Platform extension | OS and device services behind the platform crate | Become a second windowing stack in the game |
| Build target | Package through the build command | Reimplement cooking |

An editor plugin that imports a material by itself is a bug. It submits `ImportAsset`. The CLI submits that same command.

## Load

The module exports one bind entry. The engine passes the API version it speaks and the capabilities it is willing to grant. The module fills a table whose first field is the version it implements. A mismatch returns `JARVIG_VERSION_MISMATCH` and the module is unloaded. A missing capability is the same kind of failure.

The module is not given a pointer to the engine object. It is given function tables for the capabilities it declared.

## Trust

Downloaded project plugins are untrusted relative to the hub. Loading one is an explicit act. This doctrine does not make them sandboxed. It makes their reach small enough that a sandbox is possible later.

The reserved package name `packages/plugin-sdk/` is still empty. Do not fill it with a second API.
