import { z } from "zod";
import { FreshnessMetadata } from "../../invest/_lib/view-model";
import { ApiHttpError } from "../../business/invoices/_lib/data";

export {
  ApiHttpError,
  isNotFoundError,
  listInvoices,
  getInvoice,
  getRankedBids,
  listInvoiceDisputes,
  listSettlements,
} from "../../business/invoices/_lib/data";
export type {
  Invoice,
  Bid,
  Dispute,
  Settlement,
  ListInvoicesParams,
  ListSettlementsParams,
} from "../../business/invoices/_lib/data";

export {
  ApiHttpError,
  getInvoice,
  isNotFoundError,
  listInvoices,
} from "../../business/invoices/_lib/data";
export type {
  Invoice,
  InvoiceListResponse,
  ListInvoicesParams,
} from "../../business/invoices/_lib/data";
import {
  AppError,
  ErrorCategory,
  ErrorSeverity,
  NetworkError,
} from "../errors";

export const DEFAULT_TIMEOUT_MS = 10_000;

function mapHttpStatusToErrorMeta(status: number): {
  category: ErrorCategory;
  severity: ErrorSeverity;
  code: string;
  retryable: boolean;
} {
  if (status === 401) {
    return {
      category: ErrorCategory.AUTHENTICATION,
      severity: ErrorSeverity.HIGH,
      code: "HTTP_401",
      retryable: true,
    };
  }
  if (status === 403) {
    return {
      category: ErrorCategory.AUTHORIZATION,
      severity: ErrorSeverity.HIGH,
      code: "HTTP_403",
      retryable: false,
    };
  }
  if (status === 404) {
    return {
      category: ErrorCategory.BUSINESS_LOGIC,
      severity: ErrorSeverity.MEDIUM,
      code: "HTTP_404",
      retryable: false,
    };
  }
  if (status === 429) {
    return {
      category: ErrorCategory.NETWORK,
      severity: ErrorSeverity.MEDIUM,
      code: "HTTP_429",
      retryable: true,
    };
  }
  if (status >= 500) {
    return {
      category: ErrorCategory.SYSTEM,
      severity: ErrorSeverity.CRITICAL,
      code: `HTTP_${status}`,
      retryable: true,
    };
  }
  return {
    category: ErrorCategory.VALIDATION,
    severity: ErrorSeverity.MEDIUM,
    code: `HTTP_${status}`,
    retryable: false,
  };
}

export class ApiHttpError extends AppError {
  public readonly status: number;
  public readonly body: unknown;
  public readonly retryAfter?: string;

  constructor(
    status: number,
    body: unknown,
    message?: string,
    retryAfter?: string
  ) {
    const meta = mapHttpStatusToErrorMeta(status);
    const defaultMessage =
      status === 429 && retryAfter
        ? `Rate limited (HTTP 429). Retry after ${retryAfter}`
        : `HTTP ${status}`;
    super(
      message ?? defaultMessage,
      meta.category,
      meta.severity,
      meta.code,
      { additionalData: { status, body, retryAfter } },
      meta.retryable
    );
    this.name = "ApiHttpError";
    this.status = status;
    this.body = body;
    this.retryAfter = retryAfter;
    Object.setPrototypeOf(this, ApiHttpError.prototype);
  }
}

export class ApiNetworkError extends NetworkError {
  public readonly isTimeout: boolean;
  public readonly isAborted: boolean;
  public readonly cause?: unknown;

  constructor(
    message: string,
    options?: { isTimeout?: boolean; isAborted?: boolean; cause?: unknown }
  ) {
    const isTimeout = Boolean(options?.isTimeout);
    const isAborted = Boolean(options?.isAborted);
    const code = isTimeout
      ? "REQUEST_TIMEOUT"
      : isAborted
        ? "REQUEST_ABORTED"
        : "NETWORK_ERROR";
    super(message, { additionalData: { isTimeout, isAborted } }, 0, code);
    this.name = "ApiNetworkError";
    this.isTimeout = isTimeout;
    this.isAborted = isAborted;
    this.cause = options?.cause;
    Object.setPrototypeOf(this, ApiNetworkError.prototype);
  }
}

export interface RequestOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
  allow404?: boolean;
  context?: string;
}

async function parseErrorBody(res: Response): Promise<unknown> {
  try {
    const text = await res.text();
    if (!text) return null;
    try {
      return JSON.parse(text);
    } catch {
      return text;
    }
  } catch {
    return null;
  }
}

