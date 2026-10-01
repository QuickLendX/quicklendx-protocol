import type { MigrationDefinition, MigrationContext } from "../../../lib/migrations/types";

/**
 * Fixture migration with a deliberately high version number.
 *
 * Used with `v001_...` to prove that `validateMigrationFiles` detects version
 * gaps before anything is applied.
 */
const definition: MigrationDefinition = {
  version: 9,
  name: "build_context_gap",
  authoredAt: "2026-01-01T00:00:00Z",
  author: "quicklendx-tests",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("CREATE TABLE gap_table (id INTEGER PRIMARY KEY)");
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("DROP TABLE IF EXISTS gap_table");
  },
};

export default definition;