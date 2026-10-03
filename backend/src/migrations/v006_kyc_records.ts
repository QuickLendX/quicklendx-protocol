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

const SAVEPOINT_NAME = "kyc_migration";
const SAVEPOINT = "kyc_migration";

async function runInTransaction(
  db: MigrationContext["db"],
  statements: string[]
): Promise<void> {
  await db.exec(`SAVEPOINT ${SAVEPOINT_NAME}`);
  try {
    for (const statement of statements) {
      await db.exec(statement);
    }
    await db.exec(`RELEASE ${SAVEPOINT_NAME}`);
    await db.exec(`RELEASE ${SAVEPOINT}`);
  } catch (err) {
    try {
      await db.exec(`ROLLBACK TO SAVEPOINT ${SAVEPOINT_NAME}`);
      await db.exec(`RELEASE ${SAVEPOINT_NAME}`);
    } catch {
      // Preserve original error if rollback fails.
    }
    throw err;
  }
}

export default {
  version: 6,
  name: "create_kyc_records",
  authoredAt: "2026-06-23",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    const statements = schema
      .split(";")
      .map((statement) => statement.trim())
      .filter((statement) => statement.length > 0 && !statement.startsWith("--"));
    await runInTransaction(ctx.db, statements);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await runInTransaction(ctx.db, [
      "DROP INDEX IF EXISTS idx_kyc_records_user_id",
      "DROP TABLE IF EXISTS kyc_records",
    ]);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];
    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'kyc_records'"
    );

    if (existing) {
      warnings.push("Table kyc_records already exists — migration is idempotent.");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
