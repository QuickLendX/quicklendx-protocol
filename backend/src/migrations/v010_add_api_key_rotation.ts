/**
 * v010_add_api_key_rotation
 *
 * Author: QuickLendX Engineering
 * Created: 2026-06-20
 *
 * Adds columns to support API key signing secret rotation with a grace period.
 * - prev_signing_secret_hash: The hash of the previous secret
 * - prev_secret_expires_at: When the previous secret expires
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 10,
  name: "add_api_key_rotation",
  authoredAt: "2026-06-20",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    try {
      await ctx.db.exec(`
        ALTER TABLE api_keys
        ADD COLUMN prev_signing_secret_hash TEXT
      `);
    } catch (e: any) {
      if (!e.message.includes("duplicate column name")) throw e;
    }

    try {
      await ctx.db.exec(`
        ALTER TABLE api_keys
        ADD COLUMN prev_secret_expires_at TEXT
      `);
    } catch (e: any) {
      if (!e.message.includes("duplicate column name")) throw e;
    }
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const columns = await ctx.db.exec(
      "PRAGMA table_info(api_keys)"
    ) as any[];

    const hasPrevHash = columns.some((c: any) => c.name === "prev_signing_secret_hash");
    const hasPrevExpires = columns.some((c: any) => c.name === "prev_secret_expires_at");

    if (hasPrevHash && hasPrevExpires) {
      warnings.push("Columns already exist — migration is idempotent.");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
