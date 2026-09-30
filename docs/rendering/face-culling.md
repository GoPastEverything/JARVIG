# Face culling and two-sided shading

Counter-clockwise triangles are front faces. That is a JARVIG raster rule. The backend maps it. wgpu face enums stay in `jarvig_rhi_wgpu`.

## Default opaque policy

`CullMode::Back`. The reverse side of a triangle is not drawn. A closed cube or sphere already has outward triangles on every side, so you do not look through one polygon and expect its reverse to be a second surface.

The shader never sees that discarded face. It does not need to flip the normal.

## Two-sided policy

Opt in with `MaterialGraph::set_two_sided(true)`. That sets `CullMode::None` and, when the fragment is not front-facing:

- the geometric normal is negated
- tangent handedness (`tangent.w`) is negated
- the shading normal is rebuilt from that basis

A flat normal map then points out of the side you are looking at. `N·V` stays positive. Direct light and environment diffuse use that visible-side normal. This is how a leaf, cloth card, or paper sheet should work. It is not the default, and it is not bounced light.

A two-sided material that still culls is rejected.

## Bootstrap cards

The near and far bootstrap triangles are temporarily two-sided. They are flat cards used to inspect lighting, not closed solids. A real opaque mesh should leave the default and cull its back faces.

The shadow pass does not use this shaded-face cull. It culls nothing, so the geometric triangle occludes from either side. The flipped shading normal is not a second shadow caster. ADR-0043.

## What a flip is showing

| What you see | Cause |
| --- | --- |
| The card disappears | One-sided. The back face was culled. |
| A dielectric stays lit and the color shifts when the visible normal pitches | Environment diffuse. Upper sky versus lower ground. |
| A pure metal goes flat or dark away from the lamps | The direct highlight left. JRV-0073 adds the sky reflection. Inside the local probe, JRV-0074 can also show a captured view of nearby geometry. It is not a mirror. |
| One triangle lights another | Not implemented. JRV-0075. |

Do not hide any of these with a constant ambient term.
