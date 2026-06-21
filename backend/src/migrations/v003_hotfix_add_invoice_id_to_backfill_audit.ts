/**
 * v003_hotfix_add_invoice_id_to_backfill_audit
 *
 * Author: QuickLendX Engineering
 * Created: 2026-04-26
 *
 * CRITICAL HOTFIX — PRODUCTION INCIDENT RESPONSE
 *
 * Problem:
 *   After v002 deployment, we discovered backfill audit events lack invoice-level
 *   granularity, making it impossible to trace which specific invoices were affected
 *   by a backfill run. This is a compliance gap for audit trails.
 *
 * Solution:
 *   Add `invoice_id` column (nullable) to backfill_audit table.
 *   This is a breaking change: existing rows will be updated gradually via backfill.
 *   Query layer will gracefully handle NULLs (treat as "unknown invoice").
 *
 * Hotfix Flags:
 *   - meta.hotfix: true
 *   - meta.rollback_risk: medium (data loss: new column values will be lost on down)
 *   - down migration drops column (data in that column is permanently lost)
 *
 * Approval Required:
 *   - Two senior engineer signatures in .hotfix-approvals/003_hotfix_add_invoice_id.approval
 *   - Linked issue: #870 (this task)
 *
 * Rollback Plan:
 *   1. Down migration drops the `invoice_id` column from backfill_audit.
 *   2. Any data written after v003 application is lost — be prepared to replay
 *      backfill events from upstream source if needed.
 *   3. After rollback, API must handle missing invoice_id in responses gracefully.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 3,
  name: "hotfix_add_invoice_id_to_backfill_audit",
  authoredAt: "2026-04-26T14:30:00Z",
  author: "QuickLendX Engineering",
  meta: {
    hotfix: true,
    reason: "Compliance gap: backfill audit trail lacks per-invoice granularity",
    rollback_risk: "medium",
    incident_ticket: "https://github.com/QuickLendX/quicklendx-protocol/issues/870",
    required_approvals: 2,
  },
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      ALTER TABLE backfill_audit
      ADD COLUMN invoice_id TEXT
    `);

    await ctx.db.exec(`
      UPDATE backfill_audit
      SET invoice_id = NULL
      WHERE invoice_id IS NULL
    `);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      ALTER TABLE backfill_audit
      DROP COLUMN invoice_id
    `);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const existing = await ctx.db.get<{ name: string }>(
      `PRAGMA table_info(backfill_audit) WHERE name = 'invoice_id'`
    );
    if (existing) {
      warnings.push("Column invoice_id already exists — migration will be idempotent no-op.");
    }

    warnings.push(
      "ROLLBACK WARNING: down migration DROP COLUMN will permanently delete all invoice_id values written by this migration."
    );

    return warnings;
  },
} satisfies MigrationDefinition;
