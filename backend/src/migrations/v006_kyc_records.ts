/**
 * v006_kyc_records
 *
 * Author: QuickLendX Engineering
 * Created: 2026-06-23
 *
 * Migration to create the kyc_records table to back the getKycStatus verification checks.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

const schema = `
  CREATE TABLE IF NOT EXISTS kyc_records (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    status TEXT NOT NULL,
    encrypted_data TEXT NOT NULL,
    submitted_at INTEGER NOT NULL,
    verified_at INTEGER,
    metadata TEXT NOT NULL
  );

  CREATE INDEX IF NOT EXISTS idx_kyc_records_user_id ON kyc_records(user_id);
`;

export default {
  version: 6,
  name: "create_kyc_records",
  authoredAt: "2026-06-23",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    const statements = schema
      .split(";")
      .map((s) => s.trim())
      .filter((s) => s.length > 0 && !s.startsWith("--"));

    for (const stmt of statements) {
      try {
        await ctx.db.exec(stmt);
      } catch (err: any) {
        console.error("Failed to execute statement:", stmt);
        throw err;
      }
    }
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];
    return warnings;
  },
} satisfies MigrationDefinition;
