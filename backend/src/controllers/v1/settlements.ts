import { Request, Response, NextFunction } from "express";
import { SettlementStatus } from "../../types/contract";
import { applyCacheHeaders, CC_LONG } from "../../middleware/cache-headers";
import { freshnessService } from "../../services/freshnessService";
import { parsePaginationParams, PaginationError } from "../../utils/pagination";
import { settlementOrchestrator } from "../../services/settlementOrchestrator";

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

    const settlements = settlementOrchestrator.list(filters);

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