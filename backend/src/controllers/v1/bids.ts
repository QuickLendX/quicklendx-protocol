import { Request, Response, NextFunction } from "express";
import { Bid, BidStatus } from "../../types/contract";
import { freshnessService } from "../../services/freshnessService";

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

    res.json({ data: filtered, freshness: freshnessService.getFreshness() });
  } catch (error) {
    next(error);
  }
};
