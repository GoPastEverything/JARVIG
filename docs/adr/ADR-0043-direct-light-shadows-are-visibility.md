# ADR-0043 — Direct-light shadows are a visibility term

Status: Accepted
Date: 2026-09-23

## Context

The viewport already shades directional, point, and spot lights with the standard BRDF. Nothing in that sum knows when another triangle blocks the light. A darkened contact term, or a new ambient constant, would not be that answer. JRV-0075 is still the indirect-diffuse ticket and is not this one.

The bootstrap cards are temporarily two-sided. A shadow pass that culls back faces would drop a card whose back faces the light, and a pass that treated the flipped shading normal as a second occluder would fight itself.

## Decision

Each direct light has a visibility in `0..1`. The standard material multiplies that visibility onto that light's direct contribution and onto nothing else. Emissive, environment diffuse, environment specular, and local reflection-probe radiance are not shadowed.

`jarvig_direct_visibility` is the only material seam. The first implementation is a conventional shadow map behind that function: one 1024² directional map, one 1024² spot map, and one 256² point cubemap. A later virtual shadow map, or another large-world method, replaces the map class and the function body. It does not add a material-graph node and it does not recompile a material when a light or a caster moves.

Maps are light-space. The directional map is an orthographic box, half-extent 20 m, centered on the caster centroid and pulled back along the light. Spot and point maps sit on the light. Authoritative positions stay binary64. The upload is light-relative or camera-relative float32. There is no float32 absolute-world matrix. A camera move rewrites the sampling matrix. It does not redraw the maps. `world_revision` does.

Depth is reversed-Z. Near is 1, far is 0, the hardware compare is greater-or-equal, and the clear is 0. The RHI cannot sample a depth texture, so the map stores that depth in `Rgba16Float` and the shader compares it. An empty texel is farther than every receiver, so it leaves the light lit. A constant bias of 0.002 keeps the caster surface itself lit.

The shadow pipeline culls nothing. The geometric triangle is the occluder from either side. The two-sided shading-normal flip is not a second caster. Equal depth is not a shadow.

Only the first light of each class is shadowed. Further lights of that class stay lit until an atlas or a virtual shadow map exists. Probe capture binds the same textures with every record disabled, so the accepted reflection image does not gain shadows.

## Alternatives Considered

- Darken by distance to the other triangle. Rejected. That is not visibility.
- Sample hardware depth. Rejected. `Depth32Float` is not a sampled format in this RHI.
- Cull back faces in the shadow pass. Rejected. The temporary two-sided cards would disappear as occluders when the light sees their back.
- Put the shadow map in the material graph. Rejected. Materials describe a surface. Visibility is scene lighting.
- Shadow the reflection probe in this ticket. Rejected. The capture is a static accepted image. Turning shadows on inside it would change JRV-0074 and JRV-0078 without a new capture policy.

## Consequences

`docs/rendering/shadows.md` is the map. JRV-0075 can add bounce later. It must not reuse this visibility term as indirect light. Imported geometry is also later. These maps are a 20 m directional box, a 40 m punctual far plane, and one map per class.

## Supersedes

Nothing. ADR-0021 still owns reversed-Z. ADR-0029 still owns direct-light packets. ADR-0037 still owns shaded-face culling. This decision adds a separate shadow raster policy and a visibility seam.

## Superseded By

The single 1024² directional box is superseded by ADR-0047. The visibility seam is not. A direct shadow still multiplies only that light's direct term.
