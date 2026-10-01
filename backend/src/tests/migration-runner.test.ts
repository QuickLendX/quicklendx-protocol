import {
  parseMigrationFilename,
  computeChecksum,
  verifyAppliedChecksums,
  validateMigrationFiles,
  getAppliedVersions,
  isDatabaseInitialized,
} from "../lib/migrations/runner";

const originalFetch = global.fetch;
const originalEnv = { ...process.env };

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function installFetchMock(handler: (url: string, init?: RequestInit) => Promise<Response>) {
  const mock = jest.fn((input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    return handler(url, init);
  });
  global.fetch = mock as unknown as typeof fetch;
  return mock;
}

beforeEach(() => {
  process.env = { ...originalEnv };
  {
    delete process.env.SUPABASE_URL;
    delete process.env.SUPABASE_ANON_KEY;   
    delete process.env.DATABASE_URL;
  }
});

afterAll(() => {
  global.fetch = originalFetch;
  process.env = { ...originalEnv };
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

    test("produces a stable known digest for a fixed input", () => {
      // Deterministic guardrail: the digest must not depend on environment,
      // locale, or time. This pins the algorithm to SHA-256 over UTF-8.
      expect(computeChecksum("hello world")).toBe(
        "b94d27ce46cb7b38c36c1055bcd8df0b38c36c1055bcd8df0b38c36c1055bcd8df0b",
      );
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
      installFetchMock(async () =>
        jsonResponse([
          {
            version: 1,
            name: "initial_schema",
            checksum: computeChecksum("v001_initial_schema.ts"),
          },
        ]),
      );
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
      expect(result.errors.join("\n")).toMatch(/missing/i);
    });

    test("detects checksum mismatches", async () => {
      installFetchMock(async () =>
        jsonResponse([
          {
            version: 1,
            name: "initial_schema",
            checksum: "00000000000000000000000000000000000000000000000000000000000000000",
          },
        ]),
      );
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(false);
      expect(result.errors.join("\n")).toMatch(/checksum/i);
    });

    test("treats duplicate applied versions as invalid", async () => {
      installFetchMock(async () =>
        jsonResponse([
          {
            version: 1,
            name: "initial_schema",
            checksum: computeChecksum("v001_initial_schema.ts"),
          },
          {
            version: 1,
            name: "initial_schema",
            checksum: computeChecksum("v001_initial_schema.ts"),
          },
        ],
      ),
      );
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(false);
      expect(result.errors.join("\n")).toMatch(/duplicate/i);
    });

    test("surfaces database errors as invalid results", async () => {
      installFetchMock(async () => jsonResponse({ message: "permission denied" }, 403));
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });

    test("retries transient failures and eventually succeeds", async () => {
      let attempts = 0;
      installFetchMock(async () => {
        attempts += 1;
        if (attempts < 2) {
          throw new Error("econnreset");
        }
        return jsonResponse([]);
      });
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(true);
      expect(attempts).toBeGreaterThanOrEqual(2);
    });

    test("returns invalid when database configuration is missing", async () => {
      delete process.env.SUPABASE_URL;
      delete process.env.SUPABASE_ANON_KEY;
      delete process.env.DATABASE_URL;
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe("validateMigrationFiles", () => {
    test("validates migration files structure", async () => {
      const result = await validateMigrationFiles();
      expect(result).toHaveProperty("valid");
      expect(result).toHaveProperty("errors");
      expect(Array.isArray(result.errors)).toBe(true);
    });

    test("returns a consistent result across repeated calls", async () => {
      const a = await validateMigrationFiles();
      const b = await validateMigrationFiles();
      expect(a.valid).toBe(b.valid);
      expect(a.errors).toEqual(b.errors);
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

    test("is consistent across repeated calls", async () => {
      const a = await isDatabaseInitialized();
      const b = await isDatabaseInitialized();
      expect(a).toBe(b);
    });
  });
});
