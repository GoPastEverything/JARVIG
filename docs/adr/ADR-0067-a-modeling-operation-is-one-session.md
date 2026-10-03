# ADR-0067 — A modeling operation is one session

Status: Accepted
Date: 2026-10-03

## Context

ADR-0066 keeps a parametric solid as size, six face insets, and one bevel. The derived mesh is rebuilt from those numbers. History is a log of at most 24 edits. It is not a replayable feature tree.

The first editor for that record put Bevel, six Inset fields, and the raw history string on the inspector all the time, and it drew a cross on every face. A face drag committed when the mouse went up. That is enough to prove the solid is parametric. It is still a developer form. The same operation has to be startable from a toolbar icon, adjustable by dragging a handle, and settable by typing a number, with one shared value. Cancel puts the solid back. Apply keeps it.

A later request described a Fusion-style stack in which an earlier Block size could be edited and a later Extrude reapplied, and an intermediate bevel could be opened and muted. The evaluated record cannot do that. Extrude is baked into `size_m`. Successive bevels are absolute values, not stacked deltas. Replaying the log would not reconstruct the solid. This decision does not invent that stack.

## Decision

Extrude, Inset, and Bevel share one editor session. Extrude is the face push. There is no separate Press/Pull command. Inset is one face distance. Bevel is one uniform chamfer. The session stores the solid's size, insets, bevel, and local translation from the moment it opened, plus the current amount.

The toolbar says which operation. The inspector says the exact amount of that operation. The viewport shows the preview and the handle. A drag updates the amount. Typing the amount updates the preview. The three views do not keep separate copies of the number.

The toolbar keeps Select, Move, Rotate, Scale, Block, Level, Land, Character, and the rest of the accepted bar. Extrude, Inset, and Bevel are drawn after Block and before Level, with a gap before Extrude, only while a parametric solid is selected, the editor is not playing, and the workspace is not Land or Character. They are omitted otherwise, not dimmed in place. Capability still comes from `AuthoringCapabilities`. A light, a camera, a player spawn, an imported character, and an ordinary imported mesh do not show them. `BooleanOperand` stays false.

The permanent inspector no longer shows the Bevel field, the six Inset fields, or the raw history string. The registry group Shape is labeled Dimensions in the panel only. The registry names are unchanged, and the type registry stays version 9. A Features section lists the current block size and any nonzero bevel or inset, plus Extrude, Inset, Bevel, and Reset Shape. Pattern and Placement stay. While a session is open, those three start buttons and Reset Shape are hidden. The operation section shows the face (Bevel says All edges), an editable Amount in meters, Cancel, and Apply. Size X, Y, and Z stay visible and are read-only for the duration, so a size edit does not fight the session baseline.

The amount is an editor-only field. It is not a registry field and it is not sent through `SetProperty`. Preview commands update the mesh and do not append history. Apply writes one log entry when the preview differs from the session baseline, then closes. An unchanged amount closes without a new entry. Cancel restores the session baseline and does not append or erase the log.

Mouse-up ends the drag and leaves the preview open. That replaces ADR-0066's commit on mouse release. Escape, a selection change away from the solid, switching to another of these three tools, entering Play, and leaving Level for Land or Character cancel and restore. A resize or a focus loss during the drag restores the solid to the session baseline and leaves the session open at its starting amount. Keys 1 through 4 stay Select, Translate, Rotate, and scale-unavailable. They do not close the session.

No face selected uses +X for Extrude and Inset, and the editor says so. Bevel does not depend on a face. The drag measures distance along the face normal and snaps that delta to 0.05 m. A typed amount is not snapped. The drag adds the snapped delta to the amount that was current when the drag began, including a typed value such as 3.425. Extrude pushes from the session baseline by the absolute amount, so a second move does not stack on the preview. Inset increases as the drag moves against the outward normal. Bevel increases as the drag moves outward. Bevel and inset clamp to 45% of the shortest side and stay at or above zero.

Hovering a face outlines that face. Clicking the mesh selects the face and does not start a drag. During a session the outline is the session face and one arrow sits on it. Inset's arrow points inward. The overlay is lines. It does not draw dimension text. The live number is the Amount field.

The saved object is still `ParametricBlock`. A plain 2 m cube stays 12 triangles, 24 vertices, 36 indices, and 12 meshlets with exact coverage. A featured surface is not that audit. An authored block stays Exact. Einstein, Leaf Truth, visibility, and microgeometry are unchanged.

## Consequences

ADR-0066 still owns the evaluated solid, the capability table, the analytic collision box, and the rule that history is a log. This decision supersedes only the permanent Modeling form and the mouse-up commit. The ADR-0066 file is not rewritten. The disagreement with a regenerating feature tree stays open. A later ADR would have to store a base size and ordered features before a click on an intermediate bevel could rebuild the solid. This slice does not claim that.

Segments, profile, tangent propagation, shell, taper, split, fillet, arrays, union, subtract, intersect, sketch, revolve, sweep, and loft are not in this session. Neither are Cylinder, Sphere, Cone, Wedge, Plane, Capsule, Torus, or Ramp, nor Object, Edge, and Vertex selection modes. Scale stays disabled. No JRV ticket is accepted. RFC-0002 stays unstamped.

## Alternatives

Committing on mouse-up and keeping the numeric fields on screen was the ADR-0066 editor. It proved the kernel. It did not give one operation three coordinated controls.

Building a replayable feature tree in the same change would require a different saved record. The current log cannot be replayed, and presenting it as a timeline would be false.
