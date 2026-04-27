/**
 * v004_add_index_to_backend_audit_log
 *
 * Author: QuickLendX Engineering
 * Created: 2026-04-26
 *
 * Purpose: Speed up audit log queries by adding composite index on (actor, timestamp).
 * Standard forward-only migration — no rollback path.
 *
 * Performance rationale:
 *   The backend_audit_log table receives ~10k writes/day and supports queries like:
 *     SELECT * FROM backend_audit_log WHERE actor = ? ORDER BY timestamp DESC LIMIT 100
 *   Index covers the WHERE + ORDER BY, avoiding full table scan.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 4,
  name: "add_index_to_backend_audit_log",
  authoredAt: "2026-04-26",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_backend_audit_actor_timestamp
      ON backend_audit_log(actor, timestamp DESC)
    `);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    // Verify the index addition is idempotent
    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='index' AND name = 'idx_backend_audit_actor_timestamp'"
    );
    if (existing) {
      warnings.push("Index already exists — idempotent no-op");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
