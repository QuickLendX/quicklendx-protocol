/**
 * v008_create_notifications
 *
 * Author: QuickLendX Engineering
 * Created: 2026-05-28
 *
 * Creates two tables:
 *   - notifications: durable idempotency log for every send attempt.
 *     The (event_id, user_id) UNIQUE constraint is the idempotency key;
 *     a duplicate INSERT is silently ignored via INSERT OR IGNORE.
 *   - user_notification_preferences: per-user opt-in/out settings,
 *     replacing the mock getUserPreferences in notificationService.ts.
 *
 * Forward-only: no down migration.
 * Rollback: DROP TABLE notifications; DROP TABLE user_notification_preferences;
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 8,
  name: "create_notifications",
  authoredAt: "2026-05-28",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    // Durable notification send log
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS notifications (
        id TEXT PRIMARY KEY,
        event_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        notification_type TEXT NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('pending','sent','failed')),
        smtp_error TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        UNIQUE(event_id, user_id)
      )
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_notifications_event ON notifications(event_id)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications(user_id)
    `);

    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_notifications_status ON notifications(status)
    `);

    // Per-user notification preferences
    await ctx.db.exec(`
      CREATE TABLE IF NOT EXISTS user_notification_preferences (
        user_id TEXT PRIMARY KEY,
        email_enabled INTEGER NOT NULL DEFAULT 1,
        email_address TEXT,
        notify_invoice_funded INTEGER NOT NULL DEFAULT 1,
        notify_payment_received INTEGER NOT NULL DEFAULT 1,
        notify_dispute_opened INTEGER NOT NULL DEFAULT 1,
        notify_dispute_resolved INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL
      )
    `);
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    const existing = await ctx.db.get<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type='table' AND name = 'notifications'"
    );
    if (existing) {
      warnings.push("Table notifications already exists — migration is idempotent.");
    }

    return warnings;
  },
} satisfies MigrationDefinition;
