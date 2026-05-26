import { z } from "zod";

export const hexStringSchema = z
  .string()
  .regex(/^0x[a-fA-F0-9]+$/, "Must be a valid hex string (e.g., 0x1234...)");

export const stellarAddressSchema = z
  .string()
  .regex(/^G[A-Z2-7]{55}$/, "Must be a valid Stellar public key (G...)");

export const positiveAmountSchema = z
  .string()
  .regex(/^[0-9]+$/, "Must be a positive numeric value as string");

export const paginationSchema = z.object({
  page: z.coerce.number().int().positive().optional().default(1),
  limit: z.coerce.number().int().min(1).max(100).optional().default(20),
});

export const invoiceIdSchema = z.object({
  invoice_id: hexStringSchema.optional(),
});

export const businessFilterSchema = z.object({
  business: stellarAddressSchema.optional(),
});

export const statusFilterSchema = z.object({
  status: z.enum(["Pending", "Verified", "Funded", "Paid", "Defaulted", "Cancelled"]).optional(),
});

export const getInvoicesQuerySchema = z.object({
  business: stellarAddressSchema.optional(),
  status: z.enum(["Pending", "Verified", "Funded", "Paid", "Defaulted", "Cancelled"]).optional(),
  page: z.coerce.number().int().positive().optional().default(1),
  limit: z.coerce.number().int().min(1).max(100).optional().default(20),
});

export const invoiceIdParamSchema = z.object({
  id: hexStringSchema,
});

export const getBidsQuerySchema = z.object({
  invoice_id: hexStringSchema.optional(),
  investor: stellarAddressSchema.optional(),
  page: z.coerce.number().int().positive().optional().default(1),
  limit: z.coerce.number().int().min(1).max(100).optional().default(20),
});

export const getSettlementsQuerySchema = z.object({
  invoice_id: hexStringSchema.optional(),
  page: z.coerce.number().int().positive().optional().default(1),
  limit: z.coerce.number().int().min(1).max(100).optional().default(20),
});

export const settlementIdParamSchema = z.object({
  id: z.union([hexStringSchema, z.string().startsWith("0x")]),
});

export const disputeIdParamSchema = z.object({
  id: hexStringSchema,
});

export const invoiceIdParamForDisputesSchema = z.object({
  id: hexStringSchema,
});

export type HexString = z.infer<typeof hexStringSchema>;
export type StellarAddress = z.infer<typeof stellarAddressSchema>;
export type PositiveAmount = z.infer<typeof positiveAmountSchema>;
export type Pagination = z.infer<typeof paginationSchema>;
export type GetInvoicesQuery = z.infer<typeof getInvoicesQuerySchema>;
export type InvoiceIdParam = z.infer<typeof invoiceIdParamSchema>;
export type GetBidsQuery = z.infer<typeof getBidsQuerySchema>;
export type GetSettlementsQuery = z.infer<typeof getSettlementsQuerySchema>;
export type SettlementIdParam = z.infer<typeof settlementIdParamSchema>;
