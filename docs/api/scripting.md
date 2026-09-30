# Scripting

No scripting language is chosen. ADR-0017 left TypeScript as the prototype and as a possible future host. That is not a decision that gameplay scripts are TypeScript, Lua, C#, or anything else.

When a language is chosen, it is a host on the native API. It is not an implementation of the world.

Bindings come from the type registry:

- The script names the same type the inspector shows.
- Fields and flags are the registry's fields and flags.
- The script holds handles, not engine pointers.
- A binding generator emits the glue. Hand-written script wrappers for each component are a drift bug.
- The script ABI, whatever the language runtime needs, sits behind the C ABI. The language runtime is not linked into every dedicated-server tool by accident.

Until that choice, do not add a scripting crate, a `node:` import in engine code, or a second component schema "for scripts."
