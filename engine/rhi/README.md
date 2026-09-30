# RHI

The contract lives in `native/jarvig_rhi`, not in this directory and not in the editor. This folder stays empty so a TypeScript package cannot grow a second interface.

See [docs/rendering/rhi.md](../../docs/rendering/rhi.md) and [ADR-0019](../../docs/adr/ADR-0019-rhi-hides-backend-types.md). wgpu types are not allowed above the backend. The editor viewport consumes an engine surface. It does not implement one.
