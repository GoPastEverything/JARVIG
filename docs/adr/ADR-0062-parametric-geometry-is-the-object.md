# ADR-0062 — A parametric solid is the object. Meshlets and Einstein are derived.

Status: Accepted
Date: 2026-10-02

## Context

Designers should be able to start a level from JARVIG geometry. An imported triangle mesh is one source of surfaces. It is not the definition of a wall, a floor, or a block.

A first reading of that goal can collapse the object into the renderer: "a block is whatever the meshlet and Einstein pipeline emits." That reading is wrong. Einstein is a surface discretization. It does not define a volume, a transform, a coordinate frame, analytic collision, or the fact that the object exists. Meshlets, cluster hierarchy, visibility, and level of detail are views of geometry the engine already has.

ADR-0003 forbids a BSP or brush tree as the saved world. A parametric block is one entity in the existing world. It is not a brush, and it does not make the level a BSP.

ADR-0061 keeps virtual geometry on for compatible static meshes and keeps procedural microgeometry adaptive, default off. Coverage bugs on that path are still open. This decision does not flip those defaults and does not stamp RFC-0002.

## Decision

A native primitive is a persistent parametric solid.

```text
JARVIG parametric geometry
  -> canonical surface or solid
  -> derived mesh, meshlets, hierarchy, visibility, LOD
  -> Einstein surface detail, when the existing policy asks for it
  -> rasterization
```

The first solid is a Block:

- Transform: scene-local translation and a unit quaternion. Object scale stays (1, 1, 1). Size is not scale.
- Size: full extents in meters on X, Y, and Z.
- One material slot.
- Collision: the analytic oriented box. Not the triangle mesh and not Einstein elements.
- Surface definition: six bounded planes. The saved file stores the size, not a triangle list.

The visible mesh is a derived 12-triangle box rebuilt when the size changes. It is not saved. Replacing the renderer, adding ray tracing, exporting, building a navmesh, or running a physics server later does not require the block to be reinvented.

The level file writes format version 6 only when a block is present. An Empty level stays version 1 and contains only World Settings. Versions below 6 still load. A block inside a version-1 file is corrupt.

The editor ground grid, origin, and axis triad are not entities. They are not saved. They do not draw in Play, Land, or a character-asset preview.

Play's existing free-fly pawn refuses to enter the analytic box. That is not a physics solver and it is not JRV-0090. There is no gravity and no character capsule.

World coordinates stay the law in [coordinate-frames.md](../architecture/coordinate-frames.md) and ADR-0020. Right-handed, +X right, +Y up, −Z forward, 1 meter. This decision does not flip them.

The type registry version becomes 8 because `parametric_block` is type id 16. ADR-0059's note that the version was 7 stays historical.

The destination, not this slice, is one geometry system. Imported meshes, native primitives, terrain, and later procedural geometry are supposed to feed the same derived meshlet, hierarchy, visibility, and Einstein path. Users should not have to enable an experimental mode for ordinary geometry. Debug views stay visualizations. This slice does not turn that destination on. `PROCEDURAL_MICROGEOMETRY_DEFAULT` stays false. RFC-0002 stays unstamped. ADR-0061 stays.

Empty + Ground, Outdoor, and Graybox are later level files made of ordinary components. They are not new engine types. A CAD compass, booleans, and the rest of the geometry palette are not this decision.

## Consequences

Create > Block and File > New Level > Empty are the first construction commands. A resized block round-trips through save and reload as `ParametricBlock` parameters. The derived mesh and the analytic box change together.

No JRV ticket is accepted. No GPU frame of the new level was presented with this change. The open editor does not contain the menu until it is closed and rebuilt.
