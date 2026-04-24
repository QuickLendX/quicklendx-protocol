import { Router } from "express";
import invoiceRoutes from "./invoices";
import bidRoutes from "./bids";
import settlementRoutes from "./settlements";
import testErrorRoutes from "./test-errors";
import notificationRoutes from "./notifications";

const router = Router();

router.use("/invoices", invoiceRoutes);
router.use("/bids", bidRoutes);
router.use("/settlements", settlementRoutes);
router.use("/notifications", notificationRoutes);
router.use("/test-errors", testErrorRoutes);

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
