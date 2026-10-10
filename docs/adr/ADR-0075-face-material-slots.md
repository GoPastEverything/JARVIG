# ADR-0075 — A face may name a material slot

Status: Proposed
Date: 2026-10-05

## Context

ADR-0062 gave a parametric solid one material slot. ADR-0069 stored durable face, edge, and vertex ids and said the ids a per-face material would use are already on the body. Authors need to color one face, or several faces, and find that region again. The motivating look is a stylized surface with its own parameters. That look is not this decision.

ADR-0073 keeps the evaluated body as the geometry. A face assignment sits beside that body. It is not an intent step. ADR-0074 stays Proposed. Accepting this record would not accept ADR-0074, and it would not make a material assignment reconstruct the solid.

## Decision

This ADR is Proposed. It is not accepted.

A parametric solid may store additional material slots, and a face id may name one of them. Slot 0 remains `BlockRecord.material`. A face with no assignment uses slot 0. The derived mesh splits each face's triangles into the submesh for its slot, in ascending slot order, and omits an empty slot. The standard surface, the analytic collision box, and the exact-solid rule stay.

This record supersedes only the sentence "One material slot." in ADR-0062. It does not supersede ADR-0007, ADR-0028, ADR-0069, ADR-0072, or ADR-0073.

The slice recorded with this draft:

- A solid has at most 16 slots. A 17th slot is refused and the record stays as it was. A slot added from the object card copies slot 0 and is not assigned. Make Unique copies the slot the selected faces share, or slot 0 when those faces disagree, and assigns only that selection. The scheme stays Builtin and the name stays `standard_white`. The texture set stays `standard_white`. The author edits base color, roughness, and metallic. Alpha stays 1. There is no rename box and no second shading model.
- An assignment stores the concrete face id, the slot, and `provenance`. Provenance copies semantic tokens that already named the face, such as `F:seed/2`. An empty list means none were recorded. The tokens are not a geometric authority. A paint does not append `intent` and does not call `push_op`.
- The concrete face id is the binding this slice draws with. When that id is no longer on a body the assignment can bind, the row stays, the face key is omitted, and the slot and provenance stay. This slice does not rebind the row, and it does not attach an orphan to a numeric id that is later reused. Reset Shape orphans every assignment and keeps the extra slots.
- A canonical cube with no stored body, no topology history, and no analytic bevel or inset binds faces 1 through 6. Painting those faces does not write a body. A record that has topology history, or an analytic bevel or inset, and no stored body does not paint a body realized later.
- Subdivide copies the parent face's slot onto every new cell. The original face id keeps its slot. Extrude keeps the cap ids and copies the extruded face's slot onto each new wall. Bevel, and an extrude of an edge, copy a slot onto a new face only when every face beside the replaced edges already shares that slot. Mixed neighbors leave the new face on slot 0. Colors are not averaged. When that happens, the log says "The new faces stayed on the default material." once on commit.
- A block with a painted face builds one `StandardMetalRough` instance per used slot and binds that instance to that slot only. An imported mesh still binds one instance onto every slot. Collision stays the analytic box of `size_m`. An exact solid still builds no Einstein relief. The meshlet grid is not retuned.
- The inspector Surface card names the selection and the slot. Slot 0 reads Object Material. Make Unique copies the shared slot into the next slot and assigns only the selected faces. A color edit on a selection that does not already own that slot does the same copy and then writes the factor, as one undo entry. Assign Existing moves the selection onto a slot that is already there. Select Faces Using Material replaces the face selection and is not an undo entry. Mixed faces show Mixed and hide the factor fields. Object mode edits slot 0 and says that every unassigned face uses it. A selection that is every face on slot 0 edits that object material directly. There is no rename box.

Component version 1, level format 6, and type registry 9 stay. The optional keys are `materials` and `face_materials`. Both are omitted when empty. A plain unpainted cube still writes size and material only. There is no new component type and no C material API.

Later, and not built here:

- Named surface groups are [ADR-0076](ADR-0076-named-surface-groups.md). That record is Proposed and is not accepted.
- Textures. Derived triangles still get a per-triangle UV of (0, 0), (1, 0), and (0, 1). A later slice can build one planar UV per solid face when the mesh is derived, still without saving it. A slot may then multiply the existing base-color texture by the base-color factor and use a neutral ORM map. A checker is a builtin texture. Color space stays ADR-0027. Saved UVs and seams come after that. Virtual textures stay out.
- Stylized shading is a second master beside `StandardMetalRough`, compiled through the existing IR in ADR-0007. An outline is its own pass. Hatch comes after that. A node editor stays unbuilt. `jarvig.material/v1` assets, instances, presets, and import wait until a slot can show a color.

## Consequences

An author can give selected faces a flat color and select those faces again. The unpainted cube's file, triangle count, and meshlet count stay what they were. A second slot adds a submesh and does not change the analytic collider. An orphan survives a reset so a later slice can rebind it from provenance. Semantic rebinding is not this slice. Accepting this ADR does not accept ADR-0074 and does not make materials reconstruct the solid.
