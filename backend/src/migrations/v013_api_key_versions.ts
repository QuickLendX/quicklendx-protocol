import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

/** Persist OCC and revocation metadata; existing keys start at version 1. */
export default {
  version: 13,
  name: "api_key_versions",
  authoredAt: "2026-10-19",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    for (const column of ["version INTEGER NOT NULL DEFAULT 1", "revoked_at INTEGER", "revoked_by TEXT"]) {
      try {
        await ctx.db.exec(`ALTER TABLE api_keys ADD COLUMN ${column}`);
      } catch (error) {
        if (!(error instanceof Error) || !error.message.includes("duplicate column name")) throw error;
      }
    }
    // Recover historical revocation metadata where the append-only audit exists.
    await ctx.db.exec(`UPDATE api_keys SET
      revoked_at = COALESCE((SELECT CAST(strftime('%s', timestamp) AS INTEGER) * 1000
        FROM api_key_audit_log WHERE key_id = api_keys.id AND event_type = 'revoked'
        ORDER BY timestamp LIMIT 1), CAST(strftime('%s', created_at) AS INTEGER) * 1000),
      revoked_by = COALESCE((SELECT actor FROM api_key_audit_log
        WHERE key_id = api_keys.id AND event_type = 'revoked' ORDER BY timestamp LIMIT 1), created_by)
      WHERE revoked = 1 AND revoked_at IS NULL`);
  },
} satisfies MigrationDefinition;
