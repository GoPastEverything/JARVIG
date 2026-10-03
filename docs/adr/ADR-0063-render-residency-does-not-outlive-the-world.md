# ADR-0063 — A world's render residency does not outlive that world

Status: Accepted
Date: 2026-10-02

## Context

File > New Level replaced the level document and the `SceneWorld`. The outliner showed World Settings and one Block. The viewport still drew the previous level.

`MeshId` is allocated inside one `SceneWorld` and starts again at 1. The renderer kept GPU vertex buffers and meshlet records keyed only by that id. The block's derived mesh received `MeshId(1)`, which was still resident as the previous level's mesh. The editor also remembered that id and did not replace the old meshlet records. One instance then reported the previous level's cluster count and drew its triangles.

ADR-0026 says dropping one snapshot does not destroy GPU residency. That is the rule for the next frame of the same world. It is not permission for residency to survive a different world. ADR-0024 says evicting a GPU mesh does not destroy the logical mesh. The logical meshes of the replaced world are already gone with that world. ADR-0018 says the editor does not own engine functionality. The editor does not keep the buffers. It tells the renderer that the world they came from has been replaced.

Play copies the authored level into a runtime world and aliases the authored mesh ids on purpose, so that copy can reuse the residency it still names. That path is not a world replacement.

## Decision

Render residency belongs to one world. Replacing the authoritative world releases it before the next extract.

```text
OLD WORLD
    release resident mesh buffers
    release remembered meshlets and the packed GPU scene
    release parent geometry and the cluster-hierarchy cache
    release meshlet debug buffers
    release Einstein GPU chunks, pending uploads, and the in-flight request
    clear per-view cluster cuts and frozen diagnostic cuts
    clear hidden entities and terrain-grid mesh ids from that world
        |
        v
NEW WORLD
    extract
    upload only the meshes this world contains
```

The release does not destroy the device, the swapchain, material-master pipelines, or the editor grid, axis triad, and gizmo. Those are host resources. The project mesh-asset library stays, so the content browser can still list models. Those asset meshlets are not attached to the new world's runtime `MeshId`s.

An empty level then has no drawable instances. A new block uploads a mesh built from its size. Stored meshlet records for that mesh are empty, so the candidate count is zero. The first derived mesh may be 12 triangles. That mesh is not an imported asset, and it is not the previous world's mesh.

ADR-0026 still owns the snapshot. This decision adds the world boundary that ADR-0026 did not name. Nothing in ADR-0024 or ADR-0026 is superseded.

## Alternatives Considered

- Keep one renderer cache for the life of the editor and only retire meshes the current world reports. Rejected. The previous world's meshes die with that world and are never reported retired, and the next world reuses their ids.
- Give every mesh a process-global id so the cache cannot alias. Rejected for this fix. Play relies on aliased ids to reuse residency. A global id is a larger change and is not required to make a world boundary.
- Hide the previous level's entities and leave their buffers resident. Rejected. The new block would still be drawn with the old vertices.
- Paint a cube over the leftover geometry. Rejected. The block would still not be the only instance.

## Consequences

Open a project, choose File > New Level > Empty, then Create > Block. The new view has none of the previous world's mesh buffers or meshlet records, and the world has one `ParametricBlock`. No JRV ticket is accepted. No GPU frame of that path was presented with this change. Plane, Ramp, and Cylinder stay unstarted until that view has been seen.

## Supersedes

Nothing.
