# Stable C ABI

The binary contract. Headers are C. C++ callers include them inside `extern "C"`.

## What crosses

- Opaque handles. See [handles.md](handles.md).
- Plain integer and float fields with an explicit width (`uint32_t`, `uint64_t`, `float`, `double`).
- Structs of those fields, with a documented size and alignment. Do not rely on compiler packing guesses. A new field is a new struct version, not a silent append, unless the versioning note for that struct says append-only.
- Pointer plus length for strings and arrays. See [memory.md](memory.md).
- Function pointers in a versioned table, for modules the engine loads. See [game-module.md](game-module.md).

## What does not cross

- Rust references, slices, `String`, `Vec`, `Result`, trait objects.
- C++ classes, references, templates, `std::string`, `std::vector`, exceptions.
- wgpu, winit, or any other backend type.
- A raw pointer to an entity, a mesh, or a GPU resource.

## Calls the engine exports

Engine functions use a `jarvig_` prefix. They take explicit context. They do not read a hidden process-global editor. A session or world handle is an argument.

```text
JarvigResult jarvig_entity_create(JarvigWorldHandle world,
                                  const JarvigEntityDesc *desc,
                                  JarvigEntityHandle *out);
```

That signature is an example of the shape, not a frozen function. Do not add it until a world ticket needs it.

## Calls the module exports

The engine looks up one bind symbol and a version. It does not depend on a C++ vtable. See [game-module.md](game-module.md).

## Failures

Return `JarvigResult`. See [errors.md](errors.md). On failure, out-parameters are left unchanged. A panic inside the engine is a bug: catch it at the ABI edge, log it, and return a failure code. Do not let it unwind into the caller.

## Calling convention

The SDK owns the declaration. Compiler defaults are not the contract, even where Windows x64 and System V happen to agree today.

Future `sdk/c` headers define:

| Macro | Role |
| --- | --- |
| `JARVIG_EXTERN_C` | `extern "C"` in C++, empty in C |
| `JARVIG_CALL` | The calling convention on every exported function. Empty on the 64-bit ABIs we ship first. Present so a later target cannot silently change it. |
| `JARVIG_API` | `dllexport` when building the engine library, `dllimport` when a game includes the header. Empty for a static link, chosen in one place. |

Every exported function is `JARVIG_API JARVIG_CALL`. The bootstrap header does not define these macros. Adding them there would make that file look like the SDK.

## Struct and table negotiation

A global `JARVIG_API_VERSION` is not enough for a struct that lives for years. Each long-lived ABI struct and each function table starts with:

```text
uint32_t struct_size;
uint32_t api_version;
```

The caller sets `struct_size` to the size it was compiled with. The engine reads only the prefix it understands. A newer engine may accept an older struct. An older engine rejects a newer struct it does not understand, with `JARVIG_VERSION_MISMATCH`, instead of reading off the end.

Reserved words at the end of a struct (`uint64_t reserved[4]` or similar) are zero. They are not a place to hide a new field without bumping the version.

A module function table carries `struct_size`, `api_version`, a capability bit set, then the function pointers. See [game-module.md](game-module.md).

## The bootstrap header

`native/jarvig_core/include/jarvig_core.h` is a **bootstrap / transitional ABI slice**. It is **not** the public JARVIG SDK.

It exports `jarvig_clock_*` and `jarvig_runtime_*` only. Success is `0`. Failures are `-1` and `-2`. It has no `struct_size`, no `JARVIG_API`, and no mesh or world types. Leave it until a ticket migrates those two objects. Do not grow it.

The SDK, when it exists, lives apart from that file:

```text
sdk/
  c/include/jarvig/     jarvig.h, version.h, result.h, handles.h, ...
  cpp/include/jarvig/   Engine.hpp, World.hpp, Entity.hpp, ...
```

That tree is not created yet. New public C functions do not land in `jarvig_core.h` by convenience.

## The RHI

The RHI does not cross this ABI. A plugin that needs to draw uses a renderer extension capability, which the engine implements. It does not receive a `wgpu::Device`.
