# ADR-0006 — WebGPU-first renderer

Status: Partially superseded
Date: 2026-09-22

## Context

The founding renderer is GPU-driven: scene databases, compute visibility, indirect draws, and later virtualized geometry. WebGL2 can address older hosts. It cannot be the design center.

## Decision

WebGPU is the rendering target. WebGL2 is a possible compatibility backend. The CPU describes scene state and work graphs. The Phase 0 `RenderDevice` interface allows `webgpu`, `webgl2`, and `null`. Shipping hosts currently use `null` so they boot without a GPU. That does not add a third permanent backend. `null` is a test and headless stand-in.

## Alternatives Considered

- WebGL2-first, WebGPU later. Rejected. The GPU-driven design would be bent around WebGL2 limits and then rewritten.
- A native graphics API as the first backend. Rejected until a measured port. The founding target is WebGPU.

## Consequences

Material codegen targets WGSL. A WebGL2 backend, if built, consumes the same Material IR. Simulation code does not call the GPU API.

## Supersedes

Nothing.

## Superseded By

[ADR-0017](ADR-0017-native-engine-core-and-multi-host-runtime.md) supersedes the reading that WebGPU means a browser renderer, and the rejection of a native graphics API as a first backend. WebGPU remains the first RHI backend (native wgpu and browser WebGPU). D3D12, Vulkan, and a later Metal backend are RHI implementations, not a different engine.
