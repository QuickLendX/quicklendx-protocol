import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 13,
  name: "add_list_pagination_indexes",
  authoredAt: "2026-10-19",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_invoices_created_at_id
      ON invoices(created_at DESC, id ASC)
    `);
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_invoices_business_created_at_id
      ON invoices(business, created_at DESC, id ASC)
    `);
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_invoices_status_created_at_id
      ON invoices(status, created_at DESC, id ASC)
    `);
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_settlements_invoice_timestamp_id
      ON settlements(invoice_id, timestamp DESC, id ASC)
    `);
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_settlements_timestamp_id
      ON settlements(timestamp DESC, id ASC)
    `);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("DROP INDEX IF EXISTS idx_invoices_created_at_id");
    await ctx.db.exec("DROP INDEX IF EXISTS idx_invoices_business_created_at_id");
    await ctx.db.exec("DROP INDEX IF EXISTS idx_invoices_status_created_at_id");
    await ctx.db.exec("DROP INDEX IF EXISTS idx_settlements_invoice_timestamp_id");
    await ctx.db.exec("DROP INDEX IF EXISTS idx_settlements_timestamp_id");
  },
  validate: async (): Promise<string[]> => [],
} satisfies MigrationDefinition;
