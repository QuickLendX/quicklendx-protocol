import { Router } from "express";
import invoiceRoutes from "./invoices";
import bidRoutes from "./bids";
import settlementRoutes from "./settlements";
import portfolioRoutes from "./portfolio";
import testErrorRoutes from "./test-errors";
import webhookRoutes from "./webhooks";

const router = Router();

router.use("/invoices", invoiceRoutes);
router.use("/bids", bidRoutes);
router.use("/settlements", settlementRoutes);
router.use("/notifications", notificationRoutes);
router.use("/test-errors", testErrorRoutes);
router.use("/admin", adminRoutes);
router.use("/webhooks", webhookRoutes);

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
    const { eventProcessor } = await import("../../services/eventProcessor");
    const events = Array.isArray(req.body) ? req.body : [req.body];

    for (const event of events) {
      await eventProcessor.processEvent(event);
    }

    res.json({ success: true, processed: events.length });
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
