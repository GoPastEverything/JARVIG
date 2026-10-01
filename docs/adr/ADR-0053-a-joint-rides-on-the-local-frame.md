# ADR-0053 — A joint rides on the local frame

Status: Accepted
Date: 2026-09-30

## Context

Character work needs one pose that animation, IK, ragdolls, procedural motion, and imported skeletons can all write later. JARVIG already has that pose. `FrameGraph` stores a parent-relative translation and quaternion, and a child frame composes through its parent. A second skeleton transform would split the engine into incompatible character systems.

The ball joint is an engine concept. It is not support for one downloaded doll. Physics, ragdolls, IK, skinning, clips, root motion, and FBX skeletons are consumers of this pose. They are not part of this decision.

ADR-0049 keeps component membership on the entity and the payload in the subsystem that owns it. ADR-0048 refused to bump `jarvig.level` until a milestone wrote the new fields. This milestone writes joint fields, and it writes the primitive mannequin as a prefab document rather than as hard-coded demo geometry.

## Decision

The canonical character pose is the entity's local spatial frame: translation, rotation, and the mesh scale already stored for that entity. A Joint does not own a second transform.

Joint is a component. Type id 13. Multiplicity One. It requires Transform. The payload is one `JointRecord` in the world, addressed by the entity. `JointId` is a runtime id and is not saved. The record stores:

- Kind: Fixed, Hinge, Ball (swing-twist), Universal, or Prismatic.
- Rest pose: parent-relative translation and quaternion.
- Limits, in radians for angles and meters for the prismatic slide. The inspector edits angles as degrees.
- Stiffness and damping. They are stored. No solver reads them in this milestone.

Limit axes are unit vectors in the joint's rest frame. The live rotation is `rest_rotation * relative`.

- Fixed writes the rest pose.
- Hinge keeps a single rotation about `axis`. Swing is removed. The angle is clamped to the hinge interval. Zero is the rest pose.
- Ball decomposes `relative` into swing then twist about `axis` (`relative = swing * twist`, twist applied first). Swing is clamped to a cone. Twist is clamped to an interval.
- Universal is `rotation(primary) * rotation(secondary)` in the orthonormal basis `(primary, secondary, primary × secondary)`. Each angle is clamped. Twist about the bone is removed.
- Prismatic keeps the rest rotation and clamps translation along `axis`.

Children follow because their frames are parented to the parent entity's frame. That parenting is applied for joints. Ordinary meshes stay parented to the scene frame, which is the behavior they already have. Removing a joint parents its frame back under the scene frame and keeps the world pose.

`jarvig.level` version 3 is written only when a joint is present. Version 1 and version 2 files still load. A joint in an older file is corrupt. Version 2 is still the camera version when the level has a camera and no joint.

`jarvig.prefab` version 1 is a list of the same entity records, without world settings and without GPU state. The file keeps its authored uuids. Placing it into a world assigns new uuids so a second placement does not collide. The primitive mannequin is that prefab: boxes, capsules, and spheres the engine builds. Pelvis, spine, chest, neck, and head; shoulders and hips are ball joints; elbows and knees are hinges; wrists and ankles are universal joints. Fingers are not in this milestone. Fixed and prismatic exist as kinds. The mannequin uses Fixed for the root and the limb segments that should not move on their own. Prismatic is covered by tests.

Editor rotation of a joint, including the existing rotate gizmo and the transform euler fields, goes through `SetProperty` on the local frame. The world clamps that write. Joint debug (pivot, axis, swing cone, twist range, hinge arc) is an editor overlay. It is not a scene entity.

## Alternatives Considered

- A separate skeleton pose buffer. Rejected. Clips, IK, and ragdolls would each grow their own transform.
- Letting the ChamberSu ball-joint FBX define the hierarchy. Rejected. That model is a later import test, and JARVIG has no FBX importer.
- Hard-coding the mannequin in the renderer. Rejected. The character has to survive the same save, load, and placement path as other authored actors.
- Bumping every level file to version 3. Rejected. Lighting Lab has no joints. Version 1 remains version 1.

## Consequences

Animation clips, IK, skinning, ragdolls, and imported skeletons are required to write this local frame and to honor this clamp. They do not get a private pose. Stiffness and damping stay unused until a solver exists. A kinematic pawn is the next milestone after this one, and it is not authorized by this decision. The renderer still does not walk the component list.

## Supersedes

Nothing. ADR-0049's rule that membership is not a second registry still holds. This decision is the milestone that writes the joint fields ADR-0048 deferred.

## Superseded By

Nothing.
