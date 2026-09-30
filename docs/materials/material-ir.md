# Material IR

```text
Material graph asset
  -> validate and typecheck
  -> canonical Material IR
  -> optimize (constant fold, dead-node elimination)
  -> feature and shading-model analysis
  -> permutation policy
  -> backend codegen (WGSL / WebGPU, later compatibility backends)
  -> pipeline cache, shader cache, reflection metadata
  -> cooked material and instance parameter layouts
```

JRV-0051 is the first native compiler, and it is small on purpose. `MaterialIRVersion` is 3 (ADR-0022). Versions 1 and 2 were never serialized, so there is no migrator. Version 3 adds the standard surface writes, swizzle, normal decode, and lerp.

The IR is a typed instruction list. Unlit still ends in `WriteSurfaceColor`. Standard graphs emit `WriteBaseColor`, `WriteMetallic`, `WriteRoughness`, `WriteNormal`, `WriteAmbientOcclusion`, and `WriteEmissive`. Other ops: load vertex color, load a parameter, a constant, multiply, lerp, load `TexCoord0`, sample a 2D texture, swizzle, normal decode. `SampleTexture2D` names logical texture and sampler parameters. It is not `textureSample` text. That call appears only in the WGSL artifact. Resources carry an optional `TextureSemantic`. That semantic is JARVIG data, not a backend string.

Vertex semantics are `Position`, `Color0`, `TexCoord0`, `Normal`, and `Tangent`. A standard graph always requests normal and tangent. It requests `TexCoord0` when it samples or reads a uv.

The IR records the domain, shading model, blend mode, parameters, and required vertex semantics. It is not shader text. WGSL is generated from it into one field of `CompiledMaterial`. The standard artifact defines `evaluate_material` and returns a `MaterialSurface`. The fixed vertex stage transforms position in camera-relative space and transforms normals by the inverse-transpose. Future targets (HLSL, SPIR-V, MSL) are not implemented. The live fragment, until a light exists, previews `baseColor + emissive`. That preview is not an IR concept and it is not the BRDF.

JRV-0035 remains the name of the fuller compiler: optimization, reflection, and permutation metadata. Constant folding, dead-node elimination, and static permutation keys are not passes yet. The instruction list does not prevent them.

The IR is what lets the renderer change backends without destroying artist assets. A test of the compiler must be able to replace the backend and keep the same IR fixture.

## Permutations

Static switches are scarce on purpose. Every static option multiplies variants. The compiler records variant count, compile time, instruction and resource estimates, and the reason each variant exists. Prefer dynamic branches or data-driven parameters when they are cheaper overall.

The Phase 0 parameter types are `scalar`, `vector`, `texture`, `enum`, and `boolean`. There is no static-switch type yet. Add one only with the permutation budget recorded beside it.
