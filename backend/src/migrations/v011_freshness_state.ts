/**
 * v011_freshness_state
 *
 * Author: QuickLendX Engineering
 * Created: 2026-06-23
 *
 * Adds a durable freshness_state table to persist indexer cursor and timestamp.
 * Rollback: N/A (forward-only baseline)
 */

import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

const schema = `
  CREATE TABLE IF NOT EXISTS freshness_state (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    cursor TEXT NOT NULL,
    timestamp TEXT NOT NULL
  );
`;

export default {
  version: 11,
  name: "freshness_state",
  authoredAt: "2026-06-23",
  author: "QuickLendX Engineering",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(schema);
  },
  validate: async (_ctx: MigrationContext): Promise<string[]> => {
    return [];
  },
} satisfies MigrationDefinition;
