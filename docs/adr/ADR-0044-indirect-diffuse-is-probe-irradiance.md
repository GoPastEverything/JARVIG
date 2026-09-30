# ADR-0044 — Indirect diffuse is probe irradiance

Status: Accepted
Date: 2026-09-23

## Context

JRV-0072 is the analytical sky hemisphere. It does not know that another surface was lit. JRV-0074 captured that scene into a cubemap for specular reflections. The same capture already contains the colored light that hit those surfaces. JRV-0075 is the first diffuse use of it.

This is not a constant added to the direct term, and it is not a bounce computed from every surface every frame.

## Decision

Indirect diffuse is a separate lighting contribution. It is a cosine-weighted irradiance of the reflection probe's captured cube, sampled by the world normal. The map is built once, at the end of that probe's static capture, into that probe's own cube. It is not a GGX mip and not a second capture policy.

The stored value is the average of the cosine samples. The PDF already includes `cos / π`, so the shader multiplies albedo, occlusion, and `(1 - metallic)`. It does not divide by `π` again. Metals do not receive it. Shadows do not darken it. Emissive and specular are unchanged.

Inside the probe, full lighting fades the JRV-0072 hemisphere by the probe weight and adds this term with that weight. The capture's empty texels are already the sky, so leaving both at full strength would count the sky twice. Environment Diffuse Only still shows the full analytical hemisphere. Indirect Diffuse Only shows only this term.

Camera movement does not rebuild it. Moving a triangle does not recapture it. The bounce stays with the cube until a later recapture policy says otherwise.

## Alternatives Considered

- Add `albedo * 0.1`. Rejected. That is the fake ambient this ticket exists to avoid.
- Path-trace one bounce from every light every frame. Rejected. That is real-time GI. This ticket is the captured irradiance.
- Replace the analytical hemisphere everywhere. Rejected. Outside the probe the sky term stays. The debug view of JRV-0072 stays the sky.
- Shadow the bounce in this pass. Rejected. The accepted specular capture is unshadowed, and this irradiance is that capture. Live direct light remains shadowed.

## Consequences

`docs/rendering/environment-lighting.md` describes the term. The checker card can show it. The gold card cannot, because it is metal. A face whose normal points away from the other card does not receive that card's color. Look at the back of the checker, or turn it toward the gold card, and use View > Lighting Debug > Indirect Diffuse Only.

## Supersedes

Nothing. ADR-0036 still owns the sky hemisphere. ADR-0039 still owns the capture. ADR-0043 still owns direct-light visibility.

## Superseded By