export async function request(
  url: string,
  options?: RequestOptions
): Promise<Response | null> {
  const timeoutMs = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const externalSignal = options?.signal;
  const controller = new AbortController();

  let timedOut = false;
  let onExternalAbort: (() => void) | undefined;

  if (externalSignal) {
    if (externalSignal.aborted) {
      throw new ApiNetworkError("Request was aborted", { isAborted: true });
    }
    onExternalAbort = () => controller.abort();
    externalSignal.addEventListener("abort", onExternalAbort, { once: true });
  }

  const timeoutId = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, timeoutMs);

  try {
    const res = await fetch(url, { signal: controller.signal });

    if (options?.allow404 && res.status === 404) {
      return null;
    }

    if (!res.ok) {
      const body = await parseErrorBody(res);
      const retryAfter =
        res.headers?.get?.("Retry-After") ??
        res.headers?.get?.("retry-after") ??
        undefined;
      const prefix = options?.context ? `${options.context}: ` : "";
      const message =
        res.status === 429
          ? retryAfter
            ? `${prefix}Too many requests (HTTP 429). Retry after ${retryAfter}`
            : `${prefix}Too many requests (HTTP 429)`
          : `${prefix}HTTP ${res.status}`;
      throw new ApiHttpError(res.status, body, message, retryAfter);
    }

    return res;
  } catch (err) {
    if (err instanceof ApiHttpError || err instanceof ApiNetworkError) {
      throw err;
    }
    if (timedOut) {
      throw new ApiNetworkError(`Request timed out after ${timeoutMs}ms`, {
        isTimeout: true,
        cause: err,
      });
    }
    if (
      externalSignal?.aborted ||
      (err instanceof Error && err.name === "AbortError")
    ) {
      throw new ApiNetworkError("Request was aborted", {
        isAborted: true,
        cause: err,
      });
    }
    throw new ApiNetworkError(
      err instanceof Error ? err.message : "Network request failed",
      { cause: err }
    );
  } finally {
    clearTimeout(timeoutId);
    if (externalSignal && onExternalAbort) {
      externalSignal.removeEventListener("abort", onExternalAbort);
    }
  }
}

const FreshnessSchema = z.object({
  lastIndexedLedger: z.number().int(),
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
  bid_amount: z.string().regex(/^-?\d+$/, "Bid amount must be an integer string"),
  expected_return: z
    .string()
    .regex(/^-?\d+$/, "Expected return must be an integer string"),
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
  created_at: z.number().int(),
  resolved_at: z.number().int().optional(),
  contract_version: z.number().int().optional(),
  event_schema_version: z.number().int().optional(),
  indexed_at: z.string().optional(),
});

export const DisputesListResponseSchema = z.object({
  data: z.array(DisputeSchema),
  next_cursor: z.string().nullable(),
  has_more: z.boolean(),
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
  amount: z.string().regex(/^-?\d+$/, "Settlement amount must be an integer string"),
  payer: z.string(),
  recipient: z.string(),
  timestamp: z.number().int(),
  status: SettlementStatusSchema,
  contract_version: z.number().int().optional(),
  event_schema_version: z.number().int().optional(),
  indexed_at: z.string().optional(),
});

export const SettlementsListResponseSchema = z.object({
  data: z.array(SettlementSchema),
  next_cursor: z.string().nullable(),
  has_more: z.boolean(),
  freshness: FreshnessSchema.optional(),
});

export type Bid = z.infer<typeof BidSchema>;
export type RankedBidsResponse = z.infer<typeof RankedBidsResponseSchema>;
export type Dispute = z.infer<typeof DisputeSchema>;
export type DisputesListResponse = z.infer<typeof DisputesListResponseSchema>;
export type Settlement = z.infer<typeof SettlementSchema>;
export type SettlementsListResponse = z.infer<
  typeof SettlementsListResponseSchema
>;

const API_BASE_URL =
  process.env.NEXT_PUBLIC_API_URL || "http://localhost:3001/api/v1";

function addQueryParams(
  url: string,
  params?: Record<string, string | number | null | undefined>
): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params ?? {})) {
    if (value !== undefined && value !== null) {
      query.set(key, String(value));
    }
  }
  const queryString = query.toString();
  return queryString ? `${url}?${queryString}` : url;
}

