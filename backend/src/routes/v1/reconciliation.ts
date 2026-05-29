import { Router } from "express";
import { getDriftReports, runReconciliation, triggerBackfill } from "../../controllers/v1/reconciliation";
import { reconciliationRateLimitMiddleware } from "../../middleware/rate-limit";
import { requireAdminRoles } from "../../middleware/rbac";
import { OPERATIONS_WRITE_ROLES } from "../../types/rbac";

const router = Router();

router.get("/reports", reconciliationRateLimitMiddleware, getDriftReports);
router.post("/run", reconciliationRateLimitMiddleware, requireAdminRoles(OPERATIONS_WRITE_ROLES, "reconciliation:run"), runReconciliation);
router.post("/backfill", reconciliationRateLimitMiddleware, requireAdminRoles(OPERATIONS_WRITE_ROLES, "reconciliation:backfill"), triggerBackfill);

export default router;
