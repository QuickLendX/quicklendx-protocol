/**
 * migrations-kyc-idempotency.test.ts
 *
 * Verifies that:
 *   1. Running the KYC migration's `up` twice on the same DB does not throw.
 *   2. Running `down` drops kyc_records and idx_kyc_records_user_id.
 *
 * Uses the same minimal MigrationContext pattern as migrations-apply-all.test.ts.
 * Does NOT import runMigrations().
 */

import { describe, it, expect, beforeEach } from "vitest";
import Database from "better-sqlite3";
import * as path from "path";
import type { MigrationContext, MigrationDefinition } from "../lib/migrations/types";

// ── Minimal context builder (identical to migrations-apply-all.test.ts) ───────

function buildTestContext(db: ReturnType<typeof Database>): MigrationContext {
  const contextDb: MigrationContext["db"] = {
    async exec(sql: string, _params?: unknown[]): Promise<unknown[]> {
      // Use prepare() so leading SQL comments don't confuse the router.
      // better-sqlite3's `statement.reader` is set at prepare time.
      const stmt = db.prepare(sql);
      if (stmt.reader) {
        return stmt.all() as unknown[];
      }
      stmt.run();
      return [];
    },
    async get<T = unknown>(sql: string, params?: unknown[]): Promise<T | undefined> {
      const stmt = db.prepare(sql);
      const row = params && params.length > 0
        ? stmt.get(...params)
        : stmt.get();
      return (row === null ? undefined : row) as T | undefined;
    },
    async run(sql: string, params?: unknown[]): Promise<{ lastInsertRowId: number; changes: number }> {
      const stmt = db.prepare(sql);
      const result = params && params.length > 0
        ? stmt.run(...params)
        : stmt.run();
      return {
        lastInsertRowId: result.lastInsertRowid as number,
        changes: result.changes,
      };
    },
    transaction<T>(fn: (db: MigrationContext["db"]) => T): T {
      return fn(contextDb) as T;
    },
  };
  return {
    db: contextDb,
    env: process.env,
    isProduction: false,
    isTest: true,
  };
}

// ── Load the KYC migration ─────────────────────────────────────────────────────

function loadKycMigration(): MigrationDefinition {
  const migPath = path.resolve(__dirname, "../migrations/v009_kyc_records.ts");
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const mod = require(migPath);
  return mod?.default ?? mod;
}

// ── Helpers ────────────────────────────────────────────────────────────────────

function tableExists(db: ReturnType<typeof Database>, name: string): boolean {
  return (
    db
      .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name=?`)
      .get(name) != null
  );
}

function indexExists(db: ReturnType<typeof Database>, name: string): boolean {
  return (
    db
      .prepare(`SELECT name FROM sqlite_master WHERE type='index' AND name=?`)
      .get(name) != null
  );
}

// ── Tests ──────────────────────────────────────────────────────────────────────

describe("KYC migration idempotency and rollback", () => {
  let db: ReturnType<typeof Database>;
  let ctx: MigrationContext;
  let kyc: MigrationDefinition;

  beforeEach(() => {
    db = new Database(":memory:");
    ctx = buildTestContext(db);
    kyc = loadKycMigration();
  });

  it("up creates kyc_records and idx_kyc_records_user_id", async () => {
    await kyc.up(ctx);
    expect(tableExists(db, "kyc_records")).toBe(true);
    expect(indexExists(db, "idx_kyc_records_user_id")).toBe(true);
  });

  it("running up twice does not throw (idempotent)", async () => {
    await kyc.up(ctx);
    // Second call must not throw; CREATE TABLE IF NOT EXISTS is idempotent
    await expect(kyc.up(ctx)).resolves.not.toThrow();
    // Table must still exist after the second call
    expect(tableExists(db, "kyc_records")).toBe(true);
  });

  it("down drops kyc_records and idx_kyc_records_user_id", async () => {
    await kyc.up(ctx);
    expect(tableExists(db, "kyc_records")).toBe(true);

    // down is defined on this migration
    expect(kyc.down).toBeDefined();
    await kyc.down!(ctx);

    expect(tableExists(db, "kyc_records")).toBe(false);
    expect(indexExists(db, "idx_kyc_records_user_id")).toBe(false);
  });
});
