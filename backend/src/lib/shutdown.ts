/**
 * Graceful shutdown orchestrator.
 *
 * Sequence (bounded by drainTimeoutMs):
 *   1. Mark the service not-ready so the load balancer stops routing.
 *   2. Stop the HTTP listener (no new connections accepted).
 *   3. Wait for in-flight requests to finish (or drain timeout to elapse).
 *   4. Flush pending webhook events from the in-memory queue and log any
 *      that could not be delivered.
 *   5. Close the SQLite handle cleanly (WAL checkpoint completes).
 *   6. Exit.
 *
 * A second SIGTERM/SIGINT received while draining forces an immediate exit(1).
 *
 * Security: the drain loop polls the in-process counter from load-shedding
 * middleware — no network I/O occurs during shutdown, so no half-written
 * transactions can be introduced here.
 */

import http from 'http';
import { getActiveRequests } from '../middleware/load-shedding';
import { webhookQueueService } from '../services/webhookQueueService';
import { closeDatabase } from './database';
import { statusService } from '../services/statusService';

/** How long (ms) to wait for in-flight requests before forcing exit. */
export const DEFAULT_DRAIN_TIMEOUT_MS =
  parseInt(process.env.SHUTDOWN_DRAIN_TIMEOUT_MS ?? '', 10) || 30_000;

/** Poll interval for the drain loop. */
export const DRAIN_POLL_MS = 50;

let _shuttingDown = false;

/** Reset shutdown state — call in tests between cases. */
export function resetShuttingDown(): void {
  _shuttingDown = false;
}

/** True once a shutdown signal has been received. */
export function isShuttingDown(): boolean {
  return _shuttingDown;
}

/**
 * Build a signal handler that performs the full shutdown sequence.
 *
 * @param server        - The http.Server instance to close.
 * @param drainTimeoutMs - Max milliseconds to wait for requests to drain.
 *                        Defaults to DEFAULT_DRAIN_TIMEOUT_MS.
 */
export function createShutdownHandler(
  server: http.Server,
  drainTimeoutMs: number = DEFAULT_DRAIN_TIMEOUT_MS,
): (signal: string) => Promise<void> {
  return async function shutdown(signal: string): Promise<void> {
    if (_shuttingDown) {
      console.warn('[shutdown] Second signal received — forcing exit');
      process.exit(1);
      return; // guard: process.exit is a no-op in tests
    }
    _shuttingDown = true;

    console.log(`[shutdown] ${signal} — starting graceful shutdown`);

    // 1. Tell the load balancer / readiness probe this instance is going away.
    statusService.setMaintenanceMode(true);

    // 2. Stop accepting new connections.
    server.close();

    // 3. Drain in-flight requests.
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

    // 4. Flush the webhook queue and log any undelivered events.
    try {
      const pending = webhookQueueService.flush();
      if (pending.length > 0) {
        console.warn(`[shutdown] ${pending.length} webhook event(s) not delivered`);
      }
    } catch (err) {
      console.error('[shutdown] Webhook queue flush failed:', err);
    }

    // 5. Close the SQLite handle (flushes WAL, prevents corruption).
    try {
      closeDatabase();
    } catch (err) {
      console.error('[shutdown] Database close failed:', err);
    }

    console.log('[shutdown] Shutdown complete');
    process.exit(0);
  };
}
