# Representation virtualization

One cost and error model chooses a representation. This is research (milestone R2, flag `representationVirtualization`, default off).

| Condition | Representation |
| --- | --- |
| Very far | Proxy, analytical primitive, or coarse splat |
| Far to mid | HLOD or coarse cluster hierarchy |
| Near | Virtualized triangle meshlets |
| Extreme near | Procedural microgeometry or detail fields |
| Captured soft or volumetric surface | Gaussian or surfel path where it wins |

VFX is not this system. Particles stay a dedicated GPU graph. See the VFX note in the founding doc: emitters, ribbons, meshes, volumes, decals, event chaining, deterministic seeds, budget classes, and a graph compiler. VFX is not implemented.

RFC: [RFC-0003](../rfc/RFC-0003-representation-virtualization.md).
