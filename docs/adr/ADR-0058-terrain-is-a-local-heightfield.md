# ADR-0058 — Terrain is a local heightfield in the existing world

Status: Accepted
Date: 2026-10-01

## Context

A new project was a Lighting Lab with the demo meshes removed and the lab lights kept. Terrain, when it arrives, has to be created in a blank world and edited as its own workspace. Einstein microgeometry is a near-camera visual candidate. It is not a terrain generator, and RFC-0002 is not stamped.

RFC-0004 proposes clipmaps, virtual texture pages, and streaming cells. That proposal is not implemented. The shadow pass still budgets 16 casters and 250,000 triangles per caster. A 512 m terrain at 1 m spacing is 524,288 triangles in one mesh, and 64 chunks of that terrain exceed the caster count if every chunk casts.

## Decision

An Empty World level contains one World Settings entity and no demo meshes, lights, probes, or pawn. The project file stays `jarvig.project` version 1. The template name is editor session state in `Saved/Editor/template.txt`.

Land Mode is an editor workspace over the same world, in the same way the Character Editor is. It is not a second editor and not a second world. The level editor remains the default. Third Person and FPS templates are empty worlds. They do not start the kinematic pawn.

The authoritative terrain shape is one chunked heightfield on the existing scene frame. Samples are float32 meters relative to the terrain origin. `height_at` is the collision and navigation height. Derived chunk meshes are visuals. They are rebuilt from the heightfield, omitted from the level, and they do not cast shadows. The level writes format version 4 only when a terrain component is present. Versions 1 through 3 still load.

Einstein terrain detail is a stored, default-off record on that actor. It does not write height samples. Its collision flag cannot be enabled. No microtriangles are generated in this decision. The random-access lookup and the large-coordinate rule are recorded in `docs/terrain/foundation.md` before any of that generation exists.

This does not supersede RFC-0004. Page streaming, clipmaps, and a second geometry hierarchy stay unstarted. Shadow budgets stay 16 casters and 250,000 triangles.

## Consequences

`TYPE_TERRAIN` is type id 14. `TYPE_REGISTRY_VERSION` is 6. Create Terrain owns the grid. The component is not added from the generic Add Component list, and a terrain actor is not duplicated. JRV-0090 stays unstarted. JRV-0091 and JRV-0092 stay unaccepted. RFC-0002 stays unstamped. Foliage, erosion, water, voxel terrain, and procedural world generation stay out.
