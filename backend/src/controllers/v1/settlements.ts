import { Request, Response, NextFunction } from "express";
import { Settlement, SettlementStatus } from "../../types/contract";
import { labelRecord } from "../../services/versioningService";
import { freshnessService } from "../../services/freshnessService";

export const MOCK_SETTLEMENTS: Settlement[] = [
  labelRecord<Omit<Settlement, "contract_version" | "event_schema_version" | "indexed_at">>({
    id: "0xsettle123",
    invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    amount: "1000000000",
    payer: "GA...PAYER",
    recipient: "GB...RECIP",
    timestamp: Math.floor(Date.now() / 1000) - 43200,
    status: SettlementStatus.Paid,
  }),
];

export const getSettlements = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { invoice_id } = req.query;

    let filtered = [...MOCK_SETTLEMENTS];
    if (invoice_id) {
      filtered = filtered.filter((s) => s.invoice_id === invoice_id);
    }

    res.json({ data: filtered, freshness: freshnessService.getFreshness() });
  } catch (error) {
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
        error: {
          message: "Settlement not found",
          code: "SETTLEMENT_NOT_FOUND",
        },
      });
    }

    res.json({ data: settlement, freshness: freshnessService.getFreshness() });
  } catch (error) {
    next(error);
  }
};
