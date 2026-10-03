# Animation

The pose foundation is in. A Joint component stores kind, rest pose, limits, stiffness, and damping on an entity that already has a local spatial frame. That frame is the only character pose. ADR-0053. The Character Editor reads and writes those same frames and joints. ADR-0054. It is not a second skeleton. Clip playback, blending, root motion, IK, skinning, and ragdolls do not exist yet. `engine/animation/` is still reserved.

The default bodies are Base and Base Male in `samples/base-characters`. They are the ChamberSu ball-joint basemeshes, one standing copy of each, stored as `jarvig.character` version 1. Each source mesh is split into 1.5 cm island clusters, 66 rigid parts per body. The frame origin is the ball that joins that part to its contact parent. Vertices are rebased so the body stays in the source world place. The root is a Fixed joint. Every other part is a Ball joint on the same `SpatialFrame` ADR-0053 already uses. Limits are wide defaults, not measured ranges. `collision` and `animation_set` are null. ADR-0055 and ADR-0056. A player definition is `jarvig.player` version 1 and names one of those character files. `DefaultPlayer` names Base Male. The startup level is World Settings plus one Player Start that names `DefaultPlayer`. The bodies are not level entities. Preview Character is off until it is turned on. ADR-0059. Opening the project opens the level editor. Double-clicking a character opens that asset in the Character Editor and does not change the level. View > Mesh Parts lists imported mesh parts beside the contact hierarchy. Show Joints draws a 2.4 cm cross and a 4 cm axis on the selected joint. View > Show All Joints draws the rest. A joint-debug marker selects that joint only in the Character Editor. The level editor selects the part by its mesh. ADR-0057. The source files also contained extra posed copies and a black outline shell. Those stay out. `foreArmMesh.002` is one of those posed copies. Base Male's standing body includes `upperArmMesh.002` and `shoulderMesh.002`. Skinning does not exist, so the view is the rigid rest pose. The joint parent is a 0.02 m contact span from the hip, so the outliner is not an anatomical bone list. Credits stay in that sample. Do not publish the meshes.

Order from the founding doc:

1. Skeleton and clip playback. The skeleton half is the joint graph. Clip playback is later.
2. Blend and state graph.
3. Root-motion contract.
4. IK, aim, and look constraints.
5. Animation events.
6. Network-friendly state and pose compression.
7. Motion matching only when the vertical slice needs it.

The editor graph widget may be shared with materials, VFX, and AI. The animation compiler and runtime semantics are not the material IR. Root motion has to write into the local frame, not a global float32 origin.

Variable update is the presentation hook. Gameplay-affecting root motion belongs in the fixed step.
