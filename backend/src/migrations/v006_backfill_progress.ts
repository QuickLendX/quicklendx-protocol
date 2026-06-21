import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 8,
  name: "backfill_progress",
  authoredAt: "2026-05-28T00:00:00Z",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS backfill_progress (
        id TEXT PRIMARY KEY,
        audit_id INTEGER,
        last_processed_id TEXT,
        remaining_count INTEGER NOT NULL,
        total_count INTEGER NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('running', 'paused', 'completed', 'failed')),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (audit_id) REFERENCES backfill_audit(id) ON DELETE CASCADE
      );
    `);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      DROP TABLE IF EXISTS backfill_progress;
    `);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    return [];
  },
} satisfies MigrationDefinition;
