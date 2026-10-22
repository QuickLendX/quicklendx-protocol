import { z } from "zod";
import { FreshnessMetadata } from "../../invest/_lib/view-model";

const FreshnessSchema = z.object({
  lastIndexedLedger: z.number(),
  indexLagSeconds: z.number(),
  lastUpdatedAt: z.string(),
  cursor: z.string(),
});

export const ApiInvoiceSchema = z.object({
  id: z.string(),
  business: z.string().optional().default(""),
  amount: z.string(),
  currency: z.string().optional().default("XLM"),
  due_date: z.number(),
  status: z.enum([
    "Pending",
    "Verified",
    "Funded",
    "Paid",
    "Defaulted",
    "Cancelled",
  ]),
  description: z.string().optional().default(""),
  category: z.string().optional().default("General"),
  tags: z.array(z.string()).optional().default([]),
  metadata: z
    .object({
      customer_name: z.string().optional(),
      customer_address: z.string().optional(),
      tax_id: z.string().optional(),
    })
    .passthrough()
    .optional(),
  created_at: z.number().optional().default(0),
  updated_at: z.number().optional().default(0),
});

export const InvoicesResponseSchema = z.object({
  data: z.array(ApiInvoiceSchema),
  next_cursor: z.string().nullable().optional(),
  has_more: z.boolean().optional(),
  freshness: FreshnessSchema.optional(),
});

export const BidSchema = z.object({
  bid_id: z.string(),
  invoice_id: z.string(),
  investor: z.string(),
  bid_amount: z.string(),
  expected_return: z.string(),
  timestamp: z.number(),
  status: z.enum(["Placed", "Withdrawn", "Accepted", "Expired", "Cancelled"]),
  expiration_timestamp: z.number(),
});

export const BestBidResponseSchema = z.object({
  data: BidSchema,
});

export const RankedBidsResponseSchema = z.object({
  data: z.array(BidSchema),
});

const API_BASE_URL =
  process.env.NEXT_PUBLIC_API_URL || "http://localhost:3001/api";

export async function fetchInvoices(params?: {
  status?: string;
  cursor?: string | null;
  limit?: number;
}): Promise<{
  data: z.infer<typeof ApiInvoiceSchema>[];
  next_cursor: string | null;
  has_more: boolean;
  freshness?: FreshnessMetadata;
}> {
  const query = new URLSearchParams();
  if (params?.status) query.set("status", params.status);
  if (params?.cursor) query.set("cursor", params.cursor);
  if (params?.limit) query.set("limit", String(params.limit));

  const url = `${API_BASE_URL}/invoices?${query.toString()}`;
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Failed to fetch invoices: HTTP ${res.status}`);
  }
  const json = await res.json();
  const parsed = InvoicesResponseSchema.parse(json);

  return {
    data: parsed.data,
    next_cursor: parsed.next_cursor ?? null,
    has_more: Boolean(parsed.has_more ?? parsed.next_cursor),
    freshness: parsed.freshness,
  };
}

export async function fetchInvoiceById(
  id: string
): Promise<z.infer<typeof ApiInvoiceSchema>> {
  const url = `${API_BASE_URL}/invoices/${encodeURIComponent(id)}`;
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Failed to fetch invoice ${id}: HTTP ${res.status}`);
  }
  const json = await res.json();
  // Handle direct object or { data: ... }
  const rawData =
    json && typeof json === "object" && "data" in json && !("id" in json)
      ? json.data
      : json;
  return ApiInvoiceSchema.parse(rawData);
}

export async function fetchBestBid(
  invoiceId: string
): Promise<z.infer<typeof BidSchema> | null> {
  const url = `${API_BASE_URL}/bids/${encodeURIComponent(invoiceId)}/best`;
  const res = await fetch(url);
  if (res.status === 404) {
    return null;
  }
  if (!res.ok) {
    throw new Error(
      `Failed to fetch best bid for ${invoiceId}: HTTP ${res.status}`
    );
  }
  const json = await res.json();
  const parsed = BestBidResponseSchema.parse(json);
  return parsed.data;
}

export async function fetchRankedBids(
  invoiceId: string
): Promise<z.infer<typeof BidSchema>[]> {
  const url = `${API_BASE_URL}/bids/${encodeURIComponent(invoiceId)}/ranked`;
  const res = await fetch(url);
  if (res.status === 404) {
    return [];
  }
  if (!res.ok) {
    throw new Error(
      `Failed to fetch ranked bids for ${invoiceId}: HTTP ${res.status}`
    );
  }
  const json = await res.json();
  const parsed = RankedBidsResponseSchema.parse(json);
  return parsed.data;
}
