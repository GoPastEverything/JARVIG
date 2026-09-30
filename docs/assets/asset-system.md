# Asset system

Package: `@jarvig/assets`.

Runtime code uses typed handles, not editor paths. States: `unresolved`, `loading`, `resident`, `evicted`, `failed`. Legal transitions are tested. Skipping a state throws.

The streaming manager, when it exists, owns memory budgets. The handle does not know about files. A server may hold an unresolved handle for world data it has not loaded. The Phase 0 dedicated server does that for `world:default` and does not load graphics.

Dependency graphs must be inspectable in the editor and the CLI once import exists (JRV-0022). They are not inspectable yet.

Supported source kinds named by the founding doc. JRV-0022 imports GLB/glTF static meshes only. Images, audio, and the other kinds still have no importer:

`glTF / GLB`, `KTX2 / Basis`, `PNG / JPEG / HDR / EXR`, `MaterialX` (planned), `WAV / OGG`, `PLY / splats` (experimental).

See [import-pipeline.md](import-pipeline.md), [cooking.md](cooking.md), [derived-data-cache.md](derived-data-cache.md), [content-addressing.md](content-addressing.md).
