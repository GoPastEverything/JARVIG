# ADR-0009 — ECS identity separated from storage

Status: Accepted
Date: 2026-09-22

## Context

The founding ECS keeps PlayCanvas's entity/component/system idea and asks for stable identity, versioned schemas, and dense storage where that helps.

## Decision

Entity ids are UUIDs and survive save, load, and duplication (a duplicate receives a new id). Hierarchy is `parentId` on the entity. Component schemas are versioned and carry replication metadata. Snapshots are `jarvig.scene/v1`. Storage may change from the Phase 0 map to archetype or SoA storage without changing the snapshot or the schema. A schema version mismatch refuses to load until a migration exists. There is no silent downgrade.

## Alternatives Considered

- Incrementing integer ids. Rejected. They collide across editor sessions, network peers, and duplicates.
- Class inheritance as the extension model. Rejected. The founding rule is data-oriented components.

## Consequences

Systems, when they exist, query schemas. They do not own a parallel object tree. Prefabs will be templates plus overrides on top of this, not a second id space.

## Supersedes

Nothing.

## Superseded By

Nothing.
