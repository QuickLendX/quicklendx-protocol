import { Request, Response, NextFunction } from "express";
import { Dispute, DisputeStatus } from "../../types/contract";
import { freshnessService } from "../../services/freshnessService";
import { labelRecord } from "../../services/versioningService";
import { applyCacheHeaders, CC_NO_STORE } from "../../middleware/cache-headers";

export const MOCK_DISPUTES: Dispute[] = [
  labelRecord<Omit<Dispute, "contract_version" | "event_schema_version" | "indexed_at">>({
    id: "0xdispute1",
    invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    initiator: "GA...BUYER",
    reason: "Goods not delivered as per description",
    status: DisputeStatus.UnderReview,
    created_at: Math.floor(Date.now() / 1000) - 86400,
  }),
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

    const body = { data: filtered };
    applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body });
    res.json(body);
  } catch (error) {
    next(error);
  }
};
