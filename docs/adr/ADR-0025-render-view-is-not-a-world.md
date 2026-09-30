# ADR-0025 — A render view is not a world

Status: Accepted
Date: 2026-09-22

## Context

The bootstrap window had one camera and one surface, and those two were easy to treat as the same object. The editor needs several views of one world: perspective, orthographic, game, and preview. Giving each view its own world, renderer, or GPU copy would make that impossible.

## Decision

A `RenderView` is a projection of shared engine state. It is not a world, not a renderer, and not a render target.

```text
JARVIG WORLD
    |
shared meshes, frames, GPU residency
    |
    +--------+--------+
    |        |        |
View A    View B    View C
camera    camera    camera
origin    origin    origin
viewport  viewport  viewport
    |        |        |
    +--------+--------+
             |
        RenderTarget
             |
        one acquire, one present
```

Each view owns its camera, its render origin (the resolved pose of that camera frame), its viewport, its scissor, its depth resource, and its settings. Views share logical meshes, GPU mesh residency, shaders, and compatible pipelines.

A target is where pixels go. The first kind is a surface. A texture target is later. A view must not be permanently equal to a swapchain. Several views may share one surface. That surface is acquired once per frame and presented once. View order on a target is ascending slot index, not hash iteration.

`RenderViewId` is a generational renderer id. It is not an RHI resource id and not a C ABI handle. ADR-0022. Destroying a view retires its GPU resources through ADR-0024 and makes the old id fail.

Color is cleared once for the whole target. Later view passes load that color and draw only inside their viewport and scissor. A load-op clear is not a per-viewport clear.

Depth stays reversed-Z, `Depth32Float`, clear 0, compare greater-or-equal. Each view has its own depth resource. The bootstrap depth texture is the size of the target, not the viewport. That is an implementation detail, not a shared global depth buffer.

Ordinary editor views do not get `PerspectiveWorld` or `GameWorld`. A future isolated preview scene is allowed only as a deliberate tool, not as a requirement of `RenderView`.

## Alternatives Considered

- One renderer per panel. Rejected. It duplicates the world and the GPU meshes.
- Clear the color attachment inside each view pass. Rejected. That clear covers the whole target and erases the other view.
- Acquire and present once per view. Rejected. Views that share a surface cannot composite that way.
- Put `RenderViewId` in the C header now. Rejected. ADR-0022. The editor API can be designed later.

## Consequences

`docs/rendering/views.md` is the map. Scene submission (JRV-0050) is how world state becomes renderer input. It is not part of this decision. Offscreen preview targets are not implemented. The editor UI will own panel layout later. It still does not encode passes.

## Supersedes

Nothing.

## Superseded By

Nothing.
