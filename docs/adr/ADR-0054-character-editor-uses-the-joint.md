# ADR-0054 — The character editor edits the joint

Status: Accepted. Partially superseded by ADR-0056: a joint-only level opens in the level editor. The Character Editor is entered on purpose. Partially superseded by ADR-0059: a character asset opens in an isolated preview. A level that already contains a rig still uses this workspace in place.
Date: 2026-09-30

## Context

JRV-0089 put a Joint on the entity's local spatial frame and proved it with a primitive mannequin. That mannequin is a regression fixture. It is not an imported character and it is not a second skeleton.

Opening a character should feel like an asset editor: a skeleton tree, a preview of that character, and a joint inspector. The level editor stays the tool for worlds. Those are two workspaces. They are not two pose representations.

ADR-0053 already says animation, IK, skinning, ragdolls, and imported skeletons are later consumers of the same frame.

## Decision

The Character Editor is an editor workspace. It reads and writes the same `SpatialFrame` and `Joint` the runtime uses. It does not keep an editor-only skeleton.

A level whose actors are joints, plus world settings, opens in that workspace. Lighting Lab and any level that also contains ordinary meshes stay in the level editor. View > Character Editor switches the open joint level between the two presentations.

Double-clicking a prefab in the Content Browser opens the character workspace when the level is that skeleton. It does not spawn another copy. Dragging the prefab into the viewport still instantiates it.

The skeleton tree is the entity hierarchy. Its suffix is the joint kind. The preview is the existing perspective `RenderView`. The inspector shows kind, parent, child, rest pose, limits, stiffness, and damping. Reset Pose writes each joint's local frame back to its rest pose. Show Joints and Show Joint Limits toggle the existing debug overlay.

Animation, IK, and ragdoll controls are not in this workspace. Skinning is not in this workspace. An imported ball-joint character is a later consumer. FBX is not an importer. The kinematic pawn is not this decision.

## Consequences

JRV-0089 remains the mannequin regression. A character asset and a placed instance are different operations. A future skin or clip attaches to these entities. It does not replace them.
