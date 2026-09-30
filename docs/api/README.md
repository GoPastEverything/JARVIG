# JARVIG API doctrine

ADR: [ADR-0022](../adr/ADR-0022-api-layers-and-one-type-registry.md) and [ADR-0023](../adr/ADR-0023-abi-struct-negotiation-and-calling-convention.md). They fill in [ADR-0017](../adr/ADR-0017-native-engine-core-and-multi-host-runtime.md) and [ADR-0018](../adr/ADR-0018-engine-owned-capabilities.md). They do not replace them. `native/jarvig_core/include/jarvig_core.h` is a bootstrap slice, not the SDK.

This is the contract for every external consumer. It is not a pile of functions. The SDK is not implemented here. `native/jarvig_core/include/jarvig_core.h` remains a bootstrap clock and runtime. Do not copy its integer error style into the next public surface.

```text
                         JARVIG ENGINE
                              |
                 INTERNAL RUST ENGINE APIs
                unstable / high-performance
                              |
              +---------------+----------------+
              |               |                |
              v               v                v
       PUBLIC ENGINE API   TOOL API      EXTENSION API
              |               |                |
          stable C ABI     Commands        Plugin ABI
              |               |                |
       +------+------+        |          +-----+-----+
       |      |      |        v          |           |
       v      v      v      Editor       v           v
      C++   Rust   Future    CLI       Plugins   Game module
      SDK   SDK    bindings
```

Above that, one type description:

```text
                     TYPE REGISTRY
                          |
          +---------------+---------------+
          |               |               |
       Inspector     Serialization    Scripting
          |               |               |
          +---------------+---------------+
                          |
                   SAME ENGINE TYPES
```

| Layer | Page |
| --- | --- |
| How the layers relate | [layers.md](layers.md) |
| Stable C ABI | [c-abi.md](c-abi.md) |
| C++ wrapper | [cpp-sdk.md](cpp-sdk.md) |
| Handles | [handles.md](handles.md) |
| Errors | [errors.md](errors.md) |
| Memory, strings, arrays | [memory.md](memory.md) |
| Threads and async | [threading.md](threading.md) |
| Versions | [versioning.md](versioning.md) |
| One type registry | [reflection.md](reflection.md) |
| Commands versus runtime | [commands.md](commands.md) |
| Plugins | [plugins.md](plugins.md) |
| Game module | [game-module.md](game-module.md) |
| Scripting, language not chosen | [scripting.md](scripting.md) |
| Automation, wire not chosen | [automation.md](automation.md) |

Review this doctrine before scene submission, materials, assets, world mutation, plugins, or gameplay grow a public surface. Internal renderer and RHI work stays in-process Rust.