export async function fetchInvoices(params?: {
  status?: string;
  cursor?: string | null;
  limit?: number;
  signal?: AbortSignal;
  timeoutMs?: number;
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
    throw new ApiHttpError(
      `Failed to fetch invoices: HTTP ${res.status}`,
      res.status
    );
  }
  const json = await res.json();
  const res = await request(url, {
    signal: params?.signal,
    timeoutMs: params?.timeoutMs,
    context: "Failed to fetch invoices",
  });
  const json = await res!.json();
  const parsed = InvoicesResponseSchema.parse(json);

  return {
    data: parsed.data,
    next_cursor: parsed.next_cursor ?? null,
    has_more: Boolean(parsed.has_more ?? parsed.next_cursor),
    freshness: parsed.freshness,
  };
}

export async function fetchInvoiceById(
  id: string,
  options?: { signal?: AbortSignal; timeoutMs?: number }
): Promise<z.infer<typeof ApiInvoiceSchema>> {
  const url = `${API_BASE_URL}/invoices/${encodeURIComponent(id)}`;
  const res = await fetch(url);
  if (!res.ok) {
    throw new ApiHttpError(
      `Failed to fetch invoice ${id}: HTTP ${res.status}`,
      res.status
    );
  }
  const json = await res.json();
  const res = await request(url, {
    signal: options?.signal,
    timeoutMs: options?.timeoutMs,
    context: `Failed to fetch invoice ${id}`,
  });
  const json = await res!.json();
  // Handle direct object or { data: ... }
  const rawData =
    json && typeof json === "object" && "data" in json && !("id" in json)
      ? json.data
      : json;
  return ApiInvoiceSchema.parse(rawData);
}

export async function fetchBestBid(
  invoiceId: string,
  options?: { signal?: AbortSignal; timeoutMs?: number }
): Promise<z.infer<typeof BidSchema> | null> {
  const url = `${API_BASE_URL}/bids/${encodeURIComponent(invoiceId)}/best`;
  const res = await request(url, {
    signal: options?.signal,
    timeoutMs: options?.timeoutMs,
    allow404: true,
    context: `Failed to fetch best bid for ${invoiceId}`,
  });
  if (!res) {
    return null;
  }
  const json = await res.json();
  const parsed = BestBidResponseSchema.parse(json);
  return parsed.data;
}

export async function fetchRankedBids(
  invoiceId: string,
  options?: { signal?: AbortSignal; timeoutMs?: number }
): Promise<z.infer<typeof BidSchema>[]> {
  const url = `${API_BASE_URL}/bids/${encodeURIComponent(invoiceId)}/ranked`;
  const res = await request(url, {
    signal: options?.signal,
    timeoutMs: options?.timeoutMs,
    allow404: true,
    context: `Failed to fetch ranked bids for ${invoiceId}`,
  });
  if (!res) {
    return [];
  }
  const json = await res.json();
  const parsed = RankedBidsResponseSchema.parse(json);
  return parsed.data;
}

export async function getRankedBids(
  invoiceId: string
): Promise<RankedBidsResponse> {
  const url = `${API_BASE_URL}/bids/${encodeURIComponent(invoiceId)}/ranked`;
  const res = await fetch(url);
  if (res.status === 404) {
    return { data: [] };
  }
  if (!res.ok) {
    throw new Error(
      `Failed to fetch ranked bids for ${invoiceId}: HTTP ${res.status}`
    );
  }
  return RankedBidsResponseSchema.parse(await res.json());
}

export async function listInvoiceDisputes(
  invoiceId: string,
  params?: { cursor?: string | null; limit?: number }
): Promise<DisputesListResponse> {
  const baseUrl = `${API_BASE_URL}/invoices/${encodeURIComponent(invoiceId)}/disputes`;
  const url = addQueryParams(baseUrl, params);
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(
      `Failed to fetch disputes for ${invoiceId}: HTTP ${res.status}`
    );
  }
  return DisputesListResponseSchema.parse(await res.json());
}

export async function listSettlements(params?: {
  invoice_id?: string;
  cursor?: string | null;
  limit?: number;
}): Promise<SettlementsListResponse> {
  const url = addQueryParams(`${API_BASE_URL}/settlements`, params);
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Failed to fetch settlements: HTTP ${res.status}`);
  }
  return SettlementsListResponseSchema.parse(await res.json());
}
