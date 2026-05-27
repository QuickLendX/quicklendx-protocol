import { Request, Response, NextFunction } from "express";
import { Settlement, SettlementStatus } from "../../types/contract";
import { applyCacheHeaders, CC_LONG } from "../../middleware/cache-headers";
import { labelRecord } from "../../services/versioningService";
import { freshnessService } from "../../services/freshnessService";
import { parsePaginationParams, PaginationError } from "../../utils/pagination";

export const MOCK_SETTLEMENTS: Settlement[] = [
  labelRecord<Omit<Settlement, "contract_version" | "event_schema_version" | "indexed_at">>({
    id: "0xsettle123",
    invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    amount: "1000000000",
    payer: "GPAYER000000000000000000000000000000000000000000000000",
    recipient: "GRECIP000000000000000000000000000000000000000000000000",
    timestamp: Math.floor(Date.now() / 1000) - 3600,
    status: SettlementStatus.Pending,
  }),
];

export const getSettlements = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    parsePaginationParams(req.query);
    const { invoice_id, status } = req.query;

    let settlements = [...MOCK_SETTLEMENTS];
    if (invoice_id) settlements = settlements.filter((s) => s.invoice_id === (invoice_id as string));
    if (status) settlements = settlements.filter((s) => s.status === (status as SettlementStatus));

    const body = { data: settlements, freshness: freshnessService.getFreshness() };
    if (applyCacheHeaders(req, res, { cacheControl: CC_LONG, body })) {
      res.status(304).end();
      return;
    }
    res.json(body);
  } catch (error) {
    if (error instanceof PaginationError) {
      return res.status(400).json({
        error: { message: error.message, code: "INVALID_PAGINATION" },
      });
    }
    next(error);
  }
};

export const getSettlementById = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const id = req.params.id as string;
    const settlement = MOCK_SETTLEMENTS.find((s) => s.id === id);

    if (!settlement) {
      return res.status(404).json({
        error: { message: "Settlement not found", code: "SETTLEMENT_NOT_FOUND" },
      });
    }

    if (applyCacheHeaders(req, res, { cacheControl: CC_LONG, body: settlement })) {
      res.status(304).end();
      return;
    }
    res.json(settlement);
  } catch (error) {
    next(error);
  }
};
