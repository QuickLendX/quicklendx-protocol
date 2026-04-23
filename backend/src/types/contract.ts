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
