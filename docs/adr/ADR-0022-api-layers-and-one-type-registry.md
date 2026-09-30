# ADR-0022 — API layers and one type registry

Status: Accepted
Date: 2026-09-22

## Context

ADR-0017 makes Rust the core language and a stable C ABI the plugin boundary. ADR-0018 makes the editor, the CLI, and automation callers of engine capabilities. ADR-0019 keeps backend types private. Those rules are necessary and not sufficient. Worlds, meshes, materials, assets, physics, and gameplay are about to grow public surfaces. If each one picks its own handle, error, and threading story, the engine grows five copies of Transform.

`native/jarvig_core/include/jarvig_core.h` is a bootstrap slice: an opaque clock and a headless runtime, with integer return codes. It is not the SDK. The in-process RHI uses Rust newtypes such as `BufferId`. Those newtypes are not an ABI.

## Decision

External consumers talk to JARVIG through the layers in [../api/README.md](../api/README.md). Internal crates keep calling each other with Rust types. They are not routed through C for speed or for fashion.

The stable binary boundary is C. Not the Rust ABI. Not a C++ class layout. Not `std::string`, `std::vector`, a Rust `String`, or a Rust `Vec`. The C++ SDK, when it exists, is a header wrapper over that C ABI. RAII lives only in the wrapper.

There is one canonical type description. The inspector, serializers, scripts, network replication, and the sequencer are projections of it. They do not own a second schema for the same concept.

Public identities are opaque JARVIG handles. Where a slot can be reused, the handle is generational: index plus generation. A stale handle fails with `InvalidHandle`. Raw engine pointers, GPU objects, and Rust references do not cross the ABI.

Errors at the boundary are `JarvigResult` codes. A Rust panic does not unwind across the ABI. A C++ exception does not cross it either. Detail text goes through the engine log, not through an ad hoc string return.

Authoring mutations that need undo are commands. Runtime ticks are not. The editor does not reimplement the capability the command calls.

Plugins receive the capabilities their category needs. They do not receive a pointer to the entire engine. A game module is a versioned C function table loaded from its own module, not a Rust trait object across a DLL.

`JARVIG_API_VERSION`, plugin API version, schema versions, material IR version, network protocol version, and cook version move independently.

No scripting language is chosen. No automation wire protocol is chosen. Both must sit on this doctrine rather than becoming a second engine API.

## Alternatives Considered

- Force every internal crate call through C. Rejected. It throws away Rust's type system on the hot path and still does not make the ABI stable by itself.
- Publish the Rust API as the plugin SDK. Rejected. ADR-0017. The Rust ABI is not stable across compiler versions.
- Let the C++ SDK be the binary boundary. Rejected. Name mangling, exceptions, and standard-library layout break across compilers. C++ is the pleasant wrapper, not the ABI.
- One untyped string event bus and one editor-only property bag. Rejected. That is how inspector, save games, and scripts drift apart.
- Undo-wrap every runtime write. Rejected. The dedicated server and the fixed step do not have an undo stack. Commands are the authoring transaction. The runtime API is the simulation.

## Consequences

New public surface for scene submission, materials, assets, world mutation, plugins, or gameplay is reviewed against `docs/api/` before it spreads. Internal renderer and RHI work stays in-process Rust.

The bootstrap header is not rewritten by this decision. The next public C function follows `JarvigResult`, explicit ownership, and opaque handles. It does not copy the header's `-1` / `-2` style forward.

## Supersedes

Nothing. It fills in ADR-0017 and ADR-0018. It does not replace them.

## Superseded By

Nothing.
