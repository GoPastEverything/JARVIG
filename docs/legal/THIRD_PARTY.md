# Third-party register

Update this file in the same change as any new dependency, vendored source, or upstream remote. Machine list: [`../../LICENSES/dependency-manifest.json`](../../LICENSES/dependency-manifest.json). Notices: [`../../LICENSES/THIRD_PARTY_NOTICES.md`](../../LICENSES/THIRD_PARTY_NOTICES.md).

JARVIG's own license is not chosen. See [`../../LICENSES.md`](../../LICENSES.md).

## Direct toolchain dependencies

Recorded from the install on 2026-09-22. These are development tools, not shipped engine code.

| Package | Version | License | Why |
| --- | --- | --- | --- |
| typescript | 5.9.3 | Apache-2.0 | Project compile |
| vitest | 3.2.7 | MIT | Test runner |
| tsx | 4.23.15 | MIT | Run TypeScript hosts in development |
| @types/node | 22.20.4 | MIT | Node type declarations |
| koffi | 2.16.3 | MIT | Host-only FFI so Node hosts can call `jarvig_core.dll`. Not linked into the engine. |
| wgpu | 24.0.5 | MIT OR Apache-2.0 | Private GPU backend in `native/jarvig_rhi_wgpu` only. |
| winit | 0.30.13 | Apache-2.0 | Window and event loop inside `native/jarvig_platform` only. Not an engine type. |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | Passes an OS window handle from the platform crate to the wgpu backend. |
| pollster | 0.4.0 | Apache-2.0 OR MIT | Blocks on wgpu adapter and device creation inside the backend. |
| zune-jpeg | 0.5 | MIT OR Apache-2.0 OR Zlib | Decodes embedded glTF JPEG images into the mesh's own material. Not a renderer dependency. |

Transitive licenses ship with those packages. Re-run a license inventory before any public distribution. This table is not a full transitive scan.

## PlayCanvas

| Item | Value |
| --- | --- |
| Project | PlayCanvas Engine |
| Upstream | https://github.com/playcanvas/engine |
| License | MIT. Copyright (c) 2011-2026 PlayCanvas Ltd. |
| In this repository | **Not vendored.** Git remote `upstream-playcanvas` points at the upstream. No `origin` remote. |
| Policy | Reference, optional adapter, not the engine. [ADR-0017](../adr/ADR-0017-native-engine-core-and-multi-host-runtime.md). License and upstream rules remain [ADR-0002](../adr/ADR-0002-playcanvas-foundation.md). |

A future import must copy the MIT notice into `LICENSES/THIRD_PARTY_NOTICES.md` and pin the upstream commit.

## CryEngine

Not a dependency. Not vendored. Community patches that are MIT do not make the engine MIT. See [CRYENGINE_BOUNDARY.md](CRYENGINE_BOUNDARY.md).

## Reference only (Zone C)

Public documentation used as behavior reference. No code was copied.

- PlayCanvas ECS user manual
- PlayCanvas Editor repository, as an editor-architecture reference
- Epic Games documentation for materials, material instances, material functions, virtual texturing, Nanite, and World Partition
- CRYENGINE Community Edition README, for the license boundary statement

Epic and Crytek materials are not licensed for source reuse. Do not copy sample shaders, headers, or implementation from them.
