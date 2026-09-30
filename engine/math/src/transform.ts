import { quantizeQuat, quatIdentity, rotateVec, type Quat } from './quat.js';
import { addVec, quantizeVec3, subVec, type Vec3 } from './vec.js';

/** Authoritative pose of a reference frame relative to its parent. IEEE-754 binary64. */
export interface FrameTransform64 {
  readonly frameId: string;
  readonly parentFrameId: string | null;
  readonly position: Vec3;
  readonly rotation: Quat;
}

/** Local simulation/render pose. Quantized into the float32 domain on purpose. */
export interface LocalTransform32 {
  readonly position: Vec3;
  readonly rotation: Quat;
  readonly scale: Vec3;
}

export function localTransform(
  position: Vec3,
  rotation: Quat = quatIdentity(),
  scale: Vec3 = { x: 1, y: 1, z: 1 },
): LocalTransform32 {
  return { position, rotation, scale };
}

export function quantizeLocalTransform(transform: LocalTransform32): LocalTransform32 {
  return {
    position: quantizeVec3(transform.position),
    rotation: quantizeQuat(transform.rotation),
    scale: quantizeVec3(transform.scale),
  };
}

/**
 * Subtract a nearby camera origin in float64, then quantize to float32.
 * This is the render-space offset contract. It is not a GPU upload yet.
 */
export function toCameraRelativeF32(worldPosition: Vec3, cameraWorldPosition: Vec3): Vec3 {
  return quantizeVec3(subVec(worldPosition, cameraWorldPosition));
}

export function composeParentChild(parentPosition: Vec3, parentRotation: Quat, childPosition: Vec3): Vec3 {
  return addVec(parentPosition, rotateVec(parentRotation, childPosition));
}
