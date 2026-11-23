import { z } from "zod";
import type { InvoiceStatus } from "@/app/components/invoice-status";

// ---------------------------------------------------------------------------
// Zod Schemas (matching backend/openapi.yaml)
// ---------------------------------------------------------------------------

export const InvoiceStatusSchema = z.enum([
  "Pending",
  "Verified",
  "Funded",
  "Paid",
  "Defaulted",
  "Cancelled",
]);

export const InvoiceCategorySchema = z.enum([
  "Services",
  "Products",
  "Consulting",
  "Manufacturing",
  "Technology",
  "Healthcare",
  "Other",
]);

export const LineItemSchema = z.object({
  description: z.string(),
  quantity: z.string(),
  unit_price: z.string(),
  total: z.string(),
});

export const InvoiceMetadataSchema = z.object({
  customer_name: z.string(),
  customer_address: z.string(),
  tax_id: z.string().optional().default(""),
  line_items: z.array(LineItemSchema).optional().default([]),
  notes: z.string().optional().default(""),
});

export const InvoiceSchema = z.object({
  id: z.string(),
  business: z.string(),
  amount: z.string().regex(/^-?\d+$/, "Amount must be an integer string"),
  currency: z.string().optional().default("USDC"),
  due_date: z.number(),
  status: InvoiceStatusSchema,
  description: z.string().optional().default(""),
  category: InvoiceCategorySchema.optional().default("Other"),
  tags: z.array(z.string()).optional().default([]),
  metadata: InvoiceMetadataSchema.optional(),
  created_at: z.number(),
  updated_at: z.number(),
});

export const FreshnessMetadataSchema = z
  .object({
    lastIndexedLedger: z.number(),
    indexLagSeconds: z.number(),
    lastUpdatedAt: z.string(),
    cursor: z.string(),
  })
  .optional();

export const InvoiceListResponseSchema = z.object({
  data: z.array(InvoiceSchema),
  next_cursor: z.string().nullable(),
  has_more: z.boolean(),
  freshness: FreshnessMetadataSchema,
});

export const BidStatusSchema = z.enum([
  "Placed",
  "Withdrawn",
  "Accepted",
  "Expired",
  "Cancelled",
]);

export const BidSchema = z.object({
  bid_id: z.string(),
  invoice_id: z.string(),
  investor: z.string(),
  bid_amount: z
    .string()
    .regex(/^-?\d+$/, "Bid amount must be an integer string"),
  expected_return: z
    .string()
    .regex(/^-?\d+$/, "Expected return must be an integer string"),
  timestamp: z.number(),
  status: BidStatusSchema,
  expiration_timestamp: z.number(),
});

export const RankedBidsResponseSchema = z.object({
  data: z.array(BidSchema),
});

export const DisputeStatusSchema = z.enum([
  "None",
  "Disputed",
  "UnderReview",
  "Resolved",
]);

export const DisputeSchema = z.object({
  id: z.string(),
  invoice_id: z.string(),
  initiator: z.string(),
  reason: z.string(),
  status: DisputeStatusSchema,
  created_at: z.number(),
  resolved_at: z.number().optional(),
});

export const DisputesListResponseSchema = z.object({
  data: z.array(DisputeSchema),
  next_cursor: z.string().nullable().optional().default(null),
  has_more: z.boolean().optional().default(false),
});

export const SettlementStatusSchema = z.enum([
  "Pending",
  "Processing",
  "Paid",
  "Defaulted",
]);

export const SettlementSchema = z.object({
  id: z.string(),
  invoice_id: z.string(),
  amount: z
    .string()
    .regex(/^-?\d+$/, "Settlement amount must be an integer string"),
  payer: z.string(),
  recipient: z.string(),
  timestamp: z.number(),
  status: SettlementStatusSchema,
});

export const SettlementsListResponseSchema = z.object({
  data: z.array(SettlementSchema),
  freshness: FreshnessMetadataSchema,
});

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type Invoice = z.infer<typeof InvoiceSchema>;
export type InvoiceListResponse = z.infer<typeof InvoiceListResponseSchema>;
export type Bid = z.infer<typeof BidSchema>;
export type RankedBidsResponse = z.infer<typeof RankedBidsResponseSchema>;
export type Dispute = z.infer<typeof DisputeSchema>;
export type DisputesListResponse = z.infer<typeof DisputesListResponseSchema>;
export type Settlement = z.infer<typeof SettlementSchema>;
export type SettlementsListResponse = z.infer<
  typeof SettlementsListResponseSchema
