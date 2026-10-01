import type { MigrationDefinition, MigrationContext } from "../../../lib/migrations/types";

/**
 * Fixture hotfix migration.
 *
 * Used to prove the production guard rails in `runMigrations`: a hotfix may only
 * be applied in production when an approval file exists, and it carries a
 * `validate` hook so the pre-flight warning path is exercised too.
 */
const definition: MigrationDefinition = {
  version: 4,
  name: "build_context_hotfix",
  authoredAt: "2026-01-01T00:00:00Z",
  author: "quicklendx-tests",
  meta: {
    hotfix: true,
    reason: "failure_boundary_coverage",
    rollback_risk: "low",
  },
  validate: async (): Promise<string[]> => ["hotfix fixture reports a pre-flight warning"],
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("CREATE TABLE hotfix_table (id INTEGER PRIMARY KEY)");
    await ctx.db.run("INSERT INTO hotfix_table (id) VALUES (?)", [1]);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("DROP TABLE IF EXISTS hotfix_table");
  },
};

export default definition;