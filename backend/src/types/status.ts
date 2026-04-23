import { z } from "zod";

export const StatusSchema = z.object({
  status: z.enum(["operational", "degraded", "maintenance"]),
  maintenance: z.boolean(),
  degraded: z.boolean(),
  index_lag: z.number(),
  last_ledger: z.number(),
  timestamp: z.string().datetime(),
  version: z.string(),
});

export type StatusResponse = z.infer<typeof StatusSchema>;