>;

export interface ListInvoicesParams {
  business?: string;
  status?: InvoiceStatus;
  limit?: number;
  cursor?: string;
}

export interface ListSettlementsParams {
  invoice_id?: string;
  limit?: number;
  cursor?: string;
}

// ---------------------------------------------------------------------------
// HTTP Error with status code
// ---------------------------------------------------------------------------

export class ApiHttpError extends Error {
  public readonly status: number;
  public readonly code?: string;

  constructor(message: string, status: number, code?: string) {
    super(message);
    this.name = "ApiHttpError";
    this.status = status;
    this.code = code;
  }
}

export function isNotFoundError(err: unknown): boolean {
  if (!err || typeof err !== "object") return false;
  if (err instanceof ApiHttpError && err.status === 404) return true;
  const anyErr = err as {
    status?: number;
    statusCode?: number;
    code?: string;
    response?: { status?: number };
    message?: string;
  };
  if (
    anyErr.status === 404 ||
    anyErr.statusCode === 404 ||
    anyErr.response?.status === 404 ||
    anyErr.code === "INVOICE_NOT_FOUND" ||
    anyErr.code === "NOT_FOUND"
  ) {
    return true;
  }
  if (typeof anyErr.message === "string" && /\b404\b/i.test(anyErr.message)) {
    return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// Fetch Helper
// ---------------------------------------------------------------------------

function getBaseUrl(): string {
  const raw = process.env.NEXT_PUBLIC_API_URL || "http://localhost:3001/api/v1";
  return raw.replace(/\/+$/, "");
}

async function fetchJson(
  path: string,
  query?: Record<string, string | number | undefined>
): Promise<unknown> {
  const base = getBaseUrl();
  const searchParams = new URLSearchParams();

  if (query) {
    for (const [key, val] of Object.entries(query)) {
      if (val !== undefined && val !== "") {
        searchParams.set(key, String(val));
      }
    }
  }

  const qs = searchParams.toString();
  const url = `${base}${path}${qs ? `?${qs}` : ""}`;

  const response = await fetch(url, {
    method: "GET",
    headers: {
      Accept: "application/json",
    },
  });

  if (!response.ok) {
    let errorMessage = `HTTP ${response.status}`;
    let errorCode: string | undefined;
    try {
      const errBody = await response.json();
      errorMessage =
        errBody?.error?.message || errBody?.message || errorMessage;
      errorCode = errBody?.error?.code || errBody?.code;
    } catch {
      // Ignore JSON parse errors on error body
    }
    throw new ApiHttpError(errorMessage, response.status, errorCode);
  }

  return response.json();
}

// ---------------------------------------------------------------------------
// Validated Endpoint Fetchers
// ---------------------------------------------------------------------------

export async function listInvoices(
  params: ListInvoicesParams = {}
): Promise<InvoiceListResponse> {
  const raw = await fetchJson("/invoices", {
    business: params.business,
    status: params.status,
    limit: params.limit,
    cursor: params.cursor,
  });
  return InvoiceListResponseSchema.parse(raw);
}

export async function getInvoice(id: string): Promise<Invoice> {
  const raw = await fetchJson(`/invoices/${encodeURIComponent(id)}`);
  return InvoiceSchema.parse(raw);
}

export async function getRankedBids(
  invoiceId: string
): Promise<RankedBidsResponse> {
  const raw = await fetchJson(`/bids/${encodeURIComponent(invoiceId)}/ranked`);
  return RankedBidsResponseSchema.parse(raw);
}

export async function listInvoiceDisputes(
  id: string
): Promise<DisputesListResponse> {
  const raw = await fetchJson(`/invoices/${encodeURIComponent(id)}/disputes`);
  return DisputesListResponseSchema.parse(raw);
}

export async function listSettlements(
  params: ListSettlementsParams = {}
): Promise<SettlementsListResponse> {
  const raw = await fetchJson("/settlements", {
    invoice_id: params.invoice_id,
    limit: params.limit,
    cursor: params.cursor,
  });
  return SettlementsListResponseSchema.parse(raw);
}
