/**
 * v005_backfill_audit_log_metadata_json
 *
 * Author: QuickLendX Engineering
 * Created: 2026-04-26
 *
 * Purpose: Migrate legacy audit_log.details from TEXT (CSV) to JSON.
 *
 * Background:
 *   Older audit entries stored `details` as pipe-delimited CSV:
 *     "action=update|resource=invoice|id=123"
 *   New format requires structured JSON for queryability and compliance.
 *
 * Strategy:
 *   - Add new column `details_json` (TEXT, nullable)
 *   - Backfill existing rows by parsing CSV → JSON
 *   - Deprecate old `details` column in v006 (future)
 *
 * Forward-only: No down migration (data transformation; down would lose parsed data)
 *
 * Rollback plan (manual):
 *   If this migration fails halfway, the new column will be NULL for un-parsed rows.
 *   We can re-run the backfill after fixing parser issues.
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

function csvToJson(csv: string): Record<string, string> {
  const result: Record<string, string> = {};
  if (!csv) return result;
  const pairs = csv.split("|");
  for (const pair of pairs) {
    const [key, ...rest] = pair.split("=");
    if (key && rest.length > 0) {
      result[key.trim()] = rest.join("=").trim();
    }
  }
  return result;
}

export default {
  version: 5,
  name: "backfill_audit_log_metadata_to_json",
  authoredAt: "2026-04-26",
  author: "QuickLendX Engineering",
  meta: {
    description: "Migrate audit_log.details from CSV to JSON structured data",
    requires_batching: true,  // For large tables; current size ~25k rows, safe for one-shot
    estimated_duration_sec: 30,
  },
  up: async (ctx: MigrationContext): Promise<void> => {
    // Step 1: Add new column (nullable)
    await ctx.db.exec(`ALTER TABLE backend_audit_log ADD COLUMN details_json TEXT`);

    // Step 2: Fetch all rows where details is not null and details_json is null
    const rows = await ctx.db.get<Array<{ id: number; details: string }>>(
      `SELECT id, details FROM backend_audit_log WHERE details IS NOT NULL AND details_json IS NULL`
    );

    // Step 3: Transform each
    if (rows) {
      for (const row of rows) {
        const parsed = csvToJson(row.details || "");
        await ctx.db.run(
          `UPDATE backend_audit_log SET details_json = ? WHERE id = ?`,
          [JSON.stringify(parsed), row.id]
        );
      }
    }
  },
  validate: async (ctx: MigrationContext): Promise<string[]> => {
    const warnings: string[] = [];

    // Count how many rows still need conversion after migration
    const remaining = await ctx.db.get<{ count: number }>(
      `SELECT COUNT(*) as count FROM backend_audit_log WHERE details IS NOT NULL AND details_json IS NULL`
    );
    if (remaining && remaining.count > 0) {
      warnings.push(`${remaining.count} rows remain un-converted — will be processed in next batch`);
    }

    return warnings;
  },
  // No down: irreversible data transformation (JSON produced from CSV is not lossy, but would require manual CSV reconstruction)
} satisfies MigrationDefinition;
