// Deterministic failure-boundary tests for `getPreparedStatement`.
//
// The prepared-statement layer talks to `better-sqlite3`. To exercise its retry,
// eviction, permission and preparation error paths deterministically we mock the
// driver (not the module under test), so the real control flow in
// `src/lib/database.ts` runs on every case.
jest.mock("better-sqlite3", () => {
  const Database = jest.fn(() => ({
    pragma: jest.fn(),
    prepare: jest.fn(),
    close: jest.fn(),
  }));
  return { __esModule: true, default: Database };
});

import Database from "better-sqlite3";
import {
  DatabaseBusyError,
  DatabasePermissionError,
  DatabasePrepareError,
  clearStatementCache,
  getDatabase,
  getPreparedStatement,
  getStatementCacheStats,
} from "../database";

const MockDatabase = Database as unknown as jest.Mock;

let mockPrepare: jest.Mock;

function errorWithCode(message: string, code: string): Error & { code: string } {
  const err = new Error(message) as Error & { code: string };
  err.code = code;
  return err;
}

function readerStatement(getImpl: () => unknown = () => ({})) {
  return { reader: true, get: jest.fn(getImpl), run: jest.fn() };
}

function writerStatement() {
  return { reader: false, get: jest.fn(), run: jest.fn(() => ({})) };
}

beforeAll(() => {
  // Force `getDatabase()` to construct the mocked driver exactly once.
  getDatabase();
  const instance = MockDatabase.mock.results[0]?.value as { prepare: jest.Mock };
  mockPrepare = instance.prepare;
});

beforeEach(() => {
  clearStatementCache();
  mockPrepare.mockReset();
});

afterAll(() => {
  clearStatementCache();
});

describe("getPreparedStatement success and caching", () => {
  test("prepares a statement once and reuses it from the cache", () => {
    const stmt = readerStatement();
    mockPrepare.mockReturnValue(stmt);

    const first = getPreparedStatement("SELECT 1");
    const second = getPreparedStatement("SELECT 1");

    expect(first).toBe(stmt);
    expect(second).toBe(first);
    expect(mockPrepare).toHaveBeenCalledTimes(1);
  });

  test("validates writer statements through the run() probe", () => {
    const stmt = writerStatement();
    mockPrepare.mockReturnValue(stmt);

    expect(getPreparedStatement("INSERT INTO t VALUES (1)")).toBe(stmt);
    expect(stmt.run).toHaveBeenCalled();
  });
});

describe("getPreparedStatement failure boundaries", () => {
  test("retries on SQLITE_BUSY and eventually succeeds", () => {
    const stmt = readerStatement();
    mockPrepare
      .mockImplementationOnce(() => {
        throw errorWithCode("busy", "SQLITE_BUSY");
      })
      .mockImplementationOnce(() => {
        throw errorWithCode("busy", "SQLITE_BUSY");
      })
      .mockReturnValue(stmt);

    expect(getPreparedStatement("SELECT 1")).toBe(stmt);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test("throws DatabaseBusyError after max retries", () => {
    mockPrepare.mockImplementation(() => {
      throw errorWithCode("busy", "SQLITE_BUSY");
    });

    expect(() => getPreparedStatement("SELECT 1")).toThrow(DatabaseBusyError);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test("throws DatabasePermissionError on a read-only statement and does not cache it", () => {
    const stmt = readerStatement(() => {
      throw errorWithCode("readonly", "SQLITE_READONLY");
    });
    mockPrepare.mockReturnValue(stmt);

    expect(() => getPreparedStatement("SELECT 1")).toThrow(
      DatabasePermissionError,
    );
    // The failed statement must be retried (not served from cache).
    expect(() => getPreparedStatement("SELECT 1")).toThrow(
      DatabasePermissionError,
    );
    expect(mockPrepare).toHaveBeenCalledTimes(2);
    expect(getStatementCacheStats().size).toBe(0);
  });

  test("evicts a stale cached statement when validation raises SQLITE_SCHEMA", () => {
    const stale = readerStatement(() => {
      throw errorWithCode("schema changed", "SQLITE_SCHEMA");
    });
    const fresh = readerStatement();
    mockPrepare.mockReturnValueOnce(stale).mockReturnValueOnce(fresh);

    // The first preparation caches the statement; the stale-schema failure is
    // only observable when the cached entry is validated on the next lookup.
    expect(getPreparedStatement("SELECT 1")).toBe(stale);

    expect(getPreparedStatement("SELECT 1")).toBe(fresh);
    expect(getStatementCacheStats().evicts).toBe(1);
    expect(mockPrepare).toHaveBeenCalledTimes(2);
  });

  test("wraps other preparation errors in DatabasePrepareError", () => {
    mockPrepare.mockImplementation(() => {
      throw errorWithCode("syntax error", "SQLITE_ERROR");
    });

    expect(() => getPreparedStatement("BAD SQL")).toThrow(DatabasePrepareError);
  });
});
