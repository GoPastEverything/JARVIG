# ADR-0023 — ABI struct negotiation and calling convention

Status: Accepted
Date: 2026-09-22

## Context

ADR-0022 names one `JARVIG_API_VERSION` and a C ABI. It does not say how a single struct or function table survives a newer engine, and it does not name the export macros. `native/jarvig_core/include/jarvig_core.h` is easy to mistake for the SDK because it is the only C header in the tree.

## Decision

Every long-lived ABI struct and every function table begins with `struct_size` and `api_version`. The engine trusts `struct_size` as the caller's compiled size. It does not read past that. Reserved words stay zero until a version assigns them. A global version number remains, and it does not replace the per-struct fields.

The SDK headers, not the compiler's default, declare the boundary:

- `JARVIG_EXTERN_C`
- `JARVIG_CALL` on every exported function
- `JARVIG_API` for import and export

On the 64-bit ABIs JARVIG ships first, `JARVIG_CALL` may expand to nothing. The macro still exists so that emptiness is a choice.

`jarvig_core.h` is a bootstrap transitional slice. It is not the public SDK. The SDK will live under `sdk/c/include/jarvig/` and `sdk/cpp/include/jarvig/` when a ticket creates it. Those directories are not created by this decision. New public functions do not accumulate in the bootstrap header.

## Alternatives Considered

- One global version and flexible struct tails. Rejected. Callers and engines then disagree about how long a struct is.
- Leave calling convention to the platform compiler. Rejected. It has worked on Windows x64 by accident. The SDK should not depend on that accident.
- Rename `jarvig_core.h` now and start the SDK tree. Rejected. Nothing public is ready to move, and an empty SDK tree would look finished.

## Consequences

Plugin tables in [../api/game-module.md](../api/game-module.md) follow the size-and-version prefix. The bootstrap header's comment states what it is not.

## Supersedes

Nothing. It fills in ADR-0022.

## Superseded By

Nothing.
