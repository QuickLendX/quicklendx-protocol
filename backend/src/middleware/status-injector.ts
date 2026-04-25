/**
 * statusInjector middleware
 *
 * Response interceptor that appends a `_system` metadata block to every
 * outgoing JSON response.  This gives clients a consistent, always-present
 * signal about the current system health without requiring a separate
 * /status poll.
 *
 * Injected shape
 * --------------
 * {
 *   ...originalBody,
 *   _system: {
 *     status: "operational" | "degraded" | "maintenance",
 *     degraded: boolean,
 *     lag: number,
 *     level: "none" | "warn" | "critical"
 *   }
 * }
 *
 * Schema stability guarantee
 * --------------------------
 * The `_system` key is ADDITIVE.  Existing fields in the response body are
 * never modified or removed.  Clients that do not read `_system` are
 * unaffected.
 *
 * Non-JSON responses (HTML, binary, streams) are passed through unchanged.
 *
 * Security
 * --------
 * The injector only reads system state — it never modifies auth headers,
 * status codes, or security-sensitive fields.
 */

import { Request, Response, NextFunction } from "express";
import { lagMonitor } from "../services/lagMonitor";
import { statusService } from "../services/statusService";

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

export function statusInjector(
  req: Request,
  res: Response,
  next: NextFunction
): void {
  // Capture the original res.json so we can wrap it.
  const originalJson = res.json.bind(res);

  res.json = function patchedJson(body: unknown): Response {
    // Only inject into object responses (not arrays, primitives, or null).
    if (body !== null && typeof body === "object" && !Array.isArray(body)) {
      // Fire-and-forget: fetch lag status asynchronously.
      // We use a synchronous snapshot from statusService to avoid async
      // complexity inside the json override — the status service already
      // caches its state in memory.
      const lagStatus = getLagStatusSync();

      const enriched = {
        ...(body as Record<string, unknown>),
        _system: lagStatus,
      };

      return originalJson(enriched);
    }

    return originalJson(body);
  };

  next();
}

// ---------------------------------------------------------------------------
// Synchronous snapshot helper
// ---------------------------------------------------------------------------

/**
 * Returns a synchronous snapshot of the current lag status by reading the
 * in-memory state of statusService and lagMonitor directly.
 *
 * This avoids async/await inside the json() override while still providing
 * accurate data (the service state is updated by the async getStatus() calls
 * made by the /status endpoint and other callers).
 */
function getLagStatusSync(): {
  status: string;
  degraded: boolean;
  lag: number;
  level: string;
} {
  // Access the internal lastIndexedLedger via the public API surface.
  // We derive lag from the mock/real ledger synchronously.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const svc = statusService as any;
  const lastIndexed: number = svc.lastIndexedLedger ?? 0;
  const mockLedger: number | null = svc.mockCurrentLedger ?? null;

  // Replicate the same mock-ledger logic as StatusService.
  const currentLedger =
    mockLedger !== null
      ? mockLedger
      : 100000 + Math.floor((Date.now() % 3600000) / 5000);

  const lag = Math.max(0, currentLedger - lastIndexed);
  const level = lagMonitor.computeLevel(lag);
  const isDegraded = level !== "none";

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const isMaintenanceMode: boolean = (svc as any).isMaintenanceMode ?? false;

  let status: string;
  if (isMaintenanceMode) {
    status = "maintenance";
  } else if (isDegraded) {
    status = "degraded";
  } else {
    status = "operational";
  }

  return { status, degraded: isDegraded, lag, level };
}
