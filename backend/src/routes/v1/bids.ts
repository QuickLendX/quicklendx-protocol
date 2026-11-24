import { Router } from "express";
import * as bidController from "../../controllers/v1/bids";
import { createQueryValidationMiddleware, createBodyValidationMiddleware } from "../../middleware/validation";
import { getBidsQuerySchema, createBidBodySchema } from "../../validators/bids";
import { apiKeyAuthMiddleware, optionalApiKeyAuth } from "../../middleware/api-key-auth";
import {
  createQueryValidationMiddleware,
  createBodyValidationMiddleware,
  createParamsValidationMiddleware,
} from "../../middleware/validation";
import { getBidsQuerySchema, createBidBodySchema, invoiceIdParamSchema } from "../../validators/bids";
import { apiKeyAuthMiddleware } from "../../middleware/api-key-auth";
import { requireSignature } from "../../middleware/request-signing";

const router = Router();

/**
 * GET /api/v1/bids - Get ranked bids for an invoice
 * Query params: invoice_id (required), investor (optional), status (optional), limit, cursor
 */
router.get("/", createQueryValidationMiddleware(getBidsQuerySchema), optionalApiKeyAuth, bidController.getBids);

/**
 * POST /api/v1/bids - Place a new bid
 * Requires authentication (Bearer token in Authorization header)
 * Body: invoice_id, bid_amount, expected_return, expiration_timestamp
 */
router.post(
  "/",
  apiKeyAuthMiddleware,
  requireSignature,
  createBodyValidationMiddleware(createBidBodySchema),
  bidController.createBid
);

/**
 * GET /api/v1/bids/:invoiceId/best - Get the best bid for an invoice
 */
router.get("/:invoiceId/best", createParamsValidationMiddleware(invoiceIdParamSchema), bidController.getBestBid);

/**
 * GET /api/v1/bids/:invoiceId/ranked - Get ranked bids for an invoice
 */
router.get("/:invoiceId/ranked", createParamsValidationMiddleware(invoiceIdParamSchema), bidController.getTopBids);

export default router;
