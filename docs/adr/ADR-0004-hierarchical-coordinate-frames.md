# ADR-0004 — Hierarchical coordinate frames

Status: Accepted
Date: 2026-09-22

## Context

A single float32 world origin cannot hold planetary distances and centimeter FPS motion at the same time. Ships and interiors also move.

## Decision

The authoritative transform is a tree of frames. Parent-relative translation and rotation use IEEE-754 binary64 (`FrameTransform64`). Local simulation and rendering use a float32 domain near a useful origin (`LocalTransform32`, camera-relative offsets). JavaScript numbers are the Phase 0 binary64 storage. Float32 quantization uses `Math.fround`.

Renderer and physics consume the local domain. They do not take a raw root-space coordinate as their working precision.

## Alternatives Considered

- One float64 world for everything, including GPU uploads. Rejected. GPUs and many physics solvers want a local float32 origin anyway, and a single origin still makes moving interiors awkward.
- Origin rebasing of a global float32 world. Rejected as the primary model. Rebasing is a render technique, not the saved hierarchy. Camera-relative offsets are allowed as the render projection of this decision.

## Consequences

Cross-frame body transfer has to re-express velocity in the destination frame. Tests at large magnitudes are part of the contract, not a polish pass.

## Supersedes

Nothing.

## Superseded By

Nothing.
