/**
 * v006_create_api_keys
 *
 * Author: QuickLendX Engineering
 * Created: 2026-05-27
 *
 * Persists API keys and their audit logs in SQLite (replacing the in-memory
 * store in src/db/database.ts). Key hashes are SHA-256; raw secrets never
 * touch the database. The prefix index provides O(1) lookup for auth flows.
 *
 * Forward-only: no down migration.
 * Rollback: DROP TABLE api_key_audit_log; DROP TABLE api_keys;
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 6,
  name: "create_api_keys",
  authoredAt: "2026-05-27",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY,
        key_hash TEXT NOT NULL,
        prefix TEXT NOT NULL,
        name TEXT NOT NULL,
        scopes TEXT NOT NULL,
        created_at TEXT NOT NULL,
        last_used_at TEXT,
        expires_at TEXT,
        revoked INTEGER NOT NULL DEFAULT 0,
        created_by TEXT NOT NULL
      )
    `);

    await ctx.db.exec(`
      CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_api_keys_created_by ON api_keys(created_by)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_api_keys_revoked ON api_keys(revoked)
    `);

    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS api_key_audit_log (
        id TEXT PRIMARY KEY,
        event_type TEXT NOT NULL CHECK(event_type IN ('created','used','rotated','revoked')),
        key_id TEXT NOT NULL,
        actor TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        ip_address TEXT,
        endpoint TEXT,
        metadata TEXT,
        FOREIGN KEY (key_id) REFERENCES api_keys(id) ON DELETE CASCADE
      )
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_api_key_audit_key_id ON api_key_audit_log(key_id)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_api_key_audit_event_type ON api_key_audit_log(event_type)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_api_key_audit_timestamp ON api_key_audit_log(timestamp)
    `);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'api_keys'"
    );
    if (existing) {
      warnings.push("Table api_keys already exists — migration is idempotent.");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
