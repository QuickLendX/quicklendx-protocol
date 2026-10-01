// Tests for deterministic getPreparedStatement implementation
import {
  getPreparedStatement,
  getDatabase,
  clearStatementCache,
  DatabasePermissionError,
  DatabaseBusyError,
  DatabasePrepareError,
  _setGetDatabaseForTesting,
} from '../database';

// Helper to reset environment and cache between tests
beforeEach(() => {
  clearStatementCache();
  _setGetDatabaseForTesting(null);
});

afterEach(() => {
  _setGetDatabaseForTesting(null);
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

    _setGetDatabaseForTesting(() => ({
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
    _setGetDatabaseForTesting(() => ({
      prepare: mockPrepare,
    } as any));

    expect(() => getPreparedStatement('SELECT 1')).toThrow(DatabaseBusyError);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test('throws DatabasePermissionError on read‑only error', () => {
    const mockPrepare = jest.fn(() => {
      const err: any = new Error('readonly');
      err.code = 'SQLITE_READONLY';
      throw err;
    });
    _setGetDatabaseForTesting(() => ({
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
    _setGetDatabaseForTesting(() => ({
      prepare: mockPrepare,
    } as any));

    expect(() => getPreparedStatement('BAD SQL')).toThrow(DatabasePrepareError);
  });
});
