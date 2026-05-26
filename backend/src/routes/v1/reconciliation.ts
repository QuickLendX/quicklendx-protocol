import { Router } from "express";
import { getDriftReports, runReconciliation, triggerBackfill } from "../../controllers/v1/reconciliation";

const router = Router();

router.get("/reports", getDriftReports);
router.post("/run", runReconciliation);
router.post("/backfill", triggerBackfill);

export default router;
