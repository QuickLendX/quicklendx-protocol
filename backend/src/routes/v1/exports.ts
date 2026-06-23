import { Router } from "express";
import * as exportController from "../../controllers/v1/exports";
import { requireUserAuth } from "../../middleware/userAuth";
import { exportRateLimitMiddleware } from "../../middleware/rate-limit";
import { requireSignature } from "../../middleware/request-signing";

const router = Router();

/**
 * @openapi
 * /exports/generate:
 *   post:
 *     summary: Generate a signed data export link
 *     description: Returns a short-lived link to download all user data (invoices, bids, settlements).
 *     security:
 *       - BearerAuth: []
 *     parameters:
 *       - name: format
 *         in: query
 *         schema:
 *           type: string
 *           enum: [json, csv]
 *           default: json
 *     responses:
 *       200:
 *         description: Export link generated
 */
router.post("/generate", exportRateLimitMiddleware, requireUserAuth, requireSignature, exportController.requestExport);

/**
 * @openapi
 * /exports/download/{token}:
 *   get:
 *     summary: Download exported data
 *     description: Serves the export file using a signed token. No additional auth required if token is valid.
 *     parameters:
 *       - name: token
 *         in: path
 *         required: true
 *         schema:
 *           type: string
 *     responses:
 *       200:
 *         description: Export file content
 */
router.get("/download/:token", exportRateLimitMiddleware, exportController.downloadExport);

export default router;
