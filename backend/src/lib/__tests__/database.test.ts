// Tests for deterministic database connection and getPreparedStatement implementation
import fs from 'fs';
import os from 'os';
import path from 'path';
import { vi } from 'vitest';
import { config } from '../../config';
import {
  getPreparedStatement,
  getDatabase,
  closeDatabase,
  clearStatementCache,
  getStatementCacheStats,
  getDatabaseStatus,
  DatabaseError,
  DatabasePermissionError,
  DatabaseBusyError,
  DatabasePrepareError,
  _resetDatabaseState,
} from '../database';

const ORIGINAL_DATABASE_PATH = config.DATABASE_PATH;

afterEach(() => jest.restoreAllMocks());

// Helper to reset environment and cache between tests
beforeEach(() => {
  vi.restoreAllMocks();
  _resetDatabaseState();
  clearStatementCache();
  (config as { DATABASE_PATH: string }).DATABASE_PATH = ':memory:';
});

afterAll(() => {
  _resetDatabaseState();
  (config as { DATABASE_PATH: string }).DATABASE_PATH = ORIGINAL_DATABASE_PATH;
});

describe('getDatabase lifecycle and failure boundaries (real SQLite)', () => {
  test('opens a temp file DB with WAL pragma and transitions status to open', () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'qlx-db-test-'));
    const tempDbPath = path.join(tmpDir, 'test.db');
    (config as { DATABASE_PATH: string }).DATABASE_PATH = tempDbPath;

    try {
      const db = getDatabase();
      expect(db).toBeDefined();

      const status = getDatabaseStatus();
      expect(status.state).toBe('open');
      expect(status.consecutiveFailures).toBe(0);
      expect(status.lastErrorCode).toBeNull();
      expect(status.lastOpenedAt).not.toBeNull();

      const journalMode = db.pragma('journal_mode', { simple: true });
      expect(String(journalMode).toLowerCase()).toBe('wal');
    } finally {
      _resetDatabaseState();
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  test('throws OPEN_FAILED without leaking path when directory does not exist and increments consecutiveFailures', () => {
    const nonExistentPath = path.join(
      os.tmpdir(),
      `qlx-nonexistent-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      'subdir',
      'missing.db',
    );
    (config as { DATABASE_PATH: string }).DATABASE_PATH = nonExistentPath;

    let firstError: unknown;
    try {
      getDatabase();
    } catch (err) {
      firstError = err;
    }

    expect(firstError).toBeInstanceOf(DatabaseError);
    expect((firstError as DatabaseError).code).toBe('OPEN_FAILED');
    expect((firstError as DatabaseError).message).not.toContain(nonExistentPath);
    expect(getDatabaseStatus().state).toBe('error');
    expect(getDatabaseStatus().consecutiveFailures).toBe(1);
    expect(getDatabaseStatus().lastErrorCode).toBe('OPEN_FAILED');

    let secondError: unknown;
    try {
      getDatabase();
    } catch (err) {
      secondError = err;
    }

    expect(secondError).toBeInstanceOf(DatabaseError);
    expect((secondError as DatabaseError).code).toBe('OPEN_FAILED');
    expect(getDatabaseStatus().state).toBe('error');
    expect(getDatabaseStatus().consecutiveFailures).toBe(2);
  });

  test('closes opened handle and leaves dbInstance null with PRAGMA_FAILED when applyPragmas throws', () => {
    let closedHandle = false;
    let pragmaCalls = 0;

    // Open a real connection first to get the prototype, then spy on pragma / close
    const tempHandle = getDatabase();
    const proto = Object.getPrototypeOf(tempHandle);
    _resetDatabaseState();

    const closeSpy = vi.spyOn(proto, 'close').mockImplementation(function (this: any) {
      closedHandle = true;
      return this;
    });
    const pragmaSpy = vi.spyOn(proto, 'pragma').mockImplementation(() => {
      pragmaCalls++;
      throw new Error('simulated pragma failure');
    });

    try {
      let caught: unknown;
      try {
        getDatabase();
      } catch (err) {
        caught = err;
      }

      expect(pragmaCalls).toBeGreaterThan(0);
      expect(caught).toBeInstanceOf(DatabaseError);
      expect((caught as DatabaseError).code).toBe('PRAGMA_FAILED');
      expect(closedHandle).toBe(true);
      expect(getDatabaseStatus().state).toBe('error');
      expect(getDatabaseStatus().lastErrorCode).toBe('PRAGMA_FAILED');
      expect(getDatabaseStatus().consecutiveFailures).toBe(1);

      // Restore spies and verify dbInstance remained null (next call opens fresh)
      pragmaSpy.mockRestore();
      closeSpy.mockRestore();

      const freshDb = getDatabase();
      expect(freshDb).toBeDefined();
      expect(getDatabaseStatus().state).toBe('open');
    } finally {
      pragmaSpy.mockRestore();
      closeSpy.mockRestore();
    }
  });

  test('closeDatabase transitions state to closed and next getDatabase reopens', () => {
    const db1 = getDatabase();
    expect(db1).toBeDefined();
    expect(getDatabaseStatus().state).toBe('open');

    closeDatabase();
    expect(getDatabaseStatus().state).toBe('closed');
    expect(getDatabaseStatus().lastClosedAt).not.toBeNull();

    const db2 = getDatabase();
    expect(db2).toBeDefined();
    expect(db2).not.toBe(db1);
    expect(getDatabaseStatus().state).toBe('open');
  });
});

describe('getPreparedStatement success and caching', () => {
  test('prepares a statement and caches it', () => {
    // Use a real in‑memory SQLite DB for this simple case
    (config as { DATABASE_PATH: string }).DATABASE_PATH = ':memory:';
    const stmt1 = getPreparedStatement('SELECT 1');
    expect(stmt1).toBeDefined();
    const stmt2 = getPreparedStatement('SELECT 1');
    // Cached instance should be returned (reference equality)
    expect(stmt2).toBe(stmt1);
  });
});

describe('failure boundaries', () => {
  test('retries on SQLITE_BUSY and eventually succeeds', () => {
    const db = getDatabase();
    const mockPrepare = vi
      .spyOn(db, 'prepare')
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
      .mockImplementation(
        () =>
          ({
            reader: true,
            get: () => ({}) as any,
            run: () => ({}) as any,
          }) as any,
      );

    const stmt = getPreparedStatement('SELECT 1');
    expect(stmt).toBeDefined();
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test('throws DatabaseBusyError after max retries', () => {
    const db = getDatabase();
    const mockPrepare = vi.spyOn(db, 'prepare').mockImplementation(() => {
      const err: any = new Error('busy');
      err.code = 'SQLITE_BUSY';
      throw err;
    });

    expect(() => getPreparedStatement('SELECT 1')).toThrow(DatabaseBusyError);
    expect(mockPrepare).toHaveBeenCalledTimes(3);
  });

  test('throws DatabasePermissionError on read‑only error', () => {
    const db = getDatabase();
    const stmtMock = {
      reader: true,
      get: () => {
        const err: any = new Error('readonly');
        err.code = 'SQLITE_READONLY';
        throw err;
      },
      run: () => ({}),
    };
    const mockPrepare = vi.spyOn(db, 'prepare').mockImplementation(() => stmtMock as any);

    expect(() => getPreparedStatement('SELECT 1')).toThrow(DatabasePermissionError);
    // Statement should not be cached after permission error
    expect(() => getPreparedStatement('SELECT 1')).toThrow(DatabasePermissionError);
    expect(mockPrepare).toHaveBeenCalledTimes(2);
  });

  test('wraps other errors in DatabasePrepareError', () => {
    const db = getDatabase();
    vi.spyOn(db, 'prepare').mockImplementation(() => {
      const err: any = new Error('syntax error');
      err.code = 'SQLITE_ERROR';
      throw err;
    });

    expect(() => getPreparedStatement('BAD SQL')).toThrow(DatabasePrepareError);
  });

  describe('getStatementCacheStats', () => {
    test('returns correct stats when cache is empty', () => {
      const stats = getStatementCacheStats();
      expect(stats.size).toBe(0);
      expect(stats.statements).toEqual([]);
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.evicts).toBe(0);
    });

    test('counts hits and misses and resets on closeDatabase', () => {
      getPreparedStatement('SELECT 1');
      getPreparedStatement('SELECT 1');
      expect(getStatementCacheStats()).toEqual({
        size: 1,
        statements: ['SELECT 1'],
        hits: 1,
        misses: 1,
        evicts: 0,
      });

      closeDatabase();
      expect(getStatementCacheStats()).toEqual({
        size: 0,
        statements: [],
        hits: 0,
        misses: 0,
        evicts: 0,
      });
    });
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
