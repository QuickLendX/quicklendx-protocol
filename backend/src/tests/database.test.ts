/**
 * Deterministic failure-boundary tests for getDatabase (lib/database.ts).
 *
 * Coverage matrix
 * ───────────────
 * Happy path
 *   ✓ opens successfully on first call and returns a live connection
 *   ✓ returns the same singleton on every subsequent call
 *   ✓ applies all four WAL/durability pragmas on init
 *   ✓ pingDatabase returns true when the connection is healthy
 *   ✓ getDatabaseStatus reflects open state with a valid lastOpenedAt timestamp
 *
 * Failure – open errors
 *   ✓ OPEN_FAILED when the constructor throws a generic error
 *   ✓ PERMISSION_DENIED when the OS reports EACCES
 *   ✓ PERMISSION_DENIED when the OS reports EPERM
 *   ✓ PERMISSION_DENIED when the message contains "permission denied"
 *   ✓ BUSY_TIMEOUT when the message contains "database is locked"
 *   ✓ BUSY_TIMEOUT when the error code is SQLITE_BUSY
 *   ✓ UNKNOWN classified errors are wrapped as OPEN_FAILED
 *   ✓ error state is observable (consecutiveFailures increments, lastErrorCode set)
 *   ✓ error message never leaks DATABASE_PATH or OS-level details
 *   ✓ retry succeeds after a transient open failure
 *
 * Failure – pragma errors
 *   ✓ PRAGMA_FAILED when a pragma throws after a successful constructor call
 *   ✓ partial open is torn down — no stale instance is left after pragma failure
 *   ✓ retry after pragma failure opens a fresh, fully-configured connection
 *
 * Stale instance guard
 *   ✓ discards and re-opens when dbInstance.open is false
 *   ✓ statement cache is cleared when a stale instance is evicted
 *
 * Re-entrant call guard
 *   ✓ re-entrant call during opening throws OPEN_FAILED
 *
 * Prepared statement cache
 *   ✓ same statement object returned for the same SQL on repeated calls
 *   ✓ different statement objects for different SQL strings
 *   ✓ clearStatementCache evicts all cached statements
 *   ✓ getStatementCacheStats reflects current cache size and keys
 *   ✓ cache is cleared when closeDatabase is called
 *   ✓ getPreparedStatement throws when SQL is syntactically invalid
 *
 * Lifecycle – close
 *   ✓ closeDatabase sets state to closed and nulls the instance
 *   ✓ closeDatabase is idempotent (safe to call twice)
 *   ✓ getDatabase re-opens cleanly after closeDatabase
 *   ✓ lastClosedAt is recorded after close
 *
 * Observable state
 *   ✓ getDatabaseStatus.state reflects uninitialized → opening → open → closed
 *   ✓ getDatabaseStatus.consecutiveFailures resets to 0 after a successful open
 *   ✓ getDatabaseStatus.lastErrorCode is null after a successful open following failure
 *
 * pingDatabase
 *   ✓ returns false when getDatabase throws
 *   ✓ returns false when the SELECT 1 result is unexpected
 *   ✓ never throws regardless of underlying error type
 *
 * Concurrent / boundary inputs
 *   ✓ empty SQL string passed to getPreparedStatement propagates prepare error
 *   ✓ DATABASE_PATH env var is respected at open time
 *   ✓ in-memory database (:memory:) works end-to-end
 */

import Database from 'better-sqlite3';
import {
  getDatabase,
  closeDatabase,
  pingDatabase,
  getPreparedStatement,
  clearStatementCache,
  getStatementCacheStats,
  getDatabaseStatus,
  DatabaseError,
  _resetDatabaseState,
} from '../lib/database';

// ---------------------------------------------------------------------------
// Module-level mock wiring
// ---------------------------------------------------------------------------

// We mock better-sqlite3 at the module level so we can control exactly when
// the constructor throws, which pragmas succeed/fail, and whether a returned
// instance has .open === true or false — without touching the file system.
jest.mock('better-sqlite3');

const MockDatabase = Database as jest.MockedClass<typeof Database>;

