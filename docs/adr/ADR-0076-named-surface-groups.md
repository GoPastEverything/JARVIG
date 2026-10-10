# ADR-0076 — A solid may name a surface group

Status: Proposed
Date: 2026-10-05

## Context

ADR-0075 lets a face name a material slot. That slot is how the face is drawn. It is not a name for the region. An author who paints the upper rim, subdivides one of those faces, and reloads the level still needs to find that region by the name they gave it.

ADR-0073 keeps the evaluated body as the geometry. A surface group sits beside that body, the same way a face assignment does. It is not an intent step. ADR-0074 stays Proposed. ADR-0075 stays Proposed. Accepting this record would not accept either of those, and it would not make a group reconstruct the solid.

## Decision

This ADR is Proposed. It is not accepted.

A parametric solid may store named surface groups. A group has a stable id, a user-visible name, one material slot, and members. A member stores semantic tokens when those tokens were already recorded for the surface. The concrete face id is the current binding of that member. It is not the long-term identity of the group. A group is not a tessellation, and a triangle is not a member.

This record does not supersede ADR-0073. It does not supersede ADR-0075. It does not supersede ADR-0007, ADR-0028, ADR-0069, or ADR-0072.

The slice recorded with this draft:

- Create Group From Selection names the selected faces. The name is trimmed, at most 64 characters, and unique on that solid. An empty selection, an unknown face, a face that is already in a group, faces that do not share one slot, or a face with no semantic name is refused and the record stays as it was. When the selection does not already own that slot, the group copies the shared slot first, the same way Make Unique does, so the rest of the solid stays on its previous slot. When the selection already owns the slot, the group uses that slot. The group does not write a body, does not call `push_op`, and does not append `intent`.
- Rename Group changes the visible name and keeps the id. Add puts the selected faces in the group and paints them with the group's slot. Remove drops those faces from the group and leaves their color. Select Group replaces the face selection with the faces that still resolve, and it is not an undo entry. Assign Existing, while the selection is one group, moves every resolved member onto the chosen slot. A color edit of a group that does not already own its slot copies a slot for the whole group and then writes the factor, as one undo entry. A selection that covers more than one group is refused. Delete Group removes the group. The faces, their colors, and the geometry stay. The id is not reused.
- The inspector Surface card names the selected face's group. None, one name, Mixed, or a name marked unresolved are the four readings. Object mode can still pick a group that exists on the solid.
- Subdivide joins each new cell to the parent's group when that parent is in exactly one group and its semantic tokens can be composed, for example `F:cell(F:seed/2,u,v)`. The kept face id stays a member. Extrude joins a new wall when that wall has one owning face and one boundary edge, and both can be named. Split keeps the existing members. A bevel, or an edge-wall, joins only when every neighboring face agrees on one group and the new face can be named without guessing an ordinal. Mixed neighbors, a missing name, or a token that is already recorded increment an ambiguity count and the new face is not stored as a face id alone. The geometric commit stays. The log says "A new face did not join a surface group." once.
- A member whose concrete face is no longer on the body stays in the group. The face key is omitted and the provenance stays. The row is not deleted and it is not attached to a later id by guesswork. Reset Shape orphans every member the same way and keeps the group.
- The derived mesh is unchanged by the group list. Triangle counts and meshlet counts follow the material slots, as in ADR-0075. Groups do not add a submesh.

Component version 1, level format 6, and type registry 9 stay. The optional keys are `surface_groups` and `next_surface_group`. `surface_groups` is omitted when empty. `next_surface_group` is omitted when it is 1. A deleted group's id is still reserved by writing the counter when that counter is no longer 1. A plain unpainted cube still writes size and material only. There is no new component type and no C group API.

Later, and not built here: saved UVs, texture assignment, material assets and presets, stylized shading, outlines, hatch, and a node graph. Marquee selection of elements, a viewport context menu, and Outliner folders are a separate editor pass.

## Consequences

An author can name a region such as Upper Rim, assign its material, deselect it, select it again by that name, subdivide one member, and find the same group after save and reload. A member that cannot be resolved remains in the file. An ambiguous new face stays out of the group and the solid still commits. Accepting this ADR does not accept ADR-0074 or ADR-0075, and it does not make a group the geometric authority.
