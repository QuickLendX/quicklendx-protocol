import { Request, Response, NextFunction } from "express";
import { Bid, BidStatus } from "../../types/contract";
import { CreateBidBody, createBidBodySchema } from "../../validators/bids";
import { applyCacheHeaders, CC_NO_STORE, computeETag, assertConditionalWrite } from "../../middleware/cache-headers";
import { invoiceStore } from "../../services/invoiceStore";
import { MOCK_INVOICES } from "./invoices";
import { labelRecord } from "../../services/versioningService";
import { freshnessService } from "../../services/freshnessService";
import { parsePaginationParams, PaginationError, applyPagination } from "../../utils/pagination";
import { SnapshotService } from "../../services/snapshotService";
import { bidStore } from "../../services/bidStore";
import {
  exposureService,
  ExposureCapExceededError,
  InvalidAmountError,
} from "../../services/exposureService";
import crypto from "crypto";

/**
 * Create a new bid.
 * Requires authentication (apiKeyAuth middleware).
 * Validates:
 * - Invoice exists and is Verified status
 * - No duplicate active bid from same investor on same invoice
 * - Bid amount >= 1
 * - Expected return >= bid amount
 * - Investor's aggregate exposure (active bids + unsettled positions) plus
 *   this new bid does not exceed EXPOSURE_CAP_PER_INVESTOR_USD — otherwise
 *   the request is rejected with 429 EXPOSURE_CAP_EXCEEDED before any
 *   expensive chain interaction is attempted.
 */
export const createBid = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    if (!req.apiKey) {
      return res.status(401).json({
        error: {
          message: "Authentication required",
          code: "UNAUTHORIZED",
        },
      });
    }

    const validated = createBidBodySchema.parse(req.body);

    // ── Exposure-cap gate ────────────────────────────────────────────────
    // Compute the investor's current USD-equivalent exposure across
    // MOCK_BIDS, MOCK_SETTLEMENTS, and the persisted bidStore / settlement
    // tables, then check whether this new bid would push them past the
    // configured EXPOSURE_CAP_PER_INVESTOR_USD. Returning 429 here avoids
    // a wasted RPC round-trip when the chain would reject the bid anyway.
    const investor = req.apiKey.created_by;
    const currency = validated.currency ?? "USDC";
    try {
      await exposureService.assertWithinCap(
        investor,
        validated.bid_amount,
        currency,
      );
    } catch (err) {
      if (err instanceof ExposureCapExceededError) {
        return res.status(429).json({
          error: {
            message: err.message,
            code: "EXPOSURE_CAP_EXCEEDED",
            currentExposureUsd: err.currentExposureUsd.toString(),
            attemptedUsd: err.attemptedUsd.toString(),
            capUsd: err.capUsd.toString(),
            investor: err.investor,
          },
        });
      }
      if (err instanceof InvalidAmountError) {
        return res.status(400).json({
          error: {
            message: err.message,
            code: "INVALID_BID",
          },
        });
      }
      throw err;
    }

    // Generate deterministic bid_id (contract-like ID)
    const bidId = "0x" + crypto.randomBytes(32).toString("hex");
    const timestamp = Math.floor(Date.now() / 1000);

    const bid = await bidStore.createBid({
      ...validated,
      bid_id: bidId,
      investor: req.apiKey.created_by,
      timestamp,
      created_by: req.apiKey.created_by,
    });

    res.status(201).json({ data: bid });
  } catch (error: any) {
    if (error.message.includes("Invoice not found") ||
        error.message.includes("Cannot place bid") ||
        error.message.includes("already has an active bid") ||
        error.message.includes("Bid amount must be") ||
        error.message.includes("Expected return")) {
      return res.status(400).json({
        error: {
          message: error.message,
          code: "INVALID_BID",
        },
      });
    }
    next(error);
  }
};

/**
 * Get bids for an invoice with optional filtering and pagination.
 * Returns ranked bids (best first) by default.
 * Filters: invoice_id (required), investor (optional), status (optional)
 */
export const getBids = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const params = parsePaginationParams(req.query);
    const { invoice_id, investor, status } = req.query;

    if (!invoice_id) {
      return res.status(400).json({
        error: {
          message: "invoice_id is required",
          code: "MISSING_REQUIRED_FIELD",
        },
      });
    }

    const filters = {
      investor: investor as string | undefined,
      status: status as BidStatus | undefined,
    };

    const page = await bidStore.getBidsPaginated(
      invoice_id as string,
      params.limit,
      params.cursor,
      filters
    );

    applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body: page });
    res.json({ data: page.data, next_cursor: page.next_cursor, has_more: page.has_more });
  } catch (error) {
    if (error instanceof PaginationError) {
      return res.status(400).json({
        error: { message: error.message, code: "INVALID_PAGINATION" },
      });
    }
    next(error);
  }
};

/**
 * Get the best bid for an invoice.
 * Returns the highest-ranked Placed bid, or 404 if none exist.
 */
export const getBestBid = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { invoiceId } = req.params;
    const bestBid = await bidStore.getBestBid(invoiceId as string);
    if (!bestBid) {
      return res.status(404).json({ error: "No best bid found for this invoice" });
    }
    res.json({ data: bestBid });
  } catch (error) {
    next(error);
  }
};

/**
 * Get ranked bids for an invoice.
 * Returns all Placed bids sorted by contract ranking logic (best first).
 */
export const getTopBids = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { invoiceId } = req.params;
    const topBids = await bidStore.getRankedBids(invoiceId as string, 100);
    res.json({ data: topBids });
  } catch (error) {
    next(error);
  }
};

// Legacy mock export for compatibility with existing export/reporting services.
//
// IMPORTANT: this array is intentionally exposed (not encapsulated) so the
// exposureService can read live mock fixtures in tests and so the export
// pipeline keeps working. Mutations are allowed in tests; production code
// must not push to this array.
export const MOCK_BIDS: any[] = [];
