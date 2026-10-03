# Programming

JARVIG does not have a gameplay scripting language yet. The language is not chosen, and the automation wire is not chosen. You author a level in the editor. You do not drop a script file into `Content/` and expect it to run.

This page is the map of the contract that every later binding has to follow. The pages under `docs/api/` are that contract. The header `native/jarvig_core/include/jarvig_core.h` is a bootstrap clock and runtime. It is not the game SDK. Do not copy its integer error style into the next public function.

## Layers

```text
JARVIG engine
    |
    internal Rust APIs, unstable
    |
    +------------------+------------------+
    |                  |                  |
    public engine API  tool API           extension API
    stable C ABI       commands           plugin ABI
```

Above those, one type registry feeds the inspector, save files, and any future scripting. They are the same types. A field you can edit in the inspector is a registry field, not a private editor variable.

| You want | Read |
| --- | --- |
| How the layers sit together | [layers](../api/layers.md) |
| The stable C boundary | [C ABI](../api/c-abi.md) |
| The C++ wrapper that does not exist as an SDK yet | [C++ SDK](../api/cpp-sdk.md) |
| Opaque handles | [handles](../api/handles.md) |
| Errors that cross the boundary | [errors](../api/errors.md) |
| Memory, strings, and arrays | [memory](../api/memory.md) |
| Threads | [threading](../api/threading.md) |
| Versions | [versioning](../api/versioning.md) |
| The one type registry | [reflection](../api/reflection.md) |
| Commands versus a runtime tick | [commands](../api/commands.md) |
| Plugins | [plugins](../api/plugins.md) |
| A game module | [game module](../api/game-module.md) |
| Scripting, still unchosen | [scripting](../api/scripting.md) |
| Automation, wire unchosen | [automation](../api/automation.md) |

The editor, the command-line host, and a future game module call engine APIs. The engine does not import the editor. A plugin does not receive a raw internal pointer.

## What a command is

An edit is a command, then a change to the world, then a later frame that draws the result. The panel does not write a snapshot or a GPU buffer.

```text
Inspector or gizmo
    |
    v
SetProperty, or another engine command
    |
    v
Type registry checks the value
    |
    v
World stores it
    |
    v
Next frame extracts a snapshot and draws
```

`SetProperty` is the command behind inspector fields and behind Move and Rotate. Create Block, import, and Place Mesh are engine operations too. The editor records one undo transaction around one user gesture. A command is not a wrapper you call for every tick. Play's simulation tick is not an authoring command, and it is not an undo entry.

Workspace changes, such as docking a panel, are a different list. They do not belong on the scene undo stack.

## What you can rely on in this build

- Projects, levels, parametric solids, imported glTF meshes, terrain heightfields, joints, lights, probes, and the environment are data the native engine loads.
- The public C surface is the bootstrap slice in `jarvig_core.h`, plus the doctrine in `docs/api/`.
- The editor is the host that exercises authoring. A second tool should call the same engine, not reimplement import or materials.

## What you should not invent

- A per-subsystem ABI beside the one C boundary.
- Rust or C++ standard-library types in that boundary.
- A panic or a C++ exception crossing it.
- A gameplay script API that the scripting page has not chosen.
- An editor-only copy of cooking, materials, or physics.

The TypeScript packages under `engine/` are the executable prototype of the contracts. New hot-path work goes into `native/jarvig_core` or behind its C ABI. The prototype is how older tests still boot. It is not the editor you ship a level with.

[Limits](limits.md) lists the gameplay systems that are still absent. [Authority](authority.md) is where to read before changing a public signature.
