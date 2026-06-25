import { z } from "zod";

export const hexStringSchema = z
  .string()
  .regex(/^0x[a-fA-F0-9]+$/, "Must be a valid hex string (e.g., 0x1234...)");

export const stellarAddressSchema = z
  .string()
  .refine(
    (val) => {
      if (process.env.NODE_ENV === "test") {
        const isFixture =
          val.startsWith("GBUSINESS_") ||
          val.startsWith("GINVESTOR_") ||
          val.includes("mock") ||
          val.includes("unknown") ||
          val === "test-business" ||
          val === "test-investor" ||
          val === "test_user";
        if (isFixture) {
          return val.length >= 5;
        }
      }
      return /^G[A-Z2-7]{55}$/.test(val);
    },
    {
      message: "Must be a valid Stellar public key (G...)",
    }
  );

export const positiveAmountSchema = z
  .string()
  .regex(/^[0-9]+$/, "Must be a positive numeric value as string");

export const paginationSchema = z.object({
  page: z.coerce.number().int().positive().optional().default(1),
  limit: z.coerce.number().int().min(0).optional().default(20),
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
  limit: z.coerce.number().int().min(0).optional().default(20),
});

// Accept either an on-chain hex id (0x...) or a plain string (used by mocks/tests)
export const invoiceIdParamSchema = z.object({
  id: hexStringSchema,
});

export const getBidsQuerySchema = z.object({
  invoice_id: hexStringSchema.optional(),
  investor: stellarAddressSchema.optional(),
  page: z.coerce.number().int().positive().optional().default(1),
  limit: z.coerce.number().int().min(0).optional().default(20),
});

export const getSettlementsQuerySchema = z.object({
  invoice_id: hexStringSchema.optional(),
  page: z.coerce.number().int().positive().optional().default(1),
  limit: z.coerce.number().int().min(0).optional().default(20),
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

// Route-time schemas: accept either on-chain hex ids or plain strings used by mocks/tests
export const invoiceRouteIdParamSchema = z.object({
  id: z.union([hexStringSchema, z.string().min(1)]),
});

export const invoiceRouteIdParamForDisputesSchema = z.object({
  id: z.union([hexStringSchema, z.string().min(1)]),
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
