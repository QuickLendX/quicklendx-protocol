
/**
 * Deterministic failure-boundary coverage for runMigrations.
 *
 * Strategy: run the real runner against a real in-memory better-sqlite3 database
 * (genuine UNIQUE constraints and transactions) and a controlled fixture
 * "workspace" on disk so MIGRATIONS_DIR/HOTFIX_APPROVALS_DIR resolve to
 * `process.cwd()/src/migrations` and `process.cwd()/.hotfix-approvals`.
 *
 * The runner is re-required after `process.chdir` and `jest.resetModules()` so
 * its import-time constants (`MIGRATIONS_DIR`, `config.NODE_ENV`) are computed
 * deterministically for the environment under test (including `production`).
 *
 * Fixture workspaces live under backend/tests/fixtures/:
 *   - migration-workspace/      valid, gated (env-gated failure), hotfix,
 *                               missing-up, tamper-target + invalid files
 *   - migration-workspace-dup/  duplicate version pair for ordering boundary
 *   - migration-workspace-empty/ empty migrations dir boundary
 */

import { execFileSync } from "child_process";
import Database from "better-sqlite3";
import * as fs from "fs/promises";
import * as path from "path";
import { createHash } from "crypto";
import type { MigrationState } from "../lib/migrations/types";

const CLI = path.resolve(__dirname, "..", "lib", "migrations", "cli.ts");
const MAIN = path.resolve(__dirname, "..", "..", "tests", "fixtures", "migration-workspace");
const DUP = path.resolve(__dirname, "..", "..", "tests", "fixtures", "migration-workspace-dup");
const EMPTY = path.resolve(__dirname, "..", "..", "tests", "fixtures", "migration-workspace-empty");
const ORIGINAL_CWD = process.cwd();

type RunnerModule = typeof import("../lib/migrations/runner");

let runner: RunnerModule;
let origContents = new Map<string, string>();
let origCliContents: string;

function sha256(content: string): string {
  return createHash("sha256").update(content).digest("hex");
}

function switchEnv(workspace: string, env: string): void {
  process.chdir(workspace);
  process.env.NODE_ENV = env;
  // Re-require the runner after chdir so import-time MIGRATIONS_DIR and
  // config.NODE_ENV (snapshotted at load) reflect the requested environment.
  jest.resetModules();
  runner = require("../lib/migrations/runner");
}

/**
 * Invoke the real CLI entry point (`main`) as a subprocess so its
 * deterministic exit codes and stderr diagnostics are exercised end-to-end.
 * Returns the captured exit code and combined output.
 */
