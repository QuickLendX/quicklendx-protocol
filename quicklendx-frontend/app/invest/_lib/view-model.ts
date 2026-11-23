export interface FreshnessMetadata {
  lastIndexedLedger: number;
  indexLagSeconds: number;
  lastUpdatedAt: string;
  cursor: string;
}

export type InvoiceStatus =
  | "Pending"
  | "Verified"
  | "Funded"
  | "Paid"
  | "Defaulted"
  | "Cancelled";

export type InvoiceCategory = string;

export interface InvoiceMetadata {
  customer_name: string;
  customer_address: string;
  tax_id: string;
  [key: string]: unknown;
}

export interface ApiInvoice {
  id: string;
  business: string;
  amount: string; // i128 as string
  currency: string;
  due_date: number; // Unix timestamp in seconds
  status: InvoiceStatus;
  description: string;
  category: InvoiceCategory;
  tags: string[];
  metadata?: InvoiceMetadata;
  created_at: number;
  updated_at: number;
}

export type BidStatus =
  | "Placed"
  | "Withdrawn"
  | "Accepted"
  | "Expired"
  | "Cancelled";

export interface Bid {
  bid_id: string;
  invoice_id: string;
  investor: string;
  bid_amount: string;
  expected_return: string;
  timestamp: number;
  status: BidStatus;
  expiration_timestamp: number;
}

declare const BrandSymbol: unique symbol;

export type PublicInvoice = {
  readonly [BrandSymbol]: "PublicInvoice";
  id: string;
  amount: string;
  currency: string;
  due_date: number;
  status: InvoiceStatus;
  description: string;
  category: InvoiceCategory;
  tags: string[];
  created_at: number;
};

/**
 * Strips sensitive counterparty / debtor metadata (customer_name, customer_address, tax_id)
 * and returns only the 9 allowed fields for marketplace display.
 */
export function toPublicInvoice(
  invoice: ApiInvoice | Record<string, unknown>
): PublicInvoice {
  const result = {
    id: String(invoice.id ?? ""),
    amount: String(invoice.amount ?? "0"),
    currency: String(invoice.currency ?? "XLM"),
    due_date: Number(invoice.due_date ?? 0),
    status: (invoice.status as InvoiceStatus) ?? "Verified",
    description: String(invoice.description ?? ""),
    category: String(invoice.category ?? "General"),
    tags: Array.isArray(invoice.tags) ? [...invoice.tags] : [],
    created_at: Number(invoice.created_at ?? 0),
  };

  return result as unknown as PublicInvoice;
}

/**
 * Truncates Stellar / counterparty public key addresses (e.g., GBSX...7C5K).
 */
export function truncateAddress(
  address: string,
  startLength = 4,
  endLength = 4
): string {
  if (!address || address.length <= startLength + endLength) {
    return address;
  }
  return `${address.slice(0, startLength)}…${address.slice(-endLength)}`;
}
