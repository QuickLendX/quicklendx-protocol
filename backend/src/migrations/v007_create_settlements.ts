/**
 * v007_create_settlements
 *
 * Author: QuickLendX Engineering
 * Created: 2026-05-27
 *
 * Creates the settlements table for the settlement state machine.
 * Supports lifecycle: Pending -> Processing -> Paid | Defaulted.
 * The event_id UNIQUE column provides idempotent replay protection.
 *
 * Forward-only: no down migration.
 * Rollback: DROP TABLE settlements;
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 7,
  name: "create_settlements",
  authoredAt: "2026-05-27",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS settlements (
        id TEXT PRIMARY KEY,
        invoice_id TEXT NOT NULL,
        amount TEXT NOT NULL,
        payer TEXT NOT NULL,
        recipient TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('Pending','Processing','Paid','Defaulted')),
        contract_version INTEGER NOT NULL DEFAULT 1,
        event_schema_version INTEGER NOT NULL DEFAULT 1,
        indexed_at TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        event_id TEXT UNIQUE
      )
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_settlements_invoice ON settlements(invoice_id)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_settlements_status ON settlements(status)
    `);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'settlements'"
    );
    if (existing) {
      warnings.push("Table settlements already exists — migration is idempotent.");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
