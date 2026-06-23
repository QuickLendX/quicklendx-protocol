import Database from "better-sqlite3";
import { backfillService, BackfillError } from "../services/backfillService";
import { DriftReport } from "../types/reconciliation";

// ---------------------------------------------------------------------------
// Database mock — gives BackfillService a real in-memory SQLite instance so
// that the drift-backfill path (triggerDriftBackfill / getDriftProgress) can
// exercise the actual SQL without touching a real file.
// ---------------------------------------------------------------------------
let mockDb: any;

const backfillStatementCache = new Map<string, any>();

jest.mock("../lib/database", () => ({
  getDatabase: () => mockDb,
  closeDatabase: jest.fn(),
  getPreparedStatement: (sql: string) => {
    if (!backfillStatementCache.has(sql)) {
      const stmt = mockDb.prepare(sql);
      backfillStatementCache.set(sql, stmt);
    }
    return backfillStatementCache.get(sql);
  },
}));

// Mock config dependency pulled in transitively via services
jest.mock("../config", () => ({
  config: {
    jwtSecret: "test-secret",
    nodeEnv: "test",
    port: 3000,
    databasePath: ":memory:",
  },
}));

// ---------------------------------------------------------------------------
// Helper: build a fake DriftReport with `n` drift items
// ---------------------------------------------------------------------------
function makeDriftReport(n: number, timestamp = 1000): DriftReport {
  const drifts = Array.from({ length: n }, (_, i) => ({
    id: `invoice_${i + 1}`,
    type: "Invoice" as const,
    driftType: "MISSING" as const,
    onChainValue: {},
  }));
  return {
    timestamp,
    totalRecordsChecked: n,
    driftCount: n,
    drifts,
  };
}

// ---------------------------------------------------------------------------
// Setup / teardown
// ---------------------------------------------------------------------------
beforeEach(async () => {
  // Create a fresh in-memory database for each test
  mockDb = new (Database as any)(":memory:");
  mockDb.pragma("foreign_keys = ON");

  // Create tables that backfillService.triggerDriftBackfill uses
  mockDb.exec(`
    CREATE TABLE IF NOT EXISTS backfill_progress (
      id TEXT PRIMARY KEY,
      audit_id INTEGER,
      run_id TEXT NOT NULL,
      last_processed_id TEXT,
      remaining_count INTEGER NOT NULL,
      total_count INTEGER NOT NULL,
      status TEXT NOT NULL CHECK(status IN ('running','paused','completed','failed')),
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS backfill_audit (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      run_id TEXT NOT NULL,
      timestamp TEXT NOT NULL,
      event_type TEXT NOT NULL,
      actor TEXT NOT NULL,
      metadata TEXT DEFAULT '{}',
      invoice_id TEXT
    );
  `);

  process.env.BACKFILL_MAX_LEDGER_RANGE = "50";
  process.env.BACKFILL_MAX_CONCURRENCY = "2";
  await backfillService.resetForTests();
});

afterEach(() => {
  if (mockDb) {
    mockDb.close();
    mockDb = null;
  }
  backfillStatementCache.clear();
});

