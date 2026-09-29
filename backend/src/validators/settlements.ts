import { z } from "zod";
import {
  getSettlementsQuerySchema as sharedQuerySchema,
  settlementIdParamSchema,
} from "./shared";

export const getSettlementsQuerySchema = sharedQuerySchema.extend({
  status: z.enum(["Pending", "Processing", "Paid", "Defaulted"]).optional(),
});

export const transitionInputSchema = z.object({
  invoice_id: z.string().min(1),
  amount: z.string().regex(/^[0-9]+$/, "Must be a positive numeric value"),
  payer: z.string().min(1),
  recipient: z.string().min(1),
  event_id: z.string().min(1),
});

export { settlementIdParamSchema };
