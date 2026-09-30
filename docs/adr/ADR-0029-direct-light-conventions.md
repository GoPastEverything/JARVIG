# ADR-0029 — Direct light direction, units, and view packets

Status: Accepted
Date: 2026-09-22

## Context

ADR-0028 says a material graph writes a surface and does not contain a light. The first engine lights still need one direction rule, one intensity unit, and one place where a billion-meter position becomes a float32 the shader can use. If those choices stay implicit, every later shadow, cluster, and importer will guess.

## Decision

A light is world state. It has a `LightId`, a reference frame, a kind, linear-RGB color, and a physical intensity. `LightId` is not an object, mesh, material, or RHI id, and it is not in the C ABI.

Kinds are directional, point, and spot. Area lights are not implemented. The kind enum can grow later.

Emission is the frame's local -Z, after that frame is resolved.

```text
+X right, +Y up, -Z forward
light_forward = frame rotation * (0, 0, -1)
directional surface-to-light L = -light_forward
spot cone is measured around light_forward
```

Color is scene-linear RGB. Intensity is separate.

| Kind | Intensity |
| --- | --- |
| Directional | Illuminance, lux. No distance term. |
| Point | Luminous intensity, candela. Illuminance is `cd / d²`. |
| Spot | Candela, then the same inverse square, then a smooth cone. |

`d` is at least `0.01` m. That floor is one constant, `MIN_LIGHT_DISTANCE_M`. A positive range, when set, omits samples outside it. It does not rescale candela inside the range. `0` means no range cutoff. Spot half-angles satisfy `0 <= inner <= outer <= π/2`. A bad cone is rejected.

Extraction copies enabled lights once into `RenderSceneSnapshot`, with the resolved binary64 pose. Disabled lights stay in the world and are omitted from that list. The snapshot does not contain a GPU buffer or a position that already belongs to one camera.

Each `RenderView` builds its own packet. Point and spot positions subtract that view's f64 origin, then rotate into the same eye space as the fragment. Directions rotate the same way and do not carry translation. The packet is a read-only storage buffer plus a count uniform, group 2. Group 0 stays the object transform. Group 1 stays the material. The shader loops that list. The loop is not a material-graph node and it is not a clustered light list. A renderer cap of 64 is a buffer bound, not part of the material program. The shader also stops at `arrayLength`.

The direct term is the JRV-0053 BRDF times that illuminance. Ambient occlusion does not scale it. Emissive is added once and does not depend on the light list. Unlit shaders do not declare group 2. There is no constant ambient term, no shadow map, and no image-based light.

## Alternatives Considered

- Store a float32 position on the world light. Rejected. The frame tree is the high-precision position.
- Bake one camera-relative position into the snapshot. Rejected. Two views of one light must not share one float32 position.
- Put `MAX_LIGHTS` in the material compiler. Rejected. Clustering later replaces the loop, not the surface.
- A unitless intensity. Rejected. Lux and candela are the baseline. Exposure and tone mapping are later and must not redefine them.

## Consequences

`jarvig_core` owns the logical light. `jarvig_material` owns the BRDF and the generated loop, not the world light. The RHI learns a read-only storage buffer and bind group index 2. It does not learn lux, candela, or a light kind. The server may keep logical lights and still does not extract or upload them.

Changing a light updates the per-view packet. It does not recompile a material, rebuild a material pipeline, or reupload a mesh or texture. Destroying a view retires that view's packet. Future clustered or tiled lighting consumes the same snapshot and the same BRDF.

## Supersedes

Nothing. ADR-0028 still owns the surface. ADR-0021 still owns reversed-Z. ADR-0026 still owns the snapshot.
