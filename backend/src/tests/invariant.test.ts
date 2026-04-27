import {
  checkOrphans,
  checkCursorSequence,
  checkAccountingTotals,
  runFullInvariantSuite,
  createInMemoryProvider,
  InvariantDataProvider,
} from "../services/invariantService";
import {
  Invoice,
  Bid,
  Settlement,
  Dispute,
  InvoiceStatus,
  BidStatus,
  SettlementStatus,
  DisputeStatus,
  InvoiceCategory,
  InvoiceMetadata,
} from "../types/contract";

// ── Helpers ───────────────────────────────────────────────────────────────────

const VERSIONED = {
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString(),
};

const DEFAULT_METADATA: InvoiceMetadata = {
  customer_name: "Acme Corp",
  customer_address: "123 Main St",
  tax_id: "TAX-001",
  line_items: [],
  notes: "",
};

function makeInvoice(id: string): Invoice {
  return {
    ...VERSIONED,
    id,
    business: "GA_BUSINESS",
    amount: "1000000000",
    currency: "USDC",
    due_date: Math.floor(Date.now() / 1000) + 86400,
    status: InvoiceStatus.Verified,
    description: "Test invoice",
    category: InvoiceCategory.Services,
    tags: [],
    metadata: DEFAULT_METADATA,
    created_at: Math.floor(Date.now() / 1000) - 3600,
    updated_at: Math.floor(Date.now() / 1000),
  };
}

function makeBid(bidId: string, invoiceId: string, status = BidStatus.Placed, amount = "1000000000"): Bid {
  return {
    ...VERSIONED,
    bid_id: bidId,
    invoice_id: invoiceId,
    investor: "GA_INVESTOR",
    bid_amount: amount,
    expected_return: "50000000",
    timestamp: Math.floor(Date.now() / 1000) - 1800,
    status,
    expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
  };
}

function makeSettlement(id: string, invoiceId: string, status = SettlementStatus.Paid, amount = "1000000000"): Settlement {
  return {
    ...VERSIONED,
    id,
    invoice_id: invoiceId,
    amount,
    payer: "GA_PAYER",
    recipient: "GA_RECIP",
    timestamp: Math.floor(Date.now() / 1000) - 900,
    status,
  };
}

function makeDispute(id: string, invoiceId: string): Dispute {
  return {
    ...VERSIONED,
    id,
    invoice_id: invoiceId,
    initiator: "GA_BUYER",
    reason: "Goods not delivered",
    status: DisputeStatus.UnderReview,
    created_at: Math.floor(Date.now() / 1000) - 7200,
  };
}

function makeProvider(
  invoices: Invoice[],
  bids: Bid[],
  settlements: Settlement[],
  disputes: Dispute[]
): InvariantDataProvider {
  return createInMemoryProvider(invoices, bids, settlements, disputes);
}

// ── checkOrphans ──────────────────────────────────────────────────────────────

describe("checkOrphans", () => {
  it("returns zero counts when all records reference valid invoices", async () => {
    const inv = makeInvoice("inv-1");
    const bid = makeBid("bid-1", "inv-1");
    const settlement = makeSettlement("set-1", "inv-1");
    const dispute = makeDispute("dis-1", "inv-1");
    const provider = makeProvider([inv], [bid], [settlement], [dispute]);

    const report = await checkOrphans(provider);

    expect(report.orphanBids.count).toBe(0);
    expect(report.orphanSettlements.count).toBe(0);
    expect(report.orphanDisputes.count).toBe(0);
    expect(report.mismatchSettlements.count).toBe(0);
  });

  it("detects orphan bids (invoice_id has no matching invoice)", async () => {
    const bid = makeBid("bid-x", "inv-missing");
    const provider = makeProvider([], [bid], [], []);

    const report = await checkOrphans(provider);

    expect(report.orphanBids.count).toBe(1);
    expect(report.orphanBids.sampleIds).toContain("inv-missing");
  });

  it("detects orphan settlements", async () => {
    const settlement = makeSettlement("set-x", "inv-missing");
    const provider = makeProvider([], [], [settlement], []);

    const report = await checkOrphans(provider);

    expect(report.orphanSettlements.count).toBe(1);
    expect(report.orphanSettlements.sampleIds).toContain("inv-missing");
  });

  it("detects orphan disputes", async () => {
    const dispute = makeDispute("dis-x", "inv-missing");
    const provider = makeProvider([], [], [], [dispute]);

    const report = await checkOrphans(provider);

    expect(report.orphanDisputes.count).toBe(1);
    expect(report.orphanDisputes.sampleIds).toContain("inv-missing");
  });

  it("detects mismatch settlements (settlement with no corresponding bid)", async () => {
    const inv = makeInvoice("inv-1");
    // No bids at all, but there is a settlement
    const settlement = makeSettlement("set-1", "inv-1");
    const provider = makeProvider([inv], [], [settlement], []);

    const report = await checkOrphans(provider);

    expect(report.mismatchSettlements.count).toBe(1);
    expect(report.mismatchSettlements.sampleIds).toContain("set-1");
  });

  it("caps sampleIds at 5 regardless of orphan count", async () => {
    const bids = Array.from({ length: 10 }, (_, i) => makeBid(`bid-${i}`, `inv-missing-${i}`));
    const provider = makeProvider([], bids, [], []);

    const report = await checkOrphans(provider);

    expect(report.orphanBids.count).toBe(10);
    expect(report.orphanBids.sampleIds.length).toBe(5);
  });

  it("handles all empty tables without error", async () => {
    const provider = makeProvider([], [], [], []);
    const report = await checkOrphans(provider);

    expect(report.orphanBids.count).toBe(0);
    expect(report.orphanSettlements.count).toBe(0);
    expect(report.orphanDisputes.count).toBe(0);
    expect(report.mismatchSettlements.count).toBe(0);
  });

  it("returns valid ISO timestamp", async () => {
    const provider = makeProvider([], [], [], []);
    const report = await checkOrphans(provider);
    expect(() => new Date(report.timestamp)).not.toThrow();
    expect(new Date(report.timestamp).getTime()).toBeGreaterThan(0);
  });
});

