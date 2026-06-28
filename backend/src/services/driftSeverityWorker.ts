/**
 * driftSeverityWorker.ts
 *
 * Compares on-chain state against the local index and classifies any detected
 * drift by severity (PR #1391). When HIGH severity drift is detected the
 * worker:
 *
 *  1. Immediately pauses the active backfill run (idempotent).
 *  2. Routes a critical alert through the AlertRouter (deduplicated).
 *
 * Severity rules:
 *  - LOW    : invoiceMismatches === 1  AND  settlementAccountingMismatches === 0
 *  - MEDIUM : 2 <= invoiceMismatches <= 100  AND  settlementAccountingMismatches === 0
 *  - HIGH   : invoiceMismatches > 100  OR  settlementAccountingMismatches >= 1
 *
 * NOTE: This is distinct from src/services/reconciliationWorker.ts (the static
 * worker that runs reconciliation passes and produces drift-item reports). The
 * `ReconciliationWorker` class exported here is the drift-severity processor
 * that the drift-severity test exercises.
 */

import { Severity } from "../types/reconciliation";
import { DriftReport } from "../types/driftSeverity";
import {
  BackfillService,
  driftBackfillService as defaultBackfillService,
} from "./driftBackfillService";
import { AlertRouter, alertRouter as defaultAlertRouter } from "./alertRouter";

// ---------------------------------------------------------------------------
// Severity classifier (pure function - easy to unit-test in isolation)
// ---------------------------------------------------------------------------

/**
 * Classifies the severity of a drift report.
 *
 * The function is intentionally pure: it takes only the report and returns a
 * Severity value without any side effects.
 */
export function classifyDrift(report: DriftReport): Severity {
  const { invoiceMismatches, settlementAccountingMismatches } = report;

  // HIGH: any settlement accounting mismatch, or more than 100 invoice mismatches
  if (settlementAccountingMismatches >= 1 || invoiceMismatches > 100) {
    return Severity.HIGH;
  }

  // LOW: exactly 1 invoice mismatch
  if (invoiceMismatches === 1) {
    return Severity.LOW;
  }

  // MEDIUM: 2 - 100 invoice mismatches (inclusive)
  if (invoiceMismatches >= 2 && invoiceMismatches <= 100) {
    return Severity.MEDIUM;
  }

  // 0 mismatches across both dimensions - treat as LOW (no real drift)
  return Severity.LOW;
}

// ---------------------------------------------------------------------------
// Alert key derivation
// ---------------------------------------------------------------------------

/**
 * Produces a stable alert key for a given (runId, drift-type) pair.
 * Keeping the key stable across consecutive worker runs enables deduplication.
 */
export function buildAlertKey(runId: string): string {
  return `HIGH_DRIFT:${runId}`;
}

// ---------------------------------------------------------------------------
// ReconciliationWorker
// ---------------------------------------------------------------------------

export interface ReconciliationWorkerOptions {
  backfillService?: BackfillService;
  alertRouter?: AlertRouter;
}

export class ReconciliationWorker {
  private readonly backfillService: BackfillService;
  private readonly alertRouter: AlertRouter;

  constructor(options: ReconciliationWorkerOptions = {}) {
    this.backfillService = options.backfillService ?? defaultBackfillService;
    this.alertRouter = options.alertRouter ?? defaultAlertRouter;
  }

  // --------------------------------------------------------------------------
  // Main entry point
  // --------------------------------------------------------------------------

  /**
   * Processes a drift report:
   *  1. Classifies severity.
   *  2. If HIGH - pauses the backfill run and routes a critical alert.
   *  3. If MEDIUM/LOW - routes a standard (non-critical) informational alert
   *     only when there is actual drift (mismatches > 0).
   *
   * Returns the computed severity so callers can branch on it.
   */
  public async processDriftReport(report: DriftReport): Promise<Severity> {
    const severity = classifyDrift(report);

    if (severity === Severity.HIGH) {
      await this.handleHighSeverity(report);
    } else if (report.invoiceMismatches > 0) {
      // Route informational alert for non-zero LOW/MEDIUM drift
      const alertKey = buildAlertKey(report.runId);
      await this.alertRouter.routeAlert(
        alertKey,
        severity,
        `Drift detected (${severity}): ${report.invoiceMismatches} invoice mismatch(es), ` +
          `${report.settlementAccountingMismatches} settlement accounting mismatch(es)`
      );
    }

    return severity;
  }

  // --------------------------------------------------------------------------
  // HIGH-severity handler
  // --------------------------------------------------------------------------

  private async handleHighSeverity(report: DriftReport): Promise<void> {
    const { runId, invoiceMismatches, settlementAccountingMismatches } = report;

    const reason =
      `HIGH severity drift detected: ${invoiceMismatches} invoice mismatch(es), ` +
      `${settlementAccountingMismatches} settlement accounting mismatch(es)`;

    // 1. Pause the backfill run (idempotent - safe to call multiple times)
    await this.backfillService.pauseRun(runId, reason);

    // 2. Route critical alert (deduplicated by alertKey)
    const alertKey = buildAlertKey(runId);
    await this.alertRouter.routeAlert(alertKey, Severity.HIGH, reason);
  }
}

// ---------------------------------------------------------------------------
// Module-level singleton (mirrors the pattern in statusService.ts)
// ---------------------------------------------------------------------------

export const driftSeverityWorker = new ReconciliationWorker();
