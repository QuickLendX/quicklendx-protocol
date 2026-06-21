import { Request, Response, NextFunction } from "express";
import { Dispute, DisputeStatus } from "../../types/contract";
import { freshnessService } from "../../services/freshnessService";
import { labelRecord } from "../../services/versioningService";
import { applyCacheHeaders, CC_NO_STORE } from "../../middleware/cache-headers";
import { parsePaginationParams, PaginationError, applyPagination } from "../../utils/pagination";

export const MOCK_DISPUTES: Dispute[] = [
  labelRecord<Omit<Dispute, "contract_version" | "event_schema_version" | "indexed_at">>({
    id: "0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
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
    const params = parsePaginationParams(req.query);
    const { id: invoice_id } = req.params;

    let filtered = [...MOCK_DISPUTES];
    if (invoice_id) {
      filtered = filtered.filter((d) => d.invoice_id === invoice_id);
    }

    const page = applyPagination(filtered, "created_at", params);
    const body = { data: page.data, next_cursor: page.next_cursor, has_more: page.has_more };
    applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body });
    res.json(body);
  } catch (error) {
    if (error instanceof PaginationError) {
      return res.status(400).json({ error: { message: error.message, code: "INVALID_PAGINATION" } });
    }
    next(error);
  }
};
