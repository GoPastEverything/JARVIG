# Scene extraction

The world is authoritative. The renderer draws a snapshot. ADR-0026.

```text
AUTHORITATIVE WORLD
    objects, frames, logical meshes
        |
        v
extract once per presented frame
        |
        v
RenderSceneSnapshot N
    owned poses, MeshId, bounds, visibility
        |
        +------------------+
        |                  |
     View A             View B
     subtract origin A  subtract origin B
        |                  |
        +------------------+
                 |
              Renderer
              residency, passes, GPU
```

`jarvig_engine::EngineSession::run_frame` ticks the clock, and only if that profile presents does it extract and call the host. The callback receives `&RenderSceneSnapshot`, `&MeshLibrary`, `&MaterialLibrary`, and `&TextureLibrary`. It does not receive the world. The dedicated server never enters that callback, so it does not build a snapshot, compile materials, or upload textures. A standard material does not change that. The snapshot still carries a logical `MaterialInstanceId`. It does not carry a BRDF, a shader, a GPU texture, or an RHI id.

Shading positions are computed after extraction. Each view subtracts its own camera origin in f64 and uploads float32. Normals and tangents are directions. They do not pick up that translation. PBR does not put a world-space float32 position back into the snapshot.

Lights ride that same extract. `lights[]` on the snapshot is the enabled set, with resolved binary64 poses, linear color, lux or candela, and cone cosines. It is not a second extraction and it is not a GPU buffer. A disabled light is absent there and still exists in the world. Each view then builds its own group-2 packet. Two views do not share one camera-relative light position. See [lighting.md](lighting.md).

## What a snapshot is

| Field | Meaning |
| --- | --- |
| `RenderFrameId` | Which extracted frame this is. Not the simulation tick. |
| `simulation_tick` | The runtime frame that produced it. |
| `world_revision` | Bumps when an object pose, visibility, mesh, or material binding changes. |
| `material_bindings` | Slot index to a logical `MaterialInstanceId`. Not a buffer, shader, or pipeline. |
| `RenderInstance` | One drawable. `RenderInstanceId` is not `ObjectId`, `MeshId`, or a buffer id. |
| `source: ObjectId` | Diagnostic link to the world object. Not an entity UUID yet. |
| `mesh: MeshId` | Logical mesh. Vertices stay in the mesh library. |
| `pose` | Resolved binary64 pose. Not camera-relative float32. |
| `scale` | Object-local. Frames still have no scale. |
| `bounds` | The mesh's local bounds, copied so visibility later need not ask the world. |
| `visible` | Hidden instances stay in the snapshot and are not submitted. |

Cameras are extracted as resolved poses too. A view names a camera frame. The renderer subtracts that pose in `f64` and only then builds the float32 uniform. One snapshot feeds both views. View A's origin is not baked into the shared data.

Insertion order is the draw order. There is no production sorter. Reversed-Z still decides which triangle wins.

## What it is not

A snapshot is not saved, not edited by gameplay, and not a second authority. Replacing it next frame is the point. Dropping it does not destroy the logical mesh or the GPU buffers from JRV-0048. Scene presence, mesh lifetime, and residency are three different facts.

The renderer must not keep a pointer into the live world. The snapshot owns its poses. The mesh library is borrowed for the call, only to upload a mesh the residency cache does not have yet.

## Today and later

Two objects are fully extracted every presented frame. That is enough. Later extraction can use the world revision, dirty components, changed archetypes, or streaming deltas. Storage can become double-buffered, an arena, or a structure-of-arrays GPU scene. None of that is built.

Not built, and not blocked by this shape: a render thread, interpolation between simulation ticks, Play-In-Editor, or replay. The same snapshot type is what those will feed. Material slot bindings are already on the instance. Textures are JRV-0052. A material is still not a color stored on the mesh.

`RenderInstanceId` is not in the C header. Game code will talk to the world, not assemble render instances by hand.
