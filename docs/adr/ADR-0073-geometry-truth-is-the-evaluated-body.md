# ADR-0073 — Geometry truth is the evaluated body

Status: Accepted
Date: 2026-10-04

## Context

ADR-0062 makes a parametric solid the object. The mesh, the meshlets, the hierarchy, and Einstein are derived. ADR-0063 releases that residency with the world. ADR-0066 stores the evaluated parameters and refuses to treat the feature log as a construction tree. ADR-0069 stores an optional body after the first topology edit, and that body is what later edits change. The log stays capped at 24 and is not replayed.

A later geometry system can synthesize a surface for one view. That work needs a contract before it needs a compiler. The contract has to keep the solid the editor already saves, keep a face id from being mistaken for a step, and leave a record a future replay could read. It must not replace the evaluated body, and it must not change the tools.

## Decision

The authoritative solid stays the evaluated `BlockRecord`. When a body is stored, that body is the solid. When it is absent, the solid is the size, the six insets, and the one bevel. The mesh is rebuilt from that record. Replaying `history` or `steps` is not a way to obtain it.

Derived render geometry is disposable. `DerivedRenderGeometry` holds the CPU mesh and the meshlet clusters built from a stored body. Hierarchy nodes, Einstein patches, and GPU buffers are not fields of that value and are not fields of the level. `discard` drops the mesh and the clusters. The body that produced them is not inside the value. Saving a level still writes no triangles, meshlets, hierarchy nodes, or Einstein patches. Loading the level and building the surface again produces the same body and the same clusters. Component version 1, type registry 9, and level format 6 stay as they are.

Concrete topology identity and semantic step identity are different types.

| Identity | Type | Names |
| --- | --- | --- |
| Concrete | `ConcreteElement` | One face, edge, or vertex id on the evaluated body |
| Semantic | `SemanticStepId` | One recorded operation |

The numbers may match. A step id of 1 and a vertex id of 1 are still different fields. An analytic face index, the `0..5` parameter on extrude, inset, and the uniform bevel, is not a `ConcreteElement`. Topology operations record the body ids they named: an edge move, edge extrude, edge split, face subdivide, vertex move, region extrude, or edge bevel.

`GeometryStep` sits beside `BlockOp`. Index `i` of `steps` describes index `i` of `history` while the tape is aligned. Each step stores its semantic id and its concrete elements. The operation payload stays in `history`, so existing log entries keep their fields. `next_step` is the next semantic id. It is not the body's `next_id`. The level writes `steps` and `next_step` only when the tape is non-empty. A file that has no `steps` key loads, keeps the body it stored, and leaves the tape empty.

`push_op` appends a step only when the tape is already aligned with the log, including a new solid whose log is empty. A log saved before this record stays unrecorded. The next edit on that solid still appends `history` and does not invent a partial tape. When the log passes 24 entries, the aligned tape drops the same prefix. Dropped step ids are not reused. The retained suffix still lines up. The missing prefix cannot be rebuilt from the file. The body remains the solid either way.

The editor commits the evaluated body and appends the same `BlockOp` it appended before. It does not read `steps`, and it does not replay them. Selection, bevel, extrude, inset, and the other modeling tools are unchanged.

Per-camera geometry synthesis is not part of this decision.

## Alternatives Considered

- Replace the stored body with a replay of the log. Rejected. ADR-0066. The evaluated body stays the solid. A truncated log cannot rebuild the prefix that fell off the front.
- Store the step id in the face, edge, or vertex id. Rejected. Those ids are the concrete element. A later edit has to name that element after the step that created it is no longer the whole story.
- Start a partial step tape on the first edit of an older log. Rejected. A suffix that is shorter than `history` is not a record a future reader can join by index.
- Generate a different mesh per camera in this change. Rejected for this slice. The derived surface is still one mesh and one cluster set for the stored body.

## Consequences

Save, delete the derived mesh and clusters, reload, and regenerate. The authoritative body matches, and the regenerated clusters match. An older level that omits `steps` reloads that same body. No JRV ticket is accepted. RFC-0002 stays unstamped. The open editor's tools are unchanged. A process started before this record does not write `steps` until it is closed and rebuilt.

## Supersedes

Nothing. ADR-0066 still rejects replay as the source of truth. ADR-0069 still stores the evaluated body and still refuses to rebuild it from the log. This decision records a future form beside that body.
