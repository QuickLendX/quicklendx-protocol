import { Router } from "express";
import invoiceRoutes from "./invoices";
import bidRoutes from "./bids";
import settlementRoutes from "./settlements";
import testErrorRoutes from "./test-errors";
import adminRoutes from "./admin";
import monitoringRoutes from "./monitoring";

const router = Router();

router.use("/invoices", invoiceRoutes);
router.use("/bids", bidRoutes);
router.use("/settlements", settlementRoutes);
router.use("/test-errors", testErrorRoutes);
router.use("/admin", adminRoutes);
router.use("/admin/monitoring", monitoringRoutes);

// V1 Health check
router.get("/health", (req, res) => {
  res.json({
    status: "ok",
    version: "1.0.0",
    timestamp: new Date().toISOString(),
  });
});

export default router;