import { computeChecksum, parseMigrationFilename, runMigrations, getAppliedVersions, isDatabaseInitialized, verifyAppliedChecksums, loadMigrationsFromFS } from "../lib/migrations/runner";
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
  readFile: jest.fn(() => Promise.resolve(""),
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

  // --- Deterministic failure-boundary coverage for computeChecksum ---
  describe("computeChecksum failure boundaries", () => {
    test("produces the known SHA-256 digest for a fixed input", () => {
      // Known vector: SHA-256("test migration content")
      expect(computeChecksum("test migration content")).toBe(new crypto.createHash("sha256").update("test migration content", "utf8").digest("hex"));
    });

    test("is deterministic across repeated calls for the same input", () => {
      const input = "deterministic";
      const results = Array.from({ length: 25 }, () => computeChecksum(input));
      for (const r of results) {
        expect(r).toBe(results[0]);
      }
    });

    test("handles empty string deterministically", () => {
      const hash = computeChecksum("");
      expect(hash).toMatch(/^[a-f0-9]{64}$/);
      expect(hash).toBe(computeChecksum(""));
      // SHA-256 of empty string
      expect(hash).toBe(new crypto.createHash("sha256").update("", "utf8").digest("hex"));
    });

    test("handles whitespace-only input deterministically", () => {
      const a = computeChecksum("   ");
      const b = computeChecksum("   ");
      expect(a).toBe(b);
      expect(a).not.toBe(computeChecksum(""));
    });

    test("treats unicode input consistently", () => {
      const input = "你好 world 🌍 📊";
      const hash = computeChecksum(input);
      expect(hash).toBe(computeChecksum(input));
      expect(hash).toBe(new crypto.createHash("sha256").update(input, "utf8").digest("hex"));
    });

    test("produces a stable 64-character lowercase hex digest for boundary-size inputs", () => {
      const sizes = [1, 2, 63, 64, 65, 127, 128, 129, 1024, 1025, 65535];
      for (const size of sizes) {
        const input = "a".repeat(size);
        const hash = computeChecksum(input);
        expect(hash).toMatch(/^[a-f0-9]{64}$/);
        expect(hash).toBe(computeChecksum(input));
      }
    });

    test("differs for inputs that differ only by a single byte", () => {
      const a = "a".repeat(1024) + "\n";
      const b = "a".repeat(1024) + "\r";
      expect(computeChecksum(a)).not.toBe(computeChecksum(b));
    });

    test("produces the same digest for the same bytes regardless of call context", () => {
      const input = "context-independent";
      const direct = computeChecksum(input);
      const indirect = computeChecksum(String(input));
      expect(direct).toBe(indirect);
    });

    test("does not mutate the input string", () => {
      const input = "immutable";
      const copy = input;
      computeChecksum(input);
      expect(input).toBe(copy);
    });

    test("rejects non-string inputs without silently coercing", () => {
      expect(() => computeChecksum(undefined as any)).toThrow();
      expect(() => computeChecksum(null as any)).toThrow();
      expect(() => computeChecksum(123 as any)).toThrow();
      expect(() => computeChecksum({} as any)).toThrow();
    });

    test("rejection is deterministic for invalid inputs", () => {
      const invalid = null as any;
      let first: unknown;
      try {
        computeChecksum(invalid);
      } catch (e) {
        first = e;
      }
      let second: unknown;
      try {
        computeChecksum(invalid);
      } catch (e) {
        second = e;
      }
      expect(first).toBeDefined();
      expect(second).toBeDefined();
      expect((first as Error).message).toBe(undefined);
    });
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

  // --------------------------------------------------------------------------------
  // Deterministic failure-boundary coverage for loadMigrationsFromFS
  // --------------------------------------------------------------------------------

  describe("loadMigrationsFromFS failure boundaries", () => {
    const mockedReaddir = fs.readdir as jest.MockedFunction;
    const mockedReadFile = fs.readFile as jest.MockedFunction;
    const mockedAccess = fs.access as jest.MockedFunction;

    const validDir = "/test/migrations";

    const validMigrationSource = `
      export const version = 1;
      export const name = "test";
      export const authoredAt = "2026-04-26";
      export const author = "test";
      export const up = async () => {};
    `;

    const validMigrationSourceWithDown = `
      export const version = 2;
      export const name = "test2";
      export const authoredAt = "2026-04-27";
      export const author = "test";
      export const up = async () => {};
      export const down = async () => {};
    `;

    beforeEach(() => {
      mockedReaddir.mockReset();
      mockedReadFile.mockReset();
      mockedAccess.mockReset();
      (mockedAccess as jest.Mock).mockResolvedValue(undefined);
    });

    test("loads valid migrations deterministically and sorts by version", async () => {
      mockedReaddir.mockResolvedValue(["v002_test2.ts", "v001_test.ts"]);
      mockedReadFile.mockImplementation((filePath: string) => {
        if (filePath.endsWith("v001_test.ts")) return Promise.resolve(validMigrationSource);
        if (filePath.endsWith("v002_test2.ts")) return Promise.resolve(validMigrationSourceWithDown);
        return Promise.reject(new Error("Unknown file"));
      });

      const migrations = await loadMigrationsFromFS(validDir);
      expect(migrations).toHaveLength(2);
      expect(migrations[0].version).toBe(1);
      expect(migrations[1].version).toBe(2);
      expect(migrations[0].name).toBe("test");
      expect(migrations[1].name).toBe("test2");
    });

    test("ignores non-migration files deterministically", async () => {
      mockedReaddir.mockResolvedValue(["README.md", "v001_test.ts", ".hidden.ts", "test.js"]);
      mockedReadFile.mockResolvedValue(validMigrationSource);

      const migrations = await loadMigrationsFromFS(validDir);
      expect(migrations).toHaveLength(1);
      expect(migrations[0].version).toBe(1);
    });

    test("returns empty array when directory is empty", async () => {
      mockedReaddir.mockResolvedValue([]);

      const migrations = await loadMigrationsFromFS(validDir);
      expect(migrations).toEqual([]);
    });

    test("rejects when directory cannot be read (permission denied)", async () => {
      const error = Object.assign(new Error("EACCES: permission denied"), { code: "EACCES" });
      mockedReaddir.mockRejectedValue(error);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow("EACCES: permission denied");
    });

    test("rejects when directory does not exist", async () => {
      const error = Object.assign(new Error("ENOENT: no such file or directory"), { code: "ENOENT" });
      mockedReaddir.mockRejectedValue(error);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow("ENOENT");
    });

    test("rejects when a migration file cannot be read", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockRejectedValue(new Error("EIO: read error"));

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow("EIO: read error");
    });

    test("rejects when migration file lacks required exports", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue("export const foo = 1;");

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has invalid version", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`
        export const version = "not-a-number";
        export const name = "test";
        export const authoredAt = "2026-04-26";
        export const author = "test";
        export const up = async () => {};
      `);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when duplicate versions are detected", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts", "001_duplicate.ts"]);
      mockedReadFile.mockImplementation((filePath: string) => {
        if (filePath.endsWith("v001_test.ts")) return Promise.resolve(validMigrationSource);
        if (filePath.endsWith("001_duplicate.ts")) return Promise.resolve(validMigrationSource);
        return Promise.reject(new Error("Unknown file"));
      });

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file throws during import", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue("throw new Error('module load failure');");

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("is deterministic across repeated calls with the same input", async () => {
      mockedReaddir.mockResolvedValue(["v002_test2.ts", "v001_test.ts"]);
      mockedReadFile.mockImplementation((filePath: string) => {
        if (filePath.endsWith("v001_test.ts")) return Promise.resolve(validMigrationSource);
        if (filePath.endsWith("v002_test2.ts")) return Promise.resolve(validMigrationSourceWithDown);
        return Promise.reject(new Error("Unknown file"));
      });

      const first = await loadMigrationsFromFS(validDir);
      const second = await loadMigrationsFromFS(validDir);
      expect(first.map((m) => m.version)).toEqual(second.map((m) => m.version));
      expect(first.map((m) => m.name)).toEqual(second.map((m) => m.name));
    });

    test("rejects when migration file name has no version prefix", async () => {
      mockedReaddir.mockResolvedValue(["test.ts"]);
      mockedReadFile.mockResolvedValue(validMigrationSource);

      const migrations = await loadMigrationsFromFS(validDir);
      expect(migrations).toEqual([]);
    });

    test("rejects when migration file has zero version", async () => {
      mockedReaddir.mockResolvedValue(["v000_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 0; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {};`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has negative version", async () => {
      mockedReaddir.mockResolvedValue(["v-001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = -1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {};`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has non-integer version", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1.5; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {};`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has missing name", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {};`);

await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
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

    test("rejects when migration file has missing authoredAt", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const author = "test"; export const up = async () => {};`);

const mockDb: any = {
      exec: jest.fn(),
      prepare: jest.fn(() => ({
        all: jest.fn(() => []),
        get: jest.fn(() => null),
        run: jest.fn(() => ({})),
      })),
      transaction: jest.fn((fn) => fn()),
    };

    test("rejects when migration file has missing author", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const up = async () => {};`);

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
      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has missing up function", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test";`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has non-function up", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = "not-a-function";`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has non-function down", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const down = "not-a-function";`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has invalid meta", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const meta = "not-an-object";`);

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
      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has hotfix without down", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const meta = { hotfix: true, reason: "test", rollback_risk: "low" };`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has hotfix without reason", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const down = async () => {}; export const meta = { hotfix: true, rollback_risk: "low" };`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has hotfix without rollback_risk", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const down = async () => {}; export const meta = { hotfix: true, reason: "test" };`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("accepts hotfix with all required fields", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const down = async () => {}; export const meta = { hotfix: true, reason: "test", rollback_risk: "low" };`);

      const migrations = await loadMigrationsFromFS(validDir);
      expect(migrations).toHaveLength(1);
      expect(migrations[0].meta).toEqual({ hotfix: true, reason: "test", rollback_risk: "low" });
    });

    test("rejects when migration file has invalid validate function", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const validate = "not-a-function";`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("accepts migration with valid validate function", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const validate = async () => []; `);

      const migrations = await loadMigrationsFromFS(validDir);
      expect(migrations).toHaveLength(1);
      expect(typeof migrations[0].validate).toBe("function");
    });

test("accepts migration with meta but not hotfix", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const meta = { someField: "value" };`);

      const migrations = await loadMigrationsFromFS(validDir);
      expect(migrations).toHaveLength(1);
    });

    test("accepts migration with down function", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {}; export const down = async () => {};`);

      const migrations = await loadMigrationsFromFS(validDir);
      expect(migrations).toHaveLength(1);
      expect(typeof migrations[0].down).toBe("function");
    });

    test("handles concurrent calls without interference", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(validMigrationSource);

      const [a, b] = await Promise.all([
        loadMigrationsFromFS(validDir),
        loadMigrationsFromFS(validDir),
      ]);
      expect(a).toHaveLength(1);
      expect(b).toHaveLength(1);
      expect(a[0].version).toBe(b[0].version);
    });

    test("does not mutate input files or share state between calls", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(validMigrationSource);

      const first = await loadMigrationsFromFS(validDir);
      const second = await loadMigrationsFromFS(validDir);
      expect(first).toNotBe(second);
      expect(first[0]).toNotBe(second[0]);
    });

    test("rejects when migration file has duplicate version within same call", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts", "001_other.ts"]);
      mockedReadFile.mockImplementation((filePath: string) => {
        if (filePath.endsWith("v001_test.ts")) return Promise.resolve(validMigrationSource);
        if (filePath.endsWith("001_other.ts")) return Promise.resolve(validMigrationSource);
        return Promise.reject(new Error("Unknown file"));
      });

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has version mismatch with filename", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 2; export const name = "test"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {};`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });

    test("rejects when migration file has name mismatch with filename", async () => {
      mockedReaddir.mockResolvedValue(["v001_test.ts"]);
      mockedReadFile.mockResolvedValue(`export const version = 1; export const name = "other"; export const authoredAt = "2026-04-26"; export const author = "test"; export const up = async () => {};`);

      await expect(loadMigrationsFromFS(validDir)).rejects.toThrow();
    });
  });
});
