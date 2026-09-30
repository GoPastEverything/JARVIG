export interface StreamingPriorityInput {
  /** Meters, or another consistent distance unit. Must be >= 0. */
  readonly distance: number;
  /** Dot product of view direction and direction to the cell, in [-1, 1]. */
  readonly viewAlignment: number;
  /** Gameplay importance in [0, 1]. */
  readonly gameplayImportance: number;
  /** Memory pressure in [0, 1]. Higher pressure lowers priority. */
  readonly memoryPressure: number;
}

/**
 * Initial residency heuristic. Weights are tuning, not a content-balance decision.
 * Higher means "load sooner".
 */
export function streamingPriority(input: StreamingPriorityInput): number {
  const distance = finiteUnit('distance', input.distance, 0, Number.POSITIVE_INFINITY);
  const alignment = finiteUnit('viewAlignment', input.viewAlignment, -1, 1);
  const importance = finiteUnit('gameplayImportance', input.gameplayImportance, 0, 1);
  const pressure = finiteUnit('memoryPressure', input.memoryPressure, 0, 1);
  const distanceTerm = 1 / (1 + distance);
  const directionTerm = (alignment + 1) * 0.5;
  return (distanceTerm * 0.5 + directionTerm * 0.2 + importance * 0.3) * (1 - pressure * 0.5);
}

function finiteUnit(name: string, value: number, min: number, max: number): number {
  if (!Number.isFinite(value) || value < min || value > max) {
    throw new Error(`${name} must be a finite number in [${min}, ${max}]`);
  }
  return value;
}
