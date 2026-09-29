import { computeChecksum, parseMigrationFilename, runMigrations, getAppliedVersions, isDatabaseInitialized, verifyAppliedChecksums } from "../lib/migrations/runner";
import { MigrationPolicy } from "../lib/migrations/policy";
import * as fs from "fs/promises";
import * as path from "path";

// Mock the database module
jest.mock("../lib/database", () => ({
  getDatabase: jest.fn(() => ({
    exec: jest.fn(),
    prepare: jest.fn(() => ({
      all: jest.fn(() => []),
      get: jest.fn(() => undefined),
      run: jest.fn(() => ({ lastInsertRowId: 1, changes: 1 })),
    })),
  })),
  closeDatabase: jest.fn(),
}));

// Mock filesystem
jest.mock("fs/promises", () => ({
  readdir: jest.fn(() => Promise.resolve([])),
  readFile: jest.fn(() => Promise.resolve("")),
  access: jest.fn(() => Promise.resolve()),
}));
jest.mock("path");

describe("Migration Runner with Mocked Database", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  test("computeChecksum produces consistent results", () => {
    const input = "test migration content";
    const hash1 = computeChecksum(input);
    const hash2 = computeChecksum(input);
    expect(hash1).toBe(hash2);
    expect(hash1).toMatch(/^[a-f0-9]{64}$/);
  });

  test("computeChecksum differs for different inputs", () => {
    const hash1 = computeChecksum("input1");
    const hash2 = computeChecksum("input2");
    expect(hash1).not.toBe(hash2);
  });

  test("parseMigrationFilename handles various formats", () => {
    expect(parseMigrationFilename("v001_test.ts")).toEqual({ version: 1, name: "test" });
    expect(parseMigrationFilename("001_test.ts")).toEqual({ version: 1, name: "test" });
    expect(parseMigrationFilename("v123_long_name.ts")).toEqual({ version: 123, name: "long_name" });
    expect(parseMigrationFilename("invalid.txt")).toBeNull();
  });

  test("MigrationPolicy.dryRun validates migrations", async () => {
    const migrations = [
      {
        version: 1,
        name: "test",
        authoredAt: "2026-04-26",
        author: "test",
        up: async () => {},
      },
    ];

    const result = await MigrationPolicy.dryRun(migrations);
    expect(result).toHaveProperty("valid");
    expect(result).toHaveProperty("errors");
    expect(result).toHaveProperty("warnings");
  });

  test("MigrationPolicy.dryRun detects invalid migrations", async () => {
    const invalidMigrations = [
      {
        version: 1,
        name: "",
        authoredAt: "2026-04-26",
        author: "test",
        up: async () => {},
      },
    ];

    const result = await MigrationPolicy.dryRun(invalidMigrations);
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
  });

  test("MigrationPolicy.dryRun with force option", async () => {
    const migrations = [
      {
        version: 1,
        name: "test",
        authoredAt: "2026-04-26",
        author: "test",
        up: async () => {},
      },
    ];

    const result = await MigrationPolicy.dryRun(migrations, { force: true });
    expect(result).toHaveProperty("valid");
  });

  test("MigrationPolicy.dryRun with multiple migrations", async () => {
    const migrations = [
      {
        version: 1,
        name: "test1",
        authoredAt: "2026-04-26",
        author: "test",
        up: async () => {},
      },
      {
        version: 2,
        name: "test2",
        authoredAt: "2026-04-26",
        author: "test",
        up: async () => {},
      },
    ];

    const result = await MigrationPolicy.dryRun(migrations);
    expect(result).toHaveProperty("valid");
    expect(result).toHaveProperty("errors");
    expect(result).toHaveProperty("warnings");
  });

  test("MigrationPolicy.dryRun with hotfix migration missing down", async () => {
    const hotfixWithoutDown = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, reason: "test", rollback_risk: "low" },
      up: async () => {},
    };

    const result = await MigrationPolicy.dryRun([hotfixWithoutDown]);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("1_hotfix_test: Hotfix migrations must include a down function");
  });

  test("MigrationPolicy.dryRun with hotfix migration missing reason", async () => {
    const hotfixWithoutReason = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, rollback_risk: "low" },
      up: async () => {},
      down: async () => {},
    };

    const result = await MigrationPolicy.dryRun([hotfixWithoutReason]);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("1_hotfix_test: Hotfix migrations must include meta.reason");
  });

  test("MigrationPolicy.dryRun with hotfix migration missing rollback_risk", async () => {
    const hotfixWithoutRisk = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, reason: "test" },
      up: async () => {},
      down: async () => {},
    };

    const result = await MigrationPolicy.dryRun([hotfixWithoutRisk]);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("1_hotfix_test: Hotfix migrations must include meta.rollback_risk");
  });

  test("MigrationPolicy.dryRun with valid hotfix migration", async () => {
    const validHotfix = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, reason: "test", rollback_risk: "low" },
      up: async () => {},
      down: async () => {},
    };

    const result = await MigrationPolicy.dryRun([validHotfix]);
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("MigrationPolicy.dryRun with migration missing author", async () => {
    const migrationWithoutAuthor = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "",
      up: async () => {},
    };

    const result = await MigrationPolicy.dryRun([migrationWithoutAuthor]);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("1_test: Migration author is required");
  });

  test("MigrationPolicy.dryRun with migration missing authoredAt", async () => {
    const migrationWithoutDate = {
      version: 1,
      name: "test",
      authoredAt: "",
      author: "test",
      up: async () => {},
    };

    const result = await MigrationPolicy.dryRun([migrationWithoutDate]);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("1_test: Migration authoredAt date is required");
  });

  test("MigrationPolicy.dryRun with migration missing up function", async () => {
    const migrationWithoutUp = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
    } as any;

    const result = await MigrationPolicy.dryRun([migrationWithoutUp]);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("1_test: Migration up function is required");
  });

  test("MigrationPolicy.dryRun with migration having validate function", async () => {
    const migrationWithValidate = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
      validate: async () => ["warning message"],
    };

    const result = await MigrationPolicy.dryRun([migrationWithValidate]);
    expect(result).toHaveProperty("valid");
    expect(result).toHaveProperty("warnings");
    // Validate function warnings may or may not be included depending on implementation
  });

  test("MigrationPolicy.dryRun with migration having validate function returning no warnings", async () => {
    const migrationWithValidate = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
      validate: async () => [],
    };

    const result = await MigrationPolicy.dryRun([migrationWithValidate]);
    expect(result.valid).toBe(true);
    expect(result.warnings).toEqual([]);
  });

  test("MigrationPolicy.dryRun with hotfix having low rollback_risk", async () => {
    const hotfixLowRisk = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, reason: "test", rollback_risk: "low" },
      up: async () => {},
      down: async () => {},
    };

    const result = await MigrationPolicy.dryRun([hotfixLowRisk]);
    expect(result.valid).toBe(true);
  });

  test("MigrationPolicy.dryRun with migration having meta but not hotfix", async () => {
    const migrationWithMeta = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { someField: "value" },
      up: async () => {},
    };

    const result = await MigrationPolicy.dryRun([migrationWithMeta]);
    expect(result.valid).toBe(true);
  });

  test("MigrationPolicy.dryRun with empty migrations array", async () => {
    const result = await MigrationPolicy.dryRun([]);
    expect(result).toHaveProperty("valid");
    expect(result).toHaveProperty("errors");
    expect(result).toHaveProperty("warnings");
  });

  test("MigrationPolicy.dryRun with migration having down function", async () => {
    const migrationWithDown = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
      down: async () => {},
    };

    const result = await MigrationPolicy.dryRun([migrationWithDown]);
    expect(result.valid).toBe(true);
  });

  test("MigrationPolicy.dryRun with hotfix having all required fields", async () => {
    const completeHotfix = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, reason: "Critical bug fix", rollback_risk: "medium" },
      up: async () => {},
      down: async () => {},
    };

    const result = await MigrationPolicy.dryRun([completeHotfix]);
    expect(result.valid).toBe(true);
  });

  test("MigrationPolicy.dryRun with hotfix missing down function", async () => {
    const hotfixWithoutDown = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, reason: "test", rollback_risk: "low" },
      up: async () => {},
    };

    const result = await MigrationPolicy.dryRun([hotfixWithoutDown]);
    expect(result.valid).toBe(false);
  });

  test("MigrationPolicy.dryRun with hotfix missing reason", async () => {
    const hotfixWithoutReason = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, rollback_risk: "low" },
      up: async () => {},
      down: async () => {},
    };

    const result = await MigrationPolicy.dryRun([hotfixWithoutReason]);
    expect(result.valid).toBe(false);
  });

  test("MigrationPolicy.dryRun with hotfix missing rollback_risk", async () => {
    const hotfixWithoutRisk = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, reason: "test" },
      up: async () => {},
      down: async () => {},
    };

    const result = await MigrationPolicy.dryRun([hotfixWithoutRisk]);
    expect(result.valid).toBe(false);
  });
});