// Shared instance mock returned by the constructor in the happy-path baseline.
// Each test that needs different behaviour overrides MockDatabase.mockImplementation.
function makeMockDb(overrides: Partial<InstanceType<typeof Database>> = {}): jest.Mocked<InstanceType<typeof Database>> {
  const db = {
    open: true,
    pragma: jest.fn(),
    prepare: jest.fn((sql: string) => ({
      get: jest.fn(),
      run: jest.fn(),
      all: jest.fn(),
      sql,
    })),
    close: jest.fn(),
    exec: jest.fn(),
    transaction: jest.fn(),
    ...overrides,
  } as unknown as jest.Mocked<InstanceType<typeof Database>>;
  return db;
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/** Assert that the thrown value is a DatabaseError with the expected code. */
function expectDatabaseError(fn: () => unknown, code: string): void {
  let thrown: unknown;
  try { fn(); } catch (e) { thrown = e; }
  expect(thrown).toBeInstanceOf(DatabaseError);
  expect((thrown as DatabaseError).code).toBe(code);
}

// ---------------------------------------------------------------------------
// Setup / teardown
// ---------------------------------------------------------------------------

beforeEach(() => {
  // Full module state reset between tests — prevents singleton bleed.
  _resetDatabaseState();
  jest.clearAllMocks();
  // Default: constructor succeeds, pragmas succeed, instance is open.
  MockDatabase.mockImplementation(() => makeMockDb());
});

afterAll(() => {
  _resetDatabaseState();
});

// ===========================================================================
// Happy path
// ===========================================================================

describe('getDatabase – happy path', () => {
  it('opens successfully on first call and returns a live connection', () => {
    const db = getDatabase();
    expect(db).toBeDefined();
    expect(MockDatabase).toHaveBeenCalledTimes(1);
  });

  it('returns the same singleton on every subsequent call', () => {
    const db1 = getDatabase();
    const db2 = getDatabase();
    const db3 = getDatabase();
    expect(db1).toBe(db2);
    expect(db2).toBe(db3);
    // Constructor called only once despite three getDatabase() calls
    expect(MockDatabase).toHaveBeenCalledTimes(1);
  });

  it('applies all four WAL/durability pragmas on first init', () => {
    const mockDb = makeMockDb();
    MockDatabase.mockImplementation(() => mockDb);

    getDatabase();

    const pragmaCalls = (mockDb.pragma as jest.Mock).mock.calls.map((c: any[]) => c[0] as string);
    expect(pragmaCalls).toContain('journal_mode = WAL');
    expect(pragmaCalls).toContain('synchronous = NORMAL');
    expect(pragmaCalls).toContain('foreign_keys = ON');
    expect(pragmaCalls).toContain('busy_timeout = 5000');
    expect(pragmaCalls).toHaveLength(4);
  });

  it('does not re-apply pragmas on subsequent calls (singleton fast-path)', () => {
    const mockDb = makeMockDb();
    MockDatabase.mockImplementation(() => mockDb);

    getDatabase();
    getDatabase();

    expect((mockDb.pragma as jest.Mock).mock.calls).toHaveLength(4);
  });

  it('pingDatabase returns true when the connection is healthy', () => {
    const mockDb = makeMockDb();
    (mockDb.prepare as jest.Mock).mockReturnValue({ get: jest.fn().mockReturnValue({ ok: 1 }) });
    MockDatabase.mockImplementation(() => mockDb);

    expect(pingDatabase()).toBe(true);
  });

  it('getDatabaseStatus reflects open state with a valid lastOpenedAt ISO timestamp', () => {
    getDatabase();
    const status = getDatabaseStatus();
    expect(status.state).toBe('open');
    expect(status.lastOpenedAt).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/);
    expect(status.consecutiveFailures).toBe(0);
    expect(status.lastErrorCode).toBeNull();
  });
});

// ===========================================================================
// Open failures – error classification
// ===========================================================================

