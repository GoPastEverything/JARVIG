# ADR-0070 — Region extrude is authored topology, and the cage is the solid

Status: Accepted
Date: 2026-10-03

## Context

ADR-0069 stores an optional body on a parametric solid and refuses analytic face Extrude, Inset, and Bevel once that body exists. The editor told the user to extrude a face by moving its edges, or to Reset Shape back to the box. Moving every boundary edge is not the same operation as pulling a chosen set of faces out and growing the walls around them.

Subdivide Face already writes real faces, edges, and vertices. The overlay drew only the loop under the cursor and the one selected face, so a subdivided grid was hard to see. Object mode drew the analytic box and hid the new edges.

Einstein detail and the meshlet hierarchy can make the rendered surface dense without changing the authored faces. That densification is a picture of the solid. It is not a modeling operation, and it must not be the thing Subdivide changes.

## Decision

Modeling subdivision and render subdivision stay separate. Subdivide Face and Extrude of stored faces change the authored body: more faces, more edges, more vertices, with adjacency. Einstein, meshlets, and the cluster hierarchy consume the surface generated from that body. They do not add, remove, or rename authored elements, and this decision does not retune them.

Extrude on a stored body extrudes the selected faces. The command is `extrude-faces`. The selected face ids stay on the caps. Boundary vertices are duplicated and shared by the new walls, except a vertex whose whole fan is selected, which moves in place. A sweep that leaves the neighboring face grows one wall quad. A sweep that stays on that neighbor trims the neighbor to the new edge, so the overlapped strip is not a second polygon. Adjacent faces that still meet share that edge and its vertices. A candidate that would tear the solid, stack a coincident polygon, or cross into the next face is refused, and the last valid body stays. An empty selection, faces that do not share a direction, or a selection that cannot move 0.05 m along the average outward normal is refused before the session opens. Inset and Bevel on a stored body stay refused. Reset Shape still returns the box.

The viewport name of the cage is Show Grid, on the View menu. The cage is the editable topology, not a decorative lattice. Every line it draws is a stored edge. Every enclosed region is a stored face. Object mode draws the analytic box unless Show Grid is on. Face mode, and Auto once a body is stored, draw the cage without the toggle. Edge and Vertex already draw it. Hover highlights one region. A click selects that face. Shift-click adds a face on the same solid. Ctrl-click toggles one, and Ctrl wins when both are held. A click on a different solid replaces the set. The original face id stays on the cell that contains that face's first vertex, which is a corner of a grid, not the interior. Face Tools add a 4×4 button beside Subdivide. The toolbar Subdiv stays 2×2. No face selected still uses +X and says so.

The extrude arrow sits at the average of the selected face vertices and points along their average outward normal. Dragging the arrow, or pressing one of those faces and dragging, snaps the distance to 0.05 m and previews from the session baseline. Mouse-up commits and closes the session. The new cap and the new side faces are selectable at once, including for another extrude, another subdivide, or a later inset once that operation exists on a body. Apply remains for a typed amount. Escape, a right-click during the drag, and Cancel restore the baseline. A zero distance commits nothing.

The component version, the type-registry version, and the level format stay where they are. The history entry is `extrude-faces`, with the face ids and one delta. The log is still capped at 24 and is still not replayed. Collision stays the analytic box of the recentered bounds.

The first acceptance sequence is a block, a 4×4 subdivision of the +Y face, an extrude of one interior cell, an extrude of one new side face, then save and reload. The reloaded body keeps the created faces, edges, vertices, and the log lines. Meshlet coverage and the Exact classification consume that body and do not alter it.

Loop cut, edge rings, and a double-click that selects a connected island stay later. The existing double-click still promotes to the object. Dissolve, Connect, Knife, vertex weld, per-face materials, and gameplay tags stay out.

## Consequences

This decision supersedes one sentence of ADR-0069 and leaves the rest of that ADR in force: "Once a body is stored, analytic face Extrude, Inset, and uniform Bevel are refused. The editor says to extrude a face by moving its edges, or to Reset Shape back to the box." Extrude is no longer that refusal. Inset and Bevel still are. The ADR-0069 file is not rewritten.

ADR-0062 still owns the chain from the authored solid to the derived surface. ADR-0066 still owns the rule that the feature log is not a construction tree. An authored block stays Exact. No JRV ticket is accepted. The open editor does not contain this slice until it is closed and rebuilt. No GPU frame of the cage was presented.

## Alternatives

Drawing extra lines that do not match stored edges would show a grid the user cannot select. The cage is the body.

Letting Subdivide refine the Einstein patch would densify the picture and leave the editable solid unchanged. The user asked to edit the new regions. Those regions are authored faces.

Duplicating the boundary into a second, coincident polygon would open cracks the next time one copy moves. The neighbor and the new wall share one edge, or the operation refuses.
