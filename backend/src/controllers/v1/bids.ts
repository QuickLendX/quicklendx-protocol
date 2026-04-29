import { Request, Response, NextFunction } from "express";
import { Bid, BidStatus } from "../../types/contract";
import { applyCacheHeaders, CC_NO_STORE } from "../../middleware/cache-headers";
import { labelRecord } from "../../services/versioningService";
import { freshnessService } from "../../services/freshnessService";
import { parsePaginationParams, PaginationError } from "../../utils/pagination";
import { SnapshotService } from "../../services/snapshotService";

export const MOCK_BIDS: Bid[] = [
  labelRecord<Omit<Bid, "contract_version" | "event_schema_version" | "indexed_at">>({
    bid_id: "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
    invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    investor: "GA...ABC",
    bid_amount: "950000000",
    expected_return: "50000000",
    timestamp: Math.floor(Date.now() / 1000) - 3600,
    status: BidStatus.Placed,
    expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
  }),
];

// applyPagination requires items with `id` field; bids use `bid_id`
type BidWithId = Bid & { id: string };

function normalizeBids(bids: Bid[]): BidWithId[] {
  return bids.map((b) => ({ ...b, id: b.bid_id }));
}

export const getBids = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const params = parsePaginationParams(req.query);
    const { invoice_id, investor } = req.query;

    let filtered = [...MOCK_BIDS];
    if (invoice_id) filtered = filtered.filter((b) => b.invoice_id === invoice_id);
    if (investor) filtered = filtered.filter((b) => b.investor === investor);

    applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body: filtered });
    res.json({ data: filtered, freshness: freshnessService.getFreshness() });
  } catch (error) {
    next(error);
  }
};

export const getBestBid = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { invoiceId } = req.params;
    const bestBid = await SnapshotService.getBestBid(invoiceId as string);
    if (!bestBid) {
      return res.status(404).json({ error: "No best bid found for this invoice" });
    }
    res.json(bestBid);
  } catch (error) {
    next(error);
  }
};

export const getTopBids = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { invoiceId } = req.params;
    const topBids = await SnapshotService.getTopBids(invoiceId as string);
    res.json({ top_bids: topBids });
  } catch (error) {
    if (error instanceof PaginationError) {
      return res.status(400).json({
        error: { message: error.message, code: "INVALID_PAGINATION" },
      });
    }
    next(error);
  }
};
