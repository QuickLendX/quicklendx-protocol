import type { MigrationDefinition, MigrationContext } from "../../../lib/migrations/types";

/**
 * Fixture forward-only migration (no `down`).
 *
 * Used to prove that the runner refuses to roll back a migration that declares
 * no `down` function, instead of silently skipping it.
 */
const definition: MigrationDefinition = {
  version: 5,
  name: "build_context_forward_only",
  authoredAt: "2026-01-01T00:00:00Z",
  author: "quicklendx-tests",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("CREATE TABLE forward_only_table (id INTEGER PRIMARY KEY)");
  },
};

export default definition;