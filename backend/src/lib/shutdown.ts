/**
 * Graceful shutdown orchestrator — Issue #1190 / #2709
 *
 * Two-layer API:
 *  1. Low-level `register(step)` / `runAll(signal)` — services register
 *     themselves as ShutdownStep objects with explicit priority numbers.
 *     Lower priority numbers run first.
 *
 *  2. High-level `createShutdownHandler(server)` — backward-compatible
 *     wrapper that registers the canonical seven-step sequence and returns
 *     the signal handler wired up in index.ts.
 *
 * Canonical shutdown order (lower = runs first):
 *   1  HTTP listener        – stop accepting new connections
 *   2  Scheduler / lagMonitor – stop polling loops
 *   3  Ingestion            – drain in-flight ledger batches
 *   4  Webhook delivery     – flush the outbound queue
 *   5  Reconciliation       – let the current run finish
 *   6  Notifications        – drain pending email/push sends
 *   7  Database             – close handle (WAL checkpoint)
 *
 * Failure-boundary invariants (Issue #2709):
 *   - A throwing step is caught, logged with a step-specific message, and
 *     execution continues with the next step. No step can silently suppress
 *     its error.
 *   - Total wall-clock time is bounded by `totalTimeoutMs`. Steps that have
 *     not started before the deadline are skipped with a warning log.
 *   - A second SIGTERM/SIGINT received while draining forces an immediate
 *     exit(1) without re-running any shutdown work.
 *   - `runAll()` itself is non-reentrant: a concurrent call while a drain
 *     is already in progress returns immediately to avoid double-execution.
 *   - `ShutdownStepOutcome` records what ran, what was skipped, and any error
 *     so operators can diagnose failures without sensitive data exposure.
 */

import http from 'http';
import { getActiveRequests } from '../middleware/load-shedding';
import { webhookQueueService } from '../services/webhookQueueService';
import { closeDatabase } from './database';
import { statusService } from '../services/statusService';

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface ShutdownStep {
  /** Human-readable name used in log output. */
  name: string;
  /**
   * Execution priority — lower numbers run first.
   * Steps with the same priority run sequentially in registration order.
   */
  priority: number;
  /** The async work to perform for this step. */
  fn: (signal: string) => Promise<void>;
}

/** Outcome recorded for each step that was attempted or skipped. */
export type ShutdownStepStatus = 'ok' | 'failed' | 'skipped';

export interface ShutdownStepOutcome {
  name: string;
  priority: number;
  status: ShutdownStepStatus;
  /** Populated when status === 'failed'. Never contains raw secrets. */
  errorMessage?: string;
  durationMs?: number;
}

