import { z } from "zod";
import {
  getInvoicesQuerySchema,
  invoiceIdParamSchema,
  getSettlementsQuerySchema,
  settlementIdParamSchema,
  invoiceIdParamForDisputesSchema,
} from "./shared";

export { getInvoicesQuerySchema, invoiceIdParamSchema, invoiceIdParamForDisputesSchema };

export const createInvoiceBodySchema = z.object({
  business: z.string().min(1),
  amount: z.string().regex(/^[0-9]+$/, "Must be a positive numeric value as string"),
  currency: z.string().min(1),
  due_date: z.number().int().positive(),
  description: z.string().max(500),
  category: z.enum(["Services", "Products", "Consulting", "Manufacturing", "Technology", "Healthcare", "Other"]),
  tags: z.array(z.string().max(50)).max(10).optional(),
  metadata: z.object({
    customer_name: z.string().max(200),
    customer_address: z.string().max(500),
    tax_id: z.string().max(50).optional(),
    line_items: z.array(z.object({
      description: z.string().max(200),
      quantity: z.string().regex(/^[0-9]+$/),
      unit_price: z.string().regex(/^[0-9]+$/),
      total: z.string().regex(/^[0-9]+$/),
    })).optional(),
    notes: z.string().max(1000).optional(),
  }).optional(),
});

export type CreateInvoiceBody = z.infer<typeof createInvoiceBodySchema>;
