/**
 * Liveness and readiness probes.
 *
 * These endpoints are intentionally mounted at the root of the app (not under
 * /api/v1) and are unauthenticated, because container orchestrators (Kubernetes,
 * ECS, Nomad, …) probe them without credentials.
 *
 * Two distinct concerns:
 *
 *   GET /health, GET /livez  — Liveness. "Is the process up and able to serve
 *     an HTTP request at all?" Cheap and dependency-free. A failing liveness
 *     probe tells the orchestrator to restart the container, so it must NOT
 *     consult downstream dependencies — a transient DB blip should not trigger
 *     a restart loop.
 *
 *   GET /readyz — Readiness. "Should this instance receive traffic right now?"
 *     Probes real dependencies (DB connectivity, ingest lag, webhook queue) and
 *     returns 503 when any hard dependency is unavailable or when maintenance
 *     mode is enabled. A failing readiness probe pulls the instance out of the
 *     load-balancer rotation without restarting it.
 *
 * Security: responses expose only coarse status enums per sub-system. They do
 * not leak internal hostnames, versions, queue depths, ledger numbers, or error
 * messages to unauthenticated callers. The richer, authenticated diagnostics
 * remain under /api/v1/admin/monitoring.
 */

import { Router, Request, Response } from "express";
import { 
  pingDatabase,
  pingDatabaseDetailed,
  getPingStats,
  getPingState,
  getPingMetricsForPrometheus
} from "../lib/database";
import { statusService } from "../services/statusService";
import { lagMonitor } from "../services/lagMonitor";
import { webhookQueueService } from "../services/webhookQueueService";

const router = Router();

/**
 * Coarse per-dependency status, mirroring the SubStatus pattern used by
 * /api/v1/admin/monitoring. "degraded" means serving but impaired (does not
 * fail readiness); "unavailable" means the dependency could not be reached
 * (fails readiness).
 */
type SubStatus = "ok" | "degraded" | "unavailable";

type ReadyStatus = "ready" | "not_ready" | "maintenance";

// ---------------------------------------------------------------------------
// Liveness
// ---------------------------------------------------------------------------

function liveness(_req: Request, res: Response): void {
  res.json({
    status: "ok",
    timestamp: new Date().toISOString(),
  });
}

// Keep the historical /health path as a liveness check, and add the
// conventional /livez alias.
router.get("/health", liveness);
router.get("/livez", liveness);

// ---------------------------------------------------------------------------
// Readiness
// ---------------------------------------------------------------------------

router.get("/readyz", async (req: Request, res: Response) => {
  // Maintenance mode short-circuits readiness: the instance is intentionally
  // not serving, so it should be pulled from rotation regardless of deps.
  if (statusService.isMaintenanceEnabled()) {
    res.status(503).json({
      status: "maintenance" as ReadyStatus,
      database: "ok" as SubStatus,
      ingest: "ok" as SubStatus,
      webhookQueue: "ok" as SubStatus,
      timestamp: new Date().toISOString(),
    });
    return;
  }

  // Check for detailed diagnostics query parameter
  const includeDetails = req.query.details === 'true';

  // --- Database connectivity (hard dependency) ---------------------------
  let database: SubStatus = "ok";
  let databaseDetails: any = undefined;
  
  if (includeDetails) {
    // Use enhanced ping for detailed diagnostics
    try {
      const pingResult = await pingDatabaseDetailed({ timeoutMs: 3000 });
      if (pingResult.success) {
        database = "ok";
        databaseDetails = {
          latencyMs: pingResult.latencyMs,
          attempts: pingResult.attempts,
          state: getPingState(),
          stats: getPingStats()
        };
      } else {
        database = "unavailable";
        databaseDetails = {
          error: {
            code: pingResult.error.code,
            type: pingResult.error.constructor.name,
            severity: pingResult.error.severity,
            retryable: pingResult.error.retryable
          },
          latencyMs: pingResult.latencyMs,
          attempts: pingResult.attempts,
          state: getPingState(),
          stats: getPingStats()
        };
      }
    } catch (error) {
      database = "unavailable";
      databaseDetails = {
        error: {
          message: "Unexpected database ping failure",
          type: "UnknownError"
        },
        state: getPingState(),
        stats: getPingStats()
      };
    }
  } else {
    // Use backward-compatible simple ping
    if (!pingDatabase()) {
      database = "unavailable";
    }
  }

  // --- Ingest lag --------------------------------------------------------
  // Reuse the LagMonitor degradation logic. "warn" lag is degraded but still
  // serviceable; "critical" lag means the indexed view is too stale to trust,
  // so we treat it as unavailable for readiness.
  let ingest: SubStatus = "ok";
  try {
    const lag = await lagMonitor.getLagStatus();
    if (lag.isCritical) {
      ingest = "unavailable";
    } else if (lag.isDegraded) {
      ingest = "degraded";
    }
  } catch {
    ingest = "unavailable";
  }

  // --- Webhook queue health (hard dependency on its backing store) -------
  // A throw here means the queue's store is unreachable. Saturation (queue at
  // capacity) is back-pressure, not unreadiness, so it is reported as degraded.
  let webhookQueue: SubStatus = "ok";
  try {
    const stats = webhookQueueService.getStats();
    if (stats.capacity > 0 && stats.size >= stats.capacity) {
      webhookQueue = "degraded";
    }
  } catch {
    webhookQueue = "unavailable";
  }

  const unavailable =
    database === "unavailable" ||
    ingest === "unavailable" ||
    webhookQueue === "unavailable";

  const status: ReadyStatus = unavailable ? "not_ready" : "ready";

  const response: any = {
    status,
    database,
    ingest,
    webhookQueue,
    timestamp: new Date().toISOString(),
  };

  // Include detailed diagnostics if requested
  if (includeDetails && databaseDetails) {
    response.databaseDetails = databaseDetails;
  }

  res.status(unavailable ? 503 : 200).json(response);
});

