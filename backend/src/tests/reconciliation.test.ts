import { ReconciliationWorker } from "../services/reconciliationWorker";
import { MockDataProviders } from "../services/mockDataProviders";

describe("ReconciliationWorker", () => {
  beforeEach(() => {
    // Reset internal state if needed (static members are shared)
    (ReconciliationWorker as any).reports = [];
    (ReconciliationWorker as any).isRunning = false;
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
});
