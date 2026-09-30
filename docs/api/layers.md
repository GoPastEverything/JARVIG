# API layers

## 1. Internal Rust API

`jarvig_world`, `jarvig_assets`, `jarvig_renderer`, `jarvig_materials`, and `jarvig_physics` will call each other with Rust types. That surface is allowed to change with the crates. It is optimized for correctness and for the frame, not for binary stability.

Do not push those calls through C. The ABI exists so a DLL compiled last year still loads. It is not an internal calling convention.

The RHI is this layer today. `BufferId` and `PipelineId` are Rust newtypes. wgpu types stay in `jarvig_rhi_wgpu`. Neither crosses a plugin boundary.

## 2. Stable C ABI

The binary boundary for game modules, native plugins, language bindings, and middleware. Rules: [c-abi.md](c-abi.md). Opaque handles. `JarvigResult`. No Rust types, no C++ standard-library types, no exceptions, no panics.

## 3. C++ SDK

A pleasant wrapper over the C ABI. `world.CreateEntity()` may exist here. The function underneath is still C. RAII in the wrapper releases through an engine function, not through `delete` of an engine object. See [cpp-sdk.md](cpp-sdk.md).

A Rust SDK for game code, if written, is the same kind of wrapper. It is not the plugin ABI.

## 4. Tool and command API

Engine-owned operations. The editor, the CLI, automation, and a build farm call them. They do not each grow an importer.

```text
                    AssetService
                         ^
              +----------+----------+
              |          |          |
            Editor      CLI      Automation
```

There is no `EditorImporter`. See [commands.md](commands.md) and [ADR-0018](../adr/ADR-0018-engine-owned-capabilities.md).

Two altitudes, on purpose:

| Altitude | Examples | Undo |
| --- | --- | --- |
| Runtime API | create entity, set a material parameter, request an asset | No |
| Tool command | import, cook, compile a material, validate, the editor's property edit | When the operation is an authoring transaction |

## 5. Extension and plugin API

Versioned and capability-scoped. A physics backend does not receive the material compiler. Categories: [plugins.md](plugins.md).

## 6. Scripting API

Not a language choice. Bindings are generated from the type registry. A script sees the same fields the inspector sees. See [scripting.md](scripting.md).

## 7. Automation and remote tools

A process boundary in front of the engine, not a second engine. The wire format is not chosen. See [automation.md](automation.md).

## What does not get a layer

- The editor's widget layout. That is view state.
- A private gameplay API that only works inside the editor binary.
- Backend GPU objects as a public type.
