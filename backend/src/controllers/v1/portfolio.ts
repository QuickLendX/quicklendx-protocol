import { Request, Response, NextFunction } from "express";
import {
  parsePaginationParams,
  applyPagination,
  PaginationError,
} from "../../utils/pagination";

export interface PortfolioEntry {
  id: string;
  investor: string;
  invoice_id: string;
  invested_amount: string;
  expected_return: string;
  status: "Active" | "Completed" | "Defaulted" | "Refunded";
  invested_at: number;
}

export const MOCK_PORTFOLIO: PortfolioEntry[] = [
  {
    id: "0xport001",
    investor: "GA...ABC",
    invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    invested_amount: "950000000",
    expected_return: "50000000",
    status: "Active",
    invested_at: Math.floor(Date.now() / 1000) - 3600,
  },
];

export const getPortfolio = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const params = parsePaginationParams(req.query);
    const { investor } = req.query;

    if (!investor || typeof investor !== "string") {
      return res.status(400).json({
        error: { message: "investor query parameter is required", code: "MISSING_INVESTOR" },
      });
    }

    const filtered = MOCK_PORTFOLIO.filter((p) => p.investor === investor);
    const result = applyPagination(filtered, "invested_at", params);
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
