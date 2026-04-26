import { Request, Response, NextFunction } from "express";
import { Dispute, DisputeStatus } from "../../types/contract";
import { freshnessService } from "../../services/freshnessService";
import { labelRecord } from "../../services/versioningService";

const MOCK_DISPUTES: Dispute[] = [
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

    res.json({ data: filtered, freshness: freshnessService.getFreshness() });
  } catch (error) {
    next(error);
  }
};
