# Plugin ABI

Long-lived plugins do not use the Rust ABI or a C++ class layout. Both break across compiler versions. The boundary is a stable C ABI. The full doctrine is [ADR-0022](../adr/ADR-0022-api-layers-and-one-type-registry.md) and [../api/plugins.md](../api/plugins.md). This page is the short form.

The first slice of that idea is `native/jarvig_core/include/jarvig_core.h`: an opaque clock, and a headless runtime (`jarvig_runtime_create`, tick, shutdown, destroy). Profiles are integers. The server profile reports no render stage. That header is not the RHI and not the full plugin SDK. The RHI is in-process Rust in `native/jarvig_rhi` and does not cross this ABI yet.

Planned plugin kinds, none of which exist yet:

- native plugin
- engine subsystem
- editor panel (calls engine APIs, does not reimplement them)
- importer
- renderer extension, behind the RHI
- asset processor
- game module
- scripting module

An editor plugin that imports a material by itself is a bug. It calls the engine import API. The CLI calls that same API. See [ADR-0018](../adr/ADR-0018-engine-owned-capabilities.md).

Do not pass raw engine object pointers across this boundary. Handles and explicit structs only.
