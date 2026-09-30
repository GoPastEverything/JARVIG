# ADR-0035 — HDR scene color is not the swapchain

Status: Accepted
Date: 2026-09-23

## Context

Direct lighting was written straight into the sRGB swapchain. Values above 1 became hard white before any display decision. Exposure, tone mapping, and later environment light need a place to store linear light that is still above 1. The gizmo is editor chrome. If it is drawn into that same HDR buffer, exposure changes its red, green, and blue.

## Decision

The scene pass writes `Rgba16Float` scene color. One output pass per view multiplies by `2^exposure_ev`, runs one fitted tone curve, and writes linear display RGB to the swapchain. The sRGB swapchain encodes. The shader does not apply a second gamma.

```text
scene pass -> RGBA16Float
        |
        v
exposure and tone map, per view
        |
        v
sRGB swapchain
        |
        v
editor gizmo, in display color
```

Exposure is `RenderView` state, in stops, clamped to -16..+16. It is not a material, not a mesh, and not a `SceneWorld` edit. The editor camera is still not an entity.

The curve is Krzysztof Narkowicz's fitted ACES approximation. It is not Academy ACES color management. Negatives are clamped to 0 before the curve. Alpha is 1 and is not tone-mapped.

## Alternatives Considered

- Clamp lighting to 0..1 in the material shader. Rejected. That hides HDR and puts display policy in every material.
- Draw the gizmo into the HDR target. Rejected. Axis colors would change with exposure.
- A full ACES RRT/ODT. Rejected for this ticket. The fit is enough to prove rolloff.
- Fake ambient so turned faces are not black. Rejected. That is JRV-0072.

## Consequences

`docs/rendering/hdr.md` is the map. JRV-0072 can add environment light into the same HDR target. Bloom is a later pass, not this one. JRV-0077 is reserved for direct-light shadows and is not started.

## Supersedes

Nothing. ADR-0027 still owns texture color space. ADR-0034 still owns the gizmo. This decision says the gizmo is drawn after the display transform.

## Superseded By
