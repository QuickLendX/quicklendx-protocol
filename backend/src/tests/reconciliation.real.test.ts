import { ReconciliationWorker } from "../services/reconciliationWorker";
import { derivedTableStore } from "../services/replayService";
import { rpcClient } from "../services/rpcClient";
import { InvoiceStatus } from "../types/contract";
import { closeDatabase, getDatabase } from "../lib/database";

describe("ReconciliationWorker (real)", () => {
  beforeEach(async () => {
    process.env.DATABASE_PATH = ":memory:";
    closeDatabase();
    getDatabase().exec(`
      CREATE TABLE IF NOT EXISTS reconciliation_snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        run_at TEXT NOT NULL,
        checked_count INTEGER NOT NULL CHECK(checked_count >= 0),
        drift_count INTEGER NOT NULL CHECK(drift_count >= 0),
        severity TEXT NOT NULL CHECK(severity IN ('LOW', 'MEDIUM', 'HIGH'))
      );
    `);

    (ReconciliationWorker as any).reports = [];
    (ReconciliationWorker as any).isRunning = false;
    // Reset derived store
    try {
      await derivedTableStore.clearDerivedTables();
    } catch {}
  });

  afterEach(() => {
    closeDatabase();
  });

  test("detects missing and status mismatch using indexed store + rpc", async () => {
    // Seed indexed store: invoice_1 with Pending, invoice_3 Paid (invoice_2 missing)
    const idx1 = {
      id: "invoice_1",
      status: InvoiceStatus.Pending,
    };
    const idx3 = {
      id: "invoice_3",
      status: InvoiceStatus.Paid,
    };

    await (derivedTableStore as any).upsertInvoice(idx1);
    await (derivedTableStore as any).upsertInvoice(idx3);

    // Mock RPC to return canonical on-chain state
    const onChain = [
      { id: "invoice_1", status: InvoiceStatus.Verified },
      { id: "invoice_2", status: InvoiceStatus.Funded },
      { id: "invoice_3", status: InvoiceStatus.Paid },
    ];

    jest.spyOn(rpcClient, "call").mockResolvedValue(onChain);

    const report = await ReconciliationWorker.runReconciliation();

    expect(report.totalRecordsChecked).toBe(3);
    expect(report.driftCount).toBe(2);

    const missing = report.drifts.find((d) => d.driftType === "MISSING");
    const mismatch = report.drifts.find((d) => d.driftType === "STATUS_MISMATCH");

    expect(missing).toBeDefined();
    expect(missing?.id).toBe("invoice_2");

    expect(mismatch).toBeDefined();
    expect(mismatch?.id).toBe("invoice_1");

    // restore rpc mock
    (rpcClient.call as any).mockRestore?.();
  });
});
