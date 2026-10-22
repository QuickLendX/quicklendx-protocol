/**
 * v016_backfill_progress
 *
 * Author: QuickLendX Engineering
 * Created: 2026-05-28
 *
 * Creates the backfill_progress table for tracking per-run resumable progress.
 * Each row links to a backfill_audit entry and records the last processed
 * item, remaining/total counts, and current run status.
 *
 * Depends on: v001_initial_schema (backfill_audit table must exist).
 *
 * Note: An earlier draft of this migration appeared at both v006 and v011 in
 * separate parallel branches. The v006 copy has been removed; this is the
 * single authoritative migration with the stricter validate function.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 16,
  name: "backfill_progress",
  authoredAt: "2026-05-28T00:00:00Z",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(
      `CREATE TABLE IF NOT EXISTS backfill_progress (
        id TEXT PRIMARY KEY,
        audit_id INTEGER,
        last_processed_id TEXT,
        remaining_count INTEGER NOT NULL,
        total_count INTEGER NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('running','paused','completed','failed')),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (audit_id) REFERENCES backfill_audit(id) ON DELETE CASCADE
      )`
    );
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`DROP TABLE IF EXISTS backfill_progress`);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const table = await ctx.db.get(
      `SELECT name FROM sqlite_master WHERE type='table' AND name='backfill_progress'`
    );
    return table ? [] : ["backfill_progress table is missing"];
  },
} satisfies MigrationDefinition;
