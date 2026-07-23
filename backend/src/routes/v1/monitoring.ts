// @ts-nocheck
import { Router, Request, Response } from "express";
import { apiKeyAuth, AuthenticatedRequest } from "../../middleware/apiKeyAuth";
import { reconciliationRateLimitMiddleware } from "../../middleware/rate-limit";
import { statusService } from "../../services/statusService";
import {
  getInvariantCounters,
  getInvariantMetrics,
} from "../../services/invariantService";
import { webhookQueueService } from "../../services/webhookQueueService";
import { ReconciliationWorker } from "../../services/reconciliationWorker";
import { latencyTracker, DEFAULT_WINDOW_MS } from "../../services/latencyTracker";

const router = Router();
router.use(apiKeyAuth);

router.get("/health", (_req: AuthenticatedRequest, res: Response) => {
  type SubStatus = "ok" | "degraded" | "unavailable";
  let sss: SubStatus = "ok";
  let wqs: SubStatus = "ok";
  let iss: SubStatus = "ok";
  let sssFailed = false;
  let wqsFailed = false;
  let issFailed = false;

  try {
    void statusService.getLastIndexedLedger();
  } catch {
    sssFailed = true;
    sss = "unavailable";
  }

  try {
    void webhookQueueService.getStats();
  } catch {
    wqsFailed = true;
    wqs = "unavailable";
  }

  try {
    void getInvariantCounters();
  } catch {
    issFailed = true;
    iss = "unavailable";
  }

  type HealthStatus = "ok" | "degraded" | "unavailable" | "maintenance";
  let overall: HealthStatus = "ok";
  if (sssFailed || wqsFailed || issFailed) {
    overall = "unavailable";
  }

  if (statusService.isMaintenanceEnabled()) {
    overall = "maintenance";
  }

  res.json({
    status: overall,
    statusService: sss,
    webhookQueue: wqs,
    invariants: iss,
    timestamp: new Date().toISOString(),
  });
});

