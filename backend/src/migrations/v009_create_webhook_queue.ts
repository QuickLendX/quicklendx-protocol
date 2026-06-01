/**
 * v009_create_webhook_queue
 *
 * Author: QuickLendX Engineering
 * Created: 2026-06-01
 *
 * Creates:
 *   - webhook_queue: durable queue for webhook events with FIFO retrieval
 *   - queue_metadata: persistent key-value store for overflow count
 *
 * Forward-only: no down migration.
 * Rollback: DROP TABLE webhook_queue; DROP TABLE queue_metadata;
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 9,
  name: "create_webhook_queue",
  authoredAt: "2026-06-01",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    // Create the main durable queue table
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS webhook_queue (
        id TEXT PRIMARY KEY,
        type TEXT NOT NULL,
        payload TEXT NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('pending','processing','success','failed')),
        enqueued_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);

    // Create index for fast FIFO retrieval by status and time
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_queue_status_enqueued
      ON webhook_queue(status, enqueued_at)
    `);

    // Create queue metadata table for overflow count
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS queue_metadata (
        key TEXT PRIMARY KEY,
        value INTEGER NOT NULL DEFAULT 0
      )
    `);

    // Initialize overflow count if not exists
    await ctx.db.run(`
      INSERT OR IGNORE INTO queue_metadata (key, value) VALUES ('overflow_count', 0)
    `);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const existingQueue = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'webhook_queue'"
    );
    if (existingQueue) {
      warnings.push("Table webhook_queue already exists — migration is idempotent.");
    }

    const existingMetadata = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'queue_metadata'"
    );
    if (existingMetadata) {
      warnings.push("Table queue_metadata already exists — migration is idempotent.");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
