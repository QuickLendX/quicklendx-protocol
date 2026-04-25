import { Request, Response, NextFunction } from "express";
import { Settlement, SettlementStatus } from "../../types/contract";
import {
  parsePaginationParams,
  applyPagination,
  PaginationError,
} from "../../utils/pagination";

export const MOCK_SETTLEMENTS: Settlement[] = [
  {
    id: "0xsettle123",
    invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    amount: "1000000000",
    payer: "GA...PAYER",
    recipient: "GB...RECIP",
    timestamp: Math.floor(Date.now() / 1000) - 43200,
    status: SettlementStatus.Paid,
  },
];

export const getSettlements = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const params = parsePaginationParams(req.query);
    const { invoice_id } = req.query;

    let filtered = [...MOCK_SETTLEMENTS];
    if (invoice_id) filtered = filtered.filter((s) => s.invoice_id === invoice_id);

    const result = applyPagination(filtered, "timestamp", params);
    res.json(result);
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
    const { id } = req.params;
    const settlement = MOCK_SETTLEMENTS.find((s) => s.id === id);

    if (!settlement) {
      return res.status(404).json({
        error: { message: "Settlement not found", code: "SETTLEMENT_NOT_FOUND" },
      });
    }

    res.json(settlement);
  } catch (error) {
    next(error);
  }
};
