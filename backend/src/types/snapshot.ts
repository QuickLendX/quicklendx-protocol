export interface BestBidSnapshot {
  invoice_id: string;
  bid_id: string;
  investor: string;
  bid_amount: string;
  expected_return: string;
  timestamp: number;
  expiration_timestamp: number;
  block_timestamp: number; // For tie-breaking
  transaction_sequence: number; // For tie-breaking
  ledger_index: number; // For tie-breaking
  last_updated: number;
}

export interface TopBid {
  bid_id: string;
  investor: string;
  bid_amount: string;
  expected_return: string;
  timestamp: number;
  expiration_timestamp: number;
  rank: number;
}

export interface TopBidsSnapshot {
  invoice_id: string;
  top_bids: TopBid[];
  last_updated: number;
}

export interface BidEvent {
  event_type: 'BidPlaced' | 'BidUpdated' | 'BidWithdrawn';
  bid_id: string;
  invoice_id: string;
  investor: string;
  bid_amount: string;
  expected_return: string;
  timestamp: number;
  expiration_timestamp: number;
  block_timestamp: number;
  transaction_sequence: number;
  ledger_index: number;
}

// ── Periodic snapshot capture ─────────────────────────────────────────────────

/**
 * A point-in-time snapshot of all derived tables captured by the snapshot
 * scheduler.  The `atLedger` field is the highest ledger included in the
 * captured state; `capturedAt` is a wall-clock ISO-8601 timestamp.
 *
 * PII fields (investor addresses, business wallet addresses) are redacted to
 * a deterministic HMAC-SHA-256 pseudonym before being stored so that the
 * snapshot file itself contains no raw personally identifiable information.
 */
export interface DerivedStateSnapshot {
  /** Sequential snapshot ID – monotonically increasing. */
  snapshotId: string;
  /** Ledger height included in this snapshot. */
  atLedger: number;
  /** Wall-clock capture time (ISO 8601). */
  capturedAt: string;
  /** SHA-256 digest of the full derived state at `atLedger`. */
  stateHash: string;
  /** Per-table row counts, used for quick divergence triage. */
  tableCounts: TableCounts;
  /** PII-scrubbed table payloads. */
  tables: RedactedTablePayload;
  /** Whether this snapshot was taken mid-batch (during active event processing). */
  midBatch: boolean;
}

export interface TableCounts {
  invoices: number;
  bids: number;
  settlements: number;
  disputes: number;
  notifications: number;
}

/** All wallet/investor address strings are replaced with their HMAC pseudonyms. */
export interface RedactedTablePayload {
  invoices: RedactedRow[];
  bids: RedactedRow[];
  settlements: RedactedRow[];
  disputes: RedactedRow[];
  notifications: RedactedRow[];
}

export type RedactedRow = Record<string, unknown>;

// ── Replay-equivalence result ──────────────────────────────────────────────────

export type VerificationOutcome = "match" | "mismatch" | "skipped" | "error";

export interface RowDiff {
  table: keyof TableCounts;
  key: string;
  snapshotValue: unknown;
  replayedValue: unknown;
}

export interface ReplayVerificationResult {
  verificationId: string;
  snapshotId: string;
  atLedger: number;
  outcome: VerificationOutcome;
  snapshotHash: string;
  replayHash: string;
  /** Number of rows that diverged across all tables. */
  divergentRowCount: number;
  /** Up to 20 sample diffs for diagnostics. */
  diffs: RowDiff[];
  startedAt: string;
  completedAt: string;
  /** Present when outcome === "error". */
  error?: string;
}

// ── Snapshot schedule configuration ──────────────────────────────────────────

export interface SnapshotScheduleConfig {
  /** How often (in milliseconds) to capture a snapshot. Default: 60 000. */
  intervalMs: number;
  /** Maximum number of snapshots to retain before pruning. Default: 100. */
  maxRetained: number;
  /**
   * HMAC secret used to pseudonymise PII fields.  Must be a non-empty string.
   * In production this should be sourced from a secrets manager.
   */
  hmacSecret: string;
}