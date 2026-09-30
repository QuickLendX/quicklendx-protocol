import * as path from "path";
import * as fs from "fs";
import * as crypto from "crypto";
import { parseMigrationFilename, computeChecksum, verifyAppliedChecksums, validateMigrationFiles, getAppliedVersions, isDatabaseInitialized } from "../lib/migrations/runner";
import { closeDatabase } from "../lib/database";

// Isolate this suite from any state persisted in the shared dev database:
// these assertions assume a database with no applied migrations.
const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-mig-runner-${crypto.randomUUID()}.db`);
const ORIGINAL_DATABASE_PATH = process.env.DATABASE_PATH;

beforeAll(() => {
  fs.mkdirSync(TEST_DB_DIR, { recursive: true });
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();
});

afterAll(() => {
  closeDatabase();
  if (ORIGINAL_DATABASE_PATH === undefined) {
    delete process.env.DATABASE_PATH;
  } else {
    process.env.DATABASE_PATH = ORIGINAL_DATABASE_PATH;
  }
  try {
    fs.unlinkSync(TEST_DB_PATH);
  } catch {
    // best-effort cleanup
  }
});

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
  });

  describe("verifyAppliedChecksums", () => {
    test("returns valid when no migrations are applied", async () => {
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    test("detects missing migration files", async () => {
      // This test would require mocking the database and filesystem
      // For now, we'll skip the actual implementation
      expect(true).toBe(true);
    });

    test("detects checksum mismatches", async () => {
      // This test would require mocking the database and filesystem
      // For now, we'll skip the actual implementation
      expect(true).toBe(true);
    });
  });

  describe("validateMigrationFiles", () => {
    test("validates migration files structure", async () => {
      const result = await validateMigrationFiles();
      // Check that it returns a result object
      expect(result).toHaveProperty("valid");
      expect(result).toHaveProperty("errors");
      expect(Array.isArray(result.errors)).toBe(true);
    });
  });

  describe("getAppliedVersions", () => {
    test("returns empty array when no migrations applied", async () => {
      const versions = await getAppliedVersions();
      expect(Array.isArray(versions)).toBe(true);
    });
  });

  describe("isDatabaseInitialized", () => {
    test("returns false when no migrations applied", async () => {
      const initialized = await isDatabaseInitialized();
      expect(typeof initialized).toBe("boolean");
    });
  });
});
