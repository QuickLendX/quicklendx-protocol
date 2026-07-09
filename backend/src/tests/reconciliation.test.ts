import Database from "better-sqlite3";
import { ReconciliationWorker } from "../services/reconciliationWorker";
import { MockDataProviders } from "../services/mockDataProviders";
import { rpcClient } from "../services/rpcClient";
import { derivedTableStore } from "../services/replayService";

let mockDb: any;

const statementCache = new Map<string, any>();

const createSnapshotTable = () => {
  mockDb.exec(`
    CREATE TABLE IF NOT EXISTS reconciliation_snapshots (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      run_at TEXT NOT NULL,
      checked_count INTEGER NOT NULL CHECK(checked_count >= 0),
      drift_count INTEGER NOT NULL CHECK(drift_count >= 0),
      severity TEXT NOT NULL CHECK(severity IN ('LOW', 'MEDIUM', 'HIGH'))
    );
    CREATE INDEX IF NOT EXISTS idx_reconciliation_snapshots_run_at
      ON reconciliation_snapshots(run_at DESC);
  `);
};

jest.mock("../lib/database", () => ({
  getDatabase: () => mockDb,
  closeDatabase: jest.fn(),
  getPreparedStatement: (sql: string) => {
    if (!statementCache.has(sql)) {
      const stmt = mockDb.prepare(sql);
      statementCache.set(sql, stmt);
    }
    return statementCache.get(sql);
  },
}));

jest.mock("../services/rpcClient", () => ({
  rpcClient: { call: jest.fn() },
}));

jest.mock("../services/replayService", () => ({
  derivedTableStore: { listInvoices: jest.fn() },
}));

describe("ReconciliationWorker", () => {
  beforeEach(() => {
    // Create a fresh in-memory database with required tables
    mockDb = new (Database as any)(":memory:");
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
    createSnapshotTable();

    // Reset internal state if needed (static members are shared)
    (ReconciliationWorker as any).reports = [];
    (ReconciliationWorker as any).isRunning = false;

    // Wire mock data sources
    (rpcClient.call as jest.Mock).mockResolvedValue(MockDataProviders.getOnChainInvoices());
    (derivedTableStore.listInvoices as jest.Mock).mockResolvedValue(MockDataProviders.getIndexedInvoices());
  });

  afterEach(() => {
    if (mockDb) {
      mockDb.close();
      mockDb = null;
    }
    statementCache.clear();
  });

  test("should detect drift accurately", async () => {
    const report = await ReconciliationWorker.runReconciliation();

    expect(report.totalRecordsChecked).toBe(3);
    expect(report.driftCount).toBe(2);
    
    const missing = report.drifts.find(d => d.driftType === "MISSING");
    const mismatch = report.drifts.find(d => d.driftType === "STATUS_MISMATCH");

    expect(missing).toBeDefined();
    expect(missing?.id).toBe("invoice_2");
    
    expect(mismatch).toBeDefined();
    expect(mismatch?.id).toBe("invoice_1");
  });

  test("should handle missing reports during backfill", async () => {
    const result = await ReconciliationWorker.triggerBoundedBackfill({
      timestamp: 0,
      totalRecordsChecked: 0,
      driftCount: 0,
      drifts: []
    });

    expect(result.successCount).toBe(0);
    expect(result.failCount).toBe(0);
  });

  test("should trigger bounded backfill", async () => {
    const report = await ReconciliationWorker.runReconciliation();
    const result = await ReconciliationWorker.triggerBoundedBackfill(report);

    expect(result.successCount).toBe(2);
    expect(result.failCount).toBe(0);
  });

  test("should handle backfill failures", async () => {
    const report = await ReconciliationWorker.runReconciliation();
    ReconciliationWorker.failBackfill = true;
    
    try {
      const result = await ReconciliationWorker.triggerBoundedBackfill(report);
      expect(result.failCount).toBe(2);
      expect(result.errors[0]).toContain("Simulated failure");
    } finally {
      ReconciliationWorker.failBackfill = false;
    }
  });

  test("should prevent concurrent runs", async () => {
    const p1 = ReconciliationWorker.runReconciliation();
    
    await expect(ReconciliationWorker.runReconciliation()).rejects.toThrow("Reconciliation already in progress");
    
    await p1;
  });

  test("should retrieve latest report", async () => {
    expect(ReconciliationWorker.getLatestReport()).toBeNull();
    
    await ReconciliationWorker.runReconciliation();
    const report = ReconciliationWorker.getLatestReport();
    
    expect(report).not.toBeNull();
    expect(report?.driftCount).toBe(2);
  });

  test("should retrieve all reports", async () => {
    await ReconciliationWorker.runReconciliation();
    await ReconciliationWorker.runReconciliation().catch(() => {}); // ignore concurrent error
    
    const reports = ReconciliationWorker.getAllReports();
    expect(reports.length).toBeGreaterThan(0);
  });

  test("should return an empty drift trend before snapshots exist", () => {
    expect(ReconciliationWorker.getDriftTrend()).toEqual([]);
  });

  test("should persist a reconciliation snapshot per run", async () => {
    const report = await ReconciliationWorker.runReconciliation();

    const trend = ReconciliationWorker.getDriftTrend(10);

    expect(trend).toHaveLength(1);
    expect(trend[0]).toEqual({
      runAt: new Date(report.timestamp * 1000).toISOString(),
      checkedCount: 3,
      driftCount: 2,
      severity: "MEDIUM",
    });
  });

  test("should order trend by most recent run and clamp limits", () => {
    const insert = mockDb.prepare(
      `INSERT INTO reconciliation_snapshots
         (run_at, checked_count, drift_count, severity)
       VALUES (?, ?, ?, ?)`
    );
    insert.run("2026-07-09T00:00:00.000Z", 10, 0, "LOW");
    insert.run("2026-07-09T00:01:00.000Z", 10, 2, "MEDIUM");
    insert.run("2026-07-09T00:02:00.000Z", 10, 101, "HIGH");

    expect(ReconciliationWorker.getDriftTrend(0)).toEqual([
      {
        runAt: "2026-07-09T00:02:00.000Z",
        checkedCount: 10,
        driftCount: 101,
        severity: "HIGH",
      },
    ]);

    expect(ReconciliationWorker.getDriftTrend(999)).toHaveLength(3);
  });

  test("should classify snapshot severity at drift-count boundaries", async () => {
    (rpcClient.call as jest.Mock).mockResolvedValueOnce([
      { id: "invoice_2", status: "Funded" },
    ]);

    await ReconciliationWorker.runReconciliation();
    expect(ReconciliationWorker.getDriftTrend(1)[0].severity).toBe("LOW");

    const highDriftInvoices = Array.from({ length: 101 }, (_, index) => ({
      id: `missing_invoice_${index}`,
      status: "Funded",
    }));
    (rpcClient.call as jest.Mock).mockResolvedValueOnce(highDriftInvoices);

    await ReconciliationWorker.runReconciliation();
    expect(ReconciliationWorker.getDriftTrend(1)[0].severity).toBe("HIGH");
  });
});