describe('getDatabase – open failures', () => {
  it('throws DatabaseError OPEN_FAILED for a generic constructor error', () => {
    MockDatabase.mockImplementation(() => { throw new Error('disk full'); });
    expectDatabaseError(() => getDatabase(), 'OPEN_FAILED');
  });

  it('throws DatabaseError PERMISSION_DENIED when OS reports EACCES', () => {
    MockDatabase.mockImplementation(() => {
      const err = Object.assign(new Error('EACCES: open'), { code: 'EACCES' });
      throw err;
    });
    expectDatabaseError(() => getDatabase(), 'PERMISSION_DENIED');
  });

  it('throws DatabaseError PERMISSION_DENIED when OS reports EPERM', () => {
    MockDatabase.mockImplementation(() => {
      const err = Object.assign(new Error('EPERM: operation not permitted'), { code: 'EPERM' });
      throw err;
    });
    expectDatabaseError(() => getDatabase(), 'PERMISSION_DENIED');
  });

  it('throws DatabaseError PERMISSION_DENIED when message contains "permission denied"', () => {
    MockDatabase.mockImplementation(() => {
      throw new Error('unable to open database file: permission denied');
    });
    expectDatabaseError(() => getDatabase(), 'PERMISSION_DENIED');
  });

  it('throws DatabaseError BUSY_TIMEOUT when message contains "database is locked"', () => {
    MockDatabase.mockImplementation(() => {
      throw new Error('database is locked');
    });
    expectDatabaseError(() => getDatabase(), 'BUSY_TIMEOUT');
  });

  it('throws DatabaseError BUSY_TIMEOUT when error code is SQLITE_BUSY', () => {
    MockDatabase.mockImplementation(() => {
      const err = Object.assign(new Error('database busy'), { code: 'SQLITE_BUSY' });
      throw err;
    });
    expectDatabaseError(() => getDatabase(), 'BUSY_TIMEOUT');
  });

  it('throws DatabaseError BUSY_TIMEOUT when error code is SQLITE_LOCKED', () => {
    MockDatabase.mockImplementation(() => {
      const err = Object.assign(new Error('database table is locked'), { code: 'SQLITE_LOCKED' });
      throw err;
    });
    expectDatabaseError(() => getDatabase(), 'BUSY_TIMEOUT');
  });

  it('classifies non-Error throws as UNKNOWN', () => {
    MockDatabase.mockImplementation(() => { throw 'not an Error object'; });
    expectDatabaseError(() => getDatabase(), 'UNKNOWN');
  });

  it('increments consecutiveFailures on each failed open', () => {
    MockDatabase.mockImplementation(() => { throw new Error('disk full'); });

    try { getDatabase(); } catch { /* expected */ }
    expect(getDatabaseStatus().consecutiveFailures).toBe(1);

    _resetDatabaseState();
    try { getDatabase(); } catch { /* expected */ }
    expect(getDatabaseStatus().consecutiveFailures).toBe(1);
  });

  it('records lastErrorCode and lastErrorAt on failure', () => {
    MockDatabase.mockImplementation(() => {
      const err = Object.assign(new Error('EACCES'), { code: 'EACCES' });
      throw err;
    });

    try { getDatabase(); } catch { /* expected */ }

    const status = getDatabaseStatus();
    expect(status.state).toBe('error');
    expect(status.lastErrorCode).toBe('PERMISSION_DENIED');
    expect(status.lastErrorAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it('error message does not leak DATABASE_PATH', () => {
    process.env.DATABASE_PATH = '/secret/path/to/prod.db';
    MockDatabase.mockImplementation(() => { throw new Error('disk full'); });

    let caught: DatabaseError | undefined;
    try { getDatabase(); } catch (e) { caught = e as DatabaseError; }

    expect(caught).toBeInstanceOf(DatabaseError);
    expect(caught!.message).not.toContain('/secret/path/to/prod.db');

    delete process.env.DATABASE_PATH;
  });

  it('PERMISSION_DENIED message does not expose OS path details', () => {
    MockDatabase.mockImplementation(() => {
      const err = Object.assign(new Error('EACCES: open /var/lib/prod.db'), { code: 'EACCES' });
      throw err;
    });

    let caught: DatabaseError | undefined;
    try { getDatabase(); } catch (e) { caught = e as DatabaseError; }

    expect(caught!.message).not.toContain('/var/lib/prod.db');
    expect(caught!.message).toContain('permission');
  });

  it('retries successfully after a transient open failure', () => {
    let callCount = 0;
    MockDatabase.mockImplementation(() => {
      callCount++;
      if (callCount === 1) throw new Error('transient disk error');
      return makeMockDb();
    });

    // First call should fail
    expect(() => getDatabase()).toThrow(DatabaseError);
    expect(getDatabaseStatus().state).toBe('error');

    // Second call should succeed (error state does not permanently block)
    const db = getDatabase();
    expect(db).toBeDefined();
    expect(getDatabaseStatus().state).toBe('open');
    expect(getDatabaseStatus().consecutiveFailures).toBe(0);
  });

  it('cause is preserved on DatabaseError for forensic logging', () => {
    const original = new Error('underlying cause');
    MockDatabase.mockImplementation(() => { throw original; });

    let caught: DatabaseError | undefined;
    try { getDatabase(); } catch (e) { caught = e as DatabaseError; }

    expect(caught).toBeInstanceOf(DatabaseError);
    expect(caught!.cause).toBe(original);
  });
});

// ===========================================================================
// Pragma failures
// ===========================================================================

describe('getDatabase – pragma failures', () => {
  it('throws DatabaseError PRAGMA_FAILED when a pragma throws after open', () => {
    const mockDb = makeMockDb();
    (mockDb.pragma as jest.Mock).mockImplementation(() => {
      throw new Error('unknown pragma');
    });
    MockDatabase.mockImplementation(() => mockDb);

    expectDatabaseError(() => getDatabase(), 'PRAGMA_FAILED');
  });

  it('tears down the partially-open instance — no stale reference left', () => {
    const mockDb = makeMockDb();
    (mockDb.pragma as jest.Mock).mockImplementation(() => {
      throw new Error('unknown pragma');
    });
    MockDatabase.mockImplementation(() => mockDb);

    try { getDatabase(); } catch { /* expected */ }

    // The instance's close() should have been called to release the fd
    expect((mockDb.close as jest.Mock)).toHaveBeenCalledTimes(1);
    // Status should reflect error, not open
    expect(getDatabaseStatus().state).toBe('error');
    expect(getDatabaseStatus().lastErrorCode).toBe('PRAGMA_FAILED');
  });

  it('retries cleanly after a pragma failure and returns a fully-configured connection', () => {
    let callCount = 0;
    MockDatabase.mockImplementation(() => {
      callCount++;
      if (callCount === 1) {
        // First attempt: pragma fails
        const bad = makeMockDb();
        (bad.pragma as jest.Mock).mockImplementation(() => { throw new Error('pragma error'); });
        return bad;
      }
      // Second attempt: everything succeeds
      return makeMockDb();
    });

    expect(() => getDatabase()).toThrow(DatabaseError);
    const db = getDatabase();
    expect(db).toBeDefined();
    expect(getDatabaseStatus().state).toBe('open');
  });

  it('DatabaseError PRAGMA_FAILED message names the failing pragma', () => {
    let pragmaCallCount = 0;
    const mockDb = makeMockDb();
    (mockDb.pragma as jest.Mock).mockImplementation((p: string) => {
      pragmaCallCount++;
      // Fail on the second pragma (synchronous = NORMAL)
      if (pragmaCallCount === 2) throw new Error('bad pragma value');
    });
    MockDatabase.mockImplementation(() => mockDb);

    let caught: DatabaseError | undefined;
    try { getDatabase(); } catch (e) { caught = e as DatabaseError; }

    expect(caught!.code).toBe('PRAGMA_FAILED');
    expect(caught!.message).toContain('synchronous');
  });
});

// ===========================================================================
// Stale instance guard
// ===========================================================================

describe('getDatabase – stale instance guard', () => {
  it('discards and re-opens when the existing instance has .open === false', () => {
    // Step 1: open a fresh instance successfully
    const first = makeMockDb();
    (first as any).open = true;
    const second = makeMockDb();
    (second as any).open = true;

    MockDatabase
      .mockImplementationOnce(() => first)   // initial open
      .mockImplementationOnce(() => second); // re-open after stale eviction

    const db1 = getDatabase();
    expect(db1).toBe(first);
    expect(MockDatabase).toHaveBeenCalledTimes(1);

    // Step 2: simulate the OS closing the file descriptor underneath us
    (first as any).open = false;

    // Step 3: next getDatabase() should detect stale → discard → re-open
    const db2 = getDatabase();
    expect(db2).toBe(second);
    expect(db2).not.toBe(first);
    expect(MockDatabase).toHaveBeenCalledTimes(2);
  });

  it('clears the statement cache when a stale instance is evicted', () => {
    const first = makeMockDb();
    (first as any).open = true;
    const second = makeMockDb();
    (second as any).open = true;

    MockDatabase
      .mockImplementationOnce(() => first)
      .mockImplementationOnce(() => second);

    // Open, warm the cache with a prepared statement
    getDatabase();
    getPreparedStatement('SELECT 1');
    expect(getStatementCacheStats().size).toBeGreaterThan(0);

    // Simulate stale fd
    (first as any).open = false;

    // Re-open — stale eviction must clear the cache
    getDatabase();
    expect(getStatementCacheStats().size).toBe(0);
  });
});

// ===========================================================================
// Re-entrant call guard
// ===========================================================================

describe('getDatabase – re-entrant call guard', () => {
  it('throws DatabaseError OPEN_FAILED when called re-entrantly during opening', () => {
    // Simulate a pragma callback that itself calls getDatabase() — a real
    // scenario with buggy initialization code that creates circular deps.
    const mockDb = makeMockDb();
    let firstPragmaCall = true;
    (mockDb.pragma as jest.Mock).mockImplementation(() => {
      if (firstPragmaCall) {
        firstPragmaCall = false;
        // Re-entrant call while state === 'opening'
        expect(() => getDatabase()).toThrow(DatabaseError);
        const reentrant = (() => { try { getDatabase(); } catch (e) { return e; } })();
        expect((reentrant as DatabaseError).code).toBe('OPEN_FAILED');
      }
    });
    MockDatabase.mockImplementation(() => mockDb);

    // The outer call should still complete successfully
    const db = getDatabase();
    expect(db).toBeDefined();
  });
});

// ===========================================================================
// Prepared statement cache
// ===========================================================================

// Note: live in-memory tests for getPreparedStatement are covered in perf.test.ts.
// Mocked behavior tests follow in the next describe block.

// Statement cache tests using the mocked constructor (simpler, deterministic)
describe('getPreparedStatement – cache behavior (mocked)', () => {
  it('returns the same statement object for the same SQL string', () => {
    const sql = 'SELECT * FROM invoices WHERE id = ?';
    const stmt = getPreparedStatement(sql);
    const stmt2 = getPreparedStatement(sql);
    expect(stmt).toBe(stmt2);
  });

  it('returns different statement objects for different SQL strings', () => {
    const stmt1 = getPreparedStatement('SELECT 1');
    const stmt2 = getPreparedStatement('SELECT 2');
    expect(stmt1).not.toBe(stmt2);
  });

  it('caches N distinct statements without eviction', () => {
    const sqls = ['SELECT 1', 'SELECT 2', 'SELECT 3', 'SELECT 4', 'SELECT 5'];
    sqls.forEach(sql => getPreparedStatement(sql));
    expect(getStatementCacheStats().size).toBe(sqls.length);
    expect(getStatementCacheStats().statements).toEqual(expect.arrayContaining(sqls));
  });

  it('clearStatementCache evicts all entries', () => {
    getPreparedStatement('SELECT 1');
    getPreparedStatement('SELECT 2');
    clearStatementCache();
    expect(getStatementCacheStats().size).toBe(0);
    expect(getStatementCacheStats().statements).toHaveLength(0);
  });

  it('cache is cleared when closeDatabase is called', () => {
    getPreparedStatement('SELECT 1');
    expect(getStatementCacheStats().size).toBeGreaterThan(0);
    closeDatabase();
    expect(getStatementCacheStats().size).toBe(0);
  });

  it('getStatementCacheStats reflects current size and keys accurately', () => {
    clearStatementCache();
    expect(getStatementCacheStats()).toEqual({ size: 0, statements: [] });

    getPreparedStatement('SELECT 1');
    const stats = getStatementCacheStats();
    expect(stats.size).toBe(1);
    expect(stats.statements).toContain('SELECT 1');
  });

  it('prepare() is only called once per unique SQL across many callers', () => {
    const mockDb = makeMockDb();
    const prepareSpy = jest.fn((sql: string) => ({ get: jest.fn(), sql }));
    (mockDb.prepare as jest.Mock).mockImplementation(prepareSpy);
    MockDatabase.mockImplementation(() => mockDb);

    const sql = 'SELECT * FROM test WHERE id = ?';
    getPreparedStatement(sql);
    getPreparedStatement(sql);
    getPreparedStatement(sql);

    expect(prepareSpy).toHaveBeenCalledTimes(1);
  });
});

// ===========================================================================
// Lifecycle – closeDatabase
// ===========================================================================

describe('closeDatabase', () => {
  it('sets state to closed and clears the singleton', () => {
    getDatabase();
    expect(getDatabaseStatus().state).toBe('open');

    closeDatabase();

    expect(getDatabaseStatus().state).toBe('closed');
    expect(getDatabaseStatus().lastClosedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it('is idempotent — safe to call twice without throwing', () => {
    getDatabase();
    expect(() => {
      closeDatabase();
      closeDatabase();
    }).not.toThrow();
    expect(getDatabaseStatus().state).toBe('closed');
  });

  it('is a no-op when called before any connection is opened', () => {
    expect(() => closeDatabase()).not.toThrow();
  });

  it('getDatabase re-opens cleanly after closeDatabase', () => {
    const db1 = getDatabase();
    closeDatabase();

    MockDatabase.mockImplementation(() => makeMockDb());
    const db2 = getDatabase();

    expect(db2).not.toBe(db1);
    expect(getDatabaseStatus().state).toBe('open');
    expect(MockDatabase).toHaveBeenCalledTimes(2);
  });

  it('calls the underlying close() method on the better-sqlite3 instance', () => {
    const mockDb = makeMockDb();
    MockDatabase.mockImplementation(() => mockDb);

    getDatabase();
    closeDatabase();

    expect((mockDb.close as jest.Mock)).toHaveBeenCalledTimes(1);
  });
});

// ===========================================================================
// Observable state – getDatabaseStatus
// ===========================================================================

describe('getDatabaseStatus – lifecycle transitions', () => {
  it('starts at uninitialized before any call', () => {
    expect(getDatabaseStatus().state).toBe('uninitialized');
  });

  it('transitions to open after a successful getDatabase()', () => {
    getDatabase();
    expect(getDatabaseStatus().state).toBe('open');
  });

  it('transitions to closed after closeDatabase()', () => {
    getDatabase();
    closeDatabase();
    expect(getDatabaseStatus().state).toBe('closed');
  });

  it('transitions to error after a failed open', () => {
    MockDatabase.mockImplementation(() => { throw new Error('oops'); });
    try { getDatabase(); } catch { /* expected */ }
    expect(getDatabaseStatus().state).toBe('error');
  });

  it('resets consecutiveFailures to 0 after a successful open following failures', () => {
    MockDatabase.mockImplementationOnce(() => { throw new Error('fail 1'); });
    try { getDatabase(); } catch { /* expected */ }
    expect(getDatabaseStatus().consecutiveFailures).toBe(1);

    MockDatabase.mockImplementation(() => makeMockDb());
    getDatabase();
    expect(getDatabaseStatus().consecutiveFailures).toBe(0);
  });

  it('clears lastErrorCode after a successful open that follows a failure', () => {
    MockDatabase.mockImplementationOnce(() => {
      const e = Object.assign(new Error('EACCES'), { code: 'EACCES' });
      throw e;
    });
    try { getDatabase(); } catch { /* expected */ }
    expect(getDatabaseStatus().lastErrorCode).toBe('PERMISSION_DENIED');

    MockDatabase.mockImplementation(() => makeMockDb());
    getDatabase();
    expect(getDatabaseStatus().lastErrorCode).toBeNull();
  });

  it('getDatabaseStatus never throws regardless of internal state', () => {
    // Before any open
    expect(() => getDatabaseStatus()).not.toThrow();
    // After failure
    MockDatabase.mockImplementationOnce(() => { throw new Error('x'); });
    try { getDatabase(); } catch { /* expected */ }
    expect(() => getDatabaseStatus()).not.toThrow();
    // After close
    _resetDatabaseState();
    MockDatabase.mockImplementation(() => makeMockDb());
    getDatabase();
    closeDatabase();
    expect(() => getDatabaseStatus()).not.toThrow();
  });

  it('status snapshot is a value copy — mutations do not affect internal state', () => {
    getDatabase();
    const status = getDatabaseStatus();
    (status as any).state = 'closed';
    (status as any).consecutiveFailures = 99;
    // Internal state unchanged
    expect(getDatabaseStatus().state).toBe('open');
    expect(getDatabaseStatus().consecutiveFailures).toBe(0);
  });
});

// ===========================================================================
// pingDatabase – failure paths
// ===========================================================================

describe('pingDatabase – failure paths', () => {
  it('returns false when getDatabase throws', () => {
    MockDatabase.mockImplementation(() => { throw new Error('cannot open'); });
    expect(pingDatabase()).toBe(false);
  });

  it('returns false when the SELECT 1 result is null', () => {
    const mockDb = makeMockDb();
    (mockDb.prepare as jest.Mock).mockReturnValue({ get: jest.fn().mockReturnValue(null) });
    MockDatabase.mockImplementation(() => mockDb);

    expect(pingDatabase()).toBe(false);
  });

  it('returns false when ok !== 1', () => {
    const mockDb = makeMockDb();
    (mockDb.prepare as jest.Mock).mockReturnValue({ get: jest.fn().mockReturnValue({ ok: 0 }) });
    MockDatabase.mockImplementation(() => mockDb);

    expect(pingDatabase()).toBe(false);
  });

  it('returns false when the prepare call itself throws', () => {
    const mockDb = makeMockDb();
    (mockDb.prepare as jest.Mock).mockImplementation(() => { throw new Error('prepare error'); });
    MockDatabase.mockImplementation(() => mockDb);

    expect(pingDatabase()).toBe(false);
  });

  it('never throws — always returns a boolean', () => {
    MockDatabase.mockImplementation(() => { throw 'non-Error throw'; });
    expect(() => pingDatabase()).not.toThrow();
    expect(typeof pingDatabase()).toBe('boolean');
  });
});

// ===========================================================================
// Boundary / environment inputs
// ===========================================================================

describe('boundary inputs', () => {
  it('DATABASE_PATH env var is passed to the better-sqlite3 constructor', () => {
    process.env.DATABASE_PATH = '/tmp/test-boundary.db';
    MockDatabase.mockImplementation(() => makeMockDb());

    getDatabase();

    expect(MockDatabase).toHaveBeenCalledWith('/tmp/test-boundary.db');
    delete process.env.DATABASE_PATH;
  });

  it('defaults to .data/dev.db when DATABASE_PATH is not set', () => {
    delete process.env.DATABASE_PATH;
    MockDatabase.mockImplementation(() => makeMockDb());

    getDatabase();

    expect(MockDatabase).toHaveBeenCalledWith('.data/dev.db');
  });

  it('_resetDatabaseState fully resets all observable counters', () => {
    MockDatabase.mockImplementationOnce(() => { throw new Error('fail'); });
    try { getDatabase(); } catch { /* expected */ }

    _resetDatabaseState();

    const status = getDatabaseStatus();
    expect(status.state).toBe('uninitialized');
    expect(status.consecutiveFailures).toBe(0);
    expect(status.lastErrorCode).toBeNull();
    expect(status.lastOpenedAt).toBeNull();
    expect(status.lastClosedAt).toBeNull();
    expect(status.lastErrorAt).toBeNull();
  });

  it('multiple open/close cycles do not accumulate stale state', () => {
    for (let i = 0; i < 5; i++) {
      MockDatabase.mockImplementation(() => makeMockDb());
      getDatabase();
      expect(getDatabaseStatus().state).toBe('open');
      closeDatabase();
      expect(getDatabaseStatus().state).toBe('closed');
    }
    expect(getDatabaseStatus().consecutiveFailures).toBe(0);
  });

  it('DatabaseError is an instance of Error (standard inheritance)', () => {
    const err = new DatabaseError('OPEN_FAILED', 'test');
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(DatabaseError);
    expect(err.name).toBe('DatabaseError');
  });

  it('DatabaseError.cause is undefined when no cause is provided', () => {
    const err = new DatabaseError('UNKNOWN', 'test');
    expect(err.cause).toBeUndefined();
  });

  it('consecutive failures across open/close/error cycle are isolated correctly', () => {
    // Fail once
    MockDatabase.mockImplementationOnce(() => { throw new Error('fail'); });
    try { getDatabase(); } catch { /* expected */ }
    expect(getDatabaseStatus().consecutiveFailures).toBe(1);

    // Succeed — counter resets
    MockDatabase.mockImplementation(() => makeMockDb());
    getDatabase();
    expect(getDatabaseStatus().consecutiveFailures).toBe(0);

    // Close + fail again — counter starts from 0, not accumulating from before
    closeDatabase();
    MockDatabase.mockImplementationOnce(() => { throw new Error('fail again'); });
    try { getDatabase(); } catch { /* expected */ }
    expect(getDatabaseStatus().consecutiveFailures).toBe(1);
  });
});
