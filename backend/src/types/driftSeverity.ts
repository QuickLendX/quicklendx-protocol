/**
 * driftSeverity.ts
 *
 * Domain types for the drift-severity classifier and gated backfill-recovery
 * lifecycle (PR #1391).
 *
 * NOTE: These intentionally live in their own module rather than in
 * ./reconciliation because two of the names collide with already-merged
 * symbols that have a different shape:
 *   - `DriftReport`  also exists in ./reconciliation (drift-item report used by
 *     the reconciliation worker / backfill SQL path).
 *   - `BackfillRun` / `BackfillRunStatus` also exist in ./backfill (the
 *     ledger-range backfill run used by BackfillService.startBackfill).
 *
 * The shared alerting types (`Severity`, `AlertStatus`, `Alert`) live in
 * ./reconciliation because the shared AlertRouter imports them from there;
 * they are re-exported here for convenience.
 */

export { Severity, AlertStatus } from "./reconciliation";
export type { Alert } from "./reconciliation";

// ---------------------------------------------------------------------------
// Drift report (severity-classifier input)
// ---------------------------------------------------------------------------

/**
 * The output of one reconciliation pass, summarised for severity
 * classification: how many on-chain records diverge from what is indexed.
 */
export interface DriftReport {
  /** Unique identifier for this reconciliation run (UUID or similar). */
  runId: string;
  /** UTC epoch-milliseconds when this report was produced. */
  timestamp: number;
  /** Number of invoices where on-chain state !== indexed state. */
  invoiceMismatches: number;
  /** Number of settlements where amounts do not reconcile. */
  settlementAccountingMismatches: number;
}

// ---------------------------------------------------------------------------
// Backfill run lifecycle (gated recovery)
// ---------------------------------------------------------------------------

export enum BackfillRunStatus {
  Running = "Running",
  Paused = "Paused",
  Completed = "Completed",
  Failed = "Failed",
}

export interface BackfillRun {
  runId: string;
  status: BackfillRunStatus;
  /** Reason the run was paused (set when status transitions to Paused). */
  pauseReason?: string;
  /**
   * When status is Paused, whether the operator has acknowledged the
   * triggering alert. Only when this is `true` may the run be resumed.
   */
  alertAcknowledged: boolean;
}
