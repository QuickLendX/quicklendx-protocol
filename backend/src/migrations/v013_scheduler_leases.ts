import type { MigrationContext, MigrationDefinition } from "../lib/migrations/types";

export default {
  version: 13,
  name: "scheduler_leases",
  authoredAt: "2026-10-18",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS scheduler_leases (
        job_name TEXT PRIMARY KEY,
        lease_until TEXT NOT NULL,
        worker_id TEXT NOT NULL,
        last_run_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_scheduler_leases_lease_until
        ON scheduler_leases(lease_until)
    `);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("DROP INDEX IF EXISTS idx_scheduler_leases_lease_until");
    await ctx.db.exec("DROP TABLE IF EXISTS scheduler_leases");
  },
} satisfies MigrationDefinition;
