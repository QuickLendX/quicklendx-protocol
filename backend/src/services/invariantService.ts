import { z } from "zod";
import { Invoice, Bid, Settlement, Dispute, BidStatus, SettlementStatus } from "../types/contract";

const MAX_SAMPLE_IDS = 5;

// ── Schemas ───────────────────────────────────────────────────────────────────

export const InvariantCounterSchema = z.object({
  count: z.number().int().min(0),
  sampleIds: z.array(z.string()),
});

export const InvariantReportSchema = z.object({
  orphanBids: InvariantCounterSchema,
  orphanSettlements: InvariantCounterSchema,
  orphanDisputes: InvariantCounterSchema,
  mismatchSettlements: InvariantCounterSchema,
  timestamp: z.string().datetime(),
});

export const CursorRegressionSchema = z.object({
  hasRegression: z.boolean(),
  regressionCount: z.number().int().min(0),
  regressions: z.array(
    z.object({
      index: z.number().int().min(1),
      previous: z.number().int(),
      current: z.number().int(),
    })
  ),
});

export const AccountingReportSchema = z.object({
  mismatches: InvariantCounterSchema,
});

export const FullInvariantReportSchema = z.object({
  orphans: InvariantReportSchema,
  cursorSequence: CursorRegressionSchema,
  accounting: AccountingReportSchema,
  timestamp: z.string().datetime(),
  pass: z.boolean(),
});

export type InvariantCounter = z.infer<typeof InvariantCounterSchema>;
export type InvariantReport = z.infer<typeof InvariantReportSchema>;
export type CursorRegressionReport = z.infer<typeof CursorRegressionSchema>;
export type AccountingReport = z.infer<typeof AccountingReportSchema>;
export type FullInvariantReport = z.infer<typeof FullInvariantReportSchema>;

// ── Data Provider ─────────────────────────────────────────────────────────────

/** Abstracts data access so the suite can be driven by any backing store. */
export interface InvariantDataProvider {
  getInvoices(): Promise<Invoice[]>;
  getBids(): Promise<Bid[]>;
  getSettlements(): Promise<Settlement[]>;
  getDisputes(): Promise<Dispute[]>;
}

/** Creates a provider from static in-memory arrays. Useful in tests and local dev. */
export function createInMemoryProvider(
  invoices: Invoice[],
  bids: Bid[],
  settlements: Settlement[],
  disputes: Dispute[]
): InvariantDataProvider {
  return {
    getInvoices: async () => invoices,
    getBids: async () => bids,
    getSettlements: async () => settlements,
    getDisputes: async () => disputes,
  };
}

// ── Orphan detection ──────────────────────────────────────────────────────────

interface HasInvoiceId {
  invoice_id: string;
}

function scanOrphans<T extends HasInvoiceId>(
  items: T[],
  validIds: Set<string>
): InvariantCounter {
  const orphans = items.filter((item) => !validIds.has(item.invoice_id));
  return {
    count: orphans.length,
    sampleIds: orphans.slice(0, MAX_SAMPLE_IDS).map((o) => o.invoice_id),
  };
}

function scanMismatchSettlements(
  bids: Bid[],
  settlements: Settlement[]
): InvariantCounter {
  const bidInvoiceIds = new Set(bids.map((b) => b.invoice_id));
  const mismatches = settlements.filter((s) => !bidInvoiceIds.has(s.invoice_id));
  return {
    count: mismatches.length,
    sampleIds: mismatches.slice(0, MAX_SAMPLE_IDS).map((s) => s.id),
  };
}

/**
 * Detect orphan records: bids, settlements, or disputes whose invoice_id has
 * no matching invoice in the store. Also flags settlements with no matching bid.
 */
export async function checkOrphans(
  provider: InvariantDataProvider
): Promise<InvariantReport> {
  const [invoices, bids, settlements, disputes] = await Promise.all([
    provider.getInvoices(),
    provider.getBids(),
    provider.getSettlements(),
    provider.getDisputes(),
  ]);

  const validInvoiceIds = new Set(invoices.map((i) => i.id));
  return {
    orphanBids: scanOrphans(bids as HasInvoiceId[], validInvoiceIds),
    orphanSettlements: scanOrphans(settlements as HasInvoiceId[], validInvoiceIds),
    orphanDisputes: scanOrphans(disputes as HasInvoiceId[], validInvoiceIds),
    mismatchSettlements: scanMismatchSettlements(bids, settlements),
    timestamp: new Date().toISOString(),
  };
}

// ── Cursor sequence ───────────────────────────────────────────────────────────

/**
 * Verify a ledger cursor history is strictly monotonically increasing.
 * Each cursor must be greater than the one before it; equal or lower values
 * indicate a regression (duplicate replay or rollback without re-index).
 */
export function checkCursorSequence(cursors: number[]): CursorRegressionReport {
  const regressions: Array<{ index: number; previous: number; current: number }> = [];
  for (let i = 1; i < cursors.length; i++) {
    if (cursors[i] <= cursors[i - 1]) {
      regressions.push({ index: i, previous: cursors[i - 1], current: cursors[i] });
    }
  }
  return {
    hasRegression: regressions.length > 0,
    regressionCount: regressions.length,
    regressions,
  };
}

// ── Accounting totals ─────────────────────────────────────────────────────────

/**
 * Verify derived accounting totals are internally consistent.
 * Every Paid settlement must have a matching Accepted bid, and the settlement
 * amount must equal the accepted bid's bid_amount. Mismatches indicate data
 * corruption or a missed event during indexing.
 */
export async function checkAccountingTotals(
  provider: InvariantDataProvider
): Promise<AccountingReport> {
  const [bids, settlements] = await Promise.all([
    provider.getBids(),
    provider.getSettlements(),
  ]);

  const acceptedBidByInvoice = new Map<string, Bid>();
  for (const bid of bids) {
    if (bid.status === BidStatus.Accepted) {
      acceptedBidByInvoice.set(bid.invoice_id, bid);
    }
  }

  const mismatchIds: string[] = [];
  for (const settlement of settlements) {
    if (settlement.status !== SettlementStatus.Paid) continue;
    const bid = acceptedBidByInvoice.get(settlement.invoice_id);
    if (!bid) {
      mismatchIds.push(settlement.invoice_id);
      continue;
    }
    if (BigInt(settlement.amount) !== BigInt(bid.bid_amount)) {
      mismatchIds.push(settlement.invoice_id);
    }
  }

  return {
    mismatches: {
      count: mismatchIds.length,
      sampleIds: mismatchIds.slice(0, MAX_SAMPLE_IDS),
    },
  };
}

// ── Full suite ────────────────────────────────────────────────────────────────

/**
 * Run all three invariant checks in parallel and return a consolidated report.
 * `pass` is true only when every sub-check finds zero violations.
 * Safe to call periodically in production — all operations are read-only.
 */
export async function runFullInvariantSuite(
  provider: InvariantDataProvider,
  cursorHistory: number[]
): Promise<FullInvariantReport> {
  const [orphans, accounting] = await Promise.all([
    checkOrphans(provider),
    checkAccountingTotals(provider),
  ]);
  const cursorSequence = checkCursorSequence(cursorHistory);

  const pass =
    orphans.orphanBids.count === 0 &&
    orphans.orphanSettlements.count === 0 &&
    orphans.orphanDisputes.count === 0 &&
    orphans.mismatchSettlements.count === 0 &&
    !cursorSequence.hasRegression &&
    accounting.mismatches.count === 0;

  return {
    orphans,
    cursorSequence,
    accounting,
    timestamp: new Date().toISOString(),
    pass,
  };
}

