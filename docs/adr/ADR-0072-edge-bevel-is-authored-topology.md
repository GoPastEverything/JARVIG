# ADR-0072 — An edge bevel is authored topology

Status: Accepted
Date: 2026-10-04

## Context

ADR-0070 stores faces, edges, and vertices on a parametric solid and refuses Inset and Bevel once that body exists. The Bevel that refusal names is the analytic chamfer: one distance applied to every outer edge of the box, stored as `bevel_m`.

A selected edge is a different request. That edge belongs to two faces. Replacing it means inserting a strip and changing those faces, and the faces at the ends, so the body stays closed. Selecting one edge does not mean moving that line and leaving the surrounding surface untouched. When the requested inset meets another selected edge, the corner has to be built once. When the requested inset does not fit, the solid has to stop rather than open.

## Decision

Edge mode can bevel the edges the user selected. The command is `bevel-edges`. The recorded distance is the perpendicular inset that fit, in meters. The recorded edge ids are the edges the cut followed. A collinear run that shares the same two faces is one strip, including a segment the user did not name, and the editor says so. Edges that do not meet are cut in one plan. Edges that meet share the corner. Two edges close on the edge between their side faces. Three or more edges that meet insert the point where their chamfer planes meet, and do not add a separate patch face. The corner strategy is that automatic meeting. Miter, round, and patch controls are not in this decision.

A requested width that the surrounding faces cannot hold is reduced to the largest inset that still passes the closed-solid check. The Width field shows that inset. A request that cannot make any positive inset, including a coplanar edge or an end the cap cannot close, is refused and the last valid body stays. The editor says "Cannot bevel this edge." and does not draw the width arrow. A width of zero commits nothing. The candidate is validated before it is stored. The editor does not keep a torn or non-manifold body.

Click selects one edge. Shift-click adds an edge on the same solid. Ctrl-click toggles one, and Ctrl wins. A click on a different solid replaces the set. The toolbar Bevel button and the Edge shelf Bevel tile start this session when Edge mode is on. Object mode and Face mode keep the analytic bevel, and that bevel stays refused once a body is stored. Inset stays refused on a stored body. Reset Shape still returns the box.

The width arrow points along the outward bisector of the two faces of the first selected edge. Dragging the arrow, or pressing the solid and dragging, snaps the change to 0.05 m and previews from the session baseline. A typed width is exact. Mouse-up commits and closes the session. Done commits a typed width. Escape, a right-click during the drag, and Cancel restore the baseline and push no undo entry. The whole gesture is one editor transaction.

The component version, the type-registry version, and the level format stay where they are. The history entry is `bevel-edges`, with the edge ids and the applied distance. The log is still capped at 24 and is still not replayed. Collision stays the analytic box of the recentered bounds. Einstein, meshlets, and the hierarchy stay downstream and are not retuned.

Segments, a curved profile, an asymmetric bevel, a separate chamfer tool, dissolve, crease, and an edge subdivide stay later. A double-click still promotes to the object. Loop cut stays later. Cylinder, sphere, cone, wedge, ramp, prism, and plane stay unstarted.

## Consequences

This supersedes only ADR-0070's sentence "Inset and Bevel on a stored body stay refused," and only for a bevel of selected edges. Inset stays refused. A uniform bevel of every edge stays refused. The ADR-0070 decision text is not rewritten.

ADR-0062 still owns the chain from the authored solid to the derived surface. ADR-0066 still owns the rule that the feature log is not a construction tree. An authored block stays Exact. No JRV ticket is accepted. The open editor does not contain this slice until it is closed and rebuilt. No GPU frame of an edge bevel was presented.

## Alternatives

Moving only the selected edge and leaving both side faces unchanged. Rejected. The edge is the boundary of two faces. A solid that keeps the old faces and adds a strip is either open or stacked.

Refusing a corner instead of building the meeting point. Rejected. The user asked for edges that meet, including the case where a proper bevel has to run around a corner.

Cutting each selected edge on its own and deleting a shared vertex twice. Rejected. The second cut would open the body the first cut had just closed.

## Supersedes

The stored-body refusal of Bevel in ADR-0070, for selected edges only.

## Superseded By

Nothing.