// ── checkCursorSequence ───────────────────────────────────────────────────────

describe("checkCursorSequence", () => {
  it("passes for a strictly increasing sequence", () => {
    const result = checkCursorSequence([100, 200, 300, 400]);
    expect(result.hasRegression).toBe(false);
    expect(result.regressionCount).toBe(0);
    expect(result.regressions).toHaveLength(0);
  });

  it("passes for a single-element sequence (no comparison possible)", () => {
    const result = checkCursorSequence([500]);
    expect(result.hasRegression).toBe(false);
    expect(result.regressionCount).toBe(0);
  });

  it("passes for an empty sequence", () => {
    const result = checkCursorSequence([]);
    expect(result.hasRegression).toBe(false);
    expect(result.regressionCount).toBe(0);
  });

  it("detects a regression when cursor decreases", () => {
    const result = checkCursorSequence([100, 200, 150, 300]);
    expect(result.hasRegression).toBe(true);
    expect(result.regressionCount).toBe(1);
    expect(result.regressions[0]).toEqual({ index: 2, previous: 200, current: 150 });
  });

  it("detects a regression when cursor repeats (equal is not strictly greater)", () => {
    const result = checkCursorSequence([100, 100]);
    expect(result.hasRegression).toBe(true);
    expect(result.regressions[0]).toEqual({ index: 1, previous: 100, current: 100 });
  });

  it("detects multiple regressions and records all of them", () => {
    const result = checkCursorSequence([100, 50, 200, 100, 300]);
    expect(result.regressionCount).toBe(2);
    expect(result.regressions[0]).toMatchObject({ index: 1, previous: 100, current: 50 });
    expect(result.regressions[1]).toMatchObject({ index: 3, previous: 200, current: 100 });
  });

  it("detects a regression at position 0→1 (very first pair)", () => {
    const result = checkCursorSequence([500, 400]);
    expect(result.hasRegression).toBe(true);
    expect(result.regressions[0]).toEqual({ index: 1, previous: 500, current: 400 });
  });
});

// ── checkAccountingTotals ─────────────────────────────────────────────────────

