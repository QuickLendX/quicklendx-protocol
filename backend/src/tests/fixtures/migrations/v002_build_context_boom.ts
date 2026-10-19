import type { MigrationDefinition, MigrationContext } from "../../../lib/migrations/types";

/**
 * Fixture migration that fails *after* doing real work.
 *
 * Used to prove that a mid-migration failure rolls back both the DDL and the
 * runner's `_migrations` bookkeeping, so a retry cannot observe a half-applied
 * database.
 */
const definition: MigrationDefinition = {
  version: 2,
  name: "build_context_boom",
  authoredAt: "2026-01-01T00:00:00Z",
  author: "quicklendx-tests",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("CREATE TABLE boom_table (id INTEGER PRIMARY KEY)");
    await ctx.db.run("INSERT INTO boom_table (id) VALUES (?)", [1]);
    // Give the driver a chance to surface a real async failure boundary.
    await Promise.resolve();
    throw new Error("deliberate migration failure");
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("DROP TABLE IF EXISTS boom_table");
  },
};

export default definition;
