export interface CounterSample {
  readonly count: number;
  readonly milliseconds: number;
}

/** Named counters. Phase 0 records counts; millisecond samples are explicit, never guessed. */
export class Telemetry {
  private readonly samples = new Map<string, { count: number; milliseconds: number }>();

  increment(name: string, amount = 1): void {
    const current = this.samples.get(name) ?? { count: 0, milliseconds: 0 };
    current.count += amount;
    this.samples.set(name, current);
  }

  addTime(name: string, milliseconds: number): void {
    if (!Number.isFinite(milliseconds) || milliseconds < 0) {
      throw new Error(`telemetry time for ${name} must be a non-negative finite number`);
    }
    const current = this.samples.get(name) ?? { count: 0, milliseconds: 0 };
    current.milliseconds += milliseconds;
    current.count += 1;
    this.samples.set(name, current);
  }

  snapshot(): Readonly<Record<string, CounterSample>> {
    const out: Record<string, CounterSample> = {};
    for (const [name, sample] of this.samples) {
      out[name] = { count: sample.count, milliseconds: sample.milliseconds };
    }
    return out;
  }
}
