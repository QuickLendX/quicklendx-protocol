/**
 * Deterministic failure boundary for async operations.
 *
 * Two composable safety mechanisms:
 *
 * 1. `withRetry` — bounds transient failures. Retries only when `isRetryable`
 *    returns true, using exponential backoff driven by the injected `Clock`.
 *    After `maxAttempts` the terminal error propagates unchanged.
 *
 * 2. `dedupe` — prevents concurrent execution of the same logical operation
 *    (keyed by `key`). All in-flight callers share a single promise, so two
 *    simultaneous `revokeApiKey` calls for the same key never race the store.
 *    The entry is cleared in `finally`, so a rejected result doesn't permanently
 *    poison the key (a later caller re-runs).
 *
 * Both are generic (no knowledge of `ApiKeyError`); callers pass the retry
 * predicate, keeping the boundary reusable.
 */
import type { Clock } from "../services/clock";

export interface FailureBoundaryOptions {
  /** Total attempts including the first (default 3 => up to 2 retries). */
  maxAttempts?: number;
  /** Base backoff in ms; each retry doubles the previous delay. */
  baseDelayMs?: number;
  clock: Clock;
}

const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_BASE_DELAY_MS = 100;

export type RetryPredicate = (err: unknown) => boolean;

export class FailureBoundary {
  readonly maxAttempts: number;
  readonly baseDelayMs: number;
  private readonly clock: Clock;
  private readonly inFlight = new Map<string, Promise<unknown>>();

  constructor(opts: FailureBoundaryOptions) {
    this.maxAttempts = opts.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
    this.baseDelayMs = opts.baseDelayMs ?? DEFAULT_BASE_DELAY_MS;
    this.clock = opts.clock;
  }

  /**
   * Run `op`, retrying transient failures with exponential backoff.
   * Terminal errors (per `isRetryable`) propagate immediately.
   */
  async withRetry<T>(
    op: () => Promise<T>,
    isRetryable: RetryPredicate
  ): Promise<T> {
    if (this.maxAttempts < 1) {
      throw new RangeError("maxAttempts must be >= 1");
    }
    let attempt = 0;
    for (;;) {
      try {
        return await op();
      } catch (err) {
        attempt++;
        if (attempt >= this.maxAttempts || !isRetryable(err)) {
          throw err;
        }
        const backoff = this.baseDelayMs * 2 ** (attempt - 1);
        await this.clock.delay(backoff);
      }
    }
  }

  /**
   * Execute `op` at most once per `key` while an execution is in flight.
   * Subsequent callers for the same key await the shared promise. The shared
   * entry is removed once settled (success or failure).
   */
  async dedupe<T>(key: string, op: () => Promise<T>): Promise<T> {
    const existing = this.inFlight.get(key);
    if (existing !== undefined) {
      return existing as Promise<T>;
    }
    const promise = op().finally(() => {
      this.inFlight.delete(key);
    });
    this.inFlight.set(key, promise);
    return promise as Promise<T>;
  }

  /** True when there is a currently in-flight operation for `key`. */
  isPending(key: string): boolean {
    return this.inFlight.has(key);
  }

  /** Drop any tracked in-flight entry for `key` (e.g. on cancellation). */
  cancel(key: string): void {
    this.inFlight.delete(key);
  }
}
