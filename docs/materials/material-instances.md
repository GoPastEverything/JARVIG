# Material instances

JRV-0051 instances are parameter blocks on one master. They do not contain a graph, a shader, or a pipeline. The presented scene is one `StandardMetalRough` master and two instances. Near is a rough dielectric (metallic `0`, roughness `0.85`, emissive `0`) on the red/yellow checker. Far is a smoother metal (metallic `1`, roughness `0.2`, emissive `(0.35, 0.12, 0.02)`) on the blue/green checker. They share the ORM multiplier, the flat normal, the white emissive texture, and one sampler. Unlit tint instances still exist as library helpers. They are not what the window draws.

`set_parameter`, `set_texture`, and `set_sampler` bump the instance revision only. A texture or sampler change rebuilds that instance's bind group. It does not compile another shader, create another pipeline, or upload the mesh. The pipeline key is the master program, the vertex layout, and the targets. It is not `MaterialInstanceId`. The instance stores logical ids. It does not own the texture. The renderer writes that instance's uniform when the revision changes, and it does not write it again on a later frame that did not change. GPU residency is `GpuMaterialInstance` (parameter buffer and bind group), separate from `GpuMasterMaterial` (shader, pipeline, layout). Destroying or hiding a render instance does not destroy the logical instance or the master.

Two layers, still the long-term model:

- **Material instance.** Parent/child parameter overrides. Cheap. Saved with the project.
- **Runtime material instance.** Transient or persistent parameter blocks for damage, wetness, team color, burn state, holograms, and animation.
- **Material parameter collection.** World or global sets: time, weather, wind, biome, shared gameplay state, render features.

JRV-0036 covers authoring inheritance. Runtime instances and collections are part of that same contract and are not separate material types. They are parameter blocks against a compiled layout.

CLI shape, provisional, not implemented:

```text
jarvig material instance create RockCliff --set roughness=0.72
```

The Phase 0 schema can already express a roughness scalar. It cannot yet create an instance asset that points at a master.
