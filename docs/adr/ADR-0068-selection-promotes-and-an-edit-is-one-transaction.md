# ADR-0068 — Selection promotes, and an edit is one transaction

Status: Accepted
Date: 2026-10-03

## Context

ADR-0067 made Extrude, Inset, and Bevel one session. A click on a block selects a face, and the panel lists that face's commands next to the current size, bevel, and insets. That is the right preview. It is not yet a selection model, and it is not an undo stack. A drag still has to be one user intent. A double-click has to mean the whole solid. A box drag has to mean several objects. Those are editor session rules. They are not a new saved solid.

The block record is still size, six insets, one bevel, and a log of at most 24 edits. ADR-0066. That log is not a Fusion timeline. Extrude is already baked into size. A bevel entry is the current amount, not a delta that can be muted and replayed.

## Decision

A single click selects the smallest useful thing for the current mode. On a parametric solid, Auto and Face select the hit face. Object selects the entity and clears the face. A double-click promotes to the owning entity, clears the face, and does not change the mode. It does not retarget an open modeling session onto a different entity. A click on empty viewport space clears the selection unless Shift or Ctrl is held. Shift adds. Ctrl toggles. If both are held, Ctrl wins. Escape does one thing: it cancels an open modeling session, or drops a marquee without selecting, or cancels a gizmo or sculpt stroke, or clears the selection. It does not cancel a tool and clear the selection on the same press.

Select mode, and only Select mode, arms a marquee when the press misses the gizmo, the operation handle, a character joint pivot, and the scene. The drag is not a mouse capture that hides or recenters the cursor. A movement of four pixels or less is still a click. Left to right selects objects whose eight projected corners are all in front of the camera and inside the rectangle. Right to left selects objects whose on-screen corner box touches the rectangle. A corner behind the camera fails a window selection. Shift adds. Ctrl toggles. The marquee selects whole objects and clears the face, including while Face mode is on. World Settings, hidden Land actors, and a block's derived mesh are not separate targets. A terrain chunk selects the terrain actor once. The test is the projected box, so a thin mesh can be selected when the rectangle touches that box and misses the triangles.

Auto, Object, and Face are a Selection section on a parametric solid. They are not toolbar buttons. Edge and Vertex are not shown. A block has no selectable edge or vertex. Bevel is one chamfer of every edge. Tools appear only for the current element. A face shows Extrude, Inset, and Bevel. The whole object shows Bevel, Reset Shape, Duplicate, Mirror, Align, and Snap. Move and Rotate stay the toolbar tools. Scale stays disabled. Pattern and Placement are not section titles. Features lists the current block size and any nonzero bevel or inset. Clicking one of those rows reopens that current amount, with Cancel, Apply, and the arrow. It does not rebuild the solid from an earlier step.

Editor undo is a transaction stack on the editor, capped at 64, dropping the oldest. It is not the block's feature log, and it is not an ad-hoc inverse call. Ctrl+Z undoes. Ctrl+Y and Ctrl+Shift+Z redo. The Edit menu names the open gesture or the top entry. Undo and Redo are not new toolbar icons. A gesture begins when the interaction begins and commits when it ends. A bevel drag, a Move or Rotate drag, a terrain sculpt stroke, and a numeric field from focus to kill are each one entry. Intermediate amounts are not entries. Cancel, including Escape during the gesture, restores the before-stamp and pushes nothing. An unchanged Apply pushes nothing. A new action clears redo. Undo while a gesture is open cancels that gesture and does not pop the previous entry. Redo is refused while a gesture is open. Play does not record ticks, and undo and redo refuse during Play. Selection is not an entry. Loading a level clears both stacks and does not apply them to the new world.

The stack stores each touched entity's save record before and after, plus the world-settings numbers when World Settings is in the set. Those numbers are not inside the entity record, so the editor writes them back after the entities. Create Block, Create Terrain, Delete, Duplicate, Mirror, Align, Snap, Reset Shape, a committed modeling Apply, inspector property edits, reset transform, reset material, add and remove component, startup camera, probe update policy, a terrain stroke, Place Mesh, and Place Prefab are transactions. A prefab placement is one transaction with one created id per part. Place Mesh replaces the old content-browser undo list. There is no clipboard, so there is no Cut, Copy, or Paste. Collision stays the read-only analytic box. Array does not exist.

## Consequences

ADR-0066 still owns the evaluated solid and the rule that the feature log is not replayable. ADR-0067 still owns the modeling session. This decision supersedes the mix of commands into the Features list, and it supersedes the earlier statement that the editor has no undo stack. Those ADR files are not rewritten. The toolbar is unchanged.

The disagreement stays open. A regenerating construction tree would let a later edit of the original 2 m block reapply Extrude, and it would let an intermediate bevel be suppressed, duplicated, moved earlier, moved later, or deleted. The stored record cannot do that. Clicking Bevel 0.900 m reopens 0.900 m. It does not walk the log. Edge and Vertex modes wait until those elements exist. A right-click feature menu that pretends otherwise would be false.

A marquee hit is a screen box, not a triangle test. Imported-mesh wire boxes are not drawn. Non-block actors have no GPU selection silhouette. No JRV ticket is accepted. RFC-0002 stays unstamped. Shell, split, fillet, chamfer as a separate command, boolean, array, sketch, and the construction library stay out.

## Alternatives

Leaving Ctrl+Z as "undo the last mesh drop" made placement reversible and left every other edit permanent. Folding that list into the transaction stack keeps the placement check and gives the other edits the same key.

Putting Undo and Redo on the toolbar would clip Max. The bar is already full. The menu and the keys are the controls.

Showing Edge and Vertex as disabled buttons would advertise a selection the solid does not have.
