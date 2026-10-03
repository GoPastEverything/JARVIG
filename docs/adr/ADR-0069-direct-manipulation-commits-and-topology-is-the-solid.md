# ADR-0069 — Direct manipulation commits, and topology is the solid

Status: Accepted
Date: 2026-10-03

## Context

ADR-0067 made Extrude, Inset, and Bevel one session, and it said mouse-up ends the drag and leaves the preview open. ADR-0068 said Edge and Vertex are not shown, because a block had no selectable edge or vertex. Both decisions fit an analytic box of size, six insets, and one bevel.

A drag the user has already posed should commit when the mouse is released. An edge has to be selectable on its own. Moving that edge has to reshape the faces that share it, which is how a rectangular side becomes a trapezoid. Extruding that edge has to add a new edge and the surface between them. Subdivision has to create faces, edges, and vertices that can be selected again. Those elements need durable ids, because a later gameplay or editor system has to be able to name one face or one edge after further edits. A triangle index or a meshlet index does not survive that.

Einstein detail, meshlets, and the cluster hierarchy draw a surface. They do not know which edges share a face, and they are not the check that keeps an edit from opening a crack. The authored object is a JARVIG solid with vertices, edges, faces, and adjacency. The derived surface is built from that solid.

The feature log stays a log. ADR-0066, ADR-0067, and ADR-0068 refuse a replayable construction tree. This decision does not add one. Editing the original size does not reapply a later topology operation. A topology row in Features is a label of the edit that was stored. It does not reopen an earlier step and rebuild the solid.

## Decision

A viewport drag of Extrude, Inset, Bevel, Move Edge, Extrude Edge, or Move Vertex is one editor transaction. Mouse-down begins it. The drag previews from the solid at mouse-down, and each component of a topology delta snaps to 0.05 m. Mouse-up commits that preview and closes the session. Escape, or a right-click during the drag, restores the solid from before the drag and pushes nothing. Ctrl+Z undoes the committed drag as one entry. Apply remains for a typed amount, and for an explicit confirmation when the mouse was not dragged. An unchanged release pushes nothing. Choosing Move or Rotate on the toolbar cancels the open session.

A resize or a focus loss during the drag still restores the drag baseline and leaves the session open. That sentence of ADR-0067 stands.

Selection on a parametric solid is Auto, Object, Face, Edge, and Vertex. The five modes stay an inspector section. They are not toolbar buttons. Auto and Face select a face. Object selects the entity and clears the element. Edge selects one edge. Vertex selects one vertex. The whole object outlines its analytic box. A face outlines that face. A selected edge is drawn thick. A vertex is a point. Tools follow the element. Face Tools are Extrude, Inset, Bevel, and Subdivide. Edge Tools are Move Edge, Extrude Edge, and Split Edge. Vertex Tools are Move. Object Tools stay Bevel, Reset Shape, Duplicate, Mirror, Align, and Snap. Scale stays disabled. Dissolve, Connect, Chamfer as its own operation, Loop Cut, Knife, vertex weld, per-face materials, and gameplay tags are not in this decision. The ids they would use are stored now.

The authored record gains an optional body. A plain block still writes size and material only, with no body key. The component version and the type-registry version stay where they are. Switching to Edge or Vertex computes a canonical box for display and picking and does not write it. The first committed topology edit stores the body. While bevel or any inset is nonzero, an edge or vertex edit is refused until those parameters are cleared. Once a body is stored, analytic face Extrude, Inset, and uniform Bevel are refused. The editor says to extrude a face by moving its edges, or to Reset Shape back to the box. A size edit of a stored body scales the vertices about the center and keeps the ids. A scale that would collapse an edge is refused. Reset Shape clears the body. Mirror flips an axis and reverses each face loop so the outward orientation survives.

Ids belong to the solid. They are not triangle indices and they are not meshlet indices. A face, an edge, and a vertex keep their ids across a move, a split, a subdivision, a scale, and a mirror. New elements receive new ids. Split Edge keeps the original edge id on the half that starts at the stored start vertex. Subdivide Face keeps the original face id on the cell that contains that face's first vertex, and keeps each original boundary edge id on the portion that starts at the stored start vertex. Face ids and vertex ids may use the same numbers. The types are distinct. On the canonical box the face ids are 1 through 6, the vertex ids are 1 through 8, and the edge ids are 9 through 20.

