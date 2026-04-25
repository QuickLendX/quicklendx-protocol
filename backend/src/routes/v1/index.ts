import { Router } from "express";
import invoiceRoutes from "./invoices";
import bidRoutes from "./bids";
import settlementRoutes from "./settlements";
import testErrorRoutes from "./test-errors";
import webhookRoutes from "./webhooks";

const router = Router();

router.use("/invoices", invoiceRoutes);
router.use("/bids", bidRoutes);
router.use("/settlements", settlementRoutes);
router.use("/test-errors", testErrorRoutes);
router.use("/webhooks", webhookRoutes);

// V1 Health check
router.get("/health", (req, res) => {
  res.json({
    status: "ok",
    version: "1.0.0",
    timestamp: new Date().toISOString(),
  });
});

export default router;
