import { Request, Response, NextFunction } from "express";
import { Invoice, InvoiceStatus, InvoiceCategory } from "../../types/contract";
import { freshnessService } from "../../services/freshnessService";

// Mock data aligned with contract types
const MOCK_INVOICES: Invoice[] = [
  {
    id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    business: "GDVLRH4G4...7Y",
    amount: "1000000000", // 100.00 USDC (assuming 7 decimals or whatever)
    currency: "CBGHS...ABC",
    due_date: Math.floor(Date.now() / 1000) + 86400 * 30,
    status: InvoiceStatus.Verified,
    description: "Cloud Services - March 2026",
    category: InvoiceCategory.Technology,
    tags: ["cloud", "saas"],
    metadata: {
      customer_name: "TechCorp Inc",
      customer_address: "123 Silicon Valley",
      tax_id: "TX-12345",
      line_items: [
        {
          description: "AWS Instance usage",
          quantity: "1",
          unit_price: "1000000000",
          total: "1000000000",
        },
      ],
      notes: "Monthly recurring billing",
    },
    created_at: Math.floor(Date.now() / 1000) - 86400,
    updated_at: Math.floor(Date.now() / 1000) - 86400,
  },
];

export const getInvoices = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const { business, status } = req.query;

    let filtered = [...MOCK_INVOICES];
    if (business) {
      filtered = filtered.filter((i) => i.business === business);
    }
    if (status) {
      filtered = filtered.filter((i) => i.status === status);
    }

    res.json({ data: filtered, freshness: freshnessService.getFreshness() });
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
    const invoice = MOCK_INVOICES.find((i) => i.id === id);

    if (!invoice) {
      return res.status(404).json({
        error: {
          message: "Invoice not found",
          code: "INVOICE_NOT_FOUND",
        },
      });
    }

    res.json({ data: invoice, freshness: freshnessService.getFreshness() });
  } catch (error) {
    next(error);
  }
};
