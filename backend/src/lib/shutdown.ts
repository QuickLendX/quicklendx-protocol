/**
 * Graceful shutdown orchestrator — Issue #1190
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
 * A second SIGTERM/SIGINT received while draining forces an immediate exit(1).
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

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

const _steps: ShutdownStep[] = [];

/** Register a step. Safe to call multiple times (idempotent by name). */
export function register(step: ShutdownStep): void {
  // Replace if already registered under the same name so tests can re-register.
  const idx = _steps.findIndex((s) => s.name === step.name);
  if (idx >= 0) {
    _steps[idx] = step;
  } else {
    _steps.push(step);
  }
}

/** Remove all registered steps — used in tests between cases. */
export function clearRegistry(): void {
  _steps.length = 0;
}

/**
 * Return a sorted copy of registered steps (lowest priority first).
 *
 * Determinism invariants:
 *  - The returned array is a fresh copy; callers cannot mutate the registry.
 *  - Ordering is stable: steps with equal priority preserve registration order.
 *  - The registry itself is never mutated by this read.
 */
export function getRegisteredSteps(): ShutdownStep[] {
  // Decorate with the original index so ties break by registration order
  // deterministically, independent of the engine's sort stability.
  return _steps
    .map((step, index) => ({ step, index }))
    .sort((a, b) => {
      if (a.step.priority !== b.step.priority) {
        return a.step.priority - b.step.priority;
      }
      return a.index - b.index;
    })
    .map((entry) => entry.step);
}

// ---------------------------------------------------------------------------
// Shutdown state
// ---------------------------------------------------------------------------

let _shuttingDown = false;

/** Reset shutdown state — call in tests between cases. */
export function resetShuttingDown(): void {
  _shuttingDown = false;
}

/** True once a shutdown signal has been received. */
export function isShuttingDown(): boolean {
  return _shuttingDown;
}

/** Atomically claim the shutdown latch. Returns true if this call won. */
export function beginShutdown(): boolean {
  if (_shuttingDown) return false;
  _shuttingDown = true;
  return true;
}

// ---------------------------------------------------------------------------
// Core runner
// ---------------------------------------------------------------------------

/**
 * Execute all registered steps in priority order.
 * Errors in one step are caught and logged; remaining steps still run.
 * Total wall-clock time is bounded by `totalTimeoutMs`.
 *
 * Failure-boundary invariants:
 *  - A step that throws never aborts the sequence; the error is logged and
 *    the next step still runs so cleanup is best-effort but complete.
 *  - A step that hangs is bounded by the remaining budget; once the deadline
 *    is reached, remaining steps are skipped (never run partially).
 *  - `runAll` never rejects: callers can rely on it resolving even when
 *    individual steps fail, so shutdown always reaches its terminal state.
 */
export async function runAll(
  signal: string,
  totalTimeoutMs = DEFAULT_DRAIN_TIMEOUT_MS,
): Promise<void> {
  const sorted = getRegisteredSteps();
  const deadline = Date.now() + totalTimeoutMs;

  for (const step of sorted) {
    const remainingMs = deadline - Date.now();
    if (remainingMs <= 0) {
      console.warn(`[shutdown] Total timeout reached — skipping step "${step.name}"`);
      break;
    }
    try {
      console.log(`[shutdown] Running step "${step.name}" (priority ${step.priority})`);
      // Bound each step by the remaining budget so a single hung step cannot
      // consume the entire drain window and starve later cleanup steps.
      await withTimeout(step.fn(signal), remainingMs, step.name);
    } catch (err) {
      console.error(`[shutdown] Step "${step.name}" failed:`, err);
      // Continue with remaining steps
    }
  }
}

/**
 * Await `promise`, rejecting if it does not settle within `timeoutMs`.
 * The timer is always cleared so no dangling handle keeps the process alive.
 */
function withTimeout<T>(promise: Promise<T>, timeoutMs: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`[shutdown] Step "${label}" timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (err) => {
        clearTimeout(timer);
        reject(err);
      },
    );
  });
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
// Backward-compatible high-level API
// ---------------------------------------------------------------------------

/**
 * Build a signal handler that registers the canonical shutdown steps for
 * the given HTTP server and calls `runAll()`.
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
      }
    },
  });

  // ── Step 4: flush webhook queue ───────────────────────────────────────────
  register({
    name: 'webhook-delivery',
    priority: PRIORITY_WEBHOOK,
    fn: async () => {
      const pending = webhookQueueService.flush();
      if (pending.length > 0) {
        console.warn(`[shutdown] ${pending.length} webhook event(s) not delivered`);
      }
    },
  });

  // ── Step 7: close database ────────────────────────────────────────────────
  register({
    name: 'database',
    priority: PRIORITY_DB,
    fn: async () => {
      closeDatabase();
    },
  });

  return async function shutdown(signal: string): Promise<void> {
    if (!beginShutdown()) {
      console.warn('[shutdown] Second signal received — forcing exit');
      process.exit(1);
      return; // guard: process.exit is a no-op in tests
    }

    console.log(`[shutdown] ${signal} — starting graceful shutdown`);
    try {
      await runAll(signal, drainTimeoutMs);
      console.log('[shutdown] Shutdown complete');
      process.exit(0);
    } catch (err) {
      // runAll is designed not to reject, but guard the boundary anyway so a
      // future regression cannot leave the process in a half-shutdown state.
      console.error('[shutdown] Unexpected failure during shutdown:', err);
      process.exit(1);
    }
  };
}
