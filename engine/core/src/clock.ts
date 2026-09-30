export interface ClockConfig {
  /** Fixed simulation rate. Default 60. */
  readonly fixedHz?: number;
  /** Maximum fixed steps consumed from one frame. Excess time is discarded. */
  readonly maxStepsPerFrame?: number;
}

export interface ClockAdvance {
  readonly fixedSteps: number;
  readonly alpha: number;
  readonly fixedDelta: number;
  readonly frameDelta: number;
  /** True when this frame hit the step cap and leftover time was dropped. */
  readonly clamped: boolean;
}

/**
 * Fixed-step accumulator. Variable render deltas produce zero or more simulation
 * steps plus an interpolation alpha in [0, 1). A single huge frame cannot spiral.
 */
export class SimulationClock {
  readonly fixedDelta: number;
  readonly maxStepsPerFrame: number;
  private accumulator = 0;
  elapsed = 0;
  frame = 0;

  constructor(config: ClockConfig = {}) {
    const fixedHz = config.fixedHz ?? 60;
    const maxSteps = config.maxStepsPerFrame ?? 8;
    if (!(fixedHz > 0) || !Number.isFinite(fixedHz)) {
      throw new Error('fixedHz must be a positive finite number');
    }
    if (!Number.isInteger(maxSteps) || maxSteps < 1) {
      throw new Error('maxStepsPerFrame must be an integer >= 1');
    }
    this.fixedDelta = 1 / fixedHz;
    this.maxStepsPerFrame = maxSteps;
  }

  advance(frameDeltaSeconds: number): ClockAdvance {
    if (!Number.isFinite(frameDeltaSeconds)) {
      throw new Error('frame delta must be finite');
    }
    const frameDelta = frameDeltaSeconds > 0 ? frameDeltaSeconds : 0;
    this.accumulator += frameDelta;
    let fixedSteps = 0;
    while (this.accumulator + 1e-12 >= this.fixedDelta && fixedSteps < this.maxStepsPerFrame) {
      this.accumulator -= this.fixedDelta;
      fixedSteps += 1;
    }
    const clamped = this.accumulator + 1e-12 >= this.fixedDelta;
    if (clamped) {
      this.accumulator = 0;
    }
    if (this.accumulator < 0) {
      this.accumulator = 0;
    }
    this.elapsed += frameDelta;
    this.frame += 1;
    const alpha = this.fixedDelta === 0 ? 0 : this.accumulator / this.fixedDelta;
    return {
      fixedSteps,
      alpha,
      fixedDelta: this.fixedDelta,
      frameDelta,
      clamped,
    };
  }
}
