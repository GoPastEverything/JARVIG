import type { Engine } from './engine.js';
import type { TickResult } from './tick.js';

export interface EngineContext {
  readonly engine: Engine;
  readonly hooks: EngineHooks;
}

export interface EngineHooks {
  onFixedStep(fn: (stepIndex: number) => void): void;
  onVariableUpdate(fn: (info: { readonly alpha: number; readonly frameDelta: number }) => void): void;
  onStreaming(fn: () => void): void;
  onRenderPrepare(fn: () => void): void;
  onRenderExecute(fn: () => void): void;
  onTelemetry(fn: (tick: TickResult) => void): void;
}

export interface EngineModule {
  readonly id: string;
  readonly requires?: readonly string[];
  initialize(ctx: EngineContext): void;
  shutdown(ctx: EngineContext): void;
}
