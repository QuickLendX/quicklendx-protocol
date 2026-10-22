// Tests for deterministic getPreparedStatement implementation
import { getPreparedStatement, getDatabase, clearStatementCache, DatabasePermissionError, DatabaseBusyError, DatabasePrepareError } from '../database';

afterEach(() => jest.restoreAllMocks());

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

    jest.spyOn(getDatabase(), 'prepare').mockImplementation(mockPrepare);

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
    jest.spyOn(getDatabase(), 'prepare').mockImplementation(mockPrepare);

    expect(() => getPreparedStatement('SELECT 1')).toThrow(DatabaseBusyError);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test('throws DatabasePermissionError on read‑only error', () => {
    const mockPrepare = jest.fn(() => {
      const err: any = new Error('readonly');
      err.code = 'SQLITE_READONLY';
      throw err;
    });
    jest.spyOn(getDatabase(), 'prepare').mockImplementation(mockPrepare);

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
    jest.spyOn(getDatabase(), 'prepare').mockImplementation(mockPrepare);

    expect(() => getPreparedStatement('BAD SQL')).toThrow(DatabasePrepareError);
  });
});

// Preparing or retrieving a cached write must not execute it without parameters.
test('cached preparation never executes writes', () => {
  const db = getDatabase();
  db.exec('CREATE TABLE IF NOT EXISTS prepare_safety (value TEXT)');
  const sql = 'INSERT INTO prepare_safety (value) VALUES (?)';
  getPreparedStatement(sql).run('once');
  getPreparedStatement(sql);
  expect(db.prepare('SELECT value FROM prepare_safety').all()).toEqual([{ value: 'once' }]);
});
