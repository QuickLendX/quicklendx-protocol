/**
 * v007_create_bids
 *
 * Author: QuickLendX Engineering
 * Created: 2026-05-28
 *
 * Adds a bids table for persisted bid storage with proper indexing
 * and constraints aligned with contract semantics.
 *
 * Depends on: v006_create_invoices_table (invoices.id is the FK target).
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

const schema = `
  CREATE TABLE IF NOT EXISTS bids (
    bid_id VARCHAR(66) PRIMARY KEY,
    invoice_id VARCHAR(66) NOT NULL,
    investor VARCHAR(56) NOT NULL,
    bid_amount VARCHAR(32) NOT NULL,
    expected_return VARCHAR(32) NOT NULL,
    timestamp BIGINT NOT NULL,
    status VARCHAR(20) NOT NULL CHECK(status IN ('Placed', 'Withdrawn', 'Accepted', 'Expired', 'Cancelled')),
    expiration_timestamp BIGINT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(56) NOT NULL,
    FOREIGN KEY (invoice_id) REFERENCES invoices(id)
  );
  CREATE INDEX IF NOT EXISTS idx_bids_invoice_id ON bids(invoice_id);
  CREATE INDEX IF NOT EXISTS idx_bids_investor ON bids(investor);
  CREATE INDEX IF NOT EXISTS idx_bids_invoice_status ON bids(invoice_id, status);
  CREATE INDEX IF NOT EXISTS idx_bids_created_at ON bids(created_at);
  CREATE INDEX IF NOT EXISTS idx_bids_ranking ON bids(
    invoice_id,
    status
  );
`;

export default {
  version: 7,
  name: "create_bids",
  authoredAt: "2026-05-28",
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

    try {
      const result = await ctx.db.get<{ count: number }>(
        "SELECT COUNT(*) as count FROM sqlite_master WHERE type='table' AND name='bids'"
      );
      if (!result || result.count === 0) {
        warnings.push("Bids table was not created successfully");
      }
    } catch (err) {
      warnings.push("Failed to validate bids table creation");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
