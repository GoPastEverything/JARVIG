# Game module

A game consumes JARVIG. It is not part of the engine crates. It lives under `game/`, in its own module.

```text
MyGame.dll
    |
    |  stable C ABI
    v
JARVIG runtime
```

The engine loads the module and calls `jarvig_module_bind`. That name is the intended entry. The struct it fills is not frozen field-by-field yet. The pattern is frozen:

- `struct_size` is the size the module was compiled with.
- `api_version` is the version the module implements.
- A capability bit set says what the module needs.
- Function pointers follow those three fields.
- Null pointers mean "I do not implement this hook."
- The engine checks size and version before calling anything else. A table that is larger than the engine knows is a mismatch, not a blind read.

Hooks the table is expected to grow, in this role, not in this order as a frozen list:

| Hook | When |
| --- | --- |
| `on_load` | The module is bound and versions match |
| `register_types` | The type registry is open for this module's components |
| `register_systems` | Fixed and variable systems are named |
| `on_world_created` | A world exists |
| `on_fixed_update` | The simulation step |
| `on_update` | The frame, outside the fixed step |
| `on_world_destroyed` | The world is going away |
| `on_unload` | The module will be unloaded |

Exact signatures wait until a game-module ticket. Do not add the struct to `jarvig_core.h` as a guess.

The game holds handles. It does not hold entity pointers. Hot reload, when it exists, unloads and binds this module again. ADR-0017. Design the hooks so that is possible. Do not require the game to be linked into the editor binary.

A C++ class with `OnLoad` and `OnUpdate` is a reasonable wrapper the game compiles. The export the engine sees is still the C table.

The dedicated server loads the same module or a server build of it. It does not load the editor. The module does not call editor functions.
