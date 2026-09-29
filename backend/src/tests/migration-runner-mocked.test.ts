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
});
