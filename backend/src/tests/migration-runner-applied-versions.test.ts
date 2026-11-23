/**
 * Failure-boundary coverage for `getAppliedVersions` (lib/migrations/runner.ts).
 *
 * Scenarios covered:
 *  - Success: valid ledger reads with a deterministic output contract
 *    (ascending, de-duplicated, integer versions).
 *  - Boundary/duplicate inputs: version 0, MAX_SAFE_INTEGER, integer-valued
 *    strings/numbers, duplicate ledger rows.
 *  - Invalid/stale data: malformed rows and corrupt version cells reject with
 *    a typed error naming the row index, never a partial list.
 *  - Connection/permission states: handle acquisition failures (e.g. EACCES /
 *    SQLITE_CANTOPEN), missing handles, and ledger bootstrap failures.
 *  - Retry states: transient SQLITE_BUSY/LOCKED failures are flagged
 *    `retryable`, sanitized messages never leak raw payloads, and retries
 *    recover without inconsistent results.
 *  - Concurrency: parallel invocations yield identical results with idempotent
 *    bootstrap.
 *  - Regression: legacy `prepare`-only adapters, nullish row results, and
 *    `isDatabaseInitialized` caller compatibility.
 */

jest.mock("../lib/database", () => ({
  getDatabase: jest.fn(),
  closeDatabase: jest.fn(),
}));

import Database from "better-sqlite3";
import { getDatabase } from "../lib/database";
import {
  getAppliedVersions,
  isDatabaseInitialized,
  MigrationStateReadError,
} from "../lib/migrations/runner";
import type { DatabaseClient } from "../lib/migrations/runner";
import { MigrationErrorCodes } from "../lib/migrations/types";

const mockedGetDatabase = getDatabase as unknown as jest.Mock;

type FakeDbOptions = {
  /** Value returned by `prepare(...).all()`. Omitted/`undefined` means "no rows". */
  rows?: unknown;
  /** Thrown by `exec` (bootstrap / permission failures). */
  execError?: unknown;
  /** Thrown by `prepare` (schema / SQL failures). */
  prepareError?: unknown;
  /** Thrown by `all` (execution / lock failures). */
  allError?: unknown;
  /** Build an adapter without `exec` (legacy callers in this repo do this). */
  withoutExec?: boolean;
};

function createFakeDb(options: FakeDbOptions = {}) {
  const exec = jest.fn((_sql: string) => {
    if (options.execError !== undefined) throw options.execError;
  });
  const all = jest.fn((): unknown => {
    if (options.allError !== undefined) throw options.allError;
    return options.rows;
  });
  const prepare = jest.fn((_sql: string) => {
    if (options.prepareError !== undefined) throw options.prepareError;
    return { all, get: jest.fn(), run: jest.fn() };
  });
  const db = (
    options.withoutExec
      ? { prepare, transaction: jest.fn() }
      : { exec, prepare, transaction: jest.fn() }
  ) as unknown as DatabaseClient;
  return { db, exec, prepare, all };
}

/** Await a promise expected to reject; assert its error type and return it. */
async function captureError(promise: Promise<unknown>): Promise<MigrationStateReadError> {
  try {
    await promise;
  } catch (err) {
    expect(err).toBeInstanceOf(MigrationStateReadError);
    return err as MigrationStateReadError;
  }
  throw new Error("expected promise to reject with MigrationStateReadError");
}

beforeEach(() => {
  jest.clearAllMocks();
  mockedGetDatabase.mockReset();
});

describe("getAppliedVersions: valid input and output contract", () => {
  test("returns ascending unique versions regardless of adapter row order", async () => {
    const { db } = createFakeDb({ rows: [{ version: 3 }, { version: 1 }, { version: 2 }] });
    await expect(getAppliedVersions(db)).resolves.toEqual([1, 2, 3]);
  });

  test("returns [] for an empty ledger", async () => {
    const { db } = createFakeDb({ rows: [] });
    await expect(getAppliedVersions(db)).resolves.toEqual([]);
  });

  test("bootstraps the ledger table before querying (fresh-database determinism)", async () => {
    const { db, exec, prepare } = createFakeDb({ rows: [] });
    await getAppliedVersions(db);
    expect(exec).toHaveBeenCalledTimes(1);
    expect(String(exec.mock.calls[0][0])).toContain("CREATE TABLE IF NOT EXISTS _migrations");
    expect(exec.mock.invocationCallOrder[0]).toBeLessThan(prepare.mock.invocationCallOrder[0]);
  });

  test("does not consult the default connection when a db is injected", async () => {
    const { db } = createFakeDb({ rows: [] });
    await getAppliedVersions(db);
    expect(mockedGetDatabase).not.toHaveBeenCalled();
  });
});

