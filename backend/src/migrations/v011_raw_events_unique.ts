/**
 * v011_raw_events_unique
 *
 * Author: QuickLendX Engineering
 * Created: 2026-06-23
 *
 * Persists indexer raw events with a deep-layer idempotency key on
 * (tx_hash, event_index). Normal ingestion uses ON CONFLICT DO NOTHING;
 * reorg recovery may upsert to replace canonical rows.
 *
 * Forward-only: no down migration.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 11,
  name: "raw_events_unique",
  authoredAt: "2026-06-23",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS raw_events (
        id TEXT PRIMARY KEY,
        tx_hash TEXT NOT NULL,
        event_index INTEGER NOT NULL,
        ledger INTEGER NOT NULL,
        type TEXT NOT NULL,
        payload TEXT NOT NULL DEFAULT '{}',
        indexed_at TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);

    await ctx.db.exec(`
      CREATE UNIQUE INDEX IF NOT EXISTS raw_events_idempotency
        ON raw_events(tx_hash, event_index)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_raw_events_ledger
        ON raw_events(ledger)
    `);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const index = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='index' AND name = 'raw_events_idempotency'",
    );
    if (index) {
      warnings.push(
        "Index raw_events_idempotency already exists — migration is idempotent.",
      );
    }

    return warnings;
  },
} satisfies MigrationDefinition;