router.get("/cursor", async (_req: AuthenticatedRequest, res: Response) => {
  try {
    const lastIndexedLedger = statusService.getLastIndexedLedger();
    const currentLedger = await statusService.getCurrentLedger();
    const ingestLag = currentLedger - lastIndexedLedger;

    res.json({
      lastIndexedLedger,
      currentLedger,
      ingestLag,
      timestamp: new Date().toISOString(),
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : "Failed to read cursor";
    res.status(500).json({
      error: { message, code: "CURSOR_READ_ERROR" },
    });
  }
});

router.get("/invariants", (_req: AuthenticatedRequest, res: Response) => {
  try {
    const report = getInvariantCounters();
    if (report === null) {
      res.status(200).json({
        message: "No invariant report available yet",
        timestamp: new Date().toISOString(),
      });
      return;
    }
    res.json(report);
  } catch (err) {
    const message =
      err instanceof Error ? err.message : "Failed to compute invariants";
    res.status(500).json({
      error: { message, code: "INVARIANT_CHECK_ERROR" },
    });
  }
});

router.get("/invariants/metrics", (_req: AuthenticatedRequest, res: Response) => {
  try {
    const metrics = getInvariantMetrics();
    res.json(metrics);
  } catch (err) {
    const message = err instanceof Error ? err.message : "Failed to get metrics";
    res.status(500).json({
      error: { message, code: "INVARIANT_METRICS_ERROR" },
    });
  }
});

router.get("/webhook", (_req: AuthenticatedRequest, res: Response) => {
  try {
    const stats = webhookQueueService.getStats();
    res.json(stats);
  } catch (err) {
    const message =
      err instanceof Error ? err.message : "Failed to read webhook queue";
    res.status(500).json({
      error: { message, code: "WEBHOOK_QUEUE_ERROR" },
    });
  }
});

router.post("/webhook", (req: AuthenticatedRequest, res: Response) => {
  const body = req.body;
  if (
    typeof body !== "object" ||
    body === null ||
    typeof (body as Record<string, unknown>).type !== "string" ||
    (body as Record<string, unknown>).type === ""
  ) {
    res.status(400).json({
      error: {
        message: "Invalid request body",
        code: "INVALID_WEBHOOK_PAYLOAD",
      },
    });
    return;
  }

  try {
    const payload = (body as Record<string, unknown>).payload;
    const event = webhookQueueService.enqueue(
      (body as Record<string, unknown>).type as string,
      payload
    );
    res.status(201).json({
      id: event.id,
      enqueuedAt: event.enqueuedAt,
    });
  } catch (err: any) {
    if (err.statusCode === 503) {
      res.status(503).json({
        error: "Service Unavailable",
        message: "Outbound webhook queue is full. Please retry later.",
      });
      return;
    }
    const message =
      err instanceof Error ? err.message : "Failed to enqueue webhook";
    res.status(500).json({
      error: { message, code: "WEBHOOK_ENQUEUE_ERROR" },
    });
  }
});

router.post("/webhook/:id/success", (req: AuthenticatedRequest, res: Response) => {
  const id = req.params["id"] as string;
  if (!id) {
    res.status(400).json({
      error: { message: "Missing webhook id", code: "MISSING_WEBHOOK_ID" },
    });
    return;
  }

  try {
    const ok = webhookQueueService.markSuccess(id);
    res.json({ id, outcome: ok ? "success" : "not_found_or_already_resolved" });
  } catch (err) {
    const message =
      err instanceof Error ? err.message : "Failed to mark webhook success";
    res.status(500).json({
      error: { message, code: "WEBHOOK_OUTCOME_ERROR" },
    });
  }
});

router.post("/webhook/:id/fail", (req: AuthenticatedRequest, res: Response) => {
  const id = req.params["id"] as string;
  if (!id) {
    res.status(400).json({
      error: { message: "Missing webhook id", code: "MISSING_WEBHOOK_ID" },
    });
    return;
  }

  try {
    const ok = webhookQueueService.markFailed(id);
    res.json({ id, outcome: ok ? "failed" : "not_found_or_already_resolved" });
  } catch (err) {
    const message =
      err instanceof Error ? err.message : "Failed to mark webhook failed";
    res.status(500).json({
      error: { message, code: "WEBHOOK_OUTCOME_ERROR" },
    });
  }
});

// Reconciliation metrics endpoint
router.get("/reconciliation", async (_req: AuthenticatedRequest, res: Response) => {
  try {
    const latest = ReconciliationWorker.getLatestReport();
    res.json({ latest });
  } catch (err) {
    const message = err instanceof Error ? err.message : "Failed to read reconciliation";
    res.status(500).json({ error: { message, code: "RECONCILIATION_READ_ERROR" } });
  }
});

/**
 * GET /api/v1/admin/monitoring/latency
 *
 * Returns p50 / p95 / p99 latencies per normalised route over the last
 * `windowMs` milliseconds (default: 5 minutes).
 *
 * Query parameters:
 *   windowMs  (optional) Override rolling window width in ms (min: 1000, max: 3600000).
 *
 * Response shape:
 *   {
 *     routes: Array<{
 *       route: string;       // Normalised route key (e.g. /api/v1/invoices/:id)
 *       count: number;       // Samples in window
 *       p50: number | null;  // ms
 *       p95: number | null;  // ms
 *       p99: number | null;  // ms
 *       min: number | null;  // ms
 *       max: number | null;  // ms
 *       windowMs: number;
 *     }>;
 *     windowMs: number;
 *     totalRoutes: number;
 *     maxRoutes: number;
 *     overflowed: boolean;
 *     generatedAt: string;   // ISO timestamp
 *   }
 */
router.get("/latency", (_req: AuthenticatedRequest, res: Response) => {
  try {
    const rawWindow = Number((_req.query as Record<string, unknown>).windowMs);
    const windowMs =
      Number.isFinite(rawWindow) && rawWindow >= 1_000 && rawWindow <= 3_600_000
        ? rawWindow
        : DEFAULT_WINDOW_MS;

    const stats = latencyTracker.getStats(windowMs);
    res.json(stats);
  } catch (err) {
    const message = err instanceof Error ? err.message : "Failed to compute latency stats";
    res.status(500).json({ error: { message, code: "LATENCY_STATS_ERROR" } });
  }
});

export default router;
