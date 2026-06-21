import { Router } from "express";
import * as exportController from "../../controllers/v1/exports";

const router = Router();

/**
 * POST /exports
 * Creates a new export job.
 * Body: { type: "invoices" | "bids" | "settlements", format: "json" | "csv" }
 */
router.post("/", exportController.createExport);

/**
 * GET /exports/:token/download
 * Downloads the signed file identified by the token.
 * Returns X-Body-Signature and X-Body-Signature-Algorithm headers.
 */
router.get("/:token/download", exportController.downloadExport);

export default router;
