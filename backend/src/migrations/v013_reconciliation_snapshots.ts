/**
 * v013_reconciliation_snapshots
 *
 * Author: QuickLendX Engineering
 * Created: 2026-07-09
 *
 * Adds persisted reconciliation drift snapshots so operators can inspect
 * whether detected drift is a one-off event or a worsening trend.
 */

import type {
  MigrationDefinition,
  MigrationContext,
} from "../lib/migrations/types";

export default {
  version: 13,
  name: "reconciliation_snapshots",
  authoredAt: "2026-07-09",
  author: "QuickLendX Engineering",

  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS reconciliation_snapshots (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        run_at        TEXT    NOT NULL,
        checked_count INTEGER NOT NULL CHECK(checked_count >= 0),
        drift_count   INTEGER NOT NULL CHECK(drift_count >= 0),
        severity      TEXT    NOT NULL CHECK(severity IN ('LOW', 'MEDIUM', 'HIGH'))
      )
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_reconciliation_snapshots_run_at
        ON reconciliation_snapshots(run_at DESC)
    `);
  },

  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(
      `DROP INDEX IF EXISTS idx_reconciliation_snapshots_run_at`
    );
    await ctx.db.exec(`DROP TABLE IF EXISTS reconciliation_snapshots`);
  },

  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];
    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'reconciliation_snapshots'"
    );
    if (existing) {
      warnings.push(
        "Table reconciliation_snapshots already exists - migration is idempotent."
      );
    }
    return warnings;
  },
} satisfies MigrationDefinition;
