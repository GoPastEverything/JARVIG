# Material graph

The graph is a node asset: typed pins, comments, subgraphs, preview nodes, and compile diagnostics. It is the authoring form. It is not the runtime form and it is not WGSL.

JRV-0051 builds the graph in code, not in an editor. Nodes and links have JARVIG ids (`NodeId`, `SocketId`, link id), not pointers, so a later editor can serialize them without replacing the model. The nodes that exist are `VertexColor`, `VectorParameter`, `ScalarParameter`, `Constant`, `Multiply`, `Lerp`, `TexCoord`, `TextureParameter`, `SamplerParameter`, `TextureSample`, `Swizzle`, `NormalDecode`, and `SurfaceOutput`.

`SurfaceOutput` on Unlit is one color. On `StandardMetalRough` the sockets are base color (Float4), metallic (Float), roughness (Float), tangent-space normal (Float3), ambient occlusion (Float), and emissive (Float3). An unlinked standard socket lowers to the surface default. It does not lower to a light.

The presented graph samples base color, one packed map, a normal, and emissive, then swizzles. That packing is the graph's choice. `Lerp` is `mix(from, to, factor)` and is how occlusion strength keeps 1 when the strength is 0. `NormalDecode` is the +Y normal-map decode. None of these nodes is a lighting model.

`Texture2D` and `Sampler` are value types. They are not packed into the uniform. Validation rejects a missing output, a missing connection, a cycle, a bad node or socket, a type mismatch, and a duplicate parameter. It returns `MaterialError`. It does not panic. Value types are `Float`, `Float2`, `Float3`, and `Float4`. Those are JARVIG types, not WGSL types.

JRV-0034 is graph v1. Not started. The Phase 0 document has parameters and texture semantics so instances and importers have a stable target before the node editor exists.

Editor UX, when built:

- Typed nodes, search palette, function calls, reroute nodes, comments, preview pins, compile errors, shader-cost overlays.
- Preview on sphere, plane, cube, or a custom mesh, using the real engine lighting path.
- Parameter UI with groups, ranges, units, tooltips, thumbnails, presets, and live updates that do not edit the master graph.
- Diagnostics for texture residency, samplers, complexity, variants, passes, overdraw, transparency, and estimated GPU cost, visible before cook.

The graph compiler is shared as an editor framework with animation, VFX, and AI, but each graph owns its own compiler and runtime semantics. Do not evaluate a material graph with the animation runtime.
