/**
 * v002_add_webhook_event_type_index
 *
 * Author: QuickLendX Engineering
 * Created: 2026-04-26
 *
 * Purpose: Optimize webhook delivery queries by event type.
 * Adds a partial index for active webhook subscriptions filtered by event type.
 *
 * This migration demonstrates:
 *   - Safe additive schema changes (non-breaking)
 *   - Index creation for query performance
 *   - Rollback drops the index (performance degradation acceptable for emergency rollback)
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 2,
  name: "add_webhook_event_type_index",
  authoredAt: "2026-04-26",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_active_events
      ON webhook_subscriptions(is_active, events)
      WHERE is_active = 1
    `);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`DROP INDEX IF EXISTS idx_webhook_active_events`);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='index' AND name = 'idx_webhook_active_events'"
    );
    if (existing) {
      warnings.push("Index idx_webhook_active_events already exists — migration is idempotent and will be skipped.");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
