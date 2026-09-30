# ADR-0036 — Environment diffuse is scene lighting

Status: Accepted
Date: 2026-09-23

## Context

A face turned away from the directional, point, and spot lights goes black. That is the direct BRDF, not a bug in the gizmo. Filling it with `color += vec3(0.1)` would hide the missing term and would not change with the normal. Reflections, probes, and object bounce are later tickets. JRV-0071 already stores linear light above 1 before exposure.

## Decision

The world owns one environment light. It is not a direct-light kind, not a material input, and not a render-view setting. The first source is two uniform hemispheres of linear radiance split by world +Y, plus an intensity and an enabled flag. There is no world position.

The shader adds Lambert diffuse irradiance after the direct loop:

```text
baseColor * (1 - metallic) * ao * intensity * mix(lower, upper, normal.y * 0.5 + 0.5)
```

That product is `diffuseAlbedo / π` times the cosine-weighted irradiance of those hemispheres. Metallic removes it. AO scales it and does not scale direct light. The contribution is written into the existing `Rgba16Float` scene target, before exposure and tone mapping. The GPU packet is a separate 32-byte uniform, not one of the 64 direct-light records.

Disabled, the frame matches direct-only lighting. Camera motion does not change the packet.

## Alternatives Considered

- A constant ambient add. Rejected. It does not depend on the normal and it is not irradiance.
- A cubemap or asset-imported HDRI in this ticket. Rejected. The asset database is not ready. The hemisphere is a bootstrap source with the same lighting meaning.
- Stuffing the environment into the direct-light storage. Rejected. It has no position, no cone, and no view-space conversion.
- Specular IBL, probes, or triangle-to-triangle bounce. Rejected. Those are JRV-0073, JRV-0074, and JRV-0075.
- Darkening direct lights with AO. Rejected. JRV-0053 left AO off the direct term. It now applies to this indirect term only.

## Consequences

`docs/rendering/environment-lighting.md` is the map. A pure metal stays dark away from direct light until JRV-0073 adds the indirect specular lobe. Geometry still does not exchange light. Emissive still does not light its neighbors.

## Supersedes

Nothing. ADR-0028 still owns the surface. ADR-0029 still owns direct lights. ADR-0035 still owns the HDR target and the gizmo-after-tone-map order. This decision adds the first term that enters that target besides emissive and direct light.

## Superseded By
