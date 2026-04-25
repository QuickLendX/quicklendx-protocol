/**
 * Freshness metadata attached to every API response.
 * Clients use this to determine whether displayed data is near-real-time or lagging.
 *
 * Security: only public ledger data is exposed — no node addresses, validator
 * identities, or network topology.
 */
export interface FreshnessMetadata {
  /** Last ledger sequence number processed by the indexer. */
  lastIndexedLedger: number;
  /** Seconds between the indexed ledger's close time and now. 0 when current. */
  indexLagSeconds: number;
  /** ISO 8601 UTC timestamp of the indexed ledger close time. */
  lastUpdatedAt: string;
  /** Opaque pagination/replay cursor. Do not parse — treat as a black box. */
  cursor: string;
}

/** Generic response wrapper pairing any payload with freshness metadata. */
export interface ApiResponse<T> {
  data: T;
  freshness: FreshnessMetadata;
}

export enum InvoiceStatus {
  Pending = "Pending",
  Verified = "Verified",
  Funded = "Funded",
  Paid = "Paid",
  Defaulted = "Defaulted",
  Cancelled = "Cancelled",
}

export enum BidStatus {
  Placed = "Placed",
  Withdrawn = "Withdrawn",
  Accepted = "Accepted",
  Expired = "Expired",
  Cancelled = "Cancelled",
}

export enum InvestmentStatus {
  Active = "Active",
  Withdrawn = "Withdrawn",
  Completed = "Completed",
  Defaulted = "Defaulted",
  Refunded = "Refunded",
}

export enum DisputeStatus {
  None = "None",
  Disputed = "Disputed",
  UnderReview = "UnderReview",
  Resolved = "Resolved",
}

export enum SettlementStatus {
  Pending = "Pending",
  Paid = "Paid",
  Defaulted = "Defaulted",
}

export enum InvoiceCategory {
  Services = "Services",
  Products = "Products",
  Consulting = "Consulting",
  Manufacturing = "Manufacturing",
  Technology = "Technology",
  Healthcare = "Healthcare",
  Other = "Other",
}

export interface LineItem {
  description: string;
  quantity: string; // i128 represented as string for JSON safety
  unit_price: string;
  total: string;
}

export interface InvoiceMetadata {
  customer_name: string;
  customer_address: string;
  tax_id: string;
  line_items: LineItem[];
  notes: string;
}

export interface Invoice {
  id: string;
  business: string;
  amount: string;
  currency: string;
  due_date: number;
  status: InvoiceStatus;
  description: string;
  category: InvoiceCategory;
  tags: string[];
  metadata: InvoiceMetadata;
  created_at: number;
  updated_at: number;
}

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

export interface Settlement {
  id: string;
  invoice_id: string;
  amount: string;
  payer: string;
  recipient: string;
  timestamp: number;
  status: SettlementStatus;
}

export interface Dispute {
  id: string;
  invoice_id: string;
  initiator: string;
  reason: string;
  status: DisputeStatus;
  created_at: number;
  resolved_at?: number;
}
