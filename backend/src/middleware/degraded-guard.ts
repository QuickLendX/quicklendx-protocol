/**
 * degradedGuard middleware
 *
 * Feature gate for write / sensitive endpoints.
 *
 * Behaviour
 * ---------
 * - WARN level  → 503 Service Unavailable with code DEGRADED_MODE
 * - CRITICAL level → 503 Service Unavailable with code DEGRADED_MODE_CRITICAL
 * - NONE level  → passes through to next()
 *
 * Security contract
 * -----------------
 * This middleware MUST be placed AFTER any authentication / authorisation
 * middleware in the chain.  It never bypasses auth — it only adds an
 * additional availability gate on top of existing security layers.
 *
 * Usage
 * -----
 *   import { degradedGuard } from "../middleware/degraded-guard";
 *
 *   // Block all writes when degraded (warn or critical):
 *   router.post("/bids", degradedGuard(), bidController.placeBid);
 *
 *   // Block only when critically degraded:
 *   router.post("/settlements", degradedGuard({ criticalOnly: true }), ...);
 */

import { Request, Response, NextFunction } from "express";
import { lagMonitor } from "../services/lagMonitor";

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

export interface DegradedGuardOptions {
  /**
   * When true, only block at CRITICAL level.
   * When false (default), block at WARN and CRITICAL.
   */
  criticalOnly?: boolean;
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

export function degradedGuard(options: DegradedGuardOptions = {}) {
  const { criticalOnly = false } = options;

  return async function degradedGuardMiddleware(
    req: Request,
    res: Response,
    next: NextFunction
  ): Promise<void> {
    try {
      const lagStatus = await lagMonitor.getLagStatus();

      const shouldBlock = criticalOnly
        ? lagStatus.isCritical
        : lagStatus.isDegraded;

      if (shouldBlock) {
        const code =
          lagStatus.isCritical ? "DEGRADED_MODE_CRITICAL" : "DEGRADED_MODE";

        const message =
          lagStatus.isCritical
            ? "Service is critically degraded due to high indexer lag. Write operations are unavailable."
            : "Service is degraded due to indexer lag. Write operations are temporarily unavailable.";

        res.status(503).json({
          error: {
            message,
            code,
            details: {
              lag: lagStatus.lag,
              warn_threshold: lagStatus.warnThreshold,
              critical_threshold: lagStatus.criticalThreshold,
              level: lagStatus.level,
            },
          },
        });
        return;
      }

      next();
    } catch (err) {
      // If the lag check itself fails, fail open (allow the request through)
      // and let the error propagate to the global error handler.
      next(err);
    }
  };
}
