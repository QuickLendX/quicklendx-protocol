import { Router } from "express";
import { getDriftReports, runReconciliation, triggerBackfill } from "../../controllers/v1/reconciliation";
import { reconciliationRateLimitMiddleware } from "../../middleware/rate-limit";

const router = Router();

router.get("/reports", reconciliationRateLimitMiddleware, getDriftReports);
router.post("/run", reconciliationRateLimitMiddleware, runReconciliation);
router.post("/backfill", reconciliationRateLimitMiddleware, triggerBackfill);

export default router;
