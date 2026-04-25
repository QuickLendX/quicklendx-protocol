import { z } from "zod";
import { getBidsQuerySchema } from "./shared";

export { getBidsQuerySchema };

export const createBidBodySchema = z.object({
  invoice_id: z.string().regex(/^0x[a-fA-F0-9]+$/, "Must be a valid hex string"),
  bid_amount: z.string().regex(/^[0-9]+$/, "Must be a positive numeric value as string"),
  expected_return: z.string().regex(/^[0-9]+$/, "Must be a positive numeric value as string"),
  expiration_timestamp: z.number().int().positive(),
});

export type CreateBidBody = z.infer<typeof createBidBodySchema>;
