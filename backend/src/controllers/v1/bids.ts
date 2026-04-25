import { Request, Response, NextFunction } from "express";
import { Bid, BidStatus } from "../../types/contract";
import { SnapshotService } from "../../services/snapshotService";

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

    res.json(filtered);
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
    const bestBid = await SnapshotService.getBestBid(invoiceId);
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
    const topBids = await SnapshotService.getTopBids(invoiceId);
    res.json({ top_bids: topBids });
  } catch (error) {
    next(error);
  }
};
