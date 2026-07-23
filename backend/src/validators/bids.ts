import { z } from "zod";
import { getBidsQuerySchema } from "./shared";

export { getBidsQuerySchema };

/**
 * Schema for the POST /api/v1/bids body.
 *
 * Validation responsibilities are split between this schema (structural /
 * shape validation) and the exposureService (policy / business validation):
 *
 *  • This schema enforces the wire-format rules: hex invoice IDs, positive
 *    integer-string amounts, positive integer expiration timestamps.
 *  • The exposureService, invoked from the controller, enforces the
 *    per-investor exposure cap (sum of active bids + unsettled positions),
 *    returning 429 EXPOSURE_CAP_EXCEEDED when the cap would be violated.
 *
 * The two layers compose cleanly:
 *   POST /bids → schema.parse  → exposureService.assertWithinCap → bidStore.createBid
 */
export const createBidBodySchema = z.object({
  invoice_id: z.string().regex(/^0x[a-fA-F0-9]+$/, "Must be a valid hex string"),
  bid_amount: z.string().regex(/^[0-9]+$/, "Must be a positive numeric value as string"),
  expected_return: z.string().regex(/^[0-9]+$/, "Must be a positive numeric value as string"),
  expiration_timestamp: z.number().int().positive(),
  /**
   * Optional currency tag. Defaults to USDC when absent (matches the
   * exposureService default and the legacy MOCK_BIDS shape). Must be a
   * 3–4 letter code; anything else is rejected at the schema layer so the
   * controller never has to second-guess the value.
   */
  currency: z.string().regex(/^[A-Za-z]{3,4}$/, "Must be a 3–4 letter currency code").optional(),
});

export type CreateBidBody = z.infer<typeof createBidBodySchema>;
