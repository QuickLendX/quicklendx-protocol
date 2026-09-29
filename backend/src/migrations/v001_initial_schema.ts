/**
 * v001_initial_schema
 *
 * Author: QuickLendX Engineering
 * Created: 2026-04-26
 *
 * Forward-only baseline migration.
 * Sets up core tables: backfill_runs, backfill_audit, indexer_state, webhook_subscriptions.
 *
 * Rollback: N/A (forward-only baseline)
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

const schema = `
  -- Backfill run tracking (replaces in-memory Map)
  CREATE TABLE IF NOT EXISTS backfill_runs (
    id TEXT PRIMARY KEY,
    start_ledger INTEGER NOT NULL CHECK(start_ledger >= 0),
    end_ledger INTEGER NOT NULL CHECK(end_ledger >= start_ledger),
    dry_run BOOLEAN NOT NULL DEFAULT 0,
    concurrency INTEGER NOT NULL DEFAULT 1 CHECK(concurrency > 0),
    status TEXT NOT NULL CHECK(status IN ('running', 'paused', 'completed', 'failed')),
    processed_ledgers INTEGER NOT NULL DEFAULT 0,
    cursor_ledger INTEGER NOT NULL,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    error TEXT,
    idempotency_key TEXT,
    UNIQUE(id),
    UNIQUE(idempotency_key)
  );

  CREATE INDEX idx_backfill_runs_status ON backfill_runs(status);
  CREATE INDEX idx_backfill_runs_actor ON backfill_runs(actor);
  CREATE INDEX idx_backfill_runs_created ON backfill_runs(created_at);

  -- Backfill audit log (replaces .data/backfill-audit-log.jsonl)
  CREATE TABLE IF NOT EXISTS backfill_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN ('preview', 'started', 'paused', 'resumed', 'completed', 'failed', 'idempotent_reuse')),
    actor TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',
    FOREIGN KEY (run_id) REFERENCES backfill_runs(id) ON DELETE CASCADE
  );

  CREATE INDEX idx_backfill_audit_run ON backfill_audit(run_id);
  CREATE INDEX idx_backfill_audit_timestamp ON backfill_audit(timestamp);

  -- Indexer state (persists lastIndexedLedger, maintenanceMode, etc.)
  CREATE TABLE IF NOT EXISTS indexer_state (
    key TEXT PRIMARY KEY,
    value_text TEXT,
    value_number REAL,
    value_json TEXT,
    updated_at TEXT NOT NULL,
    updated_by TEXT
  );

  -- Webhook subscriptions (persisted webhook configuration)
  CREATE TABLE IF NOT EXISTS webhook_subscriptions (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL CHECK(url LIKE 'https://%'),
    secret TEXT NOT NULL,
    events TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    last_triggered_at TEXT,
    failure_count INTEGER NOT NULL DEFAULT 0,
    max_failures INTEGER NOT NULL DEFAULT 3,
    metadata TEXT DEFAULT '{}'
  );

  CREATE INDEX idx_webhook_active ON webhook_subscriptions(is_active);
  CREATE INDEX idx_webhook_events ON webhook_subscriptions(events);

  -- Rate limit state (per IP tracking)
  CREATE TABLE IF NOT EXISTS rate_limit_state (
    ip TEXT PRIMARY KEY,
    tokens INTEGER NOT NULL DEFAULT 100 CHECK(tokens BETWEEN 0 AND 100),
    last_refill_ts INTEGER NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 0,
    blocked_until INTEGER
  );

  CREATE INDEX idx_rate_limit_ts ON rate_limit_state(last_refill_ts);

  -- Audit log for all admin/backend actions
  CREATE TABLE IF NOT EXISTS backend_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details TEXT DEFAULT '{}',
    ip TEXT,
    user_agent TEXT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
  );

  CREATE INDEX idx_backend_audit_actor ON backend_audit_log(actor);
  CREATE INDEX idx_backend_audit_resource ON backend_audit_log(resource_type, resource_id);
  CREATE INDEX idx_backend_audit_timestamp ON backend_audit_log(timestamp);

  -- System configuration store (typed key-value with history)
  CREATE TABLE IF NOT EXISTS config_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    config_key TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT NOT NULL,
    actor TEXT NOT NULL,
    changed_at TEXT NOT NULL DEFAULT (datetime('now')),
    reason TEXT
  );

  -- Current config view (materialized from latest history entry per key)
  CREATE VIEW IF NOT EXISTS current_config AS
    SELECT config_key, new_value AS value
    FROM config_history
    WHERE id IN (
      SELECT MAX(id) FROM config_history GROUP BY config_key
    );
`;

export default {
  version: 1,
  name: "initial_schema",
  authoredAt: "2026-04-26",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    const statements = schema
      .split(";")
      .map((s) => s.trim())
      .filter((s) => s.length > 0 && !s.startsWith("--"));

    for (const stmt of statements) {
      try {
        await ctx.db.exec(stmt);
      } catch (err: any) {
        console.error("Failed to execute statement:", stmt);
        throw err;
      }
    }
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    if (ctx.isProduction) {
      const existing = await ctx.db.get<{ count: number }>(
        "SELECT COUNT(*) as count FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
      );
      if (existing && existing.count > 0) {
        warnings.push("Production database already contains tables; ensure this migration is truly initial.");
      }
    }

    return warnings;
  },
} satisfies MigrationDefinition;
