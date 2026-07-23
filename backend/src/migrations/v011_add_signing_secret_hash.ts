/**
 * v011_add_signing_secret_hash
 *
 * Adds signing_secret_hash column to api_keys to support per-API-key HMAC request signing.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 11,
  name: "add_signing_secret_hash",
  authoredAt: new Date().toISOString().split('T')[0],
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    try {
      await ctx.db.exec(`
        ALTER TABLE api_keys
        ADD COLUMN signing_secret_hash TEXT
      `);
    } catch (e: any) {
      if (!e.message.includes("duplicate column name")) throw e;
    }
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const columns = await ctx.db.exec("PRAGMA table_info(api_keys)") as { name: string }[];
    const hasCol = columns.some((c: { name: string }) => c.name === "signing_secret_hash");

    if (hasCol) {
      warnings.push("Column already exists — migration is idempotent.");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
