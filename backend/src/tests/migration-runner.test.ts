import { parseMigrationFilename, computeChecksum } from "../lib/migrations/runner";

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
});
