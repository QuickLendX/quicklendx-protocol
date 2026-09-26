import { Router } from "express";
import * as bidController from "../../controllers/v1/bids";
import { createQueryValidationMiddleware, createBodyValidationMiddleware } from "../../middleware/validation";
import { getBidsQuerySchema, createBidBodySchema } from "../../validators/bids";
import { apiKeyAuthMiddleware, requireScopes } from "../../middleware/api-key-auth";
import { requireSignature } from "../../middleware/request-signing";
import { BID_READ_SCOPES, BID_WRITE_SCOPES } from "../../middleware/bid-authorization";

const router = Router();

/**
 * Bid endpoints expose commercially sensitive order-flow data (amounts,
 * expected returns, timing, investor identity) and the auction-selection
 * results derived from it. Every route below therefore requires an
 * authenticated API key with the matching `read:bids` / `write:bids` scope.
 *
 * The handlers additionally enforce per-tenant binding on the `investor`
 * filter (see `middleware/bid-authorization.ts`) so authentication alone is
 * not sufficient to read another investor's book.
 */

/**
 * GET /api/v1/bids - Get ranked bids for an invoice
 * Requires authentication + `read:bids`.
 * Query params: invoice_id (required), investor (optional), status (optional), limit, cursor
 */
router.get(
  "/",
  apiKeyAuthMiddleware,
  requireScopes(BID_READ_SCOPES),
  createQueryValidationMiddleware(getBidsQuerySchema),
  bidController.getBids
);

/**
 * POST /api/v1/bids - Place a new bid
 * Requires authentication (Bearer token in Authorization header) + `write:bids`.
 * Body: invoice_id, bid_amount, expected_return, expiration_timestamp
 */
router.post(
  "/",
  apiKeyAuthMiddleware,
  requireScopes(BID_WRITE_SCOPES),
  requireSignature,
  createBodyValidationMiddleware(createBidBodySchema),
  bidController.createBid
);

/**
 * GET /api/v1/bids/:invoiceId/best - Get the best bid for an invoice
 * Requires authentication + `read:bids`.
 */
router.get(
  "/:invoiceId/best",
  apiKeyAuthMiddleware,
  requireScopes(BID_READ_SCOPES),
  bidController.getBestBid
);

/**
 * GET /api/v1/bids/:invoiceId/ranked - Get ranked bids for an invoice
 * Requires authentication + `read:bids`.
 */
router.get(
  "/:invoiceId/ranked",
  apiKeyAuthMiddleware,
  requireScopes(BID_READ_SCOPES),
  bidController.getTopBids
);

export default router;
