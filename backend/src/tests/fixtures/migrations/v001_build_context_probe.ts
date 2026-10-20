import type { MigrationDefinition, MigrationContext } from "../../../lib/migrations/types";

/**
 * Fixture migration used by buildContext failure-boundary tests.
 * Exercises every database handle exposed on the migration context.
 */
const definition: MigrationDefinition = {
  version: 1,
  name: "build_context_probe",
  authoredAt: "2026-01-01T00:00:00Z",
  author: "quicklendx-tests",
  up: async (ctx: MigrationContext): Promise<void> => {
    ctx.db.exec("CREATE TABLE probe_table (id INTEGER PRIMARY KEY, label TEXT NOT NULL)");
    ctx.db.run("INSERT INTO probe_table (id, label) VALUES (?, ?)", [1, "row-1"]);
    ctx.db.get<{ count: number }>("SELECT COUNT(*) AS count FROM probe_table");
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    ctx.db.exec("DROP TABLE IF EXISTS probe_table");
  },
};

export default definition;
