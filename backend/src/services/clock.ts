/**
 * Time abstraction used by the controller and failure boundary.
 *
 * The combined `Clock` interface lets tests drive both "now" and "delay"
 * deterministically: `FrozenClock` never sleeps (delays resolve on the
 * microtask queue) and records every scheduled backoff so tests can assert
 * the exact retry schedule without wall-clock flakiness.
 */
export interface Clock {
  /** Monotonic milliseconds used for timestamps and backoff math. */
  now(): number;
  /** Resolve after `ms`; implementation may be virtual (immediate) in tests. */
  delay(ms: number): Promise<void>;
}

/** Production clock: real wall clock + real timer. */
export class SystemClock implements Clock {
  now(): number {
    return Date.now();
  }
  delay(ms: number): Promise<void> {
    return new Promise<void>((resolve) => setTimeout(resolve, ms));
  }
}

/** Deterministic, controllable clock for tests. */
export class FrozenClock implements Clock {
  /** Every backoff scheduled via `delay`, in the order it was requested. */
  readonly scheduledDelays: number[] = [];
  private timeMs: number;

  constructor(startMs: number = 1_000_000) {
    this.timeMs = startMs;
  }

  now(): number {
    return this.timeMs;
  }

  /** Advance the virtual clock. */
  advance(ms: number): void {
    if (ms < 0) {
      throw new RangeError("Clock can only advance forward");
    }
    this.timeMs += ms;
  }

  /**
   * Virtual delay: records the requested backoff and resolves immediately
   * (no real sleep), then advances `now()` by the delay so any timestamp
   * captured after the retry reflects the backoff.
   */
  delay(ms: number): Promise<void> {
    if (!Number.isFinite(ms) || ms < 0) {
      throw new RangeError("delay() requires a non-negative finite ms");
    }
    this.scheduledDelays.push(ms);
    this.timeMs += ms;
    return Promise.resolve();
  }

  reset(startMs: number = 1_000_000): void {
    this.timeMs = startMs;
    this.scheduledDelays.length = 0;
  }
}
