# ADR-0013 — World identity is multiplayer-ready

Status: Accepted
Date: 2026-09-22

## Context

Adding multiplayer after a single-player world model usually replaces ids, ownership, and relevance. The founding doc forbids that.

## Decision

Entity UUIDs, frame ids, and world-cell ids are the identity the network will use. Component schemas carry replication metadata. Relevance for render, physics, AI, audio, and network is the same cell model. The server is authoritative for position, damage, inventory, and entitlements. Prediction and replication modes (`authority-only`, `owner-predicted`, `snapshot`, `reliable-event`, `never-replicate`) are the intended field metadata. Server meshing is not implemented and stays a flagged RFC.

Phase 0 stores a replicated boolean, not the full mode enum. Extending the schema is future work on this decision, not a new world model.

## Alternatives Considered

- Build the game, then add netcode. Rejected by the founding doc.
- Implement server meshing now. Rejected. It is research, and the slice does not need it.

## Consequences

Systems that allocate a second id for "the network copy" are drift. Replicated state is the same entity.

## Supersedes

Nothing.

## Superseded By

Nothing.
