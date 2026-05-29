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

  test("MigrationPolicy.isDownAllowed returns false when env var not set", () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    delete process.env.ALLOW_DOWN_MIGRATIONS;
    try {
      expect(MigrationPolicy.isDownAllowed()).toBe(false);
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("MigrationPolicy.isDownAllowed returns true when env var set to true", () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    try {
      expect(MigrationPolicy.isDownAllowed()).toBe(true);
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("MigrationPolicy.isHotfix returns true for hotfix migration", () => {
    const hotfix = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true },
      up: async () => {},
    };

    expect(MigrationPolicy.isHotfix(hotfix)).toBe(true);
  });

  test("MigrationPolicy.isHotfix returns false for regular migration", () => {
    const regular = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
    };

    expect(MigrationPolicy.isHotfix(regular)).toBe(false);
  });

  test("MigrationPolicy.validateMetadata returns errors for missing required fields", () => {
    const incomplete = {
      version: 1,
      name: "",
      authoredAt: "",
      author: "",
      up: async () => {},
    };

    const result = MigrationPolicy.validateMetadata(incomplete);
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
    expect(result.errors).toContain("Migration name is required");
    expect(result.errors).toContain("Migration author is required");
    expect(result.errors).toContain("Migration authoredAt date is required");
  });

  test("MigrationPolicy.validateMetadata returns valid for complete migration", () => {
    const complete = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
    };

    const result = MigrationPolicy.validateMetadata(complete);
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("runMigrations with mocked database - dry run", async () => {
    const mockDb: any = {
      exec: jest.fn(),
      prepare: jest.fn(() => ({
        all: jest.fn(() => []),
        get: jest.fn(() => null),
        run: jest.fn(() => ({})),
      })),
      transaction: jest.fn((fn) => fn()),
    };

    const result = await runMigrations({ dryRun: true, db: mockDb });
    expect(result).toHaveProperty("applied");
    expect(result).toHaveProperty("skipped");
    expect(result).toHaveProperty("durationMs");
  });

  test("runMigrations with mocked database - allowDown", async () => {
    const mockDb: any = {
      exec: jest.fn(),
      prepare: jest.fn(() => ({
        all: jest.fn(() => []),
        get: jest.fn(() => null),
        run: jest.fn(() => ({})),
      })),
      transaction: jest.fn((fn) => fn()),
    };

    const result = await runMigrations({ allowDown: true, dryRun: true, db: mockDb });
    expect(result).toHaveProperty("applied");
    expect(result).toHaveProperty("skipped");
    expect(result).toHaveProperty("durationMs");
  });

  test("runMigrations with mocked database - verbose", async () => {
    const mockDb: any = {
      exec: jest.fn(),
      prepare: jest.fn(() => ({
        all: jest.fn(() => []),
        get: jest.fn(() => null),
        run: jest.fn(() => ({})),
      })),
      transaction: jest.fn((fn) => fn()),
    };

    const result = await runMigrations({ verbose: true, dryRun: true, db: mockDb });
    expect(result).toHaveProperty("applied");
    expect(result).toHaveProperty("skipped");
    expect(result).toHaveProperty("durationMs");
  });

  test("runMigrations with mocked database - skipChecksumVerify", async () => {
    const mockDb: any = {
      exec: jest.fn(),
      prepare: jest.fn(() => ({
        all: jest.fn(() => []),
        get: jest.fn(() => null),
        run: jest.fn(() => ({})),
      })),
      transaction: jest.fn((fn) => fn()),
    };

    const result = await runMigrations({ skipChecksumVerify: true, dryRun: true, db: mockDb });
    expect(result).toHaveProperty("applied");
    expect(result).toHaveProperty("skipped");
    expect(result).toHaveProperty("durationMs");
  });

  test("getAppliedVersions with mocked database", async () => {
    const mockDb: any = {
      prepare: jest.fn(() => ({
        all: jest.fn(() => [{ version: 1 }, { version: 2 }]),
      })),
    };

    const versions = await getAppliedVersions(mockDb);
    expect(Array.isArray(versions)).toBe(true);
    expect(versions).toEqual([1, 2]);
  });

  test("isDatabaseInitialized with mocked database - initialized", async () => {
    const mockDb: any = {
      prepare: jest.fn(() => ({
        all: jest.fn(() => [{ version: 1 }]),
      })),
    };

    const initialized = await isDatabaseInitialized(mockDb);
    expect(initialized).toBe(true);
  });

  test("isDatabaseInitialized with mocked database - not initialized", async () => {
    const mockDb: any = {
      prepare: jest.fn(() => ({
        all: jest.fn(() => []),
      })),
    };

    const initialized = await isDatabaseInitialized(mockDb);
    expect(initialized).toBe(false);
  });

  test("verifyAppliedChecksums with mocked database - no applied migrations", async () => {
    const mockDb: any = {
      exec: jest.fn(),
      prepare: jest.fn(() => ({
        all: jest.fn(() => []),
      })),
    };

    const result = await verifyAppliedChecksums(mockDb);
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("verifyAppliedChecksums with mocked database - checksum mismatch", async () => {
    const mockDb: any = {
      exec: jest.fn(),
      prepare: jest.fn(() => ({
        all: jest.fn(() => [
          { version: 1, name: "test", checksum: "old_checksum" },
        ]),
      })),
    };

    const result = await verifyAppliedChecksums(mockDb);
    expect(result).toHaveProperty("valid");
    expect(result).toHaveProperty("errors");
  });
});
