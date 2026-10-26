import { z } from "zod";
import { apiClient } from "@/app/lib/api-client";
import { BusinessLogicError } from "@/app/lib/errors";

export const SettlementStatusSchema = z.enum(["Pending", "Paid", "Defaulted"]);
export type SettlementStatus = z.infer<typeof SettlementStatusSchema>;

export const SettlementSchema = z.object({
  id: z.string(),
  invoice_id: z.string(),
  amount: z.string(), // i128 as string
  payer: z.string(),
  recipient: z.string(),
  stellar_tx_hash: z.string().optional(),
  timestamp: z.number(), // unix timestamp (seconds or ms)
  status: SettlementStatusSchema,
  contract_version: z.number().optional(),
  event_schema_version: z.number().optional(),
  indexed_at: z.string().optional(),
});
export type Settlement = z.infer<typeof SettlementSchema>;

export const InvoiceStatusSchema = z.enum([
  "Pending",
  "Verified",
  "Funded",
  "Paid",
  "Defaulted",
  "Cancelled",
]);
export type InvoiceStatus = z.infer<typeof InvoiceStatusSchema>;

export const InvoiceCategorySchema = z.string().optional();

export const InvoiceSchema = z.object({
  id: z.string(),
  business: z.string(),
  amount: z.string(), // i128 as string
  currency: z.string(),
  due_date: z.number(), // unix timestamp (seconds)
  status: InvoiceStatusSchema,
  description: z.string(),
  category: InvoiceCategorySchema,
  tags: z.array(z.string()).optional(),
  metadata: z.record(z.unknown()).optional(),
  created_at: z.number(),
  updated_at: z.number(),
});
export type Invoice = z.infer<typeof InvoiceSchema>;

export async function getSettlement(
  id: string,
  signal?: AbortSignal
): Promise<Settlement> {
  const data = await apiClient.get<unknown>(`/v1/settlements/${id}`, {
    signal,
  });
  if (!data) {
    throw new BusinessLogicError("Settlement not found");
  }
  return SettlementSchema.parse(data);
}

export async function getInvoice(
  id: string,
  signal?: AbortSignal
): Promise<Invoice> {
  const data = await apiClient.get<unknown>(`/v1/invoices/${id}`, { signal });
  if (!data) {
    throw new BusinessLogicError("Invoice not found");
  }
  return InvoiceSchema.parse(data);
}
