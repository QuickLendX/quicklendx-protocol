import { z } from 'zod';

// ---------- Enums (must match backend/openapi.yaml) ----------
export const InvoiceStatus = z.enum([
  'Pending',
  'Verified',
  'Funded',
  'Paid',
  'Defaulted',
  'Cancelled',
]);
export type InvoiceStatus = z.infer<typeof InvoiceStatus>;

export const BidStatus = z.enum([
  'Placed',
  'Accepted',
  'Rejected',
  'Withdrawn',
]);
export type BidStatus = z.infer<typeof BidStatus>;

export const SettlementStatus = z.enum([
  'Pending',
  'Completed',
  'Failed',
]);
export type SettlementStatus = z.infer<typeof SettlementStatus>;

export const InvestmentStatus = z.enum([
  'Pending',
  'Active',
  'Closed',
]);
export type InvestmentStatus = z.infer<typeof InvestmentStatus>;

export const DisputeStatus = z.enum([
  'Open',
  'Resolved',
  'Cancelled',
]);
export type DisputeStatus = z.infer<typeof DisputeStatus>;

// ---------- Primitive validators ----------
export const HexId = z.string().regex(/^0x[0-9a-fA-F]+$/);
export type HexId = z.infer<typeof HexId>;

export const StellarAddress = z.string().regex(/^G[A-Z2-7]{55}$/);
export type StellarAddress = z.infer<typeof StellarAddress>;

export const I128String = z.string().regex(/^-?\d+$/);
export type I128String = z.infer<typeof I128String>;

// ---------- Component schemas ----------
export const FreshnessMetadata = z.object({
  lastIndexedLedger: z.number().int(),
  indexLagSeconds: z.number().int(),
  lastUpdatedAt: z.string().datetime(),
  cursor: z.string().optional(),
});
export type FreshnessMetadata = z.infer<typeof FreshnessMetadata>;

export const SystemStatus = z.object({
  status: z.string(),
  degraded: z.boolean(),
  lag: z.number().int(),
  level: z.string(),
});
export type SystemStatus = z.infer<typeof SystemStatus>;

export const Invoice = z.object({
  id: HexId,
  business: StellarAddress,
  amount: I128String,
  currency: z.string(),
  due_date: z.number().int(),
  status: InvoiceStatus,
  description: z.string().optional(),
  category: z.string().optional(),
  tags: z.array(z.string()).optional(),
  metadata: z.any().optional(),
  created_at: z.number().int(),
  updated_at: z.number().int(),
});
export type Invoice = z.infer<typeof Invoice>;

export const Bid = z.object({
  id: HexId,
  invoice_id: HexId,
  investor: StellarAddress,
  bid_amount: I128String,
  expected_return: I128String,
  status: BidStatus,
  timestamp: z.number().int(),
});
export type Bid = z.infer<typeof Bid>;

export const Settlement = z.object({
  id: HexId,
  invoice_id: HexId,
  amount: I128String,
  status: SettlementStatus,
  timestamp: z.number().int(),
});
export type Settlement = z.infer<typeof Settlement>;

export const PortfolioEntry = z.object({
  investor: StellarAddress,
  invoice_id: HexId,
  invested_amount: I128String,
  invested_at: z.number().int(),
});
export type PortfolioEntry = z.infer<typeof PortfolioEntry>;

export const Dispute = z.object({
  id: HexId,
  invoice_id: HexId,
  reason: z.string(),
  status: DisputeStatus,
  created_at: z.number().int(),
});
export type Dispute = z.infer<typeof Dispute>;

// ---------- Page generic ----------
export const Page = <T extends z.ZodTypeAny>(itemSchema: T) =>
  z.object({
    data: z.array(itemSchema),
    nextCursor: z.string().nullable(),
    hasMore: z.boolean(),
    freshness: FreshnessMetadata.optional(),
    system: SystemStatus.optional(),
  });

export type Page<T> = {
  data: T[];
  nextCursor: string | null;
  hasMore: boolean;
  freshness?: FreshnessMetadata;
  system?: SystemStatus;
};
