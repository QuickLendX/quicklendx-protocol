import { z } from "zod";
import {
  Invoice,
  Bid,
  Settlement,
  Dispute,
  BidStatus,
  SettlementStatus,
} from "../types/contract";
import { withSpan } from "../lib/tracing";
import { alertRouter, Severity } from "./alertRouter";

const MAX_SAMPLE_IDS = 5;

// ── Scheduler Configuration ─────────────────────────────────────────────────

export const DEFAULT_SCHEDULE_INTERVAL_MS = 30000; // 30 seconds

// Environment variable for schedule interval
export function getScheduleInterval(): number {
  const raw = process.env.INVARIANT_SCHEDULE_INTERVAL_MS;
  if (raw) {
    const parsed = parseInt(raw, 10);
    if (!Number.isNaN(parsed) && parsed > 0) {
      return parsed;
    }
  }
  return DEFAULT_SCHEDULE_INTERVAL_MS;
}

export function getCursorHistory(): number[] {
  const raw = process.env.INVARIANT_CURSOR_HISTORY;
  if (raw) {
    return raw
      .split(",")
      .map((s) => parseInt(s.trim(), 10))
      .filter((n) => !Number.isNaN(n));
  }
  return [];
}

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
    }),
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
  disputes: Dispute[],
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
  validIds: Set<string>,
): InvariantCounter {
  const orphans = items.filter((item) => !validIds.has(item.invoice_id));
  return {
    count: orphans.length,
    sampleIds: orphans.slice(0, MAX_SAMPLE_IDS).map((o) => o.invoice_id),
  };
}

function scanMismatchSettlements(
  bids: Bid[],
  settlements: Settlement[],
): InvariantCounter {
  const bidInvoiceIds = new Set(bids.map((b) => b.invoice_id));
  const mismatches = settlements.filter(
    (s) => !bidInvoiceIds.has(s.invoice_id),
  );
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
  provider: InvariantDataProvider,
): Promise<InvariantReport> {
  return withSpan("invariant.checkOrphans", {}, async () => {
    const [invoices, bids, settlements, disputes] = await Promise.all([
      provider.getInvoices(),
      provider.getBids(),
      provider.getSettlements(),
      provider.getDisputes(),
    ]);

    const validInvoiceIds = new Set(invoices.map((i) => i.id));
    return {
      orphanBids: scanOrphans(bids as HasInvoiceId[], validInvoiceIds),
      orphanSettlements: scanOrphans(
        settlements as HasInvoiceId[],
        validInvoiceIds,
      ),
      orphanDisputes: scanOrphans(disputes as HasInvoiceId[], validInvoiceIds),
      mismatchSettlements: scanMismatchSettlements(bids, settlements),
      timestamp: new Date().toISOString(),
    };
  });
}

// ── Cursor sequence ───────────────────────────────────────────────────────────

/**
 * Verify a ledger cursor history is strictly monotonically increasing.
 * Each cursor must be greater than the one before it; equal or lower values
 * indicate a regression (duplicate replay or rollback without re-index).
 */
export function checkCursorSequence(cursors: number[]): CursorRegressionReport {
  return withSpan(
    "invariant.checkCursorSequence",
    { cursor_count: cursors.length },
    () => {
      const regressions: Array<{
        index: number;
        previous: number;
        current: number;
      }> = [];
      for (let i = 1; i < cursors.length; i++) {
        if (cursors[i] <= cursors[i - 1]) {
          regressions.push({
            index: i,
            previous: cursors[i - 1],
            current: cursors[i],
          });
        }
      }
      return {
        hasRegression: regressions.length > 0,
        regressionCount: regressions.length,
        regressions,
      };
    },
  );
}

// ── Accounting totals ─────────────────────────────────────────────────────────

/**
 * Verify derived accounting totals are internally consistent.
 * Every Paid settlement must have a matching Accepted bid, and the settlement
 * amount must equal the accepted bid's bid_amount. Mismatches indicate data
 * corruption or a missed event during indexing.
 */
