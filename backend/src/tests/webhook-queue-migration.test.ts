import migration from "../migrations/v009_create_webhook_queue";
import type { MigrationContext } from "../lib/migrations/types";

describe("v009 webhook queue migration", () => {
  it("creates retry scheduling and dead-letter queue columns", async () => {
    const execStatements: string[] = [];
    const ctx = {
      db: {
        exec: jest.fn(async (sql: string) => {
          execStatements.push(sql);
          return [];
        }),
        get: jest.fn(),
        run: jest.fn(async () => ({ lastInsertRowId: 0, changes: 0 })),
        transaction: jest.fn((fn: (db: MigrationContext["db"]) => unknown) => fn(ctx.db)),
      },
      env: {},
      isProduction: false,
      isTest: true,
    } satisfies MigrationContext;

    await migration.up(ctx);

    const createQueueSql = execStatements.find((sql) => sql.includes("CREATE TABLE IF NOT EXISTS webhook_queue"));
    const indexSql = execStatements.find((sql) => sql.includes("idx_webhook_queue_status_next_attempt"));
    expect(createQueueSql).toContain("attempts INTEGER NOT NULL DEFAULT 0");
    expect(createQueueSql).toContain("max_attempts INTEGER NOT NULL DEFAULT 5");
    expect(createQueueSql).toContain("next_attempt_at TEXT");
    expect(createQueueSql).toContain("'dead_letter'");
    expect(indexSql).toContain("status, next_attempt_at");
  });
});
