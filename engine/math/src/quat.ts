import { addVec, crossVec, scaleVec, type Vec3 } from './vec.js';

export interface Quat {
  readonly x: number;
  readonly y: number;
  readonly z: number;
  readonly w: number;
}

export function quatIdentity(): Quat {
  return { x: 0, y: 0, z: 0, w: 1 };
}

export function quatFromAxisAngle(axis: Vec3, radians: number): Quat {
  const length = Math.hypot(axis.x, axis.y, axis.z);
  if (length === 0) {
    throw new Error('axis must be non-zero');
  }
  const half = radians * 0.5;
  const scale = Math.sin(half) / length;
  return {
    x: axis.x * scale,
    y: axis.y * scale,
    z: axis.z * scale,
    w: Math.cos(half),
  };
}

/** Rotate `vector` by a unit quaternion. Right-handed. */
export function rotateVec(rotation: Quat, vector: Vec3): Vec3 {
  const axis = { x: rotation.x, y: rotation.y, z: rotation.z };
  const t = scaleVec(crossVec(axis, vector), 2);
  return addVec(addVec(vector, scaleVec(t, rotation.w)), crossVec(axis, t));
}

export function quantizeQuat(rotation: Quat): Quat {
  const x = Math.fround(rotation.x);
  const y = Math.fround(rotation.y);
  const z = Math.fround(rotation.z);
  const w = Math.fround(rotation.w);
  const length = Math.hypot(x, y, z, w);
  if (length === 0) {
    return quatIdentity();
  }
  return { x: x / length, y: y / length, z: z / length, w: w / length };
}
