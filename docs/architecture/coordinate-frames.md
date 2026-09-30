# Hierarchical coordinate frames

ADR: [ADR-0004](../adr/ADR-0004-hierarchical-coordinate-frames.md).

Do not use one enormous float32 coordinate space. The authoritative model is a tree of reference frames. Renderer and physics operate in local precision domains.

```text
Star-system frame -> Planet frame -> Ship frame -> Interior frame
```

A concrete chain, still the same rule:

```text
Solar system
  -> Planet frame
    -> Spaceport frame
      -> Ship frame
        -> Interior frame
          -> Player
```

The path into a shader is:

```text
Entity local transform
  -> reference frame
  -> high-precision world location
  -> render origin / active frame
  -> camera-relative float32
  -> GPU
```

The GPU sees the player at something like `(2.42, 1.78, -6.14)` in the active render frame. It does not see `4728928182.28374` meters from a universe origin. That is what keeps planets, spacecraft, moving interiors, weapons, and millimeter detail in the same engine without a float32 world shaking itself apart.

A light uses the same tree. Point and spot positions resolve in binary64, then each view subtracts its own origin. Directional and spot emission is the frame's -Z (`+X` right, `+Y` up, `-Z` forward). A direction is rotated into eye space. It is not translated. ADR-0029.

The editor Perspective camera uses the same path. Its position is binary64 in the root frame. The view origin is that position. GPU values are the object or light minus the origin. The camera remembers the bootstrap front-camera frame as context and does not write it. Automatic planet or ship frame changes are not implemented. Do not store the editor position as a float32 world coordinate.

The long-term content hierarchy is universe, star system, planet, city, station, ship, interior, entity. Phase 0 does not simulate those scales. It does implement the transform contract they need.

JRV-0045 renders one triangle through that path. The vertices are object-local meters. The root frame of the bootstrap scene sits at one billion meters. The GPU uniform does not contain that number. Native code is `jarvig_core::space`. The TypeScript `FrameGraph` remains the same contract for the prototype. They are not two worlds.

## Render convention

ADR: [ADR-0020](../adr/ADR-0020-render-space-and-clip-convention.md). It does not replace ADR-0004.

- The world unit is the meter.
- Right-handed. +X right, +Y up, -Z forward.
- A reference frame is rigid: binary64 translation and a unit quaternion. Scale is not a frame property.
- GPU matrices are column-major float32. `clip = projection * view * model * local_position`.
- Perspective depth is reversed-Z with an infinite far plane. [ADR-0021](../adr/ADR-0021-reversed-z-infinite-far.md) partially supersedes ADR-0020 here. Near maps to 1. Greater distance maps toward 0. The opaque clear is 0 and the compare is greater-or-equal. A finite far plane is not the perspective policy.
- The render origin is the camera frame's resolved translation. View carries orientation only, so the camera-relative translation is not applied twice.

## Types

`FrameTransform64` in `@jarvig/math`:

- `frameId`, `parentFrameId`
- `position` and `rotation` stored in JavaScript numbers, which are IEEE-754 binary64

`LocalTransform32`:

- position, rotation, and scale passed through `Math.fround` by `quantizeLocalTransform`

`toCameraRelativeF32(world, camera)` subtracts in float64 and then quantizes. That is the render-offset rule. It is not a GPU upload. JRV-0017 stays open until a far-origin render stress scene exists. The helper is already covered by `tests/unit/foundation.test.ts`.

## Composition

A point in a child frame becomes `parent.position + rotate(parent.rotation, childPoint)`, then that repeats toward the root. Rotations are right-handed. A +90 degree yaw about +Y takes +X to -Z. The unit test locks that sign.

## Precision expectation

At 8,000,000 meters, a 0.25 m local offset survives in the frame composition and in the camera-relative float32 value. The same offset added in float32 at that magnitude becomes zero. That is why gameplay and rendering must not share a single float32 world origin.

## Physics transfers

When a body crosses a frame boundary the intended sequence is: detect the boundary, capture world velocity, transform into the destination frame, insert the body, preserve momentum within a tested tolerance. That sequence is not implemented. Do not fake it with a global origin shift. See [../physics/overview.md](../physics/overview.md).
