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

  describe('getStatementCacheStats', () => {
    test('returns correct stats when cache is empty', () => {
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(0);
      expect(stats.statements).toEqual([]);
      expect(stats.statements).toHaveLength(0);
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.evicts).toBe(0);
    });

    test('returns correct stats after preparing statements', () => {
      process.env.DATABASE_PATH = ':memory:';
      const stmt1 = getPreparedStatement('SELECT 1');
      const stmt2 = getPreparedStatement('SELECT 2');
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(2);
      expect(stats.statements).toContain('SELECT 1');
      expect(stats.statements).toContain('SELECT 2');
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(2);
      expect(stats.evicts).toBe(0);
    });

    test('returns consistent stats across multiple calls', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      const first = getStatementCacheStats();
      const second = getStatementCacheStats();
      expect(first).toEqual(second);
    });

    test('returns correct stats after clearing cache', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      clearStatementCache();
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(0);
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.evicts).toBe(0);
    });

    test('statements array reflects current cache keys', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 2');
      const stats = getStatementCacheStats();
      expect(stats.statements).toContain('SELECT 1');
      expect(stats.statements).toContain('SELECT 2');
      expect(stats.statements).toHaveLength(2);
    });

    test('counts a cache hit without changing size', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 1');
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(1);
      expect(stats.hits).toBe(2);
      expect(stats.misses).toBe(1);
      expect(stats.evicts).toBe(0);
    });

    test('a duplicate SQL string never inflates size or misses', () => {
      process.env.DATABASE_PATH = ':memory:';
      for (let i = 0; i < 5; i++) getPreparedStatement('SELECT 1');
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(1);
      expect(stats.misses).toBe(1);
      expect(stats.hits).toBe(4);
    });

    test('a failed preparation is counted as a miss but never cached', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      expect(() => getPreparedStatement('BAD SQL')).toThrow(DatabasePrepareError);
      const stats = getStatementCacheStats();
      // The rejected statement must not be observable in the cache.
      expect(stats.size).toBe(1);
      expect(stats.statements).toEqual(['SELECT 1']);
      expect(stats.misses).toBe(2);
    });

    test('returns a defensive copy that cannot corrupt the cache', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      const first = getStatementCacheStats();
      first.statements.push('TAMPERED');
      first.hits = 999;
      const second = getStatementCacheStats();
      expect(second.statements).toEqual(['SELECT 1']);
      expect(second.hits).toBe(0);
      expect(second.size).toBe(1);
    });

    test('reports insertion order for cached statements', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 3');
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 2');
      expect(getStatementCacheStats().statements).toEqual(['SELECT 3', 'SELECT 1', 'SELECT 2']);
    });

    test('closeDatabase clears the cache and its metrics together', () => {
      process.env.DATABASE_PATH = ':memory:';
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 1');
      expect(getStatementCacheStats().hits).toBe(1);

      closeDatabase();

      // Regression guard: closing used to leave the previous generation's
      // counters behind, reporting size 0 alongside non-zero hits/misses.
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(0);
      expect(stats.statements).toEqual([]);
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.evicts).toBe(0);
    });

    test('stats stay self-consistent across many prepares and clears', () => {
      process.env.DATABASE_PATH = ':memory:';
      for (let round = 0; round < 3; round++) {
        getPreparedStatement('SELECT 1');
        getPreparedStatement('SELECT 2');
        const stats = getStatementCacheStats();
        expect(stats.size).toBe(2);
        expect(stats.misses).toBe(2);
        expect(stats.hits).toBe(0);
        clearStatementCache();
        expect(getStatementCacheStats()).toEqual({
          size: 0,
          statements: [],
          hits: 0,
          misses: 0,
          evicts: 0,
        });
      }
    });
  });
});
