/**
 * Retention policy and safe cleanup jobs for the QuickLendX backend.
 *
 * # Data categories
 *
 * | Category          | Default TTL | Compliance hold | Notes                              |
 * |-------------------|-------------|-----------------|-------------------------------------|
 * | raw_events        | 90 days     | yes (7 years)   | Ledger events from Soroban indexer  |
 * | snapshots         | 30 days     | no              | Derived/aggregated table snapshots  |
 * | webhook_logs      | 14 days     | no              | Delivery attempt records            |
 * | operational_logs  | 7 days      | no              | Server-side debug/info logs         |
 *
 * # Safety guarantees
 *
 * 1. **Compliance hold** — raw_events with `complianceHold: true` are NEVER
 *    deleted regardless of age.  This covers AML/KYC audit trails.
 * 2. **Reconciliation window** — records younger than `reconciliationWindowMs`
 *    are never deleted even if they exceed the TTL.  This prevents cleanup
 *    from racing a backfill or re-index job.
 * 3. **Dry-run mode** — every job accepts a `dryRun` flag that returns what
 *    would be deleted without mutating state.
 * 4. **Batch limit** — each run deletes at most `batchSize` records to bound
 *    the latency impact on the running process.
 * 5. **Observable** — every run returns a `CleanupResult` with counts and
 *    the IDs that were (or would be) removed.
 */

// ── Types ─────────────────────────────────────────────────────────────────────

/** Unix timestamp in milliseconds. */
type Ms = number;

export interface RetentionConfig {
  /** Maximum age of a record before it is eligible for deletion (ms). */
  ttlMs: Ms;
  /**
   * Records younger than this window are never deleted, even if they exceed
   * the TTL.  Protects in-progress backfills and reconciliation jobs.
   */
  reconciliationWindowMs: Ms;
  /** Maximum records removed per cleanup run. */
  batchSize: number;
}

export interface RawEvent {
  id: string;
  ledger: number;
  type: string;
  payload: unknown;
  createdAt: Ms;
  /** When true the record is subject to a compliance hold and must not be deleted. */
  complianceHold: boolean;
}

export interface Snapshot {
  id: string;
  table: string;
  createdAt: Ms;
}

export interface WebhookLog {
  id: string;
  endpoint: string;
  statusCode: number;
  createdAt: Ms;
}

export interface OperationalLog {
  id: string;
  level: "debug" | "info" | "warn" | "error";
  message: string;
  createdAt: Ms;
}

export interface CleanupResult {
  category: string;
  deleted: number;
  skippedHold: number;
  skippedWindow: number;
  dryRun: boolean;
  deletedIds: string[];
}

// ── Default configuration ─────────────────────────────────────────────────────

const DAY_MS = 24 * 60 * 60 * 1000;

export const DEFAULT_CONFIG: Record<string, RetentionConfig> = {
  raw_events: {
    ttlMs: 90 * DAY_MS,
    reconciliationWindowMs: 24 * 60 * 60 * 1000, // 24 h
    batchSize: 1000,
  },
  snapshots: {
    ttlMs: 30 * DAY_MS,
    reconciliationWindowMs: 60 * 60 * 1000, // 1 h
    batchSize: 500,
  },
  webhook_logs: {
    ttlMs: 14 * DAY_MS,
    reconciliationWindowMs: 30 * 60 * 1000, // 30 min
    batchSize: 500,
  },
  operational_logs: {
    ttlMs: 7 * DAY_MS,
    reconciliationWindowMs: 15 * 60 * 1000, // 15 min
    batchSize: 2000,
  },
};

// ── In-memory store (mirrors what a DB layer would expose) ────────────────────

/**
 * RetentionStore holds all four record types in memory.
 *
 * In production this would be replaced by parameterised SQL DELETE queries.
 * The interface is kept intentionally thin so the swap is mechanical.
 */
export class RetentionStore {
  rawEvents: RawEvent[] = [];
  snapshots: Snapshot[] = [];
  webhookLogs: WebhookLog[] = [];
  operationalLogs: OperationalLog[] = [];
}

// ── Cleanup jobs ──────────────────────────────────────────────────────────────

/**
 * Determine whether a record is eligible for deletion.
 *
 * A record is eligible when ALL of the following hold:
 * - `now - createdAt >= ttlMs`  (record has aged out)
 * - `now - createdAt >= reconciliationWindowMs`  (always true when ttl >= window)
 * - `complianceHold !== true`  (not subject to a compliance hold)
 */
function isEligible(
  createdAt: Ms,
  complianceHold: boolean,
  now: Ms,
  cfg: RetentionConfig
): { eligible: boolean; reason?: "hold" | "window" } {
  if (complianceHold) return { eligible: false, reason: "hold" };
  const age = now - createdAt;
  if (age < cfg.reconciliationWindowMs) return { eligible: false, reason: "window" };
  if (age < cfg.ttlMs) return { eligible: false, reason: "window" };
  return { eligible: true };
}

/**
 * Clean up raw events older than the configured TTL.
 *
 * Records with `complianceHold: true` are NEVER deleted.
 */
