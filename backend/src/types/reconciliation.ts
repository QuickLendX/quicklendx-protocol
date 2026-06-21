export interface DriftReport {
  timestamp: number;
  totalRecordsChecked: number;
  driftCount: number;
  drifts: DriftItem[];
}

export interface DriftItem {
  id: string;
  type: "Invoice" | "Bid" | "Settlement";
  driftType: "MISSING" | "STATUS_MISMATCH" | "DATA_MISMATCH";
  indexedValue?: any;
  onChainValue?: any;
}

export interface BackfillResult {
  successCount: number;
  failCount: number;
  errors: string[];
}

// ---------------------------------------------------------------------------
// Drift-severity / alerting shared types
// ---------------------------------------------------------------------------
// These types back the drift-severity classifier and alerting subsystem
// (PR #1391). They are kept here because the shared AlertRouter
// (src/services/alertRouter.ts) imports `Alert`, `AlertStatus`, and
// `Severity` from this module. The drift-severity DriftReport and the
// BackfillRun lifecycle types live in ./driftSeverity to avoid colliding
// with the reconciliation `DriftReport` above and the `BackfillRun` in
// ./backfill.

export enum Severity {
  LOW = "LOW",
  MEDIUM = "MEDIUM",
  HIGH = "HIGH",
}

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
