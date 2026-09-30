# ADR-0021 — Reversed-Z and an infinite far plane

Status: Accepted
Date: 2026-09-22

## Context

ADR-0020 maps clip depth with 0 at the near plane and 1 at the far plane, and it uses a finite perspective. It says a depth-buffer ticket may change that far-plane mapping without touching handedness, units, or the frame tree.

The depth buffer is that ticket. JARVIG's cameras have to tolerate long sight lines: a landscape, a ship, a planet's horizon. A hard far plane of 100 m is not a visibility system. It is an accident of the first projection. Visibility, streaming, and culling stay separate. The projection should not invent a second clip.

World position and depth also stay separate problems. Reference frames and camera-relative float32 already keep astronomical coordinates off the GPU. The depth buffer only stores the value after projection.

## Decision

Perspective cameras use infinite reversed-Z in the existing 0..1 clip volume.

For a view-space point at distance `d` in front of the camera (`d > 0`, the camera looks down -Z):

```text
ndc_z = near / d
```

The near plane maps to 1. Greater distances map toward 0. There is no finite far plane in the matrix. A fragment cannot reach 0 at a finite distance.

The depth target is `Depth32Float`. The opaque pass clears it to 0 and compares with `GreaterEqual`. A closer fragment has a larger depth value, so it wins even when it was submitted first. The clear value is farther than every finite fragment.

The matrix, column-major, is:

```text
[ f/aspect   0    0    0 ]
[ 0          f    0    0 ]
[ 0          0    0   -1 ]
[ 0          0  near   0 ]
```

`f` is `1 / tan(vertical_fov / 2)`. `near` is a finite positive distance in meters. Horizontal scale still divides by the aspect ratio.

This does not render the whole universe. It only removes an artificial far clip from the projection. It does not put a root-space coordinate in the depth buffer, and it does not replace ADR-0004.

## Alternatives Considered

- Keep forward-Z, near at 0 and far at 1, with a finite far plane. Rejected for the perspective camera. A fixed far plane becomes wrong as soon as the view is longer than that constant, and the representable depths bunch up against 1. The function can stay available for a test of the old mapping. It is not the camera.
- Reversed-Z with a finite far plane. Rejected as the default. It still requires a far distance the world does not have. A later orthographic or explicit-range projection may take a far plane without changing this perspective default.
- OpenGL clip depth, -1..1. Still rejected. ADR-0020. The window is 0..1. Only the direction inside that window changes.
- A 24-bit fixed depth target. Rejected. `Depth32Float` matches a float reversed-Z mapping. A fixed-point target would need a different distribution argument. Shadow maps are not this decision.

## Consequences

Opaque pipelines that write depth use `GreaterEqual` and a clear of 0. A depth prepass, a hierarchical-Z pyramid, and shadow maps, when they exist, use this direction. They do not get a private convention.

A projection that is not this perspective, if one is added later, states its own depth direction next to this one. It does not silently flip the compare.

## Supersedes

The two sentences in ADR-0020 that set clip depth to "0 at near, 1 at far" and that make the first projection a finite far plane. Nothing else in ADR-0020.

## Superseded By

Nothing.
