import type { MigrationDefinition, MigrationContext } from "../../../lib/migrations/types";

/**
 * Fixture migration that exercises the whole `MigrationContext` surface,
 * including an explicit `ctx.db.transaction` scope and the environment flags.
 * Everything it observes is written to a table so the test can assert on it
 * after the run rather than on console output.
 */
const definition: MigrationDefinition = {
  version: 3,
  name: "build_context_surface",
  authoredAt: "2026-01-01T00:00:00Z",
  author: "quicklendx-tests",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("CREATE TABLE context_surface (key TEXT PRIMARY KEY, value TEXT)");

    const handleKeys = Object.keys(ctx.db).sort().join(",");

    await ctx.db.transaction(async (tx) => {
      await tx.exec("INSERT INTO context_surface (key, value) VALUES (?, ?)", [
        "handle_keys",
        handleKeys,
      ]);
      await tx.run("INSERT INTO context_surface (key, value) VALUES (?, ?)", [
        "is_production",
        String(ctx.isProduction),
      ]);
      await tx.run("INSERT INTO context_surface (key, value) VALUES (?, ?)", ["is_test", String(ctx.isTest)]);
      await tx.run("INSERT INTO context_surface (key, value) VALUES (?, ?)", [
        "has_env",
        String(ctx.env === process.env),
      ]);

      // A read inside the same transaction must see the rows written above.
      const seen = await tx.get<{ total: number }>("SELECT COUNT(*) AS total FROM context_surface");
      await tx.run("INSERT INTO context_surface (key, value) VALUES (?, ?)", [
        "rows_in_tx",
        String(seen?.total),
      ]);

      // A nested transaction must re-enter cleanly rather than erroring with
      // "cannot start a transaction within a transaction".
      await tx.transaction(async (nested) => {
        await nested.run("INSERT INTO context_surface (key, value) VALUES (?, ?)", [
          "nested",
          "ok",
        ]);
      });
    });

    const pragmaColumns = await ctx.db.exec("PRAGMA table_info(context_surface)");
    await ctx.db.run("INSERT INTO context_surface (key, value) VALUES (?, ?)", [
      "pragma_columns",
      String(Array.isArray(pragmaColumns) ? pragmaColumns.length : -1),
    ]);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec("DROP TABLE IF EXISTS context_surface");
  },
};

export default definition;
