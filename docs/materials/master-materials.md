# Master materials

A master material defines the features and the exposed parameters for a family: painted metal, terrain, skin, glass, and so on. Instances do not copy the graph.

JRV-0051 has in-memory masters, not disk assets. `MaterialLibrary::add_master` validates the graph, lowers it, and compiles it once. The master keeps the graph, the IR, and the compiled program, plus a revision. A later graph edit is supposed to bump that revision and recompile. Hot reload is not built.

The presented master is `standard_metal_rough`: Surface / StandardMetalRough / Opaque. Its schema is `BaseColorFactor`, `MetallicFactor`, `RoughnessFactor`, `NormalScale`, `OcclusionStrength`, `EmissiveFactor`, plus texture parameters `BaseColor`, `Orm`, `Normal`, `Emissive`, and sampler `MaterialSampler`. Scalars are still one vec4 slot each. The master does not require ORM. The graph inside this master chooses that packing. An Unlit master (`Tint`, `BaseTexture`) still compiles. Static switches are documented and not implemented. Names are for authoring. The packed bytes are what the renderer uploads.

Phase 0 has a single `jarvig.material/v1` document, not a parent/child link. That document is the stand-in for "the thing an instance will point at." JRV-0036 adds inheritance: scalar, vector, texture, enum, and limited static feature overrides without rebuilding the graph.

Do not store a second full graph on an instance to fake inheritance. The acceptance test is that a parameter-only edit does not recompile unrelated graph structure.
