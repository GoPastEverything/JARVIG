# ADR-0056 — Sockets sit on the ball, and the level editor is the default

Status: Accepted. Partially superseded by ADR-0057: a joint-debug marker is a pick target only in the Character Editor.
Date: 2026-10-01

## Context

JRV-0092 opened Base and Base Male as rigid parts on the joint. The human selected `head.001` in the Character Editor. The transform gizmo and the joint-debug axis sat at the feet. Every mesh that shared a glTF node shared that node's origin, and a bilateral mesh was one joint, so one pivot could not sit on both arms.

The same window was the Character Editor because the startup level is joints plus world settings. The human wants the level editor when a project or a level opens, and the character setup as the alternative.

This decision supersedes two sentences and nothing else. ADR-0053 still says the pose is the local frame and that FBX is not an importer. ADR-0054 still says the Character Editor reads and writes that same frame and joint. ADR-0055 still says the character file is rigid parts, wide-default limits, null collision, and a null animation set.

## Decision

Opening a project or a level opens the level editor. The window title is the level name. The tree is the World Outliner. The viewport is Perspective. The Character Editor is entered on purpose: View > Character Editor, when the open level is joints plus world settings, or a double-click of a character asset. Double-click still does not place a copy. Dragging a character asset still does not spawn. Save still writes a level file and does not overwrite the character file.

This partially supersedes only this sentence of ADR-0054: "A level whose actors are joints, plus world settings, opens in that workspace."

Each kept source mesh is split into connected islands, then islands whose world boxes meet within 1.5 cm stay one rigid part. One cluster keeps the source name. Two clusters are `{name}.L` and `{name}.R`, with L toward world −X. The doll faces −Z, so that side is the character's left. The frame origin is the ball on that part nearest the contact parent's bounds center. A ball is an island whose radius coefficient of variation is under 0.12 and whose mean radius is between 4 mm and 40 mm. If the part has no ball, the parent's ball is used when it lies within 12 cm of the part center. Otherwise the origin is the part's bounds center. The root origin is the hip bounds center. Vertices are rebased by that offset, so the rendered body stays in the source world place. The source node translation, rotation, and scale stay on the bind report. They are not the socket.

This partially supersedes only this sentence of ADR-0055: "Pivots stay the source node origins. They are not moved to guessed socket centers."

The joint parent stays the spanning tree of parts whose world boxes meet within 0.02 m, rooted at the hip. It is contact order. It is not a measured bone list. The split stops one bilateral box from parenting an elbow to the hip merely because the unsplit mesh was huge. The first box the search touches is still the parent.

A viewport click on a joint-debug pivot selects that joint. A click on the mesh selects that part. The gizmo is drawn at the frame origin, which is the socket. Rotating the joint spins around that socket.

## Consequences

JRV-0092 visual acceptance stays open. Base and Base Male are 66 rigid parts each. Triangle totals stay 158,224 and 150,752. Limits stay wide defaults. The axis stays +Y and the secondary axis stays +X. Collision and the animation set stay null. Skinning, clips, IK, and ragdoll stay absent. The shadow pass still updates 16 casters. FBX stays outside the engine.
