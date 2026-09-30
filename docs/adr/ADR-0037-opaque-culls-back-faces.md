# ADR-0037 — Opaque materials cull back faces

Status: Accepted
Date: 2026-09-23

## Context

The bootstrap triangles are single polygons. The rasterizer was not culling, so rotating a card showed its reverse side while the shader kept the authored front normal. `N·V` went negative and the direct term went black. That looked like missing light. It was a stale normal on a face that should not have been shaded as the front.

Closed solids do not need the reverse of each triangle. Thin cards do, and they must say so.

## Decision

Counter-clockwise is front.

The default opaque policy is `CullMode::Back`. Back faces are discarded. The shader does not flip normals for them.

Two-sided is an explicit material opt-in. It sets `CullMode::None`. When the fragment is not front-facing, the geometric normal is negated and the tangent handedness is negated before the shading basis is built. The visible side then has a normal that points at the camera.

Not every material is two-sided. The bootstrap triangles are temporarily two-sided only because they are inspection cards.

The cull mode is JARVIG data on the material and the render pipeline. The backend face enum stays in `jarvig_rhi_wgpu`.

## Alternatives Considered

- Leave culling off and always flip when `N·V` is negative. Rejected. A closed mesh would light interior faces, and `N·V` is also negative for grazing back faces that should stay culled.
- Make every standard material two-sided. Rejected. That hides winding errors and is the wrong default for opaque solids.
- A constant ambient term so the stale back face is not black. Rejected.

## Consequences

`docs/rendering/face-culling.md` is the map. JRV-0073 can add environment reflections on top of this normal. It must not undo the cull default. A two-sided metal can still look dark away from direct lights until that reflection exists.

## Supersedes

Nothing. ADR-0028 still owns the surface. ADR-0036 still owns environment diffuse. This decision says which side of a triangle is allowed to receive them.

## Superseded By
