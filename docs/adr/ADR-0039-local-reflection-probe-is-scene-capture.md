# ADR-0039 — A local reflection probe is captured scene radiance

Status: Accepted
Date: 2026-09-23

## Context

JRV-0073 reflects one analytical sky. Every metal sees that sky, wherever it sits. A reflection of the checker card requires radiance that came from the scene, not a second painted gradient. There is no HDR cubemap asset pipeline yet.

## Decision

A reflection probe is world lighting data. `SceneWorld` owns a sphere: a reference-frame position, an influence radius, a priority, an intensity, and an enabled flag. It is not an entity, a material, a mesh, a `RenderView`, a `LightKind`, or a selection. The renderer owns the cubemap.

The bootstrap probe is one sphere, radius 8 m, placed in the scene frame at local `(0, 0.2, -3.5)`. The first capture renders the opaque scene six times with the normal material shader, from the probe origin, into a 32² `Rgba16Float` cubemap. Direct lights, the global environment, and emissive surfaces are included. Editor overlay is not. The capture is static: once, on the first presented frame that has an enabled probe. Later motion does not recapture.

During capture the probe weight is forced to zero, so the cube is not sampled while it is being written. Empty texels are the hemisphere color of that face's look direction, not black. Higher mips are a box filter. Roughness selects `lod = roughness * (mip_count - 1)`. That is not `color * (1 - roughness)`. A GGX prefilter is later.

The shaded point, not the editor camera, picks the probe. Weight is smoothstep of distance over radius: `t = saturate(1 - d / radius)`, `weight = t² (3 - 2t)`. Incoming radiance mixes from the global JRV-0073 term to the local sample. The Karis Fresnel runs after that mix. Outside the sphere, or with the probe disabled, or with no captured cube, the result is the global term. It is never black only because no local probe applies.

Environment diffuse stays the global hemisphere. One sampled probe is the highest priority, then the lowest id. Several probes do not blend with each other yet.

The GPU center is the probe origin minus the view origin, subtracted in f64, then stored as f32. An absolute billion-meter position is not uploaded.

## Alternatives Considered

- A second analytical sky colored differently from the global one. Rejected. That would not be a reflection of the scene.
- An imported HDR file. Rejected for this ticket. There is no asset-backed cubemap pipeline.
- Full GGX importance sampling into the mips. Deferred. The mip chain and the lod select stay, so the filter can be replaced without a new material.
- Box projection, SSR, ray tracing, and diffuse GI. Rejected. They are other techniques.
- Making the probe an entity so it can be selected. Deferred. The frame graph already holds the position without spending an `EntityUuid`.

## Consequences

`docs/rendering/environment-lighting.md` records the blend. A human still has to look: the metal inside the sphere should pick up nearby scene color, and that image is an approximation from the probe origin, not a mirror. Objects that move after the capture leave a stale cube until a later update policy exists.

## Supersedes

Nothing. ADR-0038 still owns the global analytical reflection. This decision adds a local sample in front of it.

## Superseded By

ADR-0042, for the box-filter mip sentence only. The capture, the lod select, and the blend still stand.
