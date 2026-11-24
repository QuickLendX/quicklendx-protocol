import { z } from "zod";

export const INVOICE_CATEGORIES = [
  "Services",
  "Products",
  "Consulting",
  "Manufacturing",
  "Technology",
  "Healthcare",
  "Other",
] as const;

export const LIMITS = {
  description: 500,
  tag: 50,
  tags: 10,
  customerName: 200,
  customerAddress: 500,
  taxId: 50,
  lineItemDescription: 200,
  notes: 1000,
} as const;

const digits = /^[0-9]+$/;

export const createInvoiceBodySchema = z.object({
  business: z.string().min(1),
  amount: z
    .string()
    .regex(digits, "Must be a positive numeric value as string"),
  currency: z.string().min(1),
  due_date: z.number().int().positive(),
  description: z.string().max(LIMITS.description),
  category: z.enum(INVOICE_CATEGORIES),
  tags: z.array(z.string().max(LIMITS.tag)).max(LIMITS.tags).optional(),
  metadata: z
    .object({
      customer_name: z.string().max(LIMITS.customerName),
      customer_address: z.string().max(LIMITS.customerAddress),
      tax_id: z.string().max(LIMITS.taxId).optional(),
      line_items: z
        .array(
          z.object({
            description: z.string().max(LIMITS.lineItemDescription),
            quantity: z.string().regex(digits),
            unit_price: z.string().regex(digits),
            total: z.string().regex(digits),
          })
        )
        .optional(),
      notes: z.string().max(LIMITS.notes).optional(),
    })
    .optional(),
});

export type CreateInvoiceBody = z.infer<typeof createInvoiceBodySchema>;