describe("checkAccountingTotals", () => {
  it("passes when paid settlement amount matches the accepted bid", async () => {
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, "1000000000");
    const settlement = makeSettlement("set-1", "inv-1", SettlementStatus.Paid, "1000000000");
    const provider = makeProvider([], [bid], [settlement], []);

    const report = await checkAccountingTotals(provider);

    expect(report.mismatches.count).toBe(0);
  });

  it("detects mismatch when settlement amount differs from accepted bid", async () => {
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, "1000000000");
    const settlement = makeSettlement("set-1", "inv-1", SettlementStatus.Paid, "999999999");
    const provider = makeProvider([], [bid], [settlement], []);

    const report = await checkAccountingTotals(provider);

    expect(report.mismatches.count).toBe(1);
    expect(report.mismatches.sampleIds).toContain("inv-1");
  });

  it("flags a paid settlement with no accepted bid", async () => {
    const bid = makeBid("bid-1", "inv-1", BidStatus.Placed, "1000000000");
    const settlement = makeSettlement("set-1", "inv-1", SettlementStatus.Paid, "1000000000");
    const provider = makeProvider([], [bid], [settlement], []);

    const report = await checkAccountingTotals(provider);

    expect(report.mismatches.count).toBe(1);
  });

  it("ignores non-Paid settlements (Pending, Defaulted)", async () => {
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, "1000000000");
    const pending = makeSettlement("set-p", "inv-1", SettlementStatus.Pending, "999");
    const defaulted = makeSettlement("set-d", "inv-1", SettlementStatus.Defaulted, "0");
    const provider = makeProvider([], [bid], [pending, defaulted], []);

    const report = await checkAccountingTotals(provider);

    expect(report.mismatches.count).toBe(0);
  });

  it("handles large BigInt amounts correctly", async () => {
    const largeAmount = "999999999999999999999";
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, largeAmount);
    const settlement = makeSettlement("set-1", "inv-1", SettlementStatus.Paid, largeAmount);
    const provider = makeProvider([], [bid], [settlement], []);

    const report = await checkAccountingTotals(provider);

    expect(report.mismatches.count).toBe(0);
  });

  it("caps sampleIds at 5 when many mismatches exist", async () => {
    const bids = Array.from({ length: 8 }, (_, i) =>
      makeBid(`bid-${i}`, `inv-${i}`, BidStatus.Accepted, "100")
    );
    const settlements = Array.from({ length: 8 }, (_, i) =>
      makeSettlement(`set-${i}`, `inv-${i}`, SettlementStatus.Paid, "999")
    );
    const provider = makeProvider([], bids, settlements, []);

    const report = await checkAccountingTotals(provider);

    expect(report.mismatches.count).toBe(8);
    expect(report.mismatches.sampleIds.length).toBe(5);
  });

  it("returns zero mismatches for empty tables", async () => {
    const provider = makeProvider([], [], [], []);
    const report = await checkAccountingTotals(provider);
    expect(report.mismatches.count).toBe(0);
  });
});

// ── runFullInvariantSuite ─────────────────────────────────────────────────────

describe("runFullInvariantSuite", () => {
  it("returns pass=true when all checks succeed", async () => {
    const inv = makeInvoice("inv-1");
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, "1000000000");
    const settlement = makeSettlement("set-1", "inv-1", SettlementStatus.Paid, "1000000000");
    const dispute = makeDispute("dis-1", "inv-1");
    const provider = makeProvider([inv], [bid], [settlement], [dispute]);
    const cursors = [100, 200, 300];

    const report = await runFullInvariantSuite(provider, cursors);

    expect(report.pass).toBe(true);
    expect(report.orphans.orphanBids.count).toBe(0);
    expect(report.cursorSequence.hasRegression).toBe(false);
    expect(report.accounting.mismatches.count).toBe(0);
  });

  it("returns pass=false when orphan bids exist", async () => {
    const bid = makeBid("bid-x", "inv-missing");
    const provider = makeProvider([], [bid], [], []);

    const report = await runFullInvariantSuite(provider, [100, 200]);

    expect(report.pass).toBe(false);
    expect(report.orphans.orphanBids.count).toBe(1);
  });

  it("returns pass=false when cursor has regression", async () => {
    const provider = makeProvider([], [], [], []);

    const report = await runFullInvariantSuite(provider, [100, 50]);

    expect(report.pass).toBe(false);
    expect(report.cursorSequence.hasRegression).toBe(true);
  });

  it("returns pass=false when accounting mismatch exists", async () => {
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, "1000000000");
    const settlement = makeSettlement("set-1", "inv-1", SettlementStatus.Paid, "1");
    const provider = makeProvider([], [bid], [settlement], []);

    const report = await runFullInvariantSuite(provider, [100, 200]);

    expect(report.pass).toBe(false);
    expect(report.accounting.mismatches.count).toBe(1);
  });

  it("includes a valid ISO timestamp in the full report", async () => {
    const provider = makeProvider([], [], [], []);
    const report = await runFullInvariantSuite(provider, []);
    expect(() => new Date(report.timestamp)).not.toThrow();
    expect(new Date(report.timestamp).getTime()).toBeGreaterThan(0);
  });

  it("passes with empty data and empty cursor history", async () => {
    const provider = makeProvider([], [], [], []);
    const report = await runFullInvariantSuite(provider, []);
    expect(report.pass).toBe(true);
  });
});

// ── Schema validation ─────────────────────────────────────────────────────────

describe("Zod schema validation", () => {
  it("FullInvariantReportSchema validates a passing full report", async () => {
    const { FullInvariantReportSchema } = await import("../services/invariantService");
    const provider = makeProvider([], [], [], []);
    const { runFullInvariantSuite: run } = await import("../services/invariantService");
    const report = await run(provider, [10, 20, 30]);

    const result = FullInvariantReportSchema.safeParse(report);
    expect(result.success).toBe(true);
  });
});
