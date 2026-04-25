import { Request, Response, NextFunction } from "express";
import { Bid, BidStatus } from "../../types/contract";
import { applyCacheHeaders, CC_NO_STORE } from "../../middleware/cache-headers";

const MOCK_BIDS: Bid[] = [
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

export const getBids = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { invoice_id, investor } = req.query;

    let filtered = [...MOCK_BIDS];
    if (invoice_id) {
      filtered = filtered.filter((b) => b.invoice_id === invoice_id);
    }
    if (investor) {
      filtered = filtered.filter((b) => b.investor === investor);
    }

    // Bids must never be served from cache: the best-bid amount changes with
    // every new placement and serving stale data could mislead investors.
    applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body: filtered });
    res.json(filtered);
  } catch (error) {
    next(error);
  }
};
