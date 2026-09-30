# ADR-0003 — No BSP world model

Status: Accepted
Date: 2026-09-22

## Context

Quake-style BSP bakes visibility and collision into a static tree. That fights dynamic editing, streaming, moving ships, nested frames, and multiplayer authority.

## Decision

BSP is not the authoritative world representation. The world is entities in a frame tree, partitioned into cells, streamed by residency and relevance. Brush or portal preprocessing may appear later only as an optional derived acceleration structure that can be thrown away and rebuilt. It must not be the saved world.

## Alternatives Considered

- BSP as the world, with entities layered on top. Rejected. The baked tree becomes the real format.
- A heightmap-only world. Rejected. Interiors, ships, and arbitrary meshes do not fit.

## Consequences

Editing, streaming, and replication all speak entities, frames, and cells. A "compiled map" is derived data.

## Supersedes

Nothing. Matches `designdoc.html` and the bootstrap instruction.

## Superseded By

Nothing.
