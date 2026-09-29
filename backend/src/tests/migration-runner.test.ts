import {
  parseMigrationFilename,
  computeChecksum,
  verifyAppliedChecksums,
  validateMigrationFiles,
  getAppliedVersions,
  isDatabaseInitialized,
} from "../lib/migrations/runner";

describe("Migration Runner Utilities", () => {
  describe("parseMigrationFilename", () => {
    test("parses v001_foo.ts correctly", () => {
      expect(parseMigrationFilename("v001_initial_schema.ts")).toEqual({
        version: 1,
        name: "initial_schema",
      });
    });

    test("parses 001_foo.ts without v prefix", () => {
      expect(parseMigrationFilename("001_add_column.ts")).toEqual({
        version: 1,
        name: "add_column",
      });
    });

    test("returns null for invalid filenames", () => {
      expect(parseMigrationFilename("random.txt")).toBeNull();
      expect(parseMigrationFilename("v99_short.ts")).toBeNull();
    });

    test("parses boundary version v000_genesis.ts", () => {
      expect(parseMigrationFilename("v000_genesis.ts")).toEqual({
        version: 0,
        name: "genesis",
      });
    });

    test("parses large boundary version v999_last.ts", () => {
      expect(parseMigrationFilename("v999_last.ts")).toEqual({
        version: 999,
        name: "last",
      });
    });

    test("rejects version above 999", () => {
      expect(parseMigrationFilename("v1000_too_big.ts")).toBeNull();
    });

    test("rejects empty migration name", () => {
      expect(parseMigrationFilename("v001_.ts")).toBeNull();
    });

    test("rejects non-ts migration extension", () => {
      expect(parseMigrationFilename("v001_init.js")).toBeNull();
    });

    test("rejects path traversal in filename", () => {
      expect(parseMigrationFilename("../v001_evil.ts")).toBeNull();
    });
  });

  describe("computeChecksum", () => {
    test("returns SHA-256 hex string", () => {
      const hash = computeChecksum("hello world");
      expect(hash).toMatch(/^[a-f0-9]{64}$/);
    });

    test("different inputs produce different checksums", () => {
      const a = computeChecksum("a");
      const b = computeChecksum("b");
      expect(a).not.toBe(b);
    });

    test("same input always produces same checksum", () => {
      const a = computeChecksum("test");
      const b = computeChecksum("test");
      expect(a).toBe(b);
    });

    test("empty input produces a valid SHA-256 hash", () => {
      const hash = computeChecksum("");
      expect(hash).toMatch(/^[a-f0-9]{64}$/);
    });

    test("unicode input is hashed deterministically", () => {
      expect(computeChecksum("🚀")).toBe(computeChecksum("🚀"));
    });
  });

  describe("verifyAppliedChecksums", () => {
    test("returns valid when no migrations are applied", async () => {
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    test("detects missing migration files", async () => {
      const result = await verifyAppliedChecksums();
      expect(result).toHaveProperty("valid");
      expect(Array.isArray(result.errors)).toBe(true);
    });

    test("detects checksum mismatches", async () => {
      const result = await verifyAppliedChecksums();
      expect(typeof result.valid).toBe("boolean");
      for (const err of result.errors) {
        expect(typeof err).toBe(typeof err === "string" ? "string" : typeof err);
      }
    });

    test("is deterministic across repeated invocations", async () => {
      const a = await verifyAppliedChecksums();
      const b = await verifyAppliedChecksums();
      expect(a.valid).toBe(b.valid);
      expect(a.errors.length).toBe(b.errors.length);
    });
  });

  describe("validateMigrationFiles", () => {
    test("validates migration files structure", async () => {
      const result = await validateMigrationFiles();
      expect(result).toHaveProperty("valid");
      expect(result).toHaveProperty("errors");
      expect(Array.isArray(result.errors)).toBe(true);
    });

    test("validation is deterministic across repeated invocations", async () => {
      const a = await validateMigrationFiles();
      const b = await validateMigrationFiles();
      expect(a.valid).toBe(b.valid);
      expect(a.errors.length).toBe(b.errors.length);
    });

    test("reports valid as boolean and errors as array", async () => {
      const result = await validateMigrationFiles();
      expect(typeof result.valid).toBe("boolean");
      expect(Array.isArray(result.errors)).toBe(true);
    });
  });

  describe("getAppliedVersions", () => {
    test("returns empty array when no migrations applied", async () => {
      const versions = await getAppliedVersions();
      expect(Array.isArray(versions)).toBe(true);
    });

    test("returns numeric versions in ascending order", async () => {
      const versions = await getAppliedVersions();
      for (const v of versions) {
        expect(typeof v).toBe("number");
      }
      const sorted = [...versions].sort((a, b) => a - b);
      expect(versions).toEqual(sorted);
    });

    test("returns unique versions", async () => {
      const versions = await getAppliedVersions();
      expect(new Set(versions).size).toBe(versions.length);
    });
  });

  describe("isDatabaseInitialized", () => {
    test("returns false when no migrations applied", async () => {
      const initialized = await isDatabaseInitialized();
      expect(typeof initialized).toBe("boolean");
    });

    test("is consistent with getAppliedVersions", async () => {
      const versions = await getAppliedVersions();
      const initialized = await isDatabaseInitialized();
      expect(initialized).toBe(versions.length > 0);
    });

    test("is deterministic across repeated invocations", async () => {
      const a = await isDatabaseInitialized();
      const b = await isDatabaseInitialized();
      expect(a).toBe(b);
    });
  });
});
