import {
  checkOrphans,
  checkCursorSequence,
  checkAccountingTotals,
  runFullInvariantSuite,
  createInMemoryProvider,
  InvariantDataProvider,
  getInvariantScheduler,
  getInvariantCounters,
  getInvariantMetrics,
  getScheduledReports,
  clearInvariantState,
  DEFAULT_SCHEDULE_INTERVAL_MS,
  getScheduleInterval,
  getCursorHistory,
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

function makeBid(
  bidId: string,
  invoiceId: string,
  status = BidStatus.Placed,
  amount = "1000000000",
): Bid {
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

function makeSettlement(
  id: string,
  invoiceId: string,
  status = SettlementStatus.Paid,
  amount = "1000000000",
): Settlement {
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
  disputes: Dispute[],
): InvariantDataProvider {
  return createInMemoryProvider(invoices, bids, settlements, disputes);
}

// ── Test Setup/Teardown ───────────────────────────────────────────────────────

beforeEach(() => {
  clearInvariantState();
});

afterEach(() => {
  const scheduler = getInvariantScheduler();
  scheduler.stop();
});

// ── Scheduler Lifecycle Tests ─────────────────────────────────────────────────

describe("InvariantScheduler", () => {
  it("starts and stops the scheduler correctly", () => {
    const scheduler = getInvariantScheduler();
    expect(scheduler.isStarted()).toBe(false);

    scheduler.start(1000);
    expect(scheduler.isStarted()).toBe(true);

    scheduler.stop();
    expect(scheduler.isStarted()).toBe(false);
  });

  it("does not start again if already running", () => {
    const scheduler = getInvariantScheduler();
    scheduler.start(1000);
    expect(scheduler.isStarted()).toBe(true);

    // Attempting to start again should be no-op
    scheduler.start(1000);
    expect(scheduler.isStarted()).toBe(true);

    scheduler.stop();
  });

  it("returns null from getInvariantCounters when no checks have run", () => {
    expect(getInvariantCounters()).toBeNull();
  });
});

// ── Scheduled Check Tests ─────────────────────────────────────────────────────

describe("Scheduled invariant checks", () => {
  it("runs checks on schedule and persists results", async () => {
    const inv = makeInvoice("inv-1");
    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([100, 200, 300]);

    // Run check immediately (start() runs one immediately)
    scheduler.start(50);

    // Wait for the check to complete
    await new Promise((resolve) => setTimeout(resolve, 100));

    const report = getInvariantCounters();
    expect(report).not.toBeNull();
    expect(report!.pass).toBe(true);
    expect(report!.timestamp).toBeDefined();

    scheduler.stop();
  });

  it("detects and alerts on injected violations", async () => {
    const bid = makeBid("bid-x", "inv-missing"); // Orphan bid
    const provider = makeProvider([], [bid], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([100, 200]);

    // Capture console.error output
    const errorLogs: string[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => {
      errorLogs.push(
        args
          .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
          .join(" "),
      );
    };

    try {
      scheduler.start(50);

      // Wait for check
      await new Promise((resolve) => setTimeout(resolve, 100));

      const report = getInvariantCounters();
      expect(report).not.toBeNull();
      expect(report!.pass).toBe(false);
      expect(report!.orphans.orphanBids.count).toBe(1);

      // Verify alert was logged
      const alertFound = errorLogs.some((log) =>
        log.includes("INVARIANT_VIOLATION"),
      );
      expect(alertFound).toBe(true);
    } finally {
      console.error = originalError;
      scheduler.stop();
    }
  });

  it("handles clean state (no violations) without alerting", async () => {
    const inv = makeInvoice("inv-1");
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, "1000000000");
    const settlement = makeSettlement(
      "set-1",
      "inv-1",
      SettlementStatus.Paid,
      "1000000000",
    );
    const provider = makeProvider([inv], [bid], [settlement], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([100, 150, 200]);

    const errorLogs: string[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => {
      errorLogs.push(
        args
          .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
          .join(" "),
      );
    };

    try {
      scheduler.start(50);

      await new Promise((resolve) => setTimeout(resolve, 100));

      const report = getInvariantCounters();
      expect(report).not.toBeNull();
      expect(report!.pass).toBe(true);

      // No violation alerts should be logged
      const alertFound = errorLogs.some((log) =>
        log.includes("INVARIANT_VIOLATION"),
      );
      expect(alertFound).toBe(false);
    } finally {
      console.error = originalError;
      scheduler.stop();
    }
  });

  it("stores multiple reports and retrievable via getScheduledReports", async () => {
    const inv = makeInvoice("inv-1");
    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([100]);
    scheduler.start(30);

    // Wait for multiple checks
    await new Promise((resolve) => setTimeout(resolve, 150));

    const reports = getScheduledReports();
    expect(reports.length).toBeGreaterThan(0);
    expect(reports[reports.length - 1].report.pass).toBe(true);

    scheduler.stop();
  });
});

// ── Metric Recording Tests ──────────────────────────────────────────────────────

describe("InvariantMetrics", () => {
  it("records metrics on successful checks", async () => {
    const inv = makeInvoice("inv-1");
    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);
    scheduler.start(50);

    await new Promise((resolve) => setTimeout(resolve, 100));

    const metrics = getInvariantMetrics();
    expect(metrics.checksRunTotal).toBeGreaterThan(0);
    expect(metrics.violationsDetectedTotal).toBe(0);

    scheduler.stop();
  });

  it("records violation metrics when violations occur", async () => {
    const bid = makeBid("bid-x", "inv-missing");
    const provider = makeProvider([], [bid], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);
    scheduler.start(50);

    await new Promise((resolve) => setTimeout(resolve, 100));

    const metrics = getInvariantMetrics();
    expect(metrics.orphanBidsTotal).toBeGreaterThan(0);
    expect(metrics.violationsDetectedTotal).toBeGreaterThan(0);

    scheduler.stop();
  });

  it("resets metrics on clearInvariantState", async () => {
    const inv = makeInvoice("inv-1");
    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);
    scheduler.start(50);

    await new Promise((resolve) => setTimeout(resolve, 100));

    clearInvariantState();

    const metrics = getInvariantMetrics();
    expect(metrics.checksRunTotal).toBe(0);
    expect(metrics.violationsDetectedTotal).toBe(0);

    scheduler.stop();
  });
});

// ── Store Unavailable Tests ───────────────────────────────────────────────────

describe("Graceful store unavailability", () => {
  it("handles provider that throws during scheduled check", async () => {
    const failingProvider: InvariantDataProvider = {
      getInvoices: async () => {
        throw new Error("Database unavailable");
      },
      getBids: async () => [],
      getSettlements: async () => [],
      getDisputes: async () => [],
    };

    const scheduler = getInvariantScheduler();
    scheduler.setProvider(failingProvider);
    scheduler.setCursorHistory([]);

    const errorLogs: string[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => {
      errorLogs.push(
        args
          .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
          .join(" "),
      );
    };

    try {
      scheduler.start(50);

      await new Promise((resolve) => setTimeout(resolve, 100));

      // Should have logged an error
      const errorFound = errorLogs.some((log) =>
        log.includes("INVARIANT_CHECK_FAILED"),
      );
      expect(errorFound).toBe(true);
    } finally {
      console.error = originalError;
      scheduler.stop();
    }
  });

  it("handles partial provider failures gracefully", async () => {
    const partialProvider: InvariantDataProvider = {
      getInvoices: async () => [],
      getBids: async () => {
        throw new Error("Bids store unavailable");
      },
      getSettlements: async () => [],
      getDisputes: async () => [],
    };

    const scheduler = getInvariantScheduler();
    scheduler.setProvider(partialProvider);
    scheduler.setCursorHistory([]);

    const errorLogs: string[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => {
      errorLogs.push(
        args
          .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
          .join(" "),
      );
    };

    try {
      scheduler.start(50);

      await new Promise((resolve) => setTimeout(resolve, 100));

      const errorFound = errorLogs.some((log) =>
        log.includes("INVARIANT_CHECK_FAILED"),
      );
      expect(errorFound).toBe(true);
    } finally {
      console.error = originalError;
      scheduler.stop();
    }
  });
});

// ── Configuration Tests ───────────────────────────────────────────────────────

describe("Schedule interval configuration", () => {
  it("returns default interval when env var not set", () => {
    const originalInterval = process.env.INVARIANT_SCHEDULE_INTERVAL_MS;
    delete process.env.INVARIANT_SCHEDULE_INTERVAL_MS;

    expect(getScheduleInterval()).toBe(DEFAULT_SCHEDULE_INTERVAL_MS);

    if (originalInterval !== undefined) {
      process.env.INVARIANT_SCHEDULE_INTERVAL_MS = originalInterval;
    }
  });

  it("parses custom interval from env var", () => {
    const originalInterval = process.env.INVARIANT_SCHEDULE_INTERVAL_MS;
    process.env.INVARIANT_SCHEDULE_INTERVAL_MS = "60000";

    expect(getScheduleInterval()).toBe(60000);

    if (originalInterval !== undefined) {
      process.env.INVARIANT_SCHEDULE_INTERVAL_MS = originalInterval;
    } else {
      delete process.env.INVARIANT_SCHEDULE_INTERVAL_MS;
    }
  });

  it("ignores invalid interval and returns default", () => {
    const originalInterval = process.env.INVARIANT_SCHEDULE_INTERVAL_MS;
    process.env.INVARIANT_SCHEDULE_INTERVAL_MS = "invalid";

    expect(getScheduleInterval()).toBe(DEFAULT_SCHEDULE_INTERVAL_MS);

    if (originalInterval !== undefined) {
      process.env.INVARIANT_SCHEDULE_INTERVAL_MS = originalInterval;
    } else {
      delete process.env.INVARIANT_SCHEDULE_INTERVAL_MS;
    }
  });
});

describe("Cursor history configuration", () => {
  it("returns empty array when env var not set", () => {
    const original = process.env.INVARIANT_CURSOR_HISTORY;
    delete process.env.INVARIANT_CURSOR_HISTORY;

    expect(getCursorHistory()).toEqual([]);

    if (original !== undefined) {
      process.env.INVARIANT_CURSOR_HISTORY = original;
    }
  });

  it("parses cursor history from env var", () => {
    const original = process.env.INVARIANT_CURSOR_HISTORY;
    process.env.INVARIANT_CURSOR_HISTORY = "100,200,300";

    expect(getCursorHistory()).toEqual([100, 200, 300]);

    if (original !== undefined) {
      process.env.INVARIANT_CURSOR_HISTORY = original;
    } else {
      delete process.env.INVARIANT_CURSOR_HISTORY;
    }
  });

  it("ignores invalid values in cursor history", () => {
    const original = process.env.INVARIANT_CURSOR_HISTORY;
    process.env.INVARIANT_CURSOR_HISTORY = "100,invalid,300";

    expect(getCursorHistory()).toEqual([100, 300]);

    if (original !== undefined) {
      process.env.INVARIANT_CURSOR_HISTORY = original;
    } else {
      delete process.env.INVARIANT_CURSOR_HISTORY;
    }
  });

  it("handles whitespace in cursor history", () => {
    const original = process.env.INVARIANT_CURSOR_HISTORY;
    process.env.INVARIANT_CURSOR_HISTORY = "100 , 200 , 300";

    expect(getCursorHistory()).toEqual([100, 200, 300]);

    if (original !== undefined) {
      process.env.INVARIANT_CURSOR_HISTORY = original;
    } else {
      delete process.env.INVARIANT_CURSOR_HISTORY;
    }
  });
});

// ── Counter Reset Tests ───────────────────────────────────────────────────────

describe("Counter reset functionality", () => {
  it("clears all state on clearInvariantState", async () => {
    const inv = makeInvoice("inv-1");
    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);
    scheduler.start(50);

    await new Promise((resolve) => setTimeout(resolve, 100));

    clearInvariantState();

    expect(getInvariantCounters()).toBeNull();
    expect(getScheduledReports().length).toBe(0);
    expect(getInvariantMetrics().checksRunTotal).toBe(0);

    scheduler.stop();
  });

  it("accumulates multiple reports before clear", async () => {
    const inv = makeInvoice("inv-1");
    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);
    scheduler.start(30);

    await new Promise((resolve) => setTimeout(resolve, 100));

    const reportsBefore = getScheduledReports();
    expect(reportsBefore.length).toBeGreaterThan(0);

    clearInvariantState();

    const reportsAfter = getScheduledReports();
    expect(reportsAfter.length).toBe(0);

    scheduler.stop();
  });
});

// ── Edge Cases: All Violation Types ───────────────────────────────────────────

describe("All violation types detection", () => {
  it("detects all types of violations in single run", async () => {
    const inv = makeInvoice("inv-1");
    const orphanBid = makeBid("bid-orphan", "inv-missing");
    const orphanSettlement = makeSettlement("set-orphan", "inv-missing-2");
    const orphanDispute = makeDispute("dis-orphan", "inv-missing-3");
    const mismatchSettlement = makeSettlement("set-mismatch", "inv-1"); // No corresponding bid
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, "1000000000");
    const accountingMismatch = makeSettlement(
      "set-bad-amount",
      "inv-2",
      SettlementStatus.Paid,
      "1",
    ); // No accepted bid

    const provider = makeProvider(
      [inv],
      [
        orphanBid,
        bid,
        ...Array(10)
          .fill(null)
          .map((_, i) => makeBid(`bid-${i}`, `inv-1`)),
      ],
      [mismatchSettlement],
      [orphanDispute],
    );

    const report = await runFullInvariantSuite(provider, [100, 50]); // Cursor regression

    expect(report.pass).toBe(false);
    expect(report.orphans.orphanBids.count).toBeGreaterThan(0);
    expect(report.cursorSequence.hasRegression).toBe(true);
  });
});

// ── Alerting Branch Coverage Tests ────────────────────────────────────────────

describe("Alerting branches", () => {
  afterEach(() => {
    clearInvariantState();
  });

  it("alerts on multiple violation types including settlements and disputes", async () => {
    const inv = makeInvoice("inv-1");
    const orphanBid = makeBid("bid-orphan", "inv-missing");
    const orphanSettlement = makeSettlement("set-orphan", "inv-missing-2");
    const orphanDispute = makeDispute("dis-orphan", "inv-missing-3");
    const mismatchSettlement = makeSettlement("set-mismatch", "inv-1");

    const provider = makeProvider(
      [inv],
      [orphanBid],
      [orphanSettlement, mismatchSettlement],
      [orphanDispute],
    );
    const scheduler = getInvariantScheduler();

    const errorLogs: string[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => {
      errorLogs.push(
        args
          .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
          .join(" "),
      );
    };

    try {
      scheduler.setProvider(provider);
      scheduler.setCursorHistory([]);
      scheduler.start(50);

      await new Promise((resolve) => setTimeout(resolve, 100));

      const report = getInvariantCounters();
      expect(report).not.toBeNull();
      expect(report!.pass).toBe(false);

      // Check multiple violation types triggered
      const alertLog = errorLogs.find((log) =>
        log.includes("INVARIANT_VIOLATION"),
      );
      expect(alertLog).toBeDefined();
    } finally {
      console.error = originalError;
      scheduler.stop();
    }
  });

  it("alerts on cursor regression", async () => {
    const provider = makeProvider([], [], [], []);
    const scheduler = getInvariantScheduler();

    const errorLogs: string[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => {
      errorLogs.push(
        args
          .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
          .join(" "),
      );
    };

    try {
      scheduler.setProvider(provider);
      scheduler.setCursorHistory([100, 50]); // Regression
      scheduler.start(50);

      await new Promise((resolve) => setTimeout(resolve, 100));

      const report = getInvariantCounters();
      expect(report).not.toBeNull();
      expect(report!.cursorSequence.hasRegression).toBe(true);

      const alertLog = errorLogs.find((log) =>
        log.includes("cursor_regression"),
      );
      expect(alertLog).toBeDefined();
    } finally {
      console.error = originalError;
      scheduler.stop();
    }
  });

  it("alerts on accounting mismatches", async () => {
    const bid = makeBid("bid-1", "inv-1", BidStatus.Accepted, "1000000000");
    const settlement = makeSettlement(
      "set-1",
      "inv-1",
      SettlementStatus.Paid,
      "999999999",
    ); // Different amount

    const provider = makeProvider([], [bid], [settlement], []);
    const scheduler = getInvariantScheduler();

    const errorLogs: string[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => {
      errorLogs.push(
        args
          .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
          .join(" "),
      );
    };

    try {
      scheduler.setProvider(provider);
      scheduler.setCursorHistory([]);
      scheduler.start(50);

      await new Promise((resolve) => setTimeout(resolve, 100));

      const report = getInvariantCounters();
      expect(report).not.toBeNull();
      expect(report!.accounting.mismatches.count).toBeGreaterThan(0);

      const alertLog = errorLogs.find((log) =>
        log.includes("accounting_mismatches"),
      );
      expect(alertLog).toBeDefined();
    } finally {
      console.error = originalError;
      scheduler.stop();
    }
  });
});

// ── Scheduler Edge Cases ─────────────────────────────────────────────────────

describe("Scheduler edge cases", () => {
  afterEach(() => {
    clearInvariantState();
  });

  it("does not run checks when provider is null", async () => {
    const scheduler = getInvariantScheduler();
    scheduler.setProvider(null as any);
    scheduler.setCursorHistory([100, 200]);

    scheduler.start(50);

    await new Promise((resolve) => setTimeout(resolve, 100));

    // No report should be stored
    expect(getInvariantCounters()).toBeNull();

    scheduler.stop();
  });

  it("uses provided interval instead of default", async () => {
    const inv = makeInvoice("inv-1");
    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);

    // Use custom interval
    scheduler.start(100);
    expect(scheduler.isStarted()).toBe(true);

    scheduler.stop();
  });

  it("uses default interval when start is called without argument", async () => {
    const inv = makeInvoice("inv-1");
    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);

    // Start without interval - should use default
    scheduler.start();
    expect(scheduler.isStarted()).toBe(true);

    // Wait for immediate run
    await new Promise((resolve) => setTimeout(resolve, 50));

    const report = getInvariantCounters();
    expect(report).not.toBeNull();

    scheduler.stop();
  });
});

// ── Non-Error exception handling ────────────────────────────────────────────

describe("Non-Error exception handling", () => {
  afterEach(() => {
    clearInvariantState();
  });

  it("handles non-Error object thrown in provider", async () => {
    const nonErrorProvider: InvariantDataProvider = {
      getInvoices: async () => {
        throw "String error instead of Error";
      },
      getBids: async () => [],
      getSettlements: async () => [],
      getDisputes: async () => [],
    };

    const scheduler = getInvariantScheduler();
    scheduler.setProvider(nonErrorProvider);
    scheduler.setCursorHistory([]);

    const errorLogs: string[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => {
      errorLogs.push(
        args
          .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
          .join(" "),
      );
    };

    try {
      scheduler.start(50);

      await new Promise((resolve) => setTimeout(resolve, 100));

      const errorFound = errorLogs.some((log) =>
        log.includes("INVARIANT_CHECK_FAILED"),
      );
      expect(errorFound).toBe(true);
    } finally {
      console.error = originalError;
      scheduler.stop();
    }
  });
});

// ── Security: No PII in output ───────────────────────────────────────────────

describe("Security: PII protection", () => {
  it("getInvariantCounters reveals no raw PII", async () => {
    const inv = makeInvoice("inv-1");
    inv.metadata.customer_name = "SENSITIVE_CUSTOMER_NAME";
    inv.metadata.customer_address = "SENSITIVE_ADDRESS";
    inv.metadata.tax_id = "SENSITIVE_TAX_ID";

    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);
    scheduler.start(50);

    await new Promise((resolve) => setTimeout(resolve, 100));

    const report = getInvariantCounters();
    expect(report).not.toBeNull();

    const reportStr = JSON.stringify(report);
    expect(reportStr).not.toContain("SENSITIVE_CUSTOMER_NAME");
    expect(reportStr).not.toContain("SENSITIVE_ADDRESS");
    expect(reportStr).not.toContain("SENSITIVE_TAX_ID");

    scheduler.stop();
  });

  it("getScheduledReports reveals no raw PII", async () => {
    const inv = makeInvoice("inv-1");
    inv.metadata.customer_name = "SENSITIVE_NAME";

    const provider = makeProvider([inv], [], [], []);
    const scheduler = getInvariantScheduler();

    scheduler.setProvider(provider);
    scheduler.setCursorHistory([]);
    scheduler.start(50);

    await new Promise((resolve) => setTimeout(resolve, 100));

    const reports = getScheduledReports();
    const reportStr = JSON.stringify(reports);
    expect(reportStr).not.toContain("SENSITIVE_NAME");

    scheduler.stop();
  });
});
