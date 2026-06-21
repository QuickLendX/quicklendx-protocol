/**
 * reconciliation.ts
 *
 * Shared domain types for the reconciliation, drift-severity, backfill, and
 * alerting subsystems.  All other service modules import from here to maintain
 * a single source of truth.
 */

// ---------------------------------------------------------------------------
// Drift report
// ---------------------------------------------------------------------------

/**
 * The output of one reconciliation pass: counts how many on-chain records
 * diverge from what is indexed.
 */
export interface DriftReport {
  /** Unique identifier for this reconciliation run (UUID or similar). */
  runId: string;
  /** UTC epoch-milliseconds when this report was produced. */
  timestamp: number;
  /** Number of invoices where on-chain state ≠ indexed state. */
  invoiceMismatches: number;
  /** Number of settlements where amounts do not reconcile. */
  settlementAccountingMismatches: number;
}

// ---------------------------------------------------------------------------
// Severity
// ---------------------------------------------------------------------------

export enum Severity {
  LOW = "LOW",
  MEDIUM = "MEDIUM",
  HIGH = "HIGH",
}

// ---------------------------------------------------------------------------
// Backfill run
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
   * triggering alert.  Only when this is `true` may the run be resumed.
   */
  alertAcknowledged: boolean;
}

// ---------------------------------------------------------------------------
// Alerts
// ---------------------------------------------------------------------------

export enum AlertStatus {
  Open = "Open",
  Acknowledged = "Acknowledged",
}

export interface Alert {
  /** Stable key used for deduplication: one open alert per (runId, type). */
  alertKey: string;
  severity: Severity;
  message: string;
  status: AlertStatus;
  /** UTC epoch-ms of first fire. */
  createdAt: number;
  /** UTC epoch-ms of most recent acknowledgement (undefined if not yet acked). */
  acknowledgedAt?: number;
}
