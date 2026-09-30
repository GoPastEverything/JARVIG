# ADR-0007 — Materials compile through an IR

Status: Accepted
Date: 2026-09-22

## Context

Hard-coded shaders and materials stored as WGSL do not survive a renderer change, and they do not support master materials, instances, or permutation budgets.

## Decision

Authored materials are graphs or, before the graph exists, the canonical document `jarvig.material/v1`. The meaning of a material is the Material IR after validation, not the backend source. The pipeline is graph, typecheck, IR, optimization, permutation analysis, WGSL, pipeline cache.

Baseline shading is metallic/roughness PBR with explicit color spaces and channels. Static permutations stay scarce and are counted.

## Alternatives Considered

- Hand-written WGSL files as the asset. Rejected. Assets would die with the first backend change.
- Adopt MaterialX as the native authoring format now. Rejected as the native form. MaterialX import is a later interoperability layer after the IR is stable.

## Consequences

Shader strings in examples are compiler output or tests. They are not the project file format. Instances override parameters. They do not copy graphs.

JRV-0051 is the first slice of this pipeline, not a change to it. That slice is Surface, Unlit, and Opaque. The baseline shading model remains metallic/roughness PBR. JRV-0053 adds `StandardMetalRough` beside Unlit and records the surface contract in [ADR-0028](ADR-0028-standard-metal-rough-surface.md). The graph still describes the surface. It does not contain a light. Optimization, static permutations, and the pipeline cache are still later. WGSL is the first compiler target. It is not the IR.

## Supersedes

Nothing.

## Superseded By

Nothing.
