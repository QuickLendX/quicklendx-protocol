import { Router, Request, Response } from "express";
import { apiKeyAuth, AuthenticatedRequest } from "../../middleware/apiKeyAuth";
import { statusService } from "../../services/statusService";
import { getInvariantCounters } from "../../services/invariantService";
import { webhookQueueService } from "../../services/webhookQueueService";

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
    res.json(report);
  } catch (err) {
    const message =
      err instanceof Error ? err.message : "Failed to compute invariants";
    res.status(500).json({
      error: { message, code: "INVARIANT_CHECK_ERROR" },
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
  } catch (err) {
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

export default router;