Move Edge translates both endpoints by the full drag delta from the mouse-down solid and leaves adjacency alone. The attached faces reshape with it. Extrude Edge is a separate operation. It chooses the incident face whose outward normal best matches the drag, splits the two side edges so the neighboring faces include the new vertices, and adds one quad between the original edge and the new edge. The other incident face keeps the original edge. Every ordinary edge still has two faces. A zero drag is not an extrusion.

Split Edge inserts a vertex at the midpoint and updates both incident face loops. Subdivide Face applies to a quad. The inspector button divides that face into a 2 by 2 grid of quads. The operation accepts 1 through 8 on each axis. One by one changes nothing. A face that is not a quad is refused. No face selected uses +X, and the editor says so. The new faces, edges, and vertices are selectable elements of the solid. This is not a change of tessellation density on an unchanged face. Split Edge and Subdivide Face commit on the click. They are not drags.

Every ordinary edit builds a candidate and validates it before the record changes. A valid solid has closed face loops, one use of each directed edge, no orphan vertex, no zero-area face, no zero-length edge, and exactly two incident faces on every ordinary edge. A candidate that would open a crack, leave a T-junction, collapse an element, or pass a segment through the interior of a non-incident face is rejected. The last valid preview stays. If the whole drag was invalid, the release commits nothing. An operation whose purpose is to open the surface is not in this decision. A rejected drag must not be treated as that operation.

The body is recentered so its bounds center stays at the local origin, and `size_m` becomes that bounds size. The entity translation takes the same shift once, in the entity frame. A preview applies the shift from the mouse-down translation, not again from the translation already previewed. Collision remains the analytic oriented box of `size / 2`. A slanted face stays inside that box. The collider does not become a triangle mesh, and it does not shrink to a chamfer. A result outside 0.05 m to 1000 m on any axis is rejected.

The derived mesh is built from the body when one is stored, and from the analytic surface otherwise. A canonical box body is 12 triangles. Move Edge stays at 12. Split Edge is 14. Extrude Edge is 16, because the new quad adds two triangles and each split neighbor adds one. Subdivide Face at 2 by 2 is 22. An authored block stays Exact because it is an authored block. Einstein detail, meshlets, and the visibility hierarchy remain downstream representations of the surface. They are not the topology, and this decision does not retune them.

History entries for these edits are `move-edge`, `extrude-edge`, `split-edge`, `subdivide-face`, and `move-vertex`. The log is still capped at 24 and is still not replayed. Features still reopens the current size, bevel, or inset. A topology row is read-only.

## Consequences

This decision supersedes two sentences and leaves the rest of those ADRs in force. ADR-0067's sentence "Mouse-up ends the drag and leaves the preview open" is replaced for a viewport drag. ADR-0068's sentences "Edge and Vertex are not shown" and "A block has no selectable edge or vertex" are replaced. Those files are not rewritten. ADR-0066 still owns the rule that the feature log is not a construction tree. ADR-0062 still owns the chain from the authored solid to the derived surface.

The disagreement stays open on ordered replay. A later ADR would have to name that refusal and store a base plus ordered features before an edit of the original 2 m block could reapply a later move. This record cannot. Clicking a topology row does not walk the log.

No JRV ticket is accepted. The open editor does not contain this slice until it is closed and rebuilt. Boolean, sketch, shell, fillet, loop cut, knife, dissolve, and an open surface stay later.

## Alternatives

Leaving mouse-up as a preview made Apply the only commit. That is the right end for a typed number. It is the wrong end of a drag the user has already placed.

Treating the derived triangles as the editable solid would drop the ids on the next rebuild, and it would ask the surface generator to police adjacency. The body is the solid. The triangles are a picture of it.

One command that both slides an edge and extrudes it would hide the trapezoid reshape. Move Edge and Extrude Edge stay separate.

Dissolving a collapsed edge into a triangle during the drag would change topology as a side effect of a move. The drag refuses the collapse. Dissolve can be its own operation later.
