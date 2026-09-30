# ADR-0038 — Environment specular is split-sum, analytical until a cubemap exists

Status: Accepted
Date: 2026-09-23

## Context

JRV-0072 gives a metal no diffuse environment light. When the direct highlight leaves, the gold card goes flat. A reflection of the other triangle, a probe, and an imported HDR cubemap are later tickets. The sky that already exists is two hemispheres of linear radiance.

## Decision

Environment specular is added after environment diffuse, in the same HDR target, before exposure.

The lookup is one function, `jarvig_hemisphere(direction)`. Today that direction is the reflection vector, and roughness² blends it toward the hemisphere the normal faces. That blend is the stand-in for a prefiltered mip. It is not `color * (1 - roughness)`.

The Fresnel integral is Brian Karis's analytical environment BRDF. `F0` is the same value the direct term uses. Grazing view angles raise the reflectance. Ambient occlusion does not scale this term.

A later cubemap replaces the hemisphere lookup. It may take the reflection vector and a roughness mip. `MaterialSurface` does not grow an environment texture slot for that.

## Alternatives Considered

- Multiply the sky by `(1 - roughness)`. Rejected. A rough mirror and a sharp one would only change brightness, not direction.
- Sample a cubemap now. Rejected. There is no imported environment asset, and the asset database is not this ticket.
- A constant fill so the metal is not dark. Rejected.
- Darken the reflection with AO. Rejected until a specular-occlusion model exists. Direct specular stays free of AO.

## Consequences

`docs/rendering/environment-lighting.md` records the formula. JRV-0074 can introduce a local probe without changing the surface contract. The bootstrap metal reflects the sky, not the checker triangle.

## Supersedes

Nothing. ADR-0036 still owns the environment light. ADR-0037 still owns which side is shaded. This decision adds the specular lobe those feed.

## Superseded By
