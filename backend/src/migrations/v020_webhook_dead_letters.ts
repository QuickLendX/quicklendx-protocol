/**
 * v020_webhook_dead_letters
 *
 * Author: QuickLendX Engineering
 * Created: 2026-11-23
 *
 * Creates the webhook_dead_letters table to persist exhausted and permanently
 * failed webhook deliveries for operational inspection and replay.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

const CREATE_WEBHOOK_DEAD_LETTERS_TABLE = `
  CREATE TABLE IF NOT EXISTS webhook_dead_letters (
    id TEXT PRIMARY KEY,
    delivery_id TEXT,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    target_url TEXT,
    subscriber_id TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    last_error TEXT,
    status TEXT NOT NULL DEFAULT 'dead_letter'
      CHECK(status IN ('dead_letter', 'replayed')),
    replayed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
  )
`;

const CREATE_STATUS_INDEX = `
  CREATE INDEX IF NOT EXISTS idx_webhook_dead_letters_status
  ON webhook_dead_letters(status)
`;

const CREATE_CREATED_AT_INDEX = `
  CREATE INDEX IF NOT EXISTS idx_webhook_dead_letters_created_at
  ON webhook_dead_letters(created_at)
`;

const CREATE_DELIVERY_ID_INDEX = `
  CREATE INDEX IF NOT EXISTS idx_webhook_dead_letters_delivery_id
  ON webhook_dead_letters(delivery_id)
`;

export default {
  version: 20,
  name: "webhook_dead_letters",
  authoredAt: "2026-11-23",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(CREATE_WEBHOOK_DEAD_LETTERS_TABLE);
    await ctx.db.exec(CREATE_STATUS_INDEX);
    await ctx.db.exec(CREATE_CREATED_AT_INDEX);
    await ctx.db.exec(CREATE_DELIVERY_ID_INDEX);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("DROP INDEX IF EXISTS idx_webhook_dead_letters_delivery_id");
    await ctx.db.exec("DROP INDEX IF EXISTS idx_webhook_dead_letters_created_at");
    await ctx.db.exec("DROP INDEX IF EXISTS idx_webhook_dead_letters_status");
    await ctx.db.exec("DROP TABLE IF EXISTS webhook_dead_letters");
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];
    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'webhook_dead_letters'"
    );
    if (existing) {
      warnings.push("Table webhook_dead_letters already exists — migration is idempotent.");
    }
    return warnings;
  },
} satisfies MigrationDefinition;
