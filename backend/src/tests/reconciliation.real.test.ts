import { ReconciliationWorker } from "../services/reconciliationWorker";
import { derivedTableStore } from "../services/replayService";
import { rpcClient } from "../services/rpcClient";
import { InvoiceStatus } from "../types/contract";

describe("ReconciliationWorker (real)", () => {
  beforeEach(async () => {
    (ReconciliationWorker as any).reports = [];
    (ReconciliationWorker as any).isRunning = false;
    // Reset derived store
    try {
      await derivedTableStore.clearDerivedTables();
    } catch {}
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
