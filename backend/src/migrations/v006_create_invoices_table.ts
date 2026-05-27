/**
 * v006_create_invoices_table
 *
 * Author: QuickLendX Engineering
 * Created: 2026-05-27
 *
 * Sets up the invoices table to materialize the event stream into queryable state.
 *
 * Rollback: N/A (forward-only baseline)
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

const schema = `
  CREATE TABLE IF NOT EXISTS invoices (
    id TEXT PRIMARY KEY,
    business TEXT NOT NULL,
    amount TEXT NOT NULL,
    currency TEXT NOT NULL,
    due_date INTEGER NOT NULL,
    status TEXT NOT NULL,
    description TEXT NOT NULL,
    category TEXT NOT NULL,
    tags TEXT NOT NULL,
    metadata TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    contract_version INTEGER NOT NULL,
    event_schema_version INTEGER NOT NULL,
    indexed_at TEXT NOT NULL
  );

  CREATE INDEX idx_invoices_business ON invoices(business);
  CREATE INDEX idx_invoices_status ON invoices(status);
`;

export default {
  version: 6,
  name: "create_invoices_table",
  authoredAt: "2026-05-27",
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