/**
 * Deterministic failure-boundary coverage for verifyAppliedChecksums.
 *
 * Invariants:
 * - verifyAppliedChecksums must never mutate the database.
 * - A missing applied row is a no-op (not a failure).
 * - A checksum mismatch must fail deterministically and surface the version/name.
 * - A missing on-disk file for an applied migration must fail deterministically.
 * - Errors from the database layer must propagate without being swallowed.
 */
describe("verifyAppliedChecksums failure boundaries", () => {
  const getDatabase = require("../lib/database").getDatabase as jest.Mock;
  const mockReaddir = fs.readdir as jest.Mock;
  const mockReadFile = fs.readFile as jest.Mock;
  const mockAccess = fs.access as jest.Mock;

  type AppliedRow = { version: number; name: string; checksum: string };

  function installDbDouble(rows: AppliedRow | Error, options: { onPrepare?: () => void } = {}) {
    const all = jest.fn(() => {
      if (rows instanceof Error) {
        throw rows;
      }
      return rows;
    });
    const prepare = jest.fn(() => {
      if (options.onPrepare) {
        options.onPrepare();
      }
      return {
        all,
        get: jest.fn(() => undefined),
        run: jest.fn(() => ({ changes: 0, lastInsertRowId: 0 })),
      };
    });
    const exec = jest.fn(() => undefined);
    getDatabase.mockReturnValue({ exec, prepare });
    return { exec, prepare, all };
  }

  function installDiskDouble(files: Record<string, string> | Error, options: { onRead?: () => void } = {}) {
    mockReaddir.mockImplementation(async () => {
      if (files instanceof Error) {
        throw files;
      }
      return Object.keys(files);
    });
    mockReadFile.mockImplementation(async (p: known) => {
      if (options.onRead) {
        options.onRead();
      }
      if (files instanceof Error) {
        throw files;
      }
      const key = String(p);
      const match = Object.keys(files).find((k) => key.endsWith(k));
      if (!match) {
        const err = new Error(`ENOENT: no such file, open '${key}'`);
        (err as any).code = "ENOENT";
        throw err;
      }
      return files[match];
    });
    mockAccess.mockImplementation(async (p: known) => {
      if (files instanceof Error) {
        throw files;
      }
      const key = String(p);
      const match = Object.keys(files).find((k) => key.endsWith(k));
      if (!match) {
        const err = new Error(`ENOENT: no such file, access '${key}'`);
        (err as any).code = "ENOENT";
        throw err;
      }
    });
  }

  beforeEach(() => {
    jest.clearAllMocks();
    // Restore default mock behavior for fs/path between tests.
    mockReaddir.mockReset();
    mockReadFile.mockReset();
    mockAccess.mockReset();
    mockReaddir.mockImplementation(async () => []);
    mockReadFile.mockImplementation(async () => "");
    mockAccess.mockImplementation(async () => undefined);
  });

  test("returns without throwing when no migrations have been applied", async () => {
    const { exec, prepare } = installDrDouble([]);
    await expect(verifyAppliedChecksums()).resolves.toUndefined();
    expect(exec).not.toHaveBeenCalled();
    // No prepare needed when there are no applied rows.
    expect(prepare.mock.calls.length).toBe(LessThanOrEqual(1));
  });

  test("passes when every applied checksum matches the on-disk file", async () => {
    const content = "export const up = async () => {};\n";
    const checksum = computeChecksum(content);
    installDbDouble([{ version: 1, name: "init", checksum }]);
    installDiskDouble({ "001_init.ts": content });

    await expect(verifyAppliedChecksums()).resolves.toUndefined();
  });

  test("fails deterministically when an applied checksum differs from the file", async () => {
    const onDisk = "export const up = async () => {};\n";
    const staleChecksum = computeChecksum("old content");
    installDbDouble([{ version: 7, name: "add_users", checksum: staleChecksum }]);
    installDiskDouble({ "007_add_users.ts": onDisk });

    await expect(verifyAppliedChecksums()).rejects.toThrow(/7_add_users/);
  });

  test("fails when an applied migration has no corresponding on-disk file", async () => {
    installDbDouble([{ version: 3, name: "missing_file", checksum: "deadbeef" }]);
    installDiskDouble({ "001_init.ts": "export const up = async () => {};\n" });

    await expect(verifyAppliedChecksums()).rejects.toThrow(/3_missing_file/);
  });

  test("propagates database read errors without swallowing them", async () => {
    const dbError = new Error("database is locked");
    installDrDouble(dbError);
    installDiskDouble({ "001_init.ts": "export const up = async () => {};\n" });

    await expect(verifyAppliedChecksums()).rejects.toThrow(/database is locked/);
  });

  test("propagates filesystem read errors without swallowing them", async () => {
    const content = "export const up = async () => {};\n";
    installDrDouble([{ version: 1, name: "init", checksum: computeChecksum(content) }]);
    const fsError = new Error("EIO failure");
    installDiskDouble(fsError);

    await expect(verifyAppliedChecksums()).rejects.toThrow(/EIO failure/);
  });

  test("returns the same result on repeated invocations (deterministic)", async () => {
    const content = "export const up = async () => {};\n";
    const checksum = computeChecksum(content);
    installDrDouble([{ version: 1, name: "init", checksum }]);
    installDiskDouble({ "001_init.ts": content });

    const first = await verifyAppliedChecksums().then(
      () => "ok",
      (e) => `error:${(e as Error).message}`,
    );
    const second = await verifyAppliedChecksums().then(
      () => "ok",
      (e) => `error:${(e as Error).message}`,
    );
    expect(first).toBe("ok");
    expect(second).toBe(first);
  });

  test("fails deterministically on a checksum mismatch even when other migrations are valid", async () => {
    const good = "export const up = async () => {};\n";
    const bad = "export const up = async () => { throw new Error('x'); };\n";
    installDbDouble([
      { version: 1, name: "init", checksum: computeChecksum(good) },
      { version: 2, name: "broken", checksum: computeChecksum("different") },
    ]);
    installDiskDouble({
      "001_init.ts": good,
      "002_broken.ts": bad,
    });

    await expect(verifyAppliedChecksums()).rejects.toThrow(/2_broken/);
  });

  test("does not mutate the database during verification", async () => {
    const content = "export const up = async () => {};\n";
    const { exec, prepare } = installDbDouble([
      { version: 1, name: "init", checksum: computeChecksum(content) },
    ]);
    installDiskDouble({ "001_init.ts": content });

    await verifyAppliedChecksums();

    expect(exec).not.toHaveBeenCalled();
    for (const call of prepare.mock.calls) {
      const stmt = String(call[0]).toLowerCase();
      expect(stmt).not.toMatch(/^\s*(insert|update|delete|drop|alter|create|replace)\b/);
    }
  });
});
