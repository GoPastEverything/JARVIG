# ADR-0051 — An imported mesh is an asset, not a level

Status: Accepted
Date: 2026-09-24

## Context

ADR-0045 reserved `scheme: jarvig.asset` and rejected it until an asset database existed. Mesh references were builtin names. A glTF file is a common way to bring in a real mesh, and it is also a scene format. Treating the file as a level would make JARVIG's world a second copy of the glTF node graph.

JRV-0022 is the import. JRV-0028 will later cluster the result. It must not read glTF itself.

## Decision

Importing a GLB or glTF 2.0 creates one mesh asset.

- The persistent identity is an `AssetId`. It is not an `EntityUuid`, not a `MeshId`, and not a filesystem path.
- The project catalog stores the source path, importer version, and a fingerprint. That path is metadata.
- The original bytes are kept under the project's content directory.
- The derived file is JARVIG canonical geometry: positions, a vertex color, UV0, normals, tangents, indices, material-slot ranges, and bounds. A later meshlet build reads that file.
- A level mesh reference is `scheme: jarvig.asset` plus the id. Builtin meshes stay `scheme: jarvig.builtin`.
- Node transforms are baked into the vertices. Nodes do not become actors. Placement is a separate authoring command that creates one actor and keeps that actor's `EntityUuid`.
- The renderer still draws an extracted snapshot. It never sees the source filename.
- A file that cannot be imported does not create a catalog entry.
- Lighting Lab is not rewritten to contain an imported mesh. Old levels that do not use `jarvig.asset` stay valid.

## Alternatives considered

- Expanding the glTF node graph into entities on import. Rejected. That is a second scene format.
- Storing the vertex buffer in the `.jarviglevel`. Rejected. Five hundred actors would store five hundred copies, and the level would own geometry the asset should own.
- Rendering from the glTF parser every frame. Rejected. The draw path consumes the canonical mesh.

## Consequences

`JARVIGEditor` File > Import Mesh places one actor. Save writes the asset id into the level. PIE and `JARVIGGame.exe` resolve the same catalog. JRV-0066 and JRV-0028 stay unstarted.

## Supersedes

Nothing. ADR-0045 still holds for project paths, entity identity, and the refusal to store GPU handles. This decision is the asset scheme that ADR-0045 reserved.
