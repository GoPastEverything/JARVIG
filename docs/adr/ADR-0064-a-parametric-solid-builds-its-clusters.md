# ADR-0064 — A parametric solid builds its clusters from the derived surface

Status: Accepted
Date: 2026-10-02

## Context

ADR-0062 says a Block is a parametric solid, and that meshlets, cluster hierarchy, visibility, and Einstein detail are derived views of that solid. ADR-0063 then said a new block uploads a mesh built from its size and that stored meshlet records for that mesh are empty, so the candidate count is zero.

That sentence described the implementation after the residency fix. It is the gap. A rebuilt editor showed the gray 2 m cube and no longer drew Ghost City, and the same cube disappeared under Meshlet Colors and the Einstein debug views. The diagnostics read meshlet records. An empty record list means the object is absent from those views, which is a different outcome from Einstein looking at the surfaces and deciding not to refine them.

Imported meshes may keep a precomputed `.jarvigmeshlets` sidecar. A procedural solid has no asset id and no sidecar. Its source is the size. The clusters have to be built when the surface is built.

## Decision

A parametric solid keeps two representations, and only one of them is saved.

```text
ParametricBlock { size }
    analytic solid
        extents, oriented box, analytic collision
    derived render surface
        -> existing meshlet builder
        -> cluster hierarchy
        -> per-view visibility
        -> Einstein classification, then refinement only when that policy selects
        -> renderer
```

The level file stores `ParametricBlock` and the size. It does not store triangles, meshlets, hierarchy nodes, or Einstein patches. Creating the block, changing its size, and loading the level each rebuild the surface and call the existing `build_meshlets`. The resulting set is world residency. ADR-0063 releases it with the world. Play copies the set onto the aliased mesh id and does not release.

An imported mesh is unchanged. A matching sidecar is still the record source. A missing or stale sidecar still means zero candidates for that asset.

The builder is unchanged. `MESHLET_BUILDER_VERSION` stays 1. The pack limits stay 128 vertices and 128 triangles. The spatial grid stays `clamp(ceil(cbrt(max(triangles / 128, 1))), 8, 48)`. On the current 2 m cube that minimum 8-cell grid places the two triangles of each face in different cells. The measured result is 12 meshlets, one triangle each, with `meshlet_coverage` missing 0 and duplicate 0. The grid is not retuned to force one meshlet or one meshlet per face. The cluster hierarchy is built from those 12 records and its leaf count is 12.

Einstein uses the existing policy. Each derived cluster is classified. A cluster outside the view is `OffScreen`. A visible cluster whose 2 cm feature projects at or under 1 px is `BelowThreshold`. Neither case builds a patch. The block is reported as an opaque solid, so editor Auto can request patches when a close view pushes that feature over 1 px. There is no planarity exception and no change to `DETAIL_FEATURE_SIZE_M`, `DETAIL_ERROR_THRESHOLD_PX`, or `PROCEDURAL_MICROGEOMETRY_DEFAULT`. RFC-0002 stays unstamped.

Parent meshes for a derived solid may be built in memory from the same surface. Nothing is written to `.jarvigparents`. `PARENT_BUILDER_VERSION` stays 4. `PARENT_TRIANGLE_FLOOR` stays 32, so this cube's parent mesh may not reduce, and the leaves remain the draw.

## Alternatives Considered

- Store the meshlets in the level. Rejected. The file is the solid. A later fillet with thousands of derived elements would bloat every save, and a size edit would have two sources of truth.
- Write a `.jarvigmeshlets` sidecar for the block. Rejected. There is no asset id. The surface is a function of the size and is rebuilt on load.
- Retune the meshlet grid so the cube is one cluster. Rejected. The measured builder parameters stay. Twelve exact clusters are a valid result for twelve triangles.
- Skip Einstein on a flat face. Rejected. The diagnostic has to classify the surfaces. A threshold reject is a decision. An empty record list is the object being invisible to the diagnostic.
- Keep the ordinary index draw as a private path for native solids. Rejected. Plane, Ramp, Cylinder, Sphere, and Wedge are supposed to inherit this path. A second draw path would have to be retrofitted onto each of them.

## Consequences

The gray cube and the Ghost City detach were seen and accepted before this decision. Meshlet Colors, Draw From Meshlets, Cluster Hierarchy, and the Einstein classification views have not been seen on the rebuilt editor. Active Detail may stay empty when the classifier selects nothing. Plane, Ramp, Cylinder, Sphere, and Wedge stay unstarted until that look is accepted. No JRV ticket is accepted by this decision.

## Supersedes

The sentence in ADR-0063 that a new block's stored meshlet records are empty and its candidate count is zero. That sentence applied to a parametric solid. ADR-0063 still owns the world-replacement release. An imported mesh with no usable sidecar still has zero candidates. Nothing in ADR-0062 is superseded.
