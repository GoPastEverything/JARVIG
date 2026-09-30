# Animation

Not implemented. `engine/animation/` is reserved.

Order from the founding doc:

1. Skeleton and clip playback.
2. Blend and state graph.
3. Root-motion contract.
4. IK, aim, and look constraints.
5. Animation events.
6. Network-friendly state and pose compression.
7. Motion matching only when the vertical slice needs it.

The editor graph widget may be shared with materials, VFX, and AI. The animation compiler and runtime semantics are not the material IR. Root motion has to write into the local frame, not a global float32 origin.

Variable update is the presentation hook. Gameplay-affecting root motion belongs in the fixed step.
