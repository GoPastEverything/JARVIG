# Direct-light shadows

A light's visibility multiplies that light's direct term. It does not darken emissive, the global environment, a local reflection probe, or indirect diffuse.

The material calls `jarvig_direct_visibility`. That function is the seam. Cascade selection, PCF, and the contact march live there. They are not material nodes.

ADR-0046 is the scalability rule. These sizes are the baseline budget, not an Intel preset and not a ceiling. ADR-0047 is the cascade decision.

## What is shadowed

| Light | Map | Baseline |
| --- | --- | --- |
| Directional | 2×2 atlas, four 1024² cascades | Camera-relative. Practical splits. Default distance 40 m. Texel-snapped in light space. Cascades blend across 10% of the split with a smoothstep. |
| Spot | One 1024² perspective map | At the light. The field of view is the outer cone. PCSS widens the kernel as the blocker moves away. |
| Point | One 256² cubemap | At the light. The filter offsets the direction in a tangent frame and samples the cube, so faces do not seam. |

Only the first casting light of each class is shadowed. Another light of that class stays unoccluded until an atlas of lights exists. `0` on a resolution, distance, or cascade count means "use this budget".

## Bias, in meters

| Term | Default | Meaning |
| --- | --- | --- |
| Depth bias | 0.002 m | Along the light. Facing receivers use only this. |
| Slope bias | 0.010 m | Added in full when `n·l` is 0. Zero when the face points at the light. |
| Normal bias | 0.004 m | Along the normal, scaled by the same grazing factor. |

An orthographic cascade turns meters into NDC by dividing by that cascade's depth span. A point or spot light uses the perspective derivative at the receiver, so the same 2 mm is not a huge step at the far end. The old 0.001 NDC constant on a 41 m box was centimeters of peter-panning. It is gone.

## Filtering

The filter is a 16-tap Vogel disk. The radius is continuous, in texels, and the disk is rotated slowly in shadow-map UV so the pattern stays put while a cascade is texel-snapped. It is not an axis-aligned grid and it is not screen-space noise. The authored radius is the maximum. Directional and spot run a blocker search on the same disk, over the same radius as the filter. The radius eases from about three quarters of a texel up to the physical penumbra as more of those taps find a blocker, instead of popping when the first tap hits. A close blocker stays the dark core. A distant blocker widens continuously up to the authored maximum. The maximum is still the authored filter radius, clamped to 4 texels on this baseline. Point lights still use that fixed radius. A point or spot source size does not yet widen the map. That is contact hardening. It is not a blur of the shadow map.

Cascade overlap is still 10% of the split. The blend is a smoothstep, so the handoff does not leave a slope line.

## Depth storage

The map is still `Rgba16Float`. `Depth32Float` cannot be sampled. One fp16 channel near 1.0 steps by about 0.001, and on a wide cascade that is centimeters, which bands a flat floor. The stored value is split: `.r` is `floor(z * 1024) / 1024`, `.g` is the rest times 1024, and the shader reads `r + g / 1024`. A cleared texel is still `(0, 0)`, farther than every receiver.

## Contact shadows

A screen-space march of about 0.12 m, six steps, faded to nothing at the end. The step start is jittered from the view position. A hit fades in across a gap of about 8–20 mm and fades out by about 9 cm, instead of a hard window. Receivers with `n·l` below about 0.15 skip the march, and it fades back in by about 0.40, so a ray that skims the floor does not paint bands. The result multiplies direct visibility. View > Lighting Debug > Contact Shadows turns it off. That toggle is not saved.

The march reads a view-depth color target written before the lighting pass. `Depth32Float` is still not a sampled format.

## What moves the maps

Directional cascades update when the snapped footprint, the light, or the casters change. A sub-texel camera move keeps the snap, so the shadows do not swim. Point and spot update when that light or the casting geometry changes. Camera movement alone does not rebuild them.

A frame may submit up to 16 shadow passes. The rest wait. The lab fits in one frame.

## What is authored

On each light, in the level: Cast Shadows, Shadow Resolution, Bias, Slope Bias, Normal Bias, Filter Radius, and Shadow Distance. A directional light also has Cascade Count and Cascade Distribution. `0` means the baseline budget. Editing them dirties the level.

On a mesh: Cast Shadows and Receive Shadows. Both default on. Editor gizmos are not in the snapshot, so they do not cast. Probe capture still disables every shadow record.

## What this is not

- Not a change to the BRDF, the environment, the reflection probe, or indirect diffuse.
- Not dynamic GI.
- Not a vendor quality level.
- Not a requirement for ray tracing. A ray-query visibility path would be another implementation behind the same function.

View > Lighting Debug > Shadow Cascades tints the image by cascade. It does not change the light sum when it is off, and it is not written into the level.

The status line reports cascades, maps updated this frame, shadow draws, casters, the filter (`pcss`), whether contact is on, and `cpu` milliseconds. That number is the CPU time spent recording shadow passes this frame. It is 0 when nothing was rebuilt. It is not a GPU timestamp. This RHI does not query GPU timestamps.

The floor ripples in the Lighting Lab are not this filter. See [hdr.md](hdr.md).
