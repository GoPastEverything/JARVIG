export interface TickResult {
  readonly frame: number;
  readonly fixedSteps: number;
  readonly alpha: number;
  readonly frameDelta: number;
  readonly fixedDelta: number;
  readonly renderExecuted: boolean;
  readonly streamingRan: boolean;
  readonly clamped: boolean;
}