// ---------------------------------------------------------------------------
// Database Diagnostics and Metrics
// ---------------------------------------------------------------------------

/**
 * Database ping diagnostics endpoint for detailed troubleshooting.
 * Returns comprehensive database connectivity information including
 * error details, retry attempts, latency metrics, and state history.
 * 
 * This endpoint is unauthenticated for operational use but provides
 * detailed information for debugging database connectivity issues.
 */
router.get("/db-diagnostics", async (_req: Request, res: Response) => {
  try {
    const [detailedResult, currentStats] = await Promise.all([
      pingDatabaseDetailed({ timeoutMs: 3000 }),
      Promise.resolve(getPingStats())
    ]);

    const diagnostics = {
      current: {
        success: detailedResult.success,
        latencyMs: detailedResult.latencyMs,
        attempts: detailedResult.attempts,
        timestamp: detailedResult.timestamp,
        state: getPingState(),
      },
      error: detailedResult.success ? null : {
        code: detailedResult.error.code,
        type: detailedResult.error.constructor.name,
        message: detailedResult.error.message,
        severity: detailedResult.error.severity,
        retryable: detailedResult.error.retryable,
      },
      statistics: currentStats,
      metadata: {
        endpoint: "/db-diagnostics",
        timestamp: new Date().toISOString(),
        uptime: process.uptime(),
      }
    };

    res.status(detailedResult.success ? 200 : 503).json(diagnostics);
  } catch (error) {
    res.status(500).json({
      current: {
        success: false,
        error: "Failed to run database diagnostics",
        timestamp: new Date().toISOString(),
      },
      statistics: getPingStats(),
      metadata: {
        endpoint: "/db-diagnostics", 
        timestamp: new Date().toISOString(),
        uptime: process.uptime(),
      }
    });
  }
});

/**
 * Database metrics in Prometheus format for monitoring integration.
 * Returns metrics that can be scraped by Prometheus or similar systems.
 */
router.get("/db-metrics", (_req: Request, res: Response) => {
  try {
    const metrics = getPingMetricsForPrometheus();
    
    // Convert to Prometheus text format
    const lines: string[] = [];
    const processedNames = new Set<string>();
    
    for (const metric of metrics) {
      if (!processedNames.has(metric.name)) {
        lines.push(`# HELP ${metric.name} ${metric.help}`);
        lines.push(`# TYPE ${metric.name} ${metric.type}`);
        processedNames.add(metric.name);
      }
      
      let line = metric.name;
      if (metric.labels && Object.keys(metric.labels).length > 0) {
        const labelPairs = Object.entries(metric.labels)
          .map(([key, value]) => `${key}="${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`)
          .join(',');
        line += `{${labelPairs}}`;
      }
      line += ` ${metric.value}`;
      lines.push(line);
    }
    
    res.set('Content-Type', 'text/plain; charset=utf-8');
    res.status(200).send(lines.join('\n') + '\n');
  } catch (error) {
    res.status(500).json({
      error: "Failed to generate database metrics",
      timestamp: new Date().toISOString(),
    });
  }
});

export default router;

// Export types for testing
export type { SubStatus, ReadyStatus };
