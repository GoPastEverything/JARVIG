# ADR-0020 — Render space and clip convention

Status: Accepted
Date: 2026-09-22

## Context

ADR-0004 makes a tree of reference frames the authoritative transform. Parent-relative translation and rotation are binary64. The GPU receives a camera-relative float32 offset, not a root-space coordinate. Rotations are right-handed: a positive yaw about +Y takes +X to -Z.

That decision does not name the world unit, the camera forward axis, the clip depth range, or the matrix product. The first camera would otherwise inherit whatever a math crate does by default, and later code would copy it.

## Decision

The world unit is the meter.

The spatial frame is right-handed. +X is right, +Y is up, -Z is forward. That is the same yaw already tested in ADR-0004.

A reference frame is rigid. It stores a binary64 translation in meters and a unit quaternion. It does not store scale. Scale, when an object needs it, is an object-local render property applied when building the GPU matrix. It is not a second world position.

There is no global `Mat4<f32>` world transform. A resolved pose on the CPU stays binary64. The render origin for a view is the active camera frame's resolved translation. The object translation uploaded to the GPU is `f32(object - camera)`, subtracted in binary64 first. The view matrix carries the camera orientation only. It does not apply that translation a second time.

GPU matrices are column-major `f32`, and they multiply column vectors:

```text
clip = projection * view * model * local_position
```

Clip depth is 0 at the near plane and 1 at the far plane. That matches the wgpu / WebGPU clip volume. It is not OpenGL's -1 to 1 range.

The first projection is a finite perspective from a vertical field of view, an aspect ratio, and near and far distances in meters. Reversed-Z and an infinite far plane are not chosen here. Either can be revisited with the depth buffer without changing handedness, units, or the frame tree.

## Alternatives Considered

- Left-handed, +Z forward. Rejected. It contradicts the yaw test already locked by ADR-0004.
- OpenGL clip depth, -1 at the near plane. Rejected. The first backend's clip volume is 0 to 1, and a second convention inside the shader would be a silent transpose of every later depth test.
- One float32 world matrix, inverted for the view. Rejected. ADR-0004 already rejects a float32 universe. This decision only says how the camera-relative values are packed.

## Consequences

Shaders and the CPU math that fills them must use this product and this depth range. A depth-buffer ticket may change the far-plane mapping. It may not introduce a second handedness or a float32 world origin.

## Supersedes

Nothing.

## Superseded By

The clip-depth direction and the finite far plane are partially superseded by [ADR-0021](ADR-0021-reversed-z-infinite-far.md). Handedness, units, the matrix product, and camera-relative translation are not.