export function cleanRawEvents(
  store: RetentionStore,
  cfg: RetentionConfig = DEFAULT_CONFIG.raw_events,
  opts: { dryRun?: boolean; now?: Ms } = {}
): CleanupResult {
  const now = opts.now ?? Date.now();
  const dryRun = opts.dryRun ?? false;

  let deleted = 0;
  let skippedHold = 0;
  let skippedWindow = 0;
  const deletedIds: string[] = [];
  const remaining: RawEvent[] = [];

  for (const ev of store.rawEvents) {
    if (deleted >= cfg.batchSize) {
      remaining.push(ev);
      continue;
    }
    const { eligible, reason } = isEligible(ev.createdAt, ev.complianceHold, now, cfg);
    if (eligible) {
      deletedIds.push(ev.id);
      deleted++;
      if (!dryRun) continue; // drop from remaining
    } else {
      if (reason === "hold") skippedHold++;
      else skippedWindow++;
    }
    remaining.push(ev);
  }

  if (!dryRun) store.rawEvents = remaining;

  return { category: "raw_events", deleted, skippedHold, skippedWindow, dryRun, deletedIds };
}

/**
 * Clean up derived table snapshots older than the configured TTL.
 */
export function cleanSnapshots(
  store: RetentionStore,
  cfg: RetentionConfig = DEFAULT_CONFIG.snapshots,
  opts: { dryRun?: boolean; now?: Ms } = {}
): CleanupResult {
  const now = opts.now ?? Date.now();
  const dryRun = opts.dryRun ?? false;

  let deleted = 0;
  let skippedWindow = 0;
  const deletedIds: string[] = [];
  const remaining: Snapshot[] = [];

  for (const snap of store.snapshots) {
    if (deleted >= cfg.batchSize) {
      remaining.push(snap);
      continue;
    }
    const { eligible, reason } = isEligible(snap.createdAt, false, now, cfg);
    if (eligible) {
      deletedIds.push(snap.id);
      deleted++;
      if (!dryRun) continue;
    } else {
      if (reason === "window") skippedWindow++;
    }
    remaining.push(snap);
  }

  if (!dryRun) store.snapshots = remaining;

  return { category: "snapshots", deleted, skippedHold: 0, skippedWindow, dryRun, deletedIds };
}

/**
 * Clean up webhook delivery logs older than the configured TTL.
 */
export function cleanWebhookLogs(
  store: RetentionStore,
  cfg: RetentionConfig = DEFAULT_CONFIG.webhook_logs,
  opts: { dryRun?: boolean; now?: Ms } = {}
): CleanupResult {
  const now = opts.now ?? Date.now();
  const dryRun = opts.dryRun ?? false;

  let deleted = 0;
  let skippedWindow = 0;
  const deletedIds: string[] = [];
  const remaining: WebhookLog[] = [];

  for (const log of store.webhookLogs) {
    if (deleted >= cfg.batchSize) {
      remaining.push(log);
      continue;
    }
    const { eligible, reason } = isEligible(log.createdAt, false, now, cfg);
    if (eligible) {
      deletedIds.push(log.id);
      deleted++;
      if (!dryRun) continue;
    } else {
      if (reason === "window") skippedWindow++;
    }
    remaining.push(log);
  }

  if (!dryRun) store.webhookLogs = remaining;

  return { category: "webhook_logs", deleted, skippedHold: 0, skippedWindow, dryRun, deletedIds };
}

/**
 * Clean up operational logs older than the configured TTL.
 */
export function cleanOperationalLogs(
  store: RetentionStore,
  cfg: RetentionConfig = DEFAULT_CONFIG.operational_logs,
  opts: { dryRun?: boolean; now?: Ms } = {}
): CleanupResult {
  const now = opts.now ?? Date.now();
  const dryRun = opts.dryRun ?? false;

  let deleted = 0;
  let skippedWindow = 0;
  const deletedIds: string[] = [];
  const remaining: OperationalLog[] = [];

  for (const log of store.operationalLogs) {
    if (deleted >= cfg.batchSize) {
      remaining.push(log);
      continue;
    }
    const { eligible, reason } = isEligible(log.createdAt, false, now, cfg);
    if (eligible) {
      deletedIds.push(log.id);
      deleted++;
      if (!dryRun) continue;
    } else {
      if (reason === "window") skippedWindow++;
    }
    remaining.push(log);
  }

  if (!dryRun) store.operationalLogs = remaining;

  return { category: "operational_logs", deleted, skippedHold: 0, skippedWindow, dryRun, deletedIds };
}

/**
 * Run all four cleanup jobs in sequence and return their combined results.
 *
 * This is the entry point for a scheduled cron job.
 */
export function runAllCleanupJobs(
  store: RetentionStore,
  config: Partial<Record<string, RetentionConfig>> = {},
  opts: { dryRun?: boolean; now?: Ms } = {}
): CleanupResult[] {
  return [
    cleanRawEvents(store, config.raw_events ?? DEFAULT_CONFIG.raw_events, opts),
    cleanSnapshots(store, config.snapshots ?? DEFAULT_CONFIG.snapshots, opts),
    cleanWebhookLogs(store, config.webhook_logs ?? DEFAULT_CONFIG.webhook_logs, opts),
    cleanOperationalLogs(store, config.operational_logs ?? DEFAULT_CONFIG.operational_logs, opts),
  ];
}
