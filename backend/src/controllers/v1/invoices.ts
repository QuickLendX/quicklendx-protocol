import { Request, Response, NextFunction } from "express";
import { Invoice, InvoiceStatus, InvoiceCategory } from "../../types/contract";
import { applyCacheHeaders, CC_SHORT } from "../../middleware/cache-headers";
import { freshnessService } from "../../services/freshnessService";
import { labelRecord } from "../../services/versioningService";

export const MOCK_INVOICES: Invoice[] = [
  labelRecord<Omit<Invoice, "contract_version" | "event_schema_version" | "indexed_at">>({
    id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    business: "GA...BIZ",
    amount: "1000000000",
    currency: "USDC",
    due_date: Math.floor(Date.now() / 1000) + 86400 * 30,
    status: InvoiceStatus.Pending,
    description: "Test invoice",
    category: InvoiceCategory.Services,
    tags: [],
    metadata: {
      customer_name: "Test Customer",
      customer_address: "123 Test St",
      tax_id: "TAX-001",
      line_items: [],
      notes: "",
    },
    created_at: Math.floor(Date.now() / 1000) - 3600,
    updated_at: Math.floor(Date.now() / 1000) - 3600,
  }),
];

export const getInvoices = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { business, status } = req.query;

    let filtered = [...MOCK_INVOICES];
    if (typeof business === "string") {
      filtered = filtered.filter((i) => i.business === business);
    }
    if (typeof status === "string") {
      filtered = filtered.filter((i) => i.status === (status as InvoiceStatus));
    }

    const body = { data: filtered, freshness: freshnessService.getFreshness() };
    if (applyCacheHeaders(req, res, { cacheControl: CC_SHORT, body })) {
      res.status(304).end();
      return;
    }
    res.json(body);
  } catch (error) {
    next(error);
  }
};

export const getInvoiceById = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const id = req.params.id as string;
    const invoice = MOCK_INVOICES.find((i) => i.id === id);

    if (!invoice) {
      return res.status(404).json({
        error: { message: "Invoice not found", code: "INVOICE_NOT_FOUND" },
      });
    }

    if (applyCacheHeaders(req, res, { cacheControl: CC_SHORT, body: invoice })) {
      res.status(304).end();
      return;
    }
    res.json(invoice);
  } catch (error) {
    next(error);
  }
};
