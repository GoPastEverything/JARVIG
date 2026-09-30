# ADR-0047 — Direct shadows are cascaded visibility

Status: Accepted
Date: 2026-09-23

## Context

ADR-0043 made direct-light visibility a term on the matching light only. The first map was one 20 m directional box. It showed occlusion, and it also showed the limits: coarse texels, a fixed edge, and a depth bias in NDC that lifted a contact off the floor.

The Lighting Lab now has a floor, a cube, spheres, and cards. Shadow quality can be judged on that scene. Dynamic GI is not this decision. A global-illumination term sitting on a weak visibility term would amplify the error.

## Decision

Directional visibility is a camera-relative cascade set. The baseline budget is 4 cascades, practical splits (halfway between uniform and logarithmic), a configurable shadow distance, and a 2×2 atlas of 1024² quadrants. Matrices are built in binary64, texel-snapped in light space, then uploaded as camera-relative float32. Reversed-Z is unchanged: near is 1, far is 0, compare greater-or-equal, clear 0. Adjacent cascades overlap and blend. The material still calls only `jarvig_direct_visibility`. Cascade selection is inside that function.

Bias is meters, not one NDC constant.

- Depth bias: meters along the light. The default is 2 mm. Facing surfaces use only this term.
- Slope bias: extra meters at a fully grazing receiver. The default is 10 mm at `n·l = 0`, and zero when the face points at the light.
- Normal bias: meters along the geometric normal, scaled by the same grazing factor. The default is 4 mm. It is optional and on.

Orthographic cascades divide those meters by that cascade's depth span. Point and spot convert the same meters through the perspective derivative at the receiver.

Filtering is a kernel on the shadow map, not a blur of the map. PCF is the baseline. Directional and spot then use a short blocker search so a nearby blocker stays sharper than a far one (PCSS-style). The authored filter radius is the maximum kernel. Point lights filter by offsetting the sample direction in a tangent frame and sampling the cube, so a face edge is the hardware cube sample.

A short screen-space contact march is a supplement. It is on in the baseline budget, limited to about 0.12 m, faded, and rejected when the depth gap is outside a thin window so it does not paint a halo. It multiplies the same direct visibility. It does not replace the maps. Turning it off is view state.

Cast Shadows and Receive Shadows are authored. Editor gizmos are not scene instances and do not cast. Probe capture still binds disabled shadow records, so the accepted cube is not suddenly shadowed.

Directional cascades may be redrawn when the snapped camera footprint or the casters change. Point and spot maps are redrawn when the light or the casters change. Camera motion alone does not rebuild them. A frame has a pass budget so a later large scene can leave the rest for the next frame.

Shadow settings on a light are level data. Changing them dirties the level. Lighting Debug, including Shadow Cascades tint and the contact toggle, is view state and is not saved.

## Alternatives Considered

- One global NDC bias. Rejected. It was already lifting the sphere off the floor.
- Blur the whole shadow map. Rejected. That softens contact and distance by the same amount.
- Screen-space contact as the only shadow. Rejected. It cannot see off-screen casters.
- A depth-texture sample. Rejected. This RHI still cannot sample `Depth32Float`. The contact pass stores view depth in `Rgba16Float`, as the shadow maps do.
- Start dynamic GI in the same change. Rejected. Visibility comes first.

## Consequences

`docs/rendering/shadows.md` is the map. JRV-0086 is the ticket. The status line reports cascade count, maps updated, draws, casters, filter, contact, and the CPU time spent recording the passes. GPU timestamp queries are not enabled on this RHI, so that number is not a GPU duration.

## Supersedes

The single directional map in ADR-0043. Not the visibility seam.

## Superseded By
