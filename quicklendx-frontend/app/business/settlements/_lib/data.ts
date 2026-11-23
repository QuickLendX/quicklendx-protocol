import { z } from "zod";
import { apiClient } from "@/app/lib/api-client";

const settlementSchema = z.object({
  id: z.string(),
  invoice_id: z.string(),
  amount: z.string().regex(/^-?\d+$/),
  payer: z.string(),
  recipient: z.string(),
  timestamp: z.number().int().nonnegative(),
  status: z.enum(["Pending", "Paid", "Defaulted"]),
});

const settlementsResponseSchema = z.object({
  data: z.array(settlementSchema),
  freshness: z.object({
    indexLagSeconds: z.number().nonnegative(),
  }),
});

export type Settlement = z.infer<typeof settlementSchema>;
export type SettlementsResponse = z.infer<typeof settlementsResponseSchema>;

export async function listSettlements(
  _params: Record<string, never> = {}
): Promise<SettlementsResponse> {
  const response = await apiClient.get<unknown>("/v1/settlements");
  return settlementsResponseSchema.parse(response);
}
