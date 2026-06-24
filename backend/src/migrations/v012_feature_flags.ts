/**
 * v012_feature_flags
 *
 * Author: QuickLendX Engineering
 * Created: 2026-06-24
 *
 * Adds the feature_flags table for per-tenant feature gating.
 * Each row represents a (api_key_id, flag) pair where enabled=1 means the flag is ON.
 * Absence of a row is treated as flag-off (deny by default).
 *
 * Down migration drops the table and its indexes.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 12,
  name: "feature_flags",
  authoredAt: "2026-06-24",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS feature_flags (
        id         TEXT    NOT NULL PRIMARY KEY,
        api_key_id TEXT    NOT NULL,
        flag       TEXT    NOT NULL,
        enabled    INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
        created_at TEXT    NOT NULL,
        updated_at TEXT    NOT NULL,
        updated_by TEXT    NOT NULL,
        UNIQUE(api_key_id, flag)
      )
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_feature_flags_api_key_id
        ON feature_flags(api_key_id)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_feature_flags_flag
        ON feature_flags(flag)
    `);
  },

  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`DROP INDEX IF EXISTS idx_feature_flags_flag`);
    await ctx.db.exec(`DROP INDEX IF EXISTS idx_feature_flags_api_key_id`);
    await ctx.db.exec(`DROP TABLE IF EXISTS feature_flags`);
  },

  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];
    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'feature_flags'"
    );
    if (existing) {
      warnings.push("Table feature_flags already exists — migration is idempotent.");
    }
    return warnings;
  },
} satisfies MigrationDefinition;
