# Coordinates

Authoring units are meters. The axes are right-handed: +X is right, +Y is up, and −Z is forward. The editor camera looks along its local −Z. Mouse right yaws toward +X.

The ground grid and the axis triad in the viewport use that same frame. They are editor reference. They are not entities, they are not saved, and they do not draw during Play.

## Where a block sits

Scene-local numbers in the inspector are the ones you edit. The first Block is placed at (0, 1, −4). Its size is (2, 2, 2). The entity's scale stays (1, 1, 1). Size is the solid. Scale is not a second size.

Faces on that solid are numbered:

| Face | Direction |
| --- | --- |
| 0 | +X |
| 1 | −X |
| 2 | +Y |
| 3 | −Y |
| 4 | +Z |
| 5 | −Z |

An extrude on +Y grows the height and shifts the center so the opposite face stays. An inset is per face. A bevel is one value for the solid.

**Snap** in the object tools rounds local translation to 0.1 m. A modeling drag snaps its own delta to 0.05 m. The Move gizmo does not snap. Typed dimensions are exact.

## The large root

The scene root sits far from the coordinate origin, on the order of a billion meters, so the GPU sees small positions relative to the camera. You author in scene-local meters. You do not put an absolute universe position into a GPU matrix or a light. The architecture pages name the frame hierarchy and the reversed-Z projection.

| Topic | Page |
| --- | --- |
| World and cells | [world](../architecture/world.md) |
| Frames | [coordinate frames](../architecture/coordinate-frames.md) |
| Render space | [ADR-0020](../adr/ADR-0020-render-space-and-clip-convention.md) and [ADR-0021](../adr/ADR-0021-reversed-z-infinite-far.md) |

The perspective projection is right-handed, with a 0..1 clip and an infinite reversed-Z far plane. The near distance starts at 0.1 m. Vertical field of view starts at 60 degrees. Those live on the view.

## Terrain axes

A heightfield's X and Z run from negative half-extent to positive half-extent in terrain-local meters. Y on that component is the sample height, not a second forward axis. The entity transform still places the terrain in the scene.

## What stays stable

Save files store entity identities and local transforms, not a screen pixel and not a GPU address. Reloading a level rebuilds derived meshes from the parametric record, the imported asset id, or the height samples. If a number in the viewport only exists while a debug view is on, it is not something you write into gameplay code.
