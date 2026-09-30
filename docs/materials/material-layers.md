# Material layers

- Reusable **material layer** assets and separate **layer blend** assets.
- Per-pixel blends from authored masks, vertex color, height, slope, curvature, object or world position, or procedural fields.
- World-aligned and triplanar projection for terrain, architecture, and large props.
- Detail normals, macro/micro variation, and stochastic or aperiodic sampling hooks.
- Decals and mesh decals use the same surface attributes.
- Runtime virtual surface pages may cache expensive terrain or layer composition when measurement shows a benefit.

Not implemented. Layers are not a reason to add a second material schema. A layer produces the same canonical surface inputs as a flat material.

Geometry integration:

```text
Surface asset
  PBR macro attributes
  texture / UDIM set
  layer stack
  detail and material functions
  optional microgeometry descriptor
    -> representation policy
       normal map | parallax | displaced meshlet | procedural microgeometry
```