describe("getAppliedVersions: boundary and duplicate inputs", () => {
  test("accepts version 0 (lower boundary, filename regex allows v000)", async () => {
    const { db } = createFakeDb({ rows: [{ version: 0 }] });
    await expect(getAppliedVersions(db)).resolves.toEqual([0]);
  });

  test("accepts Number.MAX_SAFE_INTEGER (upper boundary)", async () => {
    const { db } = createFakeDb({ rows: [{ version: Number.MAX_SAFE_INTEGER }] });
    await expect(getAppliedVersions(db)).resolves.toEqual([Number.MAX_SAFE_INTEGER]);
  });

  test("accepts integer-valued numeric strings from driver serialization", async () => {
    const { db } = createFakeDb({ rows: [{ version: "7" }, { version: "003" }] });
    await expect(getAppliedVersions(db)).resolves.toEqual([3, 7]);
  });

  test("accepts integer-valued numbers (5.0 === 5)", async () => {
    const { db } = createFakeDb({ rows: [{ version: 5.0 }] });
    await expect(getAppliedVersions(db)).resolves.toEqual([5]);
  });

  test("de-duplicates repeated ledger rows instead of returning dups", async () => {
    const { db } = createFakeDb({
      rows: [{ version: 2 }, { version: 2 }, { version: 1 }, { version: 1 }],
    });
    await expect(getAppliedVersions(db)).resolves.toEqual([1, 2]);
  });
});

describe("getAppliedVersions: invalid ledger data rejects deterministically", () => {
  const invalidCases: Array<[label: string, rows: unknown[]]> = [
    ["a null row", [null]],
    ["an array row", [[1]]],
    ["a row missing the version property", [{}]],
    ["a null version", [{ version: null }]],
    ["a fractional version", [{ version: 1.5 }]],
    ["a negative version", [{ version: -1 }]],
    ["an unsafe-integer version", [{ version: Number.MAX_SAFE_INTEGER + 2 }]],
    ["a non-numeric string version", [{ version: "abc" }]],
    ["an empty-string version", [{ version: "" }]],
    ["a whitespace-only version", [{ version: "   " }]],
    ["a boolean version", [{ version: true }]],
    ["an object version", [{ version: { nested: true } }]],
  ];

  for (const [label, rows] of invalidCases) {
    test(`rejects ${label} with a typed error naming the row index`, async () => {
      const { db } = createFakeDb({ rows });
      const err = await captureError(getAppliedVersions(db));
      expect(err.name).toBe("MigrationStateReadError");
      expect(err.code).toBe(MigrationErrorCodes.MIGRATION_STATE_READ_FAILED);
      expect(err.message).toContain("index 0");
      expect(err.retryable).toBe(false);
    });
  }

  test("a stale/corrupt row at a later index still rejects the whole read (no partial list)", async () => {
    const { db } = createFakeDb({ rows: [{ version: 1 }, { version: "stale" }] });
    const err = await captureError(getAppliedVersions(db));
    expect(err.message).toContain("index 1");
  });

  test("structured row payloads are never echoed into the error message", async () => {
    const secret = "sk_live_must_not_leak";
    const { db } = createFakeDb({ rows: [{ version: { token: secret } }] });
    const err = await captureError(getAppliedVersions(db));
    expect(err.message).not.toContain(secret);
    expect(err.message).toContain("index 0");
  });

  test("a non-array row set rejects instead of crashing on .map", async () => {
    const { db } = createFakeDb({ rows: { version: 1 } });
    const err = await captureError(getAppliedVersions(db));
    expect(err.message).toContain("expected an array of rows");
  });
});