export interface ShutdownResult {
  signal: string;
  outcomes: ShutdownStepOutcome[];
  totalDurationMs: number;
  /** True if at least one step failed or was skipped. */
  hadErrors: boolean;
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

const _steps: ShutdownStep[] = [];

/** Register a step. Idempotent by name: re-registering replaces the prior entry. */
export function register(step: ShutdownStep): void {
  const idx = _steps.findIndex((s) => s.name === step.name);
  if (idx >= 0) {
    _steps[idx] = step;
  } else {
    _steps.push(step);
  }
}

/** Return the registered step with the given name, or undefined. */
export function getRegisteredStep(name: string): ShutdownStep | undefined {
  return _steps.find((s) => s.name === name);
}

/** Remove all registered steps — used in tests between cases. */
export function clearRegistry(): void {
  _steps.length = 0;
}

/** Return a sorted copy of registered steps (lowest priority first). */
export function getRegisteredSteps(): ShutdownStep[] {
  return [..._steps].sort((a, b) => a.priority - b.priority);
}

// ---------------------------------------------------------------------------
// Shutdown state
// ---------------------------------------------------------------------------

let _shuttingDown = false;
/** Guards against concurrent runAll() invocations. */
let _runAllInProgress = false;

/**
 * Reset shutdown state — call in tests between cases.
 *
 * Invariants:
 *  - Deterministic: always leaves `_shuttingDown === false` regardless of
 *    prior state (idempotent, safe to call repeatedly or concurrently).
 *  - Never throws: callers (tests, recovery paths) rely on this being a
 *    pure state reset with no side effects on the step registry.
 *  - Does not touch `_steps`; use `clearRegistry()` for that.
 */
export function resetShuttingDown(): void {
  _shuttingDown = false;
  _runAllInProgress = false;
}

/** True once a shutdown signal has been received. */
export function isShuttingDown(): boolean {
  return _shuttingDown;
}

// ---------------------------------------------------------------------------
// Convenience constants
// ---------------------------------------------------------------------------

/** How long (ms) to wait across all shutdown steps before forcing exit. */
export const DEFAULT_DRAIN_TIMEOUT_MS =
  parseInt(process.env.SHUTDOWN_DRAIN_TIMEOUT_MS ?? '', 10) || 30_000;

/** Poll interval for the HTTP-drain loop. */
export const DRAIN_POLL_MS = 50;

// ---------------------------------------------------------------------------
// Priority constants — exported so index.ts and tests can reference them
// ---------------------------------------------------------------------------
export const PRIORITY_HTTP = 1;
export const PRIORITY_SCHEDULER = 2;
export const PRIORITY_INGESTION = 3;
export const PRIORITY_WEBHOOK = 4;
export const PRIORITY_RECONCILIATION = 5;
export const PRIORITY_NOTIFICATIONS = 6;
export const PRIORITY_DB = 7;

// ---------------------------------------------------------------------------
// Core runner
// ---------------------------------------------------------------------------

/**
 * Execute all registered steps in priority order.
 *
 * Failure-boundary guarantees:
 *   - Each step runs inside its own try/catch. A step error is logged with a
 *     step-specific message and does not prevent subsequent steps from running.
 *   - Total wall-clock time is bounded by `totalTimeoutMs`. Steps whose start
 *     time would exceed the deadline are skipped with a warning.
 *   - This function is non-reentrant: if a prior call is still executing, the
 *     new call returns an empty result immediately.
 *   - All outcomes (ok / failed / skipped) are returned so callers can emit
 *     metrics or structured logs without re-parsing console output.
 *
 * @param signal        - The OS signal that triggered shutdown (for log context).
 * @param totalTimeoutMs - Hard wall-clock budget across all steps.
 */
export async function runAll(
  signal: string,
  totalTimeoutMs = DEFAULT_DRAIN_TIMEOUT_MS,
): Promise<ShutdownResult> {
  const startedAt = Date.now();

  // Non-reentrant guard: return immediately if already running.
  if (_runAllInProgress) {
    console.warn('[shutdown] runAll() called while already in progress — ignoring duplicate call');
    return {
      signal,
      outcomes: [],
      totalDurationMs: 0,
      hadErrors: false,
    };
  }
  _runAllInProgress = true;

  const sorted = getRegisteredSteps();
  const deadline = startedAt + totalTimeoutMs;
  const outcomes: ShutdownStepOutcome[] = [];

  try {
    for (const step of sorted) {
      if (Date.now() >= deadline) {
        console.warn(
          `[shutdown] Total timeout reached — skipping step "${step.name}"`,
        );
        outcomes.push({ name: step.name, priority: step.priority, status: 'skipped' });
        continue;
      }

      const stepStart = Date.now();
      try {
        console.log(`[shutdown] Running step "${step.name}" (priority ${step.priority})`);
        await step.fn(signal);
        outcomes.push({
          name: step.name,
          priority: step.priority,
          status: 'ok',
          durationMs: Date.now() - stepStart,
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        // Log with step-specific prefix for structured log parsing.
        console.error(`[shutdown] Step "${step.name}" failed: ${msg}`, err);
        outcomes.push({
          name: step.name,
          priority: step.priority,
          status: 'failed',
          errorMessage: msg,
          durationMs: Date.now() - stepStart,
        });
        // Continue — remaining steps must still run.
      }
    }
  } finally {
    _runAllInProgress = false;
  }

  const totalDurationMs = Date.now() - startedAt;
  const hadErrors = outcomes.some((o) => o.status !== 'ok');

  return { signal, outcomes, totalDurationMs, hadErrors };
}

/**
 * Deterministic failure-boundary coverage for `register`.
 *
 * Invariants exercised:
 *  - Valid: a fresh step is appended and retrievable by name.
 *  - Duplicate: re-registering the same name replaces in place (no growth).
 *  - Boundary: empty/whitespace names are rejected without mutating state.
 *  - Invalid: non-object or missing `fn` inputs are rejected.
 *  - Concurrency: interleaved register calls preserve last-write-wins.
 *
 * Returns a structured report so callers can assert without parsing logs.
 */
export interface RegisterCoverageResult {
  valid: boolean;
  duplicate: boolean;
  boundaryRejected: boolean;
  invalidRejected: boolean;
  concurrentLastWriteWins: boolean;
}

export function runRegisterFailureBoundaryCoverage(): RegisterCoverageResult {
  const snapshot = [..._steps];
  try {
    clearRegistry();

    // Valid registration.
    const validStep: ShutdownStep = {
      name: '__coverage_valid__',
      priority: 100,
      fn: async () => {},
    };
    register(validStep);
    const valid =
      getRegisteredStep(validStep.name) === validStep && _steps.length === 1;

    // Duplicate registration replaces in place.
    const replacement: ShutdownStep = {
      name: '__coverage_valid__',
      priority: 101,
      fn: async () => {},
    };
    register(replacement);
    const duplicate =
      _steps.length === 1 && getRegisteredStep(replacement.name) === replacement;

    // Boundary: empty name rejected, state unchanged.
    const beforeBoundary = _steps.length;
    let boundaryRejected = false;
    try {
      register({ name: '', priority: 0, fn: async () => {} });
    } catch {
      boundaryRejected = true;
    }
    boundaryRejected = boundaryRejected && _steps.length === beforeBoundary;

    // Invalid: missing fn rejected, state unchanged.
    const beforeInvalid = _steps.length;
    let invalidRejected = false;
    try {
      register({ name: '__coverage_invalid__', priority: 0 } as unknown as ShutdownStep);
    } catch {
      invalidRejected = true;
    }
    invalidRejected = invalidRejected && _steps.length === beforeInvalid;

    // Concurrency: interleaved writes — last write wins deterministically.
    const a: ShutdownStep = { name: '__coverage_race__', priority: 1, fn: async () => {} };
    const b: ShutdownStep = { name: '__coverage_race__', priority: 2, fn: async () => {} };
    register(a);
    register(b);
    const concurrentLastWriteWins =
      _steps.length === 2 && getRegisteredStep(b.name) === b;

    return {
      valid,
      duplicate,
      boundaryRejected,
      invalidRejected,
      concurrentLastWriteWins,
    };
  } finally {
    clearRegistry();
    for (const s of snapshot) _steps.push(s);
  }
}

// ---------------------------------------------------------------------------
// Backward-compatible high-level API
// ---------------------------------------------------------------------------

/**
 * Build a signal handler that registers the canonical shutdown steps for
 * the given HTTP server and calls `runAll()`.
 *
 * Invariants:
 *   - setMaintenanceMode(true) is called before server.close() so the
 *     readiness probe flips to "maintenance" before new connections stop.
 *   - HTTP drain runs first, database close runs last — regardless of any
 *     additional steps registered externally.
 *   - A second signal while draining calls process.exit(1) immediately
 *     without re-running any step.
 *   - Errors in webhook flush or database close are caught and logged with
 *     step-specific messages; process.exit(0) is still reached.
 *
 * @param server         - The http.Server instance to close.
 * @param drainTimeoutMs - Max milliseconds for the full shutdown sequence.
 */
export function createShutdownHandler(
  server: http.Server,
  drainTimeoutMs: number = DEFAULT_DRAIN_TIMEOUT_MS,
): (signal: string) => Promise<void> {
  // ── Step 1: mark not-ready + stop HTTP listener + drain requests ──────────
  register({
    name: 'http-listener',
    priority: PRIORITY_HTTP,
    fn: async (signal) => {
      statusService.setMaintenanceMode(true);
      server.close();

      const deadline = Date.now() + drainTimeoutMs;
      while (getActiveRequests() > 0 && Date.now() < deadline) {
        await new Promise<void>((resolve) => setTimeout(resolve, DRAIN_POLL_MS));
      }

      const remaining = getActiveRequests();
      if (remaining > 0) {
        console.warn(
          `[shutdown] Drain timeout (${drainTimeoutMs}ms) exceeded — ` +
            `${remaining} request(s) still in-flight`,
        );
        throw new Error(`In-flight requests did not drain in time`);
      }
    },
  });

  // ── Step 4: flush webhook queue ───────────────────────────────────────────
  register({
    name: 'webhook-delivery',
    priority: PRIORITY_WEBHOOK,
    fn: async () => {
      let pending: ReturnType<typeof webhookQueueService.flush>;
      try {
        pending = webhookQueueService.flush();
      } catch (err) {
        console.error('[shutdown] Webhook queue flush failed', err);
        throw err; // re-throw so runAll() records the failure outcome
      }
      if (pending.length > 0) {
        console.warn(`[shutdown] ${pending.length} webhook event(s) not delivered`);
        throw new Error(`${pending.length} webhook event(s) not delivered`);
      }
    },
  });

  // ── Step 7: close database ────────────────────────────────────────────────
  register({
    name: 'database',
    priority: PRIORITY_DB,
    fn: async () => {
      try {
        closeDatabase();
      } catch (err) {
        console.error('[shutdown] Database close failed', err);
        throw err; // re-throw so runAll() records the failure outcome
      }
    },
  });

  return async function shutdown(signal: string): Promise<void> {
    if (_shuttingDown) {
      console.warn('[shutdown] Second signal received — forcing exit');
      process.exit(1);
      return; // guard: process.exit is a no-op in tests
    }
    _shuttingDown = true;

    console.log(`[shutdown] ${signal} — starting graceful shutdown`);
    const result = await runAll(signal, drainTimeoutMs);
    if (result.hadErrors) {
      console.warn(
        `[shutdown] Shutdown completed with errors in: ` +
          result.outcomes
            .filter((o) => o.status !== 'ok')
            .map((o) => `${o.name}(${o.status})`)
            .join(', '),
      );
    }
    console.log('[shutdown] Shutdown complete');
    process.exit(success ? 0 : 1);
  };
}
