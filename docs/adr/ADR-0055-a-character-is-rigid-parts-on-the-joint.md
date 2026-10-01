# ADR-0055 — A character document is rigid parts on the joint

Status: Accepted. Partially superseded by ADR-0056: the frame origin is the ball that joins the part to its parent. Vertices stay in the source world place.
Date: 2026-10-01

## Context

JRV-0092's default bodies are the ChamberSu Base and Base Male ball-joint dolls. Each source file is a pile of rigid mesh parts. Every mesh parent in the glTF is `RootNode`. The file has no skin. The first import merged each download into one mesh and left Base Male's `upperArmMesh.002` shoulder chain out, at 137,992 triangles. The standing detailed body is 38 parts and 150,752 triangles. The female standing body is 35 parts and 158,224 triangles.

ADR-0053 says an imported skeleton is a later consumer and that FBX is not an importer. ADR-0054 says an imported ball-joint character is a later consumer of the same joint. This decision is that consumer for the rigid bind pose. It does not supersede either ADR. Skinning, clips, IK, ragdoll, collision, and the kinematic pawn stay later.

## Decision

`jarvig.character` version 1 stores the entity records that already carry Transform, MeshRenderer, and Joint. Instantiation wraps those records in `jarvig.prefab` version 1. `collision` and `animation_set` are JSON null. Any other value is rejected.

The character file is in source space. A placed level may add one translation to the root. Part-to-part deltas stay the source node deltas. Vertices stay in the source local frame. The entity scale is the source node scale. The default glTF importer still bakes node world matrices and merges primitives. That path is unchanged.

Assembly keeps one standing body per source. The kept cluster uses material slot 0, world boxes that meet within 0.02 m, height 1.40–2.05 m, depth under 0.50 m, width under 0.90 m, and feet within 0.05 m of y = 0. Among those survivors it keeps the cluster with the most triangles. `foreArmMesh.002` is a different posed copy and stays out. The outline shell stays out.

The root entity uses the character name and is a Fixed joint. Every other part is a Ball joint. Limits are `wide-defaults` (unlocked), not measured ranges. The axis is +Y and the secondary axis is +X. Pivots stay the source node origins. They are not moved to guessed socket centers. Parts that share a node translation share that origin, so their joint-local pose is identity.

The joint parent is a spanning tree of parts whose world boxes meet within 0.02 m, rooted at the hip. The source parent remains `RootNode` and is listed in the bind-pose report. The tree is contact order. It is not a measured bone chain. An anatomical parent list would be a new decision.

Double-clicking the character asset opens the Character Editor on that one body. It does not place a copy, and Save does not overwrite the character file. Dragging a character asset does not spawn. The startup level `Content/Levels/Base.jarviglevel` still holds both bodies, so the project opens on the pair.

## Consequences

JRV-0092 visual acceptance stays open. The primitive mannequin stays the JRV-0089 fixture. FBX stays outside the engine. Collision and an animation set stay null until a later version bump. Rotating a part spins around the source node origin. Rotating a contact-tree parent moves every part the span hung under it. On these dolls the hip box meets the elbow box, so the elbow is a direct child of the hip in the outliner even though the mesh sits on the arm.
