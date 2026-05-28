import { Router } from "express";
import invoiceRoutes from "./invoices";
import bidRoutes from "./bids";
import settlementRoutes from "./settlements";
import portfolioRoutes from "./portfolio";
import testErrorRoutes from "./test-errors";
import webhookRoutes from "./webhooks";
import exportRoutes from "./exports";
import notificationRoutes from "./notifications";
import adminRoutes from "./admin";
import monitoringRoutes from "./monitoring";
import { lagMonitor } from "../../services/lagMonitor";
import { degradedGuard } from "../../middleware/degraded-guard";
import { eventProcessor } from "../../services/eventProcessor";
import {
  DefaultEventValidator,
  getStableEventId,
  validateEventBatch,
  EventValidationResult,
  SorobanEvent,
} from "../../services/eventValidator";
import { FileSystemRawEventStore } from "../../services/rawEventStore";

const router = Router();
const eventIdStore = new FileSystemRawEventStore(new DefaultEventValidator());

router.use("/invoices", invoiceRoutes);
router.use("/bids", bidRoutes);
router.use("/settlements", settlementRoutes);
// FIX: portfolioRoutes was imported but never mounted – /portfolio always returned 404.
// This was a drift between openapi.yaml (which documents /portfolio) and the live routes.
router.use("/portfolio", portfolioRoutes);
router.use("/test-errors", testErrorRoutes);
router.use("/webhooks", webhookRoutes);
router.use("/notifications", notificationRoutes);
router.use("/exports", exportRoutes);
router.use("/admin", adminRoutes);
router.use("/admin/monitoring", monitoringRoutes);

// ---------------------------------------------------------------------------
// System status endpoint
// Returns the current lag status including degradation level and thresholds.
// ---------------------------------------------------------------------------
router.get("/status", async (req, res, next) => {
  try {
    const lagStatus = await lagMonitor.getLagStatus();
    res.json(lagStatus);
  } catch (err) {
    next(err);
  }
});

// ---------------------------------------------------------------------------
// Demo write endpoint – gated by degradedGuard (warn + critical)
// In a real system this would be a bid placement, settlement trigger, etc.
// ---------------------------------------------------------------------------
router.post(
  "/write-action",
  degradedGuard(),
  (req, res) => {
    res.status(201).json({ success: true, message: "Write action accepted" });
  }
);

// ---------------------------------------------------------------------------
// Demo critical-only gated endpoint
// Only blocked when lag >= criticalThreshold.
// ---------------------------------------------------------------------------
router.post(
  "/critical-action",
  degradedGuard({ criticalOnly: true }),
  (req, res) => {
    res.status(201).json({ success: true, message: "Critical action accepted" });
  }
);

// Event processing endpoint (for indexer to post events)
router.post("/events", async (req, res) => {
  try {
    const events = Array.isArray(req.body) ? req.body : [req.body];
    const validation = validateEventBatch(events);

    if (validation.errors) {
      res.status(400).json({
        success: false,
        results: validation.errors.map((error) => ({
          status: "rejected",
          error,
        })),
      });
      return;
    }

    const response: Array<{
      index: number;
      id?: string;
      type?: string;
      status: "processed" | "duplicate" | "rejected" | "failed";
      errors?: string[];
      error?: string;
    }> = [];

    for (let i = 0; i < events.length; i++) {
      const result = validation.results[i] as EventValidationResult;

      if (!result.success) {
        response.push({
          index: i,
          status: "rejected",
          errors: result.errors,
        });
        continue;
      }

      const event = result.data;
      const eventId = getStableEventId(event);

      if (await eventIdStore.hasEventId(eventId)) {
        response.push({
          index: i,
          id: eventId,
          type: event.type,
          status: "duplicate",
        });
        continue;
      }

      try {
        await eventProcessor.processEvent(event);
        await eventIdStore.storeEvents([event as SorobanEvent]);
        response.push({
          index: i,
          id: eventId,
          type: event.type,
          status: "processed",
        });
      } catch (err) {
        response.push({
          index: i,
          id: eventId,
          type: event.type,
          status: "failed",
          error: "Processing failed",
        });
      }
    }

    const hasRejected = response.some((result) => result.status === "rejected");
    const hasFailed = response.some((result) => result.status === "failed");

    res.status(hasRejected ? 400 : hasFailed ? 500 : 200).json({
      success: !hasRejected && !hasFailed,
      results: response,
    });
  } catch (error) {
    console.error("Error processing events:", error);
    res.status(500).json({ error: "Internal server error" });
  }
});

// V1 Health check
router.get("/health", (req, res) => {
  res.json({
    status: "ok",
    version: "1.0.0",
    timestamp: new Date().toISOString(),
  });
});

export default router;
