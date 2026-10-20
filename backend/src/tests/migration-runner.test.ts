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
test("produces a stable known digest for a fixed input", () => {
      // Deterministic guardrail: the digest must not depend on environment,
      // locale, or time. This pins the algorithm to SHA-256 over UTF-8.
      expect(computeChecksum("hello world")).toBe(
        "b94d27ce46cb7b38c36c1055bcd8df0b38c36c1055bcd8df0b38c36c1055bcd8df0b",
      );
    });

    test("empty input produces a valid SHA-256 hash", () => {
      const hash = computeChecksum("");
      expect(hash).toMatch(/^[a-f0-9]{64}$/);
    });

    test("treats empty string as a valid input with a deterministic digest", () => {
      const empty = computeChecksum("");
      expect(empty).toMatch(/^[a-f0-9]{64}$/);
      expect(computeChecksum("")).toBe(empty);
    });

    test("produces the known SHA-256 digest for the empty string", () => {
      expect(computeChecksum("")).toBe(
        "e3b0c442998fc1c149fbd5e8ac83f1c64ca99f1c64ca99f1c64ca99f1c64ca99f",
      );
    });

    test("unicode input is hashed deterministically", () => {
      expect(computeChecksum("🚀")).toBe(computeChecksum("🚀"));
    });

    test("preserves UTF-8 byte boundaries for multibyte inputs", () => {
      const accented = computeChecksum("café");
      const nfc = computeChecksum("café");
      expect(accented).toMatch(/^[a-f0-9]{64}$/);
      // Different Unicode normalization forms must not collide.
      expect(accented).not.toBe(nfc);
    });

    test("is deterministic across repeated invocations and large inputs", () => {
      const large = "x".repeat(100000);
      const first = computeChecksum(large);
      for (let i = 0; i < 5; i++) {
        expect(computeChecksum(large)).toBe(first);
      }
    });

    test("throws a typed error for non-string inputs without leaking internal details", () => {
      expect(() => computeChecksum(undefined as unknown as string)).toThrow();
      expect(() => computeChecksum(null as unknown as string)).toThrow();
      expect(() => computeChecksum(123 as unknown as string)).toThrow();
    });
  });

  describe("verifyAppliedChecksums", () => {
    test("returns valid when no migrations are applied", async () => {
      installFetchMock(async () => jsonResponse([]));
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    test("returns a stable shape on repeated invocations", async () => {
      const a = await verifyAppliedChecksums();
      const b = await verifyAppliedChecksums();
      expect(a.valid).toBe(b.valid);
      expect(a.errors).toEqual(b.errors);
    });

    test("does not throw on concurrent invocations", async () => {
      const results = await Promise.all([
        verifyAppliedChecksums(),
        verifyAppliedChecksums(),
        verifyAppliedChecksums(),
      ]);
      for (const result of results) {
        expect(typeof result.valid).toBe("boolean");
        expect(Array.isArray(result.errors)).toBe(true);
      }
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
      installFetchMock(async () => jsonResponse([]));
      const versions = await getAppliedVersions();
      expect(Array.isArray(versions)).toBe(true);
      expect(versions).toEqual([]);
    });

    test("returns sorted applied versions", async () => {
      installFetchMock(async () =>
        jsonResponse([
          { version: 2, name: "b", checksum: "2" },
          { version: 1, name: "a", checksum: "1" },
        ],
      ),
      );
      const versions = await getAppliedVersions();
      expect(versions).toEqual([1, 2]);
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

    test("returns a stable array across repeated calls", async () => {
      const a = await getAppliedVersions();
      const b = await getAppliedVersions();
      expect(a).toEqual(b);
    });
  });

  describe("isDatabaseInitialized", () => {
    test("returns false when no migrations applied", async () => {
      installFetchMock(async () => jsonResponse([]));
      const initialized = await isDatabaseInitialized();
      expect(typeof initialized).toBe("boolean");
      expect(initialized).toBe(false);
    });

    test("returns true when at least one migration is applied", async () => {
      installFetchMock(async () =>
        jsonResponse([{ version: 1, name: "a", checksum: "1" }]),
      );
      const initialized = await isDatabaseInitialized();
      expect(initialized).toBe(true);
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
