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
  });

  describe("verifyAppliedChecksums", () => {
    test("returns valid when no migrations are applied", async () => {
      installFetchMock(async () => jsonResponse([]));
      const result = await verifyAppliedChecksums();
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
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
  });
});