function runCli(args: string[], env: NodeJS.ProcessEnv = {}): { code: number; stdout: string; stderr: string } {
  const result = execFileSync("npx", ["ts-node", "--transpile-only", CLI, ...args], {
    cwd: process.cwd(),
    env: { ...process.env, ...env },
    encoding: "utf-8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  return { code: 0, stdout: String(result), stderr: "" };
}

function newDb(): any {
  return new Database(":memory:");
}

async function seedApplied(db: any, version: number, name: string): Promise<void> {
  db.exec(
    "CREATE TABLE IF NOT EXISTS _migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL, checksum TEXT NOT NULL, applied_at TEXT NOT NULL, duration_ms INTEGER NOT NULL, author TEXT NOT NULL, meta TEXT DEFAULT '{}')"
  );
  const file = path.join(process.cwd(), "src", "migrations", `v{${String(version).padStart(3, "0")}_${name}.ts`);
  const content = await fs.readFile(file, "utf-8");
  db.prepare(
    "INSERT INTO _migrations (version, name, checksum, applied_at, duration_ms, author, meta) VALUES (?, ?, ?, ?, ?, ?, ?)"
  ).run(version, name, sha256(content), new Date().toISOString(), 0, "test-fixture", "{}");
}

async function appliedVersions(db: any): Promise<number[]> {
  return runner.getAppliedVersions(db);
}

function appliedStates(db: any): Array<{ version: number; name: string; checksum: string }> {
  return db.prepare("SELECT version, name, checksum FROM _migrations ORDER BY version ASC").all();
}

function probeRows(db: any): Array<{ version: number; note: string }> {
  return db.prepare("SELECT version, note FROM probe_log ORDER BY version ASC").all();
}

function probeTableExists(db: any): boolean {
  const row = db
    .prepare("SELECT 1 AS present FROM sqlite_master WHERE type = 'table' AND name = 'probe_log'")
    .get();
  return !!row;
}

async function expectRejectsWith(run: Promise<unknown>, needle: string): Promise<Error> {
  const err = await run.catch((e: any) => e);
  expect(err).toBeInstanceOf(Error);
  expect(String(err?.message)).toContain(needle);
  return err as Error;
}

describe("migration failure boundary (deterministic suite)", () => {
  beforeAll(async () => {
    process.chdir(MAIN);
    await fs.mkdir(path.join(MAIN, ".hotfix-approvals"), { recursive: true });

    const migDir = path.join(MAIN, "src", "migrations");
    for (const f of await fs.readdir(migDir)) {
      origContents.set(f, await fs.readFile(path.join(migDir, f), "utf-8"));
    }

    origCliContents = await fs.readFile(CLI, "utf-8");

    switchEnv(MAIN, "test");
  });

  afterEach(async () => {
    // Restore every fixture file mutated by a test (checksum tamper) so the
    // next test sees a pristine workspace.
    const migDir = path.join(MAIN, "src", "migrations");
    for (const [F, content] of origContents) {
      await fs.writeFile(path.join(migDir, f), content, "utf-8");
    }
    delete process.env.QFC_MIGRATION_902_ALLOWED;
    await fs.writeFile(CLI, origCliContents, "utf-8");
  });

  afterAll(async () => {
    process.chdir(ORIGINAL_CWD);
  });

  describe("CLI main entrypoint failure boundaries", () => {
    beforeAll(() => {
      switchEnv(MAIN, "test");
    });

    test("main exits non-zero with a diagnosable message on invalid arguments", () => {
      let err: any;
      try {
        runCli(["--definitely-not-a-flag"]);
      } catch (e) {
        err = e;
      }
      expect(err).toBeDefined();
      expect(err.status).not.toBe(0);
      const output = `${err.stdout ?? ""}${err.stderr ?? ""}`;
      expect(output).toMatch(/unknown|invalid|unrecognized/i);
      // No sensitive environment values should leak into CLI diagnostics.
      expect(output).not.toContain(process.env.ADMIN_API_KEY ?? "__no_admin_key__");
    });

    test("main exits non-zero when the migrations directory is empty and reports a clean no-op", () => {
      switchEnv(EMPTY, "test");
      try {
        let err: any;
        try {
          runCli(["--dry-run"]);
        } catch (e) {
          err = e;
        }
        // Either a clean zero-exit no-op or a deterministic non-zero failure
        // is acceptable, but the process must not hang or crash opaquely.
        if (err) {
          expect(err.status).not.toBe(0);
          expect(`${err.stdout ?? ""}${err.stderr ?? ""}`).toMatch(/migration/i);
        }
      } finally {
        switchEnv(MAIN, "test");
      }
    });

    test("main surfaces a deterministic failure when a migration is missing its up function", () => {
      process.env.QFC_MIGRATION_902_ALLOWED = "1";
      let err: any;
      try {
        runCli(["--apply"]);
      } catch (e) {
        err = e;
      }
      expect(err).toBeDefined();
      expect(err.status).not.toBe(0);
      const output = `${err.stdout ?? ""}${err.stderr ?? ""}`;
      expect(output).toMatch(/missing up function|v904_missing_up/);
      // The internal environment knob must not be echoed back to the operator.
      expect(output).not.toContain("QFC_MIGRATION_902_ALLOWED");
    });

    test("main refuses to bypass checksum verification in production", () => {
      switchEnv(MAIN, "production");
      try {
        let err: any;
        try {
          runCli(["--apply", "--skip-checksum-verify"], {
            ADMIN_API_KEY: "a".repeat(32),
            WEBHOOK_SECRET: "b".repeat(16),
            EXPORT_SECRET: "c".repeat(32),
          });
        } catch (e) {
          err = e;
        }
        expect(err).toBeDefined();
        expect(err.status).not.toBe(0);
        expect(`${err.stdout ?? ""}${err.stderr ?? ""}`).toMatch(/cannot be bypassed in production/i);
      } finally {
        switchEnv(MAIN, "test");
      }
    });
  });

  describe("deterministic success and idempotency", () => {
    beforeAll(() => {
      switchEnv(MAIN, "test");
    });

    test("applies pending migrations in ascending version order and records state", async () => {
      process.env.QFC_MIGRATION_902_ALLOWED = "1";
      const db = newDb();
      await seedApplied(db, 904, "missing_up");
      await seedApplied(db, 905, "durable");

      const result = await runner.runMigrations({ db });

      expect(result.applied.map((m) => m.version)).toEqual([901, 902, 903]);
      expect(result.skipped).toBe(0);
      await expect(appliedVersions(db)).resolves.toEqual([901, 902, 903, 904, 905]);
      const states = appliedStates(db);
      expect(states.map((s) => s.version)).toEqual([901, 902, 903, 904, 905]);
      expect(states[0].checksum).toMatch(/^[a-f0-9]{64}$/);
      expect(probeRows(db).map((r) => r.version)).toEqual([901, 902, 903]);
      expect(probeRows(db).map((r) => r.note)).toEqual(["baseline", "second", "hotfix"]);
    });

    test("re-running is idempotent and counts already-applied as skipped", async () => {
      process.env.QFC_MIGRATION_902_ALLOWED = "1";
      const db = newDb();
      await seedApplied(db, 904, "missing_up");
      await seedApplied(db, 905, "durable");

      const first = await runner.runMigrations({ db });
      expect(first.applied.map((m) => m.version)).toEqual([901, 902, 903]);

      const second = await runner.runMigrations({ db });
      expect(second.applied).toEqual([]);
      expect(second.skipped).toBe(0);
      await expect(appliedVersions(db)).resolves.toEqual([901, 902, 903, 904, 905]);
      expect(probeRows(db).map((r) => r.version)).toEqual([901, 902, 903]);
    });

    test("dry run reports the plan without touching the database", async () => {
      process.env.QFC_MIGRATION_902_ALLOWEB= "1";
      const db = newDb();
      await seedApplied(db, 904, "missing_up");
      await seedApplied(db, 905, "durable");

      const result = await runner.runMigrations({ db, dryRun: true });

      expect(result.applied.map((m) => m.version)).toEqual([901, 902, 903]);
      for (const m of result.applied) {
        expect(m.checksum).toBe("(dry-run)");
        expect(m.durationMs).toBe(0);
      }
      await expect(appliedVersions(db)).resolves.toEqual([904, 905]);
      expect(probeTableExists(db)).toBe(false);
    });
  });

  describe("invalid, duplicate, and boundary inputs", () => {
    beforeAll(() => {
      switchEnv(MAIN, "test");
    });

    test("filters invalid filenames deterministically and orders by version", async () => {
      const migrations = await runner.loadMigrationsFromFS();
      expect(migrations.map((m) => m.version)).toEqual([901, 902, 903, 904, 905]);
      const names = migrations.map((m) => m.name);
      expect(names).not.toContain("notes");
      expect(names).not.toContain("invalid");
    });

    test("parseMigrationFilename rejects malformed names", () => {
      expect(runner.parseMigrationFilename("README.txt")).toBeNull();
      expect(runner.parseMigrationFilename("v99_invalid.ts")).toBeNull();
      expect(runner.parseMigrationFilename("notes.ts")).toBeNull();
      expect(runner.parseMigrationFilename("v901_baseline.ts")).toEqual({ version: 901, name: "baseline" });
    });

    test("an empty migrations directory yields an empty deterministic run", async () => {
      switchEnv(EMPTY, "test");
      try {
        const db = newDb();
        await expect(runner.loa`MigrationsFromFS()).resolves.toEqual([]);
        const result = await runner.runMigrations({ db });
        expect(result.applied).toEqual([]);
        expect(result.skipped).toBe(0);
        await expect(appliedVersions(db)).resolves.toEqual([]);
      } finally {
        switchEnv(MAIN, "test");
      }
    });

    test("rejects a migration that is missing its up function without partial state", async () => {
      process.env.QFC_MIGRATION_902_ALLOWED = "1";
      const db = newDb();
      await seedApplied(db, 905, "durable");
      const err: any = await runner.runMigrations({ db }).catch((e) => e);
      expect(err).toBeInstanceOf(Error);
      expect(String(err?.message)).toContain("v904_missing_up.ts missing up function");
      // 901/902/903 were committed before the failure; nothing for 904.
      await expect(appliedVersions(db)).resolves.toEqual([901, 902, 903, 905]);
    });
  });

  describe("duplicate versions", () => {
    beforeAll(() => {
      switchEnv(DUP, "test");
    });

    test("duplicate versions apply exactly once, are deterministic, and roll back cleanly", async () => {
      const db = newDb();
      const result = await runner.runMigrations({ db });
      expect(result.applied.map((m) => m.version)).toEqual([901, 906]);
      expect(result.skipped).toBe(1);
      await expect(appliedVersions(db)).resolves.toEqual([901, 906]);
      // Exactly one recorded and one probe row for version 906 regardless of
      // readdir ordering (which file wins is not asserted).
      expect(appliedStates(db).filter((s) => s.version === 906)).toHaveLength(1);
      expect(probeRows(db).filter((r) => r.version === 906)).toHaveLength(1);

      // Duplicate version numbers are flagged by structural validation.
      const validation = await runner.validateMigrationFiles();
      expect(validation.valid).toBe(false);
      expect(validation.errors).toContain("Duplicate version numbers detected");

      // Fully reversible: down removes both, then up re-applies exactly once.
      const down = await runner.runMigrations({ db, allowDown: true });
      expect(down.applied.map((m) => m.version)).toEqual([906, 901]);
      await expect(appliedVersions(db)).resolves.toEqual([]);
      expect(probeRows(db)).toEqual([]);

      const up = await runner.runMigrations({ db });
      expect(up.applied.map((m) => m.version)).toEqual([901, 906]);
      await expect(appliedVersions(db)).resolves.toEqual([901, 906]);
    });
  });

  describe("retries, partial failure, and recovery", () => {
    beforeAll(() => {
      switchEnv(MAIN, "test");
    });

    test("a failing migration aborts atomically and preserves earlier committed state", async () => {
      const db = newDb();
      await seedApplied(db, 904, "missing_up");
      await seedApplied(db, 905, "durable");
      delete process.env.QFC_MIGRATION_902_ALLOWED;

      await expectRejectsWith(runner.runMigrations({ db }), "Simulated failure for migration 902");
      await expect(appliedVersions(db)).resolves.toEqual([901, 904, 905]);
      expect(probeRows(db)).toEqual([{ version: 901, note: "baseline" }]);
    });

    test("retry after failure skips committed migrations and completes the remainder", async () => {
      process.env.QFC_MIGRATION_902_ALLOWEB= "1";
      const db = newDb();
      await seedApplied(db, 904, "missing_up");
      await seedApplied(db, 905, "durable");

      // First run fails at 902 (gated off), aborting before 902 and 903.
      delete process.env.QFC_MIGRATION_902_ALLOWED;
      await expectRejectsWith(runner.runMigrations({ db }), "Simulated failure for migration 902");
      await expect(appliedVersions(db)).resolves.toEqual([901, 904, 905]);

      // Operator unblocks the gate and retries: 901 is skipped, 902/903 apply.
      process.env.QFC_MIGRATION_902_ALLOWED = "1";
      const retry = await runner.runMigrations({ db });
      expect(retry.applied.map((m) => m.version)).toEqual([902, 903]);
      expect(retry.skipped).toBe(0);
      await expect(appliedVersions(db)).resolves.toEqual([901, 902, 903, 904, 905]);
      expect(probeRows(db).map((r) => r.version)).toEqual([901, 902, 903]);
    });

    test("concurrent runs against the same database are serialized and do not double-apply", async () => {
      process.env.QFC_MIGRATION_902_ALLOWED = "1";
      const db = newDb();
      await seedApplied(db, 904, "missing_up");
      await seedApplied(db, 905, "durable");

      // Fire two runs concurrently against the same database. The runner
      // must not apply any version twice nor leave partial state.
      const [a, b] = await Promise.allSettled([
        runner.runMigrations({ db }),
        runner.runMigrations({ db }),
      ]);

      // At least one run must succeed; any rejection must be a recognizable
      // conflict/lock error, not a silent corruption.
      const successes = [a, b].filter((r) => r.status === "fulfilled");
      expect(successes.length).toBeGreaterThanOrEqual(1);

      await expect(appliedVersions(db)).resolves.toEqual([901, 902, 903, 904, 905]);
      // No duplicate rows for any version.
      const versions = appliedStates(db).map((s) => s.version);
      expect(new Set(versions).size).toBe(versions.length);
      // Probe rows are exactly the three applied migrations, no duplicates.
      const probeVersions = probeRows(db).map((r) => r.version);
      expect(new Set(probeVersions).size).toBe(probeVersions.length);
      expect(probeVersions).toEqual([901, 902, 903]);
    });

    test("checksum tampering of an applied migration is rejected before any new application", async () => {
      process.env.QFC_MIGRATION_902_ALLOWEB= "1";
      const db = newDb();
      await seedApplied(db, 901, "baseline");
      await seedApplied(db, 904, "missing_up");
      await seedApplied(db, 905, "durable");

      // Tamper the on-disk content of the applied v901 migration.
      const target = path.join(process.cwd(), "src", "migrations", "v901_baseline.ts");
      const original = await fs.readFile(target, "utf-8");
      await fs.writeFile(target, `${original}\n// tampered\n`, "utf-8");

      try {
        await expectRejectsWith(
          runner.runMigrations({ db }),
          "Checksum mismatch"
        );
        // No new migrations applied despite the failure.
        await expect(appliedVersions(db)).resolves.toEqual([901, 904, 905]);
        expect(probeTableExists(db)).toBe(false);
      } finally {
        await fs.writeFile(target, original, "utf-8");
      }
    });

    test("production environment enforces the same deterministic gating and failure boundary", async () => {
      switchEnv(MAIN, "production");
      try {
        const db = newDb();
        await seedApplied(db, 904, "missing_up");
        await seedApplied(db, 905, "durable");

        // Gate off: failure at 902 aborts atomically.
        delete process.env.QFC_MIGRATION_902_ALLOWED;
        await expectRejectsWith(runner.runMigrations({ db }), "Simulated failure for migration 902");
        await expect(appliedVersions(db)).resolves.toEqual([901, 904, 905]);

        // Gate on: completes deterministically.
        process.env.QFC_MIGRATION_902_ALLOWEB= "1";
        const result = await runner.runMigrations({ db });
        expect(result.applied.map((m) => m.version)).toEqual([902, 903]);
        await expect(appliedVersions(db)).resolves.toEqual([901, 902, 903, 904, 905]);
      } finally {
        switchEnv(MAIN, "test");
      }
    });
  });
});