describe("getAppliedVersions: connection, permission, and driver failure states", () => {
  test("rejects with a typed error when the connection cannot be acquired", async () => {
    const cantOpen = Object.assign(new Error("unable to open database file"), {
      code: "SQLITE_CANTOPEN",
    });
    mockedGetDatabase.mockImplementationOnce(() => {
      throw cantOpen;
    });
    const err = await captureError(getAppliedVersions());
    expect(err.message).toContain("unable to open database file");
    expect(err.cause).toBe(cantOpen);
    expect(err.retryable).toBe(false);
  });

  test("tolerates non-Error throws from the connection layer", async () => {
    mockedGetDatabase.mockImplementationOnce(() => {
      throw "boom";
    });
    const err = await captureError(getAppliedVersions());
    expect(err.message).toContain("boom");
    expect(err.retryable).toBe(false);
  });

  test("rejects deterministically when the connection layer returns no handle", async () => {
    mockedGetDatabase.mockReturnValue(undefined);
    const err = await captureError(getAppliedVersions());
    expect(err.message).toContain("no database handle available");
    expect(err.retryable).toBe(false);
  });

  test("permission failure during ledger bootstrap rejects (never a silent empty list)", async () => {
    const { db } = createFakeDb({
      rows: [],
      execError: new Error("attempt to write a readonly database"),
    });
    const err = await captureError(getAppliedVersions(db));
    expect(err.message).toContain("Failed to ensure _migrations ledger exists");
    expect(err.message).toContain("attempt to write a readonly database");
    expect(err.retryable).toBe(false);
  });

  test("statement preparation failures reject with the cause preserved", async () => {
    const prepareFailure = new Error("no such table: _migrations");
    const { db } = createFakeDb({ prepareError: prepareFailure });
    const err = await captureError(getAppliedVersions(db));
    expect(err.message).toContain("Failed to read applied migration ledger");
    expect(err.cause).toBe(prepareFailure);
  });

  test("transient SQLITE_BUSY failures are flagged retryable", async () => {
    const busy = Object.assign(new Error("database is locked"), {
      code: "SQLITE_BUSY",
      errcode: 5,
    });
    const { db } = createFakeDb({ rows: [], allError: busy });
    const err = await captureError(getAppliedVersions(db));
    expect(err.retryable).toBe(true);
    expect(err.code).toBe(MigrationErrorCodes.MIGRATION_STATE_READ_FAILED);
  });

  test("extended SQLite busy errcodes (0x105 busy-snapshot) are flagged retryable", async () => {
    const busySnapshot = Object.assign(new Error("sqlite busy snapshot"), { errcode: 0x105 });
    const { db } = createFakeDb({ rows: [], allError: busySnapshot });
    const err = await captureError(getAppliedVersions(db));
    expect(err.retryable).toBe(true);
  });

  test("message-only lock detection works for adapters without a code field", async () => {
    const { db } = createFakeDb({ rows: [], allError: { message: "database is locked" } });
    const err = await captureError(getAppliedVersions(db));
    expect(err.message).toContain("database is locked");
    expect(err.retryable).toBe(true);
  });

  test("driver messages are sanitized: single-line and length-capped, cause untouched", async () => {
    const noisy = new Error(`SQLITE_CORRUPT: malformed header\nstack line\n${"x".repeat(500)}`);
    const { db } = createFakeDb({ rows: [], allError: noisy });
    const err = await captureError(getAppliedVersions(db));
    expect(err.message).not.toContain("\n");
    expect(err.message.length).toBeLessThan(300);
    expect(err.cause).toBe(noisy);
  });
});

