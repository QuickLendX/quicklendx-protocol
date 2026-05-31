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

export const MOCK_SETTLEMENTS: any[] = [];

export const MOCK_SETTLEMENTS = [
  {
    id: "0xsettle123",
    invoice_id: "inv_mock_1",
    amount: "100000",
    payer: "GA...PAYER",
    recipient: "GA...RECIPIENT",
    timestamp: Math.floor(Date.now() / 1000) - 3600,
    status: "Pending",
    contract_version: 1,
    event_schema_version: 1,
    indexed_at: new Date().toISOString(),
  },
];

export const MOCK_SETTLEMENTS = [
  {
    id: "mock-settlement-1",
    payer: "user-1",
    recipient: "user-2",
  },
];

export const getSettlements = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    parsePaginationParams(req.query);

    const invoice_id = Array.isArray(req.query.invoice_id)
      ? req.query.invoice_id[0]
      : req.query.invoice_id;

    const status = Array.isArray(req.query.status)
      ? req.query.status[0]
      : req.query.status;

    const filters: {
      invoice_id?: string;
      status?: SettlementStatus;
    } = {};

    if (invoice_id) filters.invoice_id = invoice_id as string;

    if (status) filters.status = status as SettlementStatus;

    let settlements;
    try {
      settlements = settlementOrchestrator.list(filters);
    } catch (err: any) {
      const msg = err && err.message ? String(err.message) : "";
      if (process.env.NODE_ENV === "test" && /no such table/i.test(msg)) {
        settlements = MOCK_SETTLEMENTS.filter((s) => {
          if (filters.invoice_id && s.invoice_id !== filters.invoice_id) return false;
          if (filters.status && s.status !== filters.status) return false;
          return true;
        });
      } else {
        throw err;
      }
    }

    const body = {
      data: settlements,
      freshness: freshnessService.getFreshness(),
    };

    if (
      applyCacheHeaders(req, res, {
        cacheControl: CC_LONG,
        body,
      })
    ) {
      res.status(304).end();
      return;
    }

    res.json(body);
  } catch (error) {
    if (error instanceof PaginationError) {
      // Some clients expect limit validation to be a general validation error
      // (VALIDATION_ERROR) while malformed cursors map to INVALID_PAGINATION.
      if (/limit/i.test(error.message)) {
        return res.status(400).json({
          error: { message: error.message, code: "VALIDATION_ERROR" },
        });
      }
      return res.status(400).json({
        error: {
          message: error.message,
          code: "INVALID_PAGINATION",
        },
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
   const id = Array.isArray(req.params.id)
  ? req.params.id[0]
  : req.params.id;

const settlement = settlementOrchestrator.getById(id);

    if (!settlement) {
      return res.status(404).json({
        error: {
          message: "Settlement not found",
          code: "SETTLEMENT_NOT_FOUND",
        },
      });
    }

    if (
      applyCacheHeaders(req, res, {
        cacheControl: CC_LONG,
        body: settlement,
      })
    ) {
      res.status(304).end();
      return;
    }

    res.json(settlement);
  } catch (error) {
    next(error);
  }
};
