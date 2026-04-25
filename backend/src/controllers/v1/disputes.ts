import { Request, Response, NextFunction } from "express";
import { Dispute, DisputeStatus } from "../../types/contract";
import { applyCacheHeaders, CC_NO_STORE } from "../../middleware/cache-headers";

const MOCK_DISPUTES: Dispute[] = [
  {
    id: "0xdispute1",
    invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    initiator: "GA...BUYER",
    reason: "Goods not delivered as per description",
    status: DisputeStatus.UnderReview,
    created_at: Math.floor(Date.now() / 1000) - 86400,
  },
];

export const getDisputes = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { id: invoice_id } = req.params;

    let filtered = [...MOCK_DISPUTES];
    if (invoice_id) {
      filtered = filtered.filter((d) => d.invoice_id === invoice_id);
    }

    // Disputes must never be served from cache: status has legal/compliance
    // implications and must always reflect the current on-chain state.
    applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body: filtered });
    res.json(filtered);
  } catch (error) {
    next(error);
  }
};
