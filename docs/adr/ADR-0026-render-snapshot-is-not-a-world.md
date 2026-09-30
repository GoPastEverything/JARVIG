# ADR-0026 — A render snapshot is not a world

Status: Accepted
Date: 2026-09-22

## Context

JRV-0049 put two views on one world. The renderer still built that world itself and drew the two bootstrap meshes by name. That is a toy loop. A renderer that queries the authoritative world while it encodes cannot later run beside a fixed simulation step, a second thread, Play-In-Editor, or a replay.

## Decision

The world is the truth. A `RenderSceneSnapshot` is a derived, disposable, renderer-facing copy of what one render frame needs to depict that truth. It is not a second world, not a save, and not something gameplay edits.

```text
AUTHORITATIVE WORLD
        |
        v
  render extraction, once
        |
        v
RenderSceneSnapshot N
        |
        +--------+--------+
        |                 |
     View A            View B
        |                 |
        +--------+--------+
                 |
              Renderer
```

`jarvig_engine` reads the world and extracts. `jarvig_renderer::render_target` takes the snapshot and the mesh library. It does not receive the world. `jarvig_core` does not depend on the renderer. The dedicated server ticks and does not extract.

The snapshot stores resolved binary64 poses, scale, local bounds, visibility, a logical `MeshId`, an `ObjectId` source, and a distinct `RenderInstanceId`. It does not store vertices, index bytes, or RHI buffer ids. Camera-relative float32 is applied per view after extraction, because the views do not share an origin.

`RenderFrameId`, the simulation tick, and the world revision are different numbers. Today an editor frame advances all three together. They are not the same counter.

Dropping a snapshot does not destroy a mesh or its GPU residency. Hiding an instance removes it from submission only.

Order is insertion order. Depth, not a sorter, decides opaque visibility.

A full extract of the bootstrap scene every presented frame is acceptable. Later extraction may use revisions and dirty sets. The snapshot type does not require a full rebuild forever. Interpolation, a render thread, and Play-In-Editor are not implemented. The snapshot is what they will consume.

## Alternatives Considered

- `renderer.render(&world)` and query objects while drawing. Rejected. The renderer would own the world's lifetime and threading.
- Bake one camera origin into the snapshot. Rejected. JRV-0049. Two views do not share an origin.
- Copy vertex buffers into each instance. Rejected. Geometry is a shared logical mesh.
- Extract once per view. Rejected. The views must share one coherent frame.
- A new crate only to hold the snapshot. Rejected. `jarvig_core` already holds frames and meshes, and neither the engine nor the renderer should depend on the other.

## Consequences

`docs/rendering/scene-extraction.md` is the map. JRV-0051, the first material, is not this decision. A material is not a color stored on the mesh.

## Supersedes

Nothing. It does not replace ADR-0009, ADR-0022, or ADR-0025.

## Superseded By

Nothing.