// ===========================================================================
// Section 1 – existing ledger-range backfill tests (BackfillService core)
// ===========================================================================
describe("BackfillService – ledger-range runs", () => {
  it("returns null for a missing run and empty list initially", () => {
    expect(backfillService.getRun("missing")).toBeNull();
    expect(backfillService.listRuns()).toEqual([]);
  });

  it("throws INVALID_LEDGER_RANGE when endLedger < startLedger", async () => {
    await expect(
      backfillService.startBackfill(
        { startLedger: 10, endLedger: 5, dryRun: false, concurrency: 1 },
        "ops",
      ),
    ).rejects.toMatchObject<Partial<BackfillError>>({ code: "INVALID_LEDGER_RANGE" });
  });

  it("applies default max range when env is invalid", async () => {
    process.env.BACKFILL_MAX_LEDGER_RANGE = "not-a-number";
    await expect(
      backfillService.startBackfill(
        { startLedger: 1, endLedger: 6001, dryRun: false, concurrency: 1 },
        "ops",
      ),
    ).rejects.toMatchObject<Partial<BackfillError>>({ code: "MAX_RANGE_EXCEEDED" });
  });

  it("applies default max concurrency when env is invalid", async () => {
    process.env.BACKFILL_MAX_CONCURRENCY = "NaN";
    await expect(
      backfillService.startBackfill(
        { startLedger: 1, endLedger: 5, dryRun: false, concurrency: 5 },
        "ops",
      ),
    ).rejects.toMatchObject<Partial<BackfillError>>({ code: "MAX_CONCURRENCY_EXCEEDED" });
  });

  it("throws RUN_NOT_FOUND for pause/resume on missing run IDs", async () => {
    await expect(backfillService.pauseRun("missing", "ops")).rejects.toMatchObject<
      Partial<BackfillError>
    >({ code: "RUN_NOT_FOUND" });
    await expect(backfillService.resumeRun("missing", "ops")).rejects.toMatchObject<
      Partial<BackfillError>
    >({ code: "RUN_NOT_FOUND" });
  });

  it("throws RUN_NOT_RUNNING when pausing a completed run", async () => {
    const started = await backfillService.startBackfill(
      { startLedger: 1, endLedger: 1, dryRun: false, concurrency: 1 },
      "ops",
    );
    expect(started.run).toBeDefined();
    await new Promise((resolve) => setTimeout(resolve, 20));
    await expect(backfillService.pauseRun(started.run!.id, "ops")).rejects.toMatchObject<
      Partial<BackfillError>
    >({ code: "RUN_NOT_RUNNING" });
  });

  it("throws RUN_NOT_RESUMABLE when resuming a completed run", async () => {
    const started = await backfillService.startBackfill(
      { startLedger: 1, endLedger: 1, dryRun: false, concurrency: 1 },
      "ops",
    );
    await new Promise((resolve) => setTimeout(resolve, 20));
    await expect(backfillService.resumeRun(started.run!.id, "ops")).rejects.toMatchObject<
      Partial<BackfillError>
    >({ code: "RUN_NOT_RESUMABLE" });
  });

  it("no-ops processRun for an unknown run id", async () => {
    await expect((backfillService as any).processRun("missing")).resolves.toBeUndefined();
  });

  it("resumes failed runs and clears the previous error", async () => {
    backfillService.setFailureAtLedgerForTests(5);
    const started = await backfillService.startBackfill(
      { startLedger: 1, endLedger: 30, dryRun: false, concurrency: 1 },
      "ops",
    );
    await new Promise((resolve) => setTimeout(resolve, 25));

    const failedRun = backfillService.getRun(started.run!.id);
    expect(failedRun?.status).toBe("failed");
    expect(failedRun?.error).toBeDefined();

    backfillService.setFailureAtLedgerForTests(null);
    const resumed = await backfillService.resumeRun(started.run!.id, "ops");
    expect(resumed.status).toBe("running");
    expect(resumed.error).toBeUndefined();
  });

  it("handles stale idempotency index entries gracefully", async () => {
    (backfillService as any).idempotencyIndex.set("stale-key", "missing-run");
    const result = await backfillService.startBackfill(
      {
        startLedger: 1,
        endLedger: 3,
        dryRun: false,
        concurrency: 1,
        idempotencyKey: "stale-key",
      },
      "ops",
    );
    expect(result.idempotentReuse).toBeUndefined();
    expect(result.run).toBeDefined();
  });

  it("returns an existing run on idempotent re-use", async () => {
    const first = await backfillService.startBackfill(
      {
        startLedger: 1,
        endLedger: 3,
        dryRun: false,
        concurrency: 1,
        idempotencyKey: "idem-key",
      },
      "ops",
    );
    const second = await backfillService.startBackfill(
      {
        startLedger: 1,
        endLedger: 3,
        dryRun: false,
        concurrency: 1,
        idempotencyKey: "idem-key",
      },
      "ops",
    );
    expect(second.idempotentReuse).toBe(true);
    expect(second.run?.id).toBe(first.run?.id);
  });

  it("returns only a preview for dry-run requests", async () => {
    const result = await backfillService.startBackfill(
      { startLedger: 1, endLedger: 10, dryRun: true, concurrency: 1 },
      "ops",
    );
    expect(result.run).toBeUndefined();
    expect(result.preview.range.totalLedgers).toBe(10);
  });

  it("listRuns returns all known runs", async () => {
    await backfillService.startBackfill(
      { startLedger: 1, endLedger: 3, dryRun: false, concurrency: 1 },
      "ops",
    );
    await backfillService.startBackfill(
      { startLedger: 4, endLedger: 6, dryRun: false, concurrency: 1 },
      "ops",
    );
    expect(backfillService.listRuns().length).toBe(2);
  });
});