export async function checkAccountingTotals(
  provider: InvariantDataProvider,
): Promise<AccountingReport> {
  return withSpan("invariant.checkAccountingTotals", {}, async () => {
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
  });
}

// ── Full suite ────────────────────────────────────────────────────────────────

/**
 * Run all three invariant checks in parallel and return a consolidated report.
 * `pass` is true only when every sub-check finds zero violations.
 * Safe to call periodically in production — all operations are read-only.
 */
export async function runFullInvariantSuite(
  provider: InvariantDataProvider,
  cursorHistory: number[],
): Promise<FullInvariantReport> {
  return withSpan(
    "invariant.runFullInvariantSuite",
    { cursor_history_count: cursorHistory.length },
    async () => {
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
    },
  );
}

// ── Persistence ───────────────────────────────────────────────────────────────

export const ScheduledReportSchema = z.object({
  report: FullInvariantReportSchema,
  cursorHistory: z.array(z.number().int()),
  runAt: z.string().datetime(),
});

export type ScheduledReport = z.infer<typeof ScheduledReportSchema>;

interface InvariantReportStore {
  add(report: ScheduledReport): void;
  getLatest(): ScheduledReport | undefined;
  getAll(): ScheduledReport[];
  clear(): void;
}

class InMemoryInvariantReportStore implements InvariantReportStore {
  private reports: ScheduledReport[] = [];

  add(report: ScheduledReport): void {
    this.reports.push(report);
  }

  getLatest(): ScheduledReport | undefined {
    return this.reports.length > 0
      ? this.reports[this.reports.length - 1]
      : undefined;
  }

  getAll(): ScheduledReport[] {
    return [...this.reports];
  }

  clear(): void {
    this.reports = [];
  }
}

const reportStore = new InMemoryInvariantReportStore();

// ── Metrics Counter ──────────────────────────────────────────────────────────

export interface InvariantMetrics {
  orphanBidsTotal: number;
  orphanSettlementsTotal: number;
  orphanDisputesTotal: number;
  mismatchSettlementsTotal: number;
  cursorRegressionsTotal: number;
  accountingMismatchesTotal: number;
  violationsDetectedTotal: number;
  checksRunTotal: number;
}

let metrics: InvariantMetrics = {
  orphanBidsTotal: 0,
  orphanSettlementsTotal: 0,
  orphanDisputesTotal: 0,
  mismatchSettlementsTotal: 0,
  cursorRegressionsTotal: 0,
  accountingMismatchesTotal: 0,
  violationsDetectedTotal: 0,
  checksRunTotal: 0,
};

function resetMetrics(): void {
  metrics = {
    orphanBidsTotal: 0,
    orphanSettlementsTotal: 0,
    orphanDisputesTotal: 0,
    mismatchSettlementsTotal: 0,
    cursorRegressionsTotal: 0,
    accountingMismatchesTotal: 0,
    violationsDetectedTotal: 0,
    checksRunTotal: 0,
  };
}

function recordMetrics(report: FullInvariantReport): void {
  metrics.orphanBidsTotal += report.orphans.orphanBids.count;
  metrics.orphanSettlementsTotal += report.orphans.orphanSettlements.count;
  metrics.orphanDisputesTotal += report.orphans.orphanDisputes.count;
  metrics.mismatchSettlementsTotal += report.orphans.mismatchSettlements.count;
  metrics.cursorRegressionsTotal += report.cursorSequence.regressionCount;
  metrics.accountingMismatchesTotal += report.accounting.mismatches.count;
  metrics.checksRunTotal += 1;

  const totalViolations =
    report.orphans.orphanBids.count +
    report.orphans.orphanSettlements.count +
    report.orphans.orphanDisputes.count +
    report.orphans.mismatchSettlements.count +
    report.cursorSequence.regressionCount +
    report.accounting.mismatches.count;

  if (totalViolations > 0) {
    metrics.violationsDetectedTotal += 1;
  }
}

// ── Alerting ────────────────────────────────────────────────────────────────

