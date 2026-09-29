// Tests for deterministic getPreparedStatement implementation
import { getPreparedStatement, getDatabase, clearStatementCache, getStatementCacheStats, DatabasePermissionError, DatabaseBusyError, DatabasePrepareError } from '../database';

// Helper to reset environment and cache between tests
beforeEach(() => {
  clearStatementCache();
});

describe('getPreparedStatement success and caching', () => {
  test('prepares a statement and caches it', () => {
    // Use a real in‑memory SQLite DB for this simple case
    process.env.DATABASE_PATH = ':memory:';
    const stmt1 = getPreparedStatement('SELECT 1');
    expect(stmt1).toBeDefined();
    const stmt2 = getPreparedStatement('SELECT 1');
    // Cached instance should be returned (reference equality)
    expect(stmt2).toBe(stmt1);
  });
});

describe('failure boundaries', () => {
  test('retries on SQLITE_BUSY and eventually succeeds', () => {
    const mockPrepare = jest.fn()
      .mockImplementationOnce(() => {
        const err: any = new Error('busy');
        err.code = 'SQLITE_BUSY';
        throw err;
      })
      .mockImplementationOnce(() => {
        const err: any = new Error('busy');
        err.code = 'SQLITE_BUSY';
        throw err;
      })
      .mockImplementation(() => ({
        reader: true,
        get: () => ({}) as any,
        run: () => ({}) as any,
      }));

    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    const stmt = getPreparedStatement('SELECT 1');
    expect(stmt).toBeDefined();
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test('throws DatabaseBusyError after max retries', () => {
    const mockPrepare = jest.fn(() => {
      const err: any = new Error('busy');
      err.code = 'SQLITE_BUSY';
      throw err;
    });
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    expect(() => getPreparedStatement('SELECT 1')).toThrow(DatabaseBusyError);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test('throws DatabasePermissionError on read‑only error', () => {
    const stmtMock = {
      reader: true,
      get: () => {
        const err: any = new Error('readonly');
        err.code = 'SQLITE_READONLY';
        throw err;
      },
      run: () => ({}),
    };
    const mockPrepare = jest.fn(() => stmtMock);
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    expect(() => getPreparedStatement('SELECT 1')).toThrow(DatabasePermissionError);
    // Statement should not be cached after permission error
    expect(() => getPreparedStatement('SELECT 1')).toThrow(DatabasePermissionError);
    expect(mockPrepare).toHaveBeenCalledTimes(2);
  });

  test('wraps other errors in DatabasePrepareError', () => {
    const mockPrepare = jest.fn(() => {
      const err: any = new Error('syntax error');
      err.code = 'SQLITE_ERROR';
      throw err;
    });
    jest.spyOn(require('../database'), 'getDatabase').mockImplementation(() => ({
      prepare: mockPrepare,
    } as any));

    expect(() => getPreparedStatement('BAD SQL')).toThrow(DatabasePrepareError);
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
  });
});