// ===========================================================================
// Section 2 – drift backfill (triggerDriftBackfill + getDriftProgress)
// ===========================================================================
describe("BackfillService – drift backfill (resumable)", () => {
  it("processes all items in a fresh run with batchSize >= total", async () => {
    const report = makeDriftReport(5);
    const result = await backfillService.triggerDriftBackfill(report, 10);
    expect(result.successCount).toBe(5);
    expect(result.failCount).toBe(0);
    expect(result.errors).toHaveLength(0);
  });

  it("processes only batchSize items per call (bounded pass)", async () => {
    const report = makeDriftReport(10);
    const result = await backfillService.triggerDriftBackfill(report, 4);
    expect(result.successCount).toBe(4);
    expect(result.failCount).toBe(0);
  });

  it("resumes from the checkpoint after a simulated crash mid-batch", async () => {
    const report = makeDriftReport(6, 2000);

    // First pass: process 3 items
    const pass1 = await backfillService.triggerDriftBackfill(report, 3);
    expect(pass1.successCount).toBe(3);

    // Simulate crash — do NOT reset the DB. Second call must resume from last_processed_id.
    const pass2 = await backfillService.triggerDriftBackfill(report, 3);
    expect(pass2.successCount).toBe(3); // remaining 3
    expect(pass2.failCount).toBe(0);
  });

  it("marks progress as completed when all items are processed", async () => {
    const report = makeDriftReport(3, 3000);
    await backfillService.triggerDriftBackfill(report, 10);

    const progress: any = backfillService.getDriftProgress();
    expect(progress).not.toBeNull();
    expect(progress.status).toBe("completed");
    expect(progress.remaining_count).toBe(0);
  });

  it("returns empty result and is idempotent on an already-completed run", async () => {
    const report = makeDriftReport(2, 4000);
    await backfillService.triggerDriftBackfill(report, 10); // complete it
    const second = await backfillService.triggerDriftBackfill(report, 10);
    // completed status → should return early with 0 counts
    expect(second.successCount).toBe(0);
    expect(second.failCount).toBe(0);
  });

  it("handles empty drift report (0 items) without errors", async () => {
    const report = makeDriftReport(0, 5000);
    const result = await backfillService.triggerDriftBackfill(report, 10);
    expect(result.successCount).toBe(0);
    expect(result.failCount).toBe(0);
    expect(result.errors).toHaveLength(0);
  });

  it("records failures in result and audit log when failBackfill = true", async () => {
    const report = makeDriftReport(3, 6000);
    const result = await backfillService.triggerDriftBackfill(report, 10, true);
    expect(result.failCount).toBe(3);
    expect(result.successCount).toBe(0);
    expect(result.errors[0]).toContain("Simulated failure");

    // Verify audit log entries were written for failures
    const auditRows = mockDb
      .prepare("SELECT * FROM backfill_audit WHERE event_type = 'failed'")
      .all();
    expect(auditRows.length).toBeGreaterThan(0);
  });

  it("getDriftProgress returns null when no progress has been recorded", () => {
    const progress = backfillService.getDriftProgress();
    expect(progress).toBeUndefined(); // SQLite .get() returns undefined for no rows
  });

  it("getDriftProgress reflects last_processed_id after partial run", async () => {
    const report = makeDriftReport(5, 7000);
    await backfillService.triggerDriftBackfill(report, 2);

    const progress: any = backfillService.getDriftProgress();
    expect(progress).not.toBeNull();
    expect(progress.last_processed_id).toBe("invoice_2");
    expect(progress.remaining_count).toBe(3);
    expect(progress.status).toBe("running");
  });

  it("records audit log entries per successfully backfilled item", async () => {
    const report = makeDriftReport(3, 8000);
    await backfillService.triggerDriftBackfill(report, 10);

    const rows = mockDb
      .prepare("SELECT * FROM backfill_audit WHERE event_type = 'completed'")
      .all();
    expect(rows.length).toBe(3);
    const ids = rows.map((r: any) => r.invoice_id);
    expect(ids).toContain("invoice_1");
    expect(ids).toContain("invoice_2");
    expect(ids).toContain("invoice_3");
  });

  it("creates only one progress row for multiple calls with the same report timestamp", async () => {
    const report = makeDriftReport(6, 9000);
    await backfillService.triggerDriftBackfill(report, 3);
    await backfillService.triggerDriftBackfill(report, 3); // second call = resume

    const rows = mockDb
      .prepare("SELECT COUNT(*) as cnt FROM backfill_progress WHERE run_id = 'drift_9000'")
      .get() as { cnt: number };
    expect(rows.cnt).toBe(1); // exactly one row, no duplicates
  });
});