export function emitInvariantAlert(report: FullInvariantReport): void {
  withSpan("invariant.emitInvariantAlert", { pass: report.pass }, () => {
    if (report.pass) return;

    const violations: string[] = [];
    if (report.orphans.orphanBids.count > 0)
      violations.push(`orphan_bids: ${report.orphans.orphanBids.count}`);
    if (report.orphans.orphanSettlements.count > 0)
      violations.push(
        `orphan_settlements: ${report.orphans.orphanSettlements.count}`,
      );
    if (report.orphans.orphanDisputes.count > 0)
      violations.push(
        `orphan_disputes: ${report.orphans.orphanDisputes.count}`,
      );
    if (report.orphans.mismatchSettlements.count > 0)
      violations.push(
        `mismatch_settlements: ${report.orphans.mismatchSettlements.count}`,
      );
    if (report.cursorSequence.hasRegression)
      violations.push(
        `cursor_regression: ${report.cursorSequence.regressionCount}`,
      );
    if (report.accounting.mismatches.count > 0)
      violations.push(
        `accounting_mismatches: ${report.accounting.mismatches.count}`,
      );

    const message = `Invariant violation detected: ${violations.join(", ")}`;

    console.error(
      JSON.stringify({
        level: "ALERT",
        type: "INVARIANT_VIOLATION",
        timestamp: report.timestamp,
        violations,
        message,
      }),
    );

    // Route to alert system via alertRouter
    // Determine severity based on violation count
    const totalViolations = violations.length;
    const severity =
      totalViolations > 2 ? Severity.HIGH : totalViolations > 0 ? Severity.MEDIUM : Severity.LOW;

    alertRouter
      .routeAlert("invariant-violation", severity, message)
      .catch((err) => console.error("Failed to route invariant alert:", err));
  });
}

// ── Scheduler ───────────────────────────────────────────────────────────────

export class InvariantScheduler {
  private static instance: InvariantScheduler;
  private timer: NodeJS.Timeout | null = null;
  private isRunning = false;
  private provider: InvariantDataProvider | null = null;
  private cursorHistory: number[] = [];

  private constructor() {}

  public static getInstance(): InvariantScheduler {
    if (!InvariantScheduler.instance) {
      InvariantScheduler.instance = new InvariantScheduler();
    }
    return InvariantScheduler.instance;
  }

  setProvider(provider: InvariantDataProvider): void {
    this.provider = provider;
  }

  setCursorHistory(history: number[]): void {
    this.cursorHistory = history;
  }

  start(intervalMs?: number): void {
    if (this.isRunning) return;
    const actualInterval = intervalMs ?? getScheduleInterval();
    this.isRunning = true;
    this.timer = setInterval(() => void this.runCheck(), actualInterval);
    // Run immediately on start
    void this.runCheck();
  }

  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.isRunning = false;
  }

  private async runCheck(): Promise<void> {
    await withSpan(
      "invariant.scheduler.runCheck",
      { cursor_history_count: this.cursorHistory.length },
      async () => {
        if (!this.provider) return;

        try {
          const report = await runFullInvariantSuite(
            this.provider,
            this.cursorHistory,
          );
          recordMetrics(report);
          reportStore.add({
            report,
            cursorHistory: [...this.cursorHistory],
            runAt: report.timestamp,
          });
          emitInvariantAlert(report);
        } catch (err) {
          console.error(
            JSON.stringify({
              level: "ERROR",
              type: "INVARIANT_CHECK_FAILED",
              timestamp: new Date().toISOString(),
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        }
      },
    );
  }

  isStarted(): boolean {
    return this.isRunning;
  }
}

// ── Export functions ─────────────────────────────────────────────────────────

export function getInvariantScheduler(): InvariantScheduler {
  return InvariantScheduler.getInstance();
}

export function getInvariantCounters(): FullInvariantReport | null {
  const latest = reportStore.getLatest();
  return latest ? latest.report : null;
}

export function getInvariantMetrics(): InvariantMetrics {
  return { ...metrics };
}

export function getScheduledReports(): ScheduledReport[] {
  return reportStore.getAll();
}

export function clearInvariantState(): void {
  reportStore.clear();
  resetMetrics();
}
