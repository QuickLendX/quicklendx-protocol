import { Request, Response, NextFunction } from "express";
import { Bid, BidStatus } from "../../types/contract";
import {
  parsePaginationParams,
  applyPagination,
  PaginationError,
} from "../../utils/pagination";

export const MOCK_BIDS: Bid[] = [
  {
    bid_id: "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
    invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    investor: "GA...ABC",
    bid_amount: "950000000",
    expected_return: "50000000",
    timestamp: Math.floor(Date.now() / 1000) - 3600,
    status: BidStatus.Placed,
    expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
  },
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

    const result = applyPagination(normalizeBids(filtered), "timestamp", params);
    // Strip the synthetic `id` field from response items
    res.json({
      ...result,
      data: result.data.map(({ id: _id, ...bid }) => bid),
    });
  } catch (error) {
    if (error instanceof PaginationError) {
      return res.status(400).json({
        error: { message: error.message, code: "INVALID_PAGINATION" },
      });
    }
    next(error);
  }
};
