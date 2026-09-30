import { SimulationClock, type ClockConfig } from './clock.js';
import { resolveFeatureFlags, type FeatureFlags } from './flags.js';
import type { EngineContext, EngineHooks, EngineModule } from './module.js';
import { sortModules } from './sort-modules.js';
import { Telemetry } from './telemetry.js';
import type { TickResult } from './tick.js';

export type EngineProfile = 'editor' | 'client' | 'server' | 'test';

export interface EngineCreateOptions extends ClockConfig {
  readonly profile: EngineProfile;
  readonly modules?: readonly EngineModule[];
  readonly featureFlags?: Partial<FeatureFlags>;
}

type FixedHook = (stepIndex: number) => void;
type VariableHook = (info: { readonly alpha: number; readonly frameDelta: number }) => void;
type VoidHook = () => void;
type TelemetryHook = (tick: TickResult) => void;

/**
 * Host-agnostic engine lifecycle. The same class boots the editor, the client,
 * and the dedicated server. Server profiles skip render stages.
 */
export class Engine {
  readonly profile: EngineProfile;
  readonly clock: SimulationClock;
  readonly flags: FeatureFlags;
  readonly telemetry = new Telemetry();
  private readonly fixedHooks: FixedHook[] = [];
  private readonly variableHooks: VariableHook[] = [];
  private readonly streamingHooks: VoidHook[] = [];
  private readonly renderPrepareHooks: VoidHook[] = [];
  private readonly renderExecuteHooks: VoidHook[] = [];
  private readonly telemetryHooks: TelemetryHook[] = [];
  private modules: EngineModule[] = [];
  private readonly moduleIds = new Set<string>();
  private running = false;
  private readonly context: EngineContext;

  private constructor(options: EngineCreateOptions) {
    this.profile = options.profile;
    this.clock = new SimulationClock(options);
    this.flags = resolveFeatureFlags(options.featureFlags);
    const hooks: EngineHooks = {
      onFixedStep: (fn) => this.fixedHooks.push(fn),
      onVariableUpdate: (fn) => this.variableHooks.push(fn),
      onStreaming: (fn) => this.streamingHooks.push(fn),
      onRenderPrepare: (fn) => this.renderPrepareHooks.push(fn),
      onRenderExecute: (fn) => this.renderExecuteHooks.push(fn),
      onTelemetry: (fn) => this.telemetryHooks.push(fn),
    };
    this.context = { engine: this, hooks };
  }

  static create(options: EngineCreateOptions): Engine {
    const engine = new Engine(options);
    const sorted = sortModules(options.modules ?? []);
    const initialized: EngineModule[] = [];
    try {
      for (const mod of sorted) {
        mod.initialize(engine.context);
        initialized.push(mod);
        engine.moduleIds.add(mod.id);
      }
    } catch (error) {
      for (const mod of initialized.reverse()) {
        mod.shutdown(engine.context);
      }
      throw error;
    }
    engine.modules = initialized;
    engine.running = true;
    engine.telemetry.increment('engine.start');
    return engine;
  }

  hasModule(id: string): boolean {
    return this.moduleIds.has(id);
  }

  moduleList(): readonly string[] {
    return this.modules.map((mod) => mod.id);
  }

  tick(frameDeltaSeconds: number): TickResult {
    if (!this.running) {
      throw new Error('engine is shut down');
    }
    const advance = this.clock.advance(frameDeltaSeconds);
    for (let step = 0; step < advance.fixedSteps; step += 1) {
      for (const hook of this.fixedHooks) hook(step);
    }
    const variable = { alpha: advance.alpha, frameDelta: advance.frameDelta };
    for (const hook of this.variableHooks) hook(variable);
    for (const hook of this.streamingHooks) hook();
    const renderExecuted = this.profile !== 'server';
    if (renderExecuted) {
      for (const hook of this.renderPrepareHooks) hook();
      for (const hook of this.renderExecuteHooks) hook();
    }
    const tick: TickResult = {
      frame: this.clock.frame,
      fixedSteps: advance.fixedSteps,
      alpha: advance.alpha,
      frameDelta: advance.frameDelta,
      fixedDelta: advance.fixedDelta,
      renderExecuted,
      streamingRan: true,
      clamped: advance.clamped,
    };
    this.telemetry.increment('frame.tick');
    this.telemetry.increment('frame.fixedSteps', advance.fixedSteps);
    if (!renderExecuted) this.telemetry.increment('frame.renderSkipped');
    for (const hook of this.telemetryHooks) hook(tick);
    return tick;
  }

  shutdown(): void {
    if (!this.running) {
      throw new Error('engine is already shut down');
    }
    for (const mod of [...this.modules].reverse()) {
      mod.shutdown(this.context);
    }
    this.running = false;
    this.telemetry.increment('engine.shutdown');
  }
}
