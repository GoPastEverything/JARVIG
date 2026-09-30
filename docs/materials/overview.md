# Materials

Materials are a first-class asset system, not a pile of hard-coded shaders. The goal is an Unreal-class workflow: reusable master materials, lightweight instances, imported PBR libraries, layered surfaces, runtime parameters, and a way to see shader and texture cost before shipping.

ADR: [ADR-0007](../adr/ADR-0007-material-graph-ir.md).

```text
Material functions
  -> master material (graph + exposed parameters)
  -> material instance (parameter overrides)
  -> runtime instance (gameplay-driven values)
```

The compile pipeline, which is the meaning of a material:

```text
Material graph
  -> validation and type checking
  -> canonical Material IR
  -> optimization
  -> feature and permutation analysis
  -> backend code generation
  -> WGSL / WebGPU (and later compatibility backends)
  -> pipeline cache
```

Phase 0 implements the canonical document `jarvig.material/v1` in `@jarvig/materials`: shading model, parameters, and texture semantics. That document is what must survive a renderer change. The native first slice is below. It does not replace this long-term pipeline, and it does not replace the PBR baseline in ADR-0007.

## First slice (JRV-0051)

`native/jarvig_material` is the logical material system. It does not depend on wgpu, winit, or the RHI. The renderer turns a compiled master into GPU residency. The RHI still does not know what a material is.

```text
programmatic Material Graph
  -> validation
  -> Material IR version 3
  -> compiler
  -> CompiledMaterial
       domain, shading, blend, parameter layout, vertex requirements, key
       shader artifact (WGSL is one field, not the material)
  -> MasterMaterial
       |
       +-- MaterialInstance A   Tint
       +-- MaterialInstance B   Tint
```

These stay separate: mesh and material, graph and IR, IR and WGSL, master and instance, logical material and GPU material, dynamic parameter and shader permutation. A mesh stores a `material_slot` integer. It does not own a material. The world object binds that slot to a `MaterialInstanceId`. The snapshot copies that id. It does not copy a buffer, a shader, or a pipeline.

Implemented now: domain Surface, blend Opaque, and shading `Unlit` or `StandardMetalRough`. The presented bootstrap is one standard master and two instances. Unlit graphs remain (`VertexColor * Tint`, and the textured unlit master). Changing a runtime factor, texture, or sampler uploads or rebinds. It does not compile again and it does not key a pipeline by instance.

JRV-0052 added `TexCoord0`, `Texture2D`, a sampler, and `SampleTexture2D`. ADR-0027. JRV-0053 adds the standard surface, swizzle, normal decode, and lerp. The contract is [pbr.md](pbr.md) and [ADR-0028](../adr/ADR-0028-standard-metal-rough-surface.md). The graph writes surface properties. It does not contain a light. JRV-0054 loops world lights after `evaluate_material`. That loop is the shading stage, not a graph node.

Not implemented: Decal, PostProcess, UI, Particle, Volume, Masked, Translucent, Additive, IBL, shadows, static switches, file import, the node editor, material files, and a public C material or light API. Direct world lights are [../rendering/lighting.md](../rendering/lighting.md).

Missing slot: the submesh is skipped and counted. Unknown instance, or a mesh that lacks a semantic the master requires: the frame fails. Standard masters require `Position`, `Normal`, `Tangent`, and `TexCoord0`. Unlit still requires whatever the graph uses (`Color0`, `TexCoord0`). No silent draw. A magenta error material for a bad master is later. An unbound texture is a default, not that error. See [textures.md](textures.md).

Binding spaces today, documented so they are not allocated by accident:

| Group | Contents |
| --- | --- |
| 0 | View and object transform uniform. Already the bootstrap camera group. |
| 1 | Material parameter uniform. |

Later spaces are likely view/frame, material, object, and pass. This ticket does not renumber group 0 to make that pretty. Backend alignment is JARVIG's packed layout. WGSL struct layout is not the authoring contract.

A future pipeline key includes the compiled program, vertex layout, target formats, depth, raster state, pass compatibility, and static permutations. It does not include `MaterialInstanceId`. The production cache is not built. Today one master plus one vertex layout produces one pipeline, and both instances use it.

The dedicated server does not compile materials and does not build GPU residency. A material reference on the server, if one existed, would be inert. The bootstrap server holds an empty library.

## Pages

- [pbr.md](pbr.md)
- [material-graph.md](material-graph.md)
- [material-ir.md](material-ir.md)
- [master-materials.md](master-materials.md)
- [material-instances.md](material-instances.md)
- [material-functions.md](material-functions.md)
- [material-layers.md](material-layers.md)
- [virtual-textures.md](virtual-textures.md)
- [pbr-import.md](pbr-import.md)
- [material-library.md](material-library.md)

Milestone P4M. CLI parity is required for every important editor material operation. Provisional commands are listed on [../cli/overview.md](../cli/overview.md). Phase 0 CLI does not implement them yet.
