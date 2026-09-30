import {
  composeParentChild,
  type FrameTransform64,
  type Vec3,
} from '@jarvig/math';

export type FrameId = string;

/**
 * Tree of reference frames. Renderer and physics are expected to consume a
 * local precision domain, not one enormous float32 world origin.
 */
export class FrameGraph {
  private readonly frames = new Map<FrameId, FrameTransform64>();

  add(frame: FrameTransform64): void {
    if (this.frames.has(frame.frameId)) {
      throw new Error(`frame '${frame.frameId}' already exists`);
    }
    if (frame.parentFrameId !== null && !this.frames.has(frame.parentFrameId)) {
      throw new Error(`parent frame '${frame.parentFrameId}' is not registered`);
    }
    this.frames.set(frame.frameId, frame);
  }

  get(frameId: FrameId): FrameTransform64 {
    const frame = this.frames.get(frameId);
    if (!frame) throw new Error(`frame '${frameId}' does not exist`);
    return frame;
  }

  has(frameId: FrameId): boolean {
    return this.frames.has(frameId);
  }

  size(): number {
    return this.frames.size;
  }

  /** Compose a point expressed in `frameId` into root-frame coordinates. */
  rootPosition(frameId: FrameId, localPoint: Vec3): Vec3 {
    let point = localPoint;
    let current: FrameId | null = frameId;
    const seen = new Set<FrameId>();
    while (current !== null) {
      if (seen.has(current)) throw new Error(`frame cycle at '${current}'`);
      seen.add(current);
      const frame = this.get(current);
      point = composeParentChild(frame.position, frame.rotation, point);
      current = frame.parentFrameId;
    }
    return point;
  }
}
