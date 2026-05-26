import { MockDataProviders } from "./mockDataProviders";
import { DriftReport, DriftItem, BackfillResult } from "../types/reconciliation";
import { Invoice } from "../types/contract";

export class ReconciliationWorker {
  private static reports: DriftReport[] = [];
  private static isRunning: boolean = false;
  private static backfillBatchSize: number = 10;
  public static failBackfill: boolean = false;


  static async runReconciliation(): Promise<DriftReport> {
    if (this.isRunning) {
      throw new Error("Reconciliation already in progress");
    }

    this.isRunning = true;
    try {
      // Simulate network latency
      await new Promise(resolve => setTimeout(resolve, 100));
      const indexed = MockDataProviders.getIndexedInvoices();
      const onChain = MockDataProviders.getOnChainInvoices();
      const drifts: DriftItem[] = [];

      // Check for missing or mismatched records
      onChain.forEach((oc) => {
        const idx = indexed.find((i) => i.id === oc.id);
        if (!idx) {
          drifts.push({
            id: oc.id,
            type: "Invoice",
            driftType: "MISSING",
            onChainValue: oc,
          });
        } else if (idx.status !== oc.status) {
          drifts.push({
            id: oc.id,
            type: "Invoice",
            driftType: "STATUS_MISMATCH",
            indexedValue: idx.status,
            onChainValue: oc.status,
          });
        }
      });

      const report: DriftReport = {
        timestamp: Math.floor(Date.now() / 1000),
        totalRecordsChecked: onChain.length,
        driftCount: drifts.length,
        drifts,
      };

      this.reports.push(report);
      return report;
    } finally {
      this.isRunning = false;
    }
  }

  static async triggerBoundedBackfill(report: DriftReport): Promise<BackfillResult> {
    const result: BackfillResult = {
      successCount: 0,
      failCount: 0,
      errors: [],
    };

    // Process only up to backfillBatchSize items
    const toProcess = report.drifts.slice(0, this.backfillBatchSize);

    for (const drift of toProcess) {
      try {
        if (ReconciliationWorker.failBackfill) {
          throw new Error("Simulated failure");
        }
        // Simulate backfill logic
        console.log(`Backfilling ${drift.type} ${drift.id}...`);

        result.successCount++;
      } catch (error: any) {
        result.failCount++;
        result.errors.push(`Failed to backfill ${drift.id}: ${error.message}`);
      }
    }

    return result;
  }

  static getLatestReport(): DriftReport | null {
    return this.reports.length > 0 ? this.reports[this.reports.length - 1] : null;
  }

  static getAllReports(): DriftReport[] {
    return this.reports;
  }
}