describe("getAppliedVersions: retry and concurrency safety", () => {
  test("repeated calls are idempotent (same result, no state change)", async () => {
    const { db } = createFakeDb({ rows: [{ version: 1 }, { version: 2 }] });
    const first = await getAppliedVersions(db);
    const second = await getAppliedVersions(db);
    expect(second).toEqual(first);
    expect(first).toEqual([1, 2]);
  });

  test("concurrent invocations all resolve to identical results", async () => {
    const { db, exec } = createFakeDb({ rows: [{ version: 2 }, { version: 1 }] });
    const results = await Promise.all(
      Array.from({ length: 5 }, () => getAppliedVersions(db))
    );
    expect(results).toEqual([
      [1, 2],
      [1, 2],
      [1, 2],
      [1, 2],
      [1, 2],
    ]);
    // Bootstrap runs per call but is idempotent DDL (CREATE TABLE IF NOT EXISTS).
    expect(exec).toHaveBeenCalledTimes(5);
    expect(new Set(exec.mock.calls.map((c) => String(c[0]))).size).toBe(1);
  });

  test("a transient failure rejects, and a retry after the lock clears succeeds", async () => {
    const options: FakeDbOptions = {
      rows: [{ version: 1 }],
      allError: Object.assign(new Error("database is locked"), { code: "SQLITE_BUSY" }),
    };
    const { db } = createFakeDb(options);
    const err = await captureError(getAppliedVersions(db));
    expect(err.retryable).toBe(true);
    // Lock released: the same adapter now serves the read with no state drift.
    options.allError = undefined;
    await expect(getAppliedVersions(db)).resolves.toEqual([1]);
  });
});

describe("getAppliedVersions: regression and caller compatibility", () => {
  test("legacy prepare-only adapters keep working (existing callers)", async () => {
    const { db } = createFakeDb({ rows: [{ version: 1 }, { version: 2 }], withoutExec: true });
    await expect(getAppliedVersions(db)).resolves.toEqual([1, 2]);
  });

  test("legacy nullish all() results are treated as an empty ledger", async () => {
    const nullRows = createFakeDb({ rows: null });
    await expect(getAppliedVersions(nullRows.db)).resolves.toEqual([]);
    const undefinedRows = createFakeDb();
    await expect(getAppliedVersions(undefinedRows.db)).resolves.toEqual([]);
  });

  test("fresh database: bootstrap prevents 'no such table' and yields []", async () => {
    // Stateful adapter modeling better-sqlite3 on an uninitialized database.
    let tableExists = false;
    const db = {
      exec: jest.fn((_sql: string) => {
        tableExists = true;
      }),
      prepare: jest.fn((_sql: string) => ({
        all: jest.fn((): unknown => {
          if (!tableExists) throw new Error("no such table: _migrations");
          return [];
        }),
      })),
      transaction: jest.fn(),
    } as unknown as DatabaseClient;

    await expect(getAppliedVersions(db)).resolves.toEqual([]);
    expect(tableExists).toBe(true);
  });

  test("isDatabaseInitialized stays true when versions exist", async () => {
    const { db } = createFakeDb({ rows: [{ version: 1 }] });
    await expect(isDatabaseInitialized(db)).resolves.toBe(true);
  });

  test("isDatabaseInitialized stays false on an empty ledger", async () => {
    const { db } = createFakeDb({ rows: [] });
    await expect(isDatabaseInitialized(db)).resolves.toBe(false);
  });

  test("real in-memory SQLite: fresh database reads [] and applied rows are returned", async () => {
    const DatabaseCtor = Database as any;
    const sqlite = new DatabaseCtor(":memory:");
    try {
      const db = {
        exec: (sql: string): void => {
          sqlite.exec(sql);
        },
        prepare: (sql: string) => sqlite.prepare(sql),
        transaction: (fn: () => void): void => {
          fn();
        },
      } as unknown as DatabaseClient;

      // Fresh (uninitialized) database: deterministic empty ledger, not a throw.
      await expect(getAppliedVersions(db)).resolves.toEqual([]);

      const insert = (version: number, name: string) =>
        sqlite
          .prepare(
            "INSERT INTO _migrations (version, name, checksum, applied_at, duration_ms, author, meta) VALUES (?, ?, ?, ?, ?, ?, ?)"
          )
          .run(
            version,
            name,
            `checksum-${version}`,
            new Date().toISOString(),
            5,
            "test-author",
            "{}"
          );

      // Insert out of order: read order must still be deterministic.
      insert(2, "second");
      insert(1, "first");
      await expect(getAppliedVersions(db)).resolves.toEqual([1, 2]);

      // Database-level duplicate guard holds; the failed write cannot corrupt reads.
      expect(() => insert(1, "duplicate")).toThrow();
      await expect(getAppliedVersions(db)).resolves.toEqual([1, 2]);
    } finally {
      sqlite.close();
    }
  });
});
