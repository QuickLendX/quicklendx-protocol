import { Request, Response, NextFunction } from "express";
import { InvoiceStatus } from "../../types/contract";
import { applyCacheHeaders, CC_SHORT } from "../../middleware/cache-headers";
import { freshnessService } from "../../services/freshnessService";
import { invoiceStore } from "../../services/invoiceStore";
export const MOCK_INVOICES = [
  {
    id: "mock-invoice-1",
  },
];

export const getInvoices = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { business, status } = req.query;

    const filter: { business?: string; status?: InvoiceStatus } = {};
    if (typeof business === 'string') {
      filter.business = business;
    }
    if (typeof status === 'string') {
      filter.status = status as InvoiceStatus;
    }

    const filtered = invoiceStore.findInvoices(filter);

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
    const { id } = req.params;
    const invoice = invoiceStore.findInvoiceById(id as string);

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
