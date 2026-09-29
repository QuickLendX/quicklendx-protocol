import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

const CREATE_WEBHOOK_DELIVERIES_TABLE = `
  CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    subscriber_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
      CHECK(status IN ('pending','processing','success','failed','dead_letter')),
    enqueued_at TEXT NOT NULL DEFAULT (datetime('now')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    next_retry_at TEXT,
    last_error TEXT,
    last_attempt_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
  )
`;

const CREATE_RETRY_INDEX = `
  CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_status_next_retry
  ON webhook_deliveries(status, next_retry_at)
`;

const CREATE_CLEANUP_INDEX = `
  CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_created_at
  ON webhook_deliveries(created_at)
`;

export default {
  version: 6,
  name: "webhook_deliveries",
  authoredAt: "2026-06-23",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(CREATE_WEBHOOK_DELIVERIES_TABLE);
    await ctx.db.exec(CREATE_RETRY_INDEX);
    await ctx.db.exec(CREATE_CLEANUP_INDEX);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];
    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'webhook_deliveries'"
    );
    if (existing) {
      warnings.push("Table webhook_deliveries already exists — migration is idempotent.");
    }
    return warnings;
  },
} satisfies MigrationDefinition;
