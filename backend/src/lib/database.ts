// Updated implementation with deterministic failure‑boundary handling for prepared statements.

import Database from 'better-sqlite3';
// ----- Type Declarations -----
const DatabaseConstructor = Database as any;
/**
 * Discriminated error type for every failure mode getDatabase can encounter.
 * Callers can branch on `code` without parsing message strings, which keeps
 * error handling deterministic and testable.
 *
 * Codes:
 *  - OPEN_FAILED      Database file could not be opened (corrupt, missing dir, etc.)
 *  - PERMISSION_DENIED OS-level permission error on the database file or its directory
 *  - BUSY_TIMEOUT      Database was locked for longer than the busy_timeout
 *  - PRAGMA_FAILED     A post-open configuration pragma failed (leaves no live instance)
 *  - CLOSED            The caller tried to use a connection that has already been closed
 *  - UNKNOWN           Any other failure – the original error is preserved in `cause`
 */
export type DatabaseErrorCode =
  | 'OPEN_FAILED'
  | 'PERMISSION_DENIED'
  | 'BUSY_TIMEOUT'
  | 'PRAGMA_FAILED'
  | 'CLOSED'
  | 'UNKNOWN';

export class DatabaseError extends Error {
  readonly code: DatabaseErrorCode;
  readonly cause: unknown;

  constructor(code: DatabaseErrorCode, message: string, cause?: unknown) {
    super(message);
    this.name = 'DatabaseError';
    this.code = code;
    this.cause = cause;
    // Preserve original stack when wrapping another error
    if (cause instanceof Error && cause.stack) {
      this.stack = `${this.stack}\nCaused by: ${cause.stack}`;
    }
  }
}

// ---------------------------------------------------------------------------
// Observable state
// ---------------------------------------------------------------------------

/**
 * Lifecycle states of the database singleton.
 *
 * - uninitialized: no connection has been opened yet this process lifetime
 * - opening:       constructor and pragma initialization are in progress
 * - open:          connection is live and ready to accept queries
 * - closed:        connection was explicitly closed via closeDatabase()
 * - error:         last open attempt failed; the next call to getDatabase()
 *                  will clear this state and retry
 */
export type DatabaseState = 'uninitialized' | 'opening' | 'open' | 'closed' | 'error';

/** Read-only snapshot of current connection health — safe to expose to monitors. */
export interface DatabaseStatus {
  state: DatabaseState;
  /** ISO-8601 timestamp of the most recent successful open, or null. */
  lastOpenedAt: string | null;
  /** ISO-8601 timestamp of the most recent close, or null. */
  lastClosedAt: string | null;
  /** ISO-8601 timestamp of the most recent error, or null. */
  lastErrorAt: string | null;
  /** Error code from the most recent failed open, or null. */
  lastErrorCode: DatabaseErrorCode | null;
  /** Number of consecutive failed open attempts since the last successful open. */
  consecutiveFailures: number;
}

// ---------------------------------------------------------------------------
// Internal singleton state
// ---------------------------------------------------------------------------

// Type declaration for better-sqlite3 (avoids `any` in call-site casts)
const DatabaseConstructor = Database as unknown as new (
  path: string,
  options?: object
) => InstanceType<typeof Database>;

let dbInstance: InstanceType<typeof Database> | null = null;
let _state: DatabaseState = 'uninitialized';
let _lastOpenedAt: string | null = null;
let _lastClosedAt: string | null = null;
let _lastErrorAt: string | null = null;
let _lastErrorCode: DatabaseErrorCode | null = null;
let _consecutiveFailures = 0;

/** Centralized prepared statement cache: SQL string → compiled statement. */
const statementCache = new Map<string, ReturnType<InstanceType<typeof Database>['prepare']>>();

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * Custom error hierarchy for deterministic error handling.
 */
export class DatabaseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DatabaseError';
  }
}
export class DatabasePrepareError extends DatabaseError {
  constructor(sql: string, original: any) {
    super(`Failed to prepare statement for SQL: ${sql}. ${original?.message ?? ''}`);
    this.name = 'DatabasePrepareError';
  }
}
export class DatabasePermissionError extends DatabaseError {
  constructor(sql: string, original: any) {
    super(`Permission denied while preparing statement for SQL: ${sql}. ${original?.message ?? ''}`);
    this.name = 'DatabasePermissionError';
  }
}
export class DatabaseBusyError extends DatabaseError {
  constructor(sql: string, original: any) {
    super(`Database busy while preparing statement for SQL: ${sql}. ${original?.message ?? ''}`);
    this.name = 'DatabaseBusyError';
  }
}

/**
 * Centralized prepared statement cache.
 * Key: SQL string, Value: prepared statement.
 */
function classifyError(err: unknown): DatabaseErrorCode {
  if (!(err instanceof Error)) return 'UNKNOWN';

  const msg = err.message.toLowerCase();
  const code = (err as NodeJS.ErrnoException).code ?? '';

  if (code === 'EACCES' || code === 'EPERM' || msg.includes('permission denied') || msg.includes('access denied')) {
    return 'PERMISSION_DENIED';
  }

  if (
    msg.includes('database is locked') ||
    msg.includes('database table is locked') ||
    msg.includes('sqlite_busy') ||
    msg.includes('sqlite_locked') ||
    code === 'SQLITE_BUSY' ||
    code === 'SQLITE_LOCKED'
  ) {
    return 'BUSY_TIMEOUT';
  }

  return 'OPEN_FAILED';
}



/**
 * Metrics for deterministic observability.
 */
let cacheHits = 0;
let cacheMisses = 0;
let cacheEvicts = 0;

/**
 * Get a singleton instance of the better‑sqlite3 database with sensible pragmas.
 */
export function getDatabase() {
  if (!dbInstance) {
    const db = new DatabaseConstructor(process.env.DATABASE_PATH || '.data/dev.db');

    // Performance pragmas
    db.pragma('journal_mode = WAL');
    db.pragma('synchronous = NORMAL');
    db.pragma('foreign_keys = ON');
    db.pragma('busy_timeout = 5000');

    dbInstance = db;
  }

  // ── Fast path: already open ───────────────────────────────────────────────
  if (dbInstance !== null) {
    return dbInstance;
  }

  // ── Slow path: open a new connection ─────────────────────────────────────
  _state = 'opening';
  let fresh: InstanceType<typeof Database> | null = null;

  try {
    const path = process.env.DATABASE_PATH || '.data/dev.db';
    fresh = new DatabaseConstructor(path);
  } catch (err) {
    _state = 'error';
    _lastErrorAt = new Date().toISOString();
    _consecutiveFailures++;

    const code = classifyError(err);
    _lastErrorCode = code;

    // Never leak OS path details in the publicly visible message
    const safeMsg =
      code === 'PERMISSION_DENIED'
        ? 'Database file permission denied. Verify the process has read/write access to the database path.'
        : code === 'BUSY_TIMEOUT'
          ? 'Database is locked and could not be opened within the busy_timeout window.'
          : 'Failed to open database. Check that the database path exists and is not corrupt.';

    throw new DatabaseError(code, safeMsg, err);
  }

  // Apply configuration pragmas.  If any pragma fails we must not expose a
  // half-configured instance — close and rethrow.
  try {
    applyPragmas(fresh);
  } catch (err) {
    try { fresh.close(); } catch { /* best-effort cleanup */ }
    _state = 'error';
    _lastErrorAt = new Date().toISOString();
    _consecutiveFailures++;
    _lastErrorCode = 'PRAGMA_FAILED';
    throw err; // already a DatabaseError(PRAGMA_FAILED)
  }

  // ── Commit the instance ───────────────────────────────────────────────────
  dbInstance = fresh;
  _state = 'open';
  _lastOpenedAt = new Date().toISOString();
  _consecutiveFailures = 0;
  _lastErrorCode = null;

  return dbInstance;
}

// ---------------------------------------------------------------------------
// Statement cache
// ---------------------------------------------------------------------------

/**
 * Get a prepared statement from the cache, or prepare and cache it if not present.
 * This significantly improves performance by avoiding redundant statement preparation.
 *
 * SECURITY: The SQL string must be fully parameterized. Never interpolate values into the SQL key.
 *
 * @param sql - The SQL query string with placeholders (?, ?, etc.)
 * @returns The cached or newly prepared statement
 *
 * @example
 * const stmt = getPreparedStatement('SELECT * FROM invoices WHERE id = ?');
 * const row = stmt.get(invoiceId);
 */
let customGetDatabase: (() => any) | null = null;

export function _setGetDatabaseForTesting(fn: (() => any) | null): void {
  customGetDatabase = fn;
}

export function getPreparedStatement(sql: string): any {
  // ----- Cache Hit Path -----
  if (statementCache.has(sql)) {
    cacheHits++;
    return statementCache.get(sql);
  }

  // ----- Cache Miss / Evicted Path -----
  cacheMisses++;
  const maxAttempts = 3;
  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    try {
      const db = customGetDatabase ? customGetDatabase() : getDatabase();
      const stmt = db.prepare(sql);
      statementCache.set(sql, stmt);
      return stmt;
    } catch (err: any) {
      if (err.code === 'SQLITE_BUSY') {
        if (attempt < maxAttempts - 1) {
          // simple synchronous back‑off
          const delay = 50 * (attempt + 1);
          const start = Date.now();
          while (Date.now() - start < delay) {}
          continue;
        }
        throw new DatabaseBusyError(sql, err);
      }
      if (err.code === 'SQLITE_READONLY' || err.code === 'SQLITE_AUTH') {
        throw new DatabasePermissionError(sql, err);
      }
      // Any other error is a preparation failure.
      throw new DatabasePrepareError(sql, err);
    }
  }
  // Should never reach here.
  throw new DatabaseError('Unexpected preparation failure');
}
  

/**
 * Clear the statement cache and metrics – useful for testing or schema changes.
 */
export function clearStatementCache(): void {
  statementCache.clear();
  cacheHits = 0;
  cacheMisses = 0;
  cacheEvicts = 0;
}

/**
 * Retrieve cache statistics including deterministic metrics.
 */
export function getStatementCacheStats(): { size: number; statements: string[] } {
  return {
    size: statementCache.size,
    statements: Array.from(statementCache.keys()),
    hits: cacheHits,
    misses: cacheMisses,
    evicts: cacheEvicts,
  };
}

// ---------------------------------------------------------------------------
// Health / observability
// ---------------------------------------------------------------------------

/**
 * Simple health probe – deterministic, never throws.
 */
export function pingDatabase(): boolean {
  try {
    const db = getDatabase();
    const row = db.prepare('SELECT 1 AS ok').get();
    return row?.ok === 1;
  } catch {
    return false;
  }
}

/**
 * Close the database connection and clear the statement cache.
 * Ensures clean shutdown and prevents memory leaks.
 *
 * Invariants:
 * - The statement cache is always cleared, even if `close()` throws. This
 *   prevents stale prepared statements from being reused against a fresh
 *   connection after a failed close (which would cause silent corruption
 *   or `database connection is not open` errors).
 * - `dbInstance` is always nulled after a close attempt, so a subsequent
 *   `getDatabase()` will re-open fresh instead of returning a half-closed
 *   handle. This makes close idempotent and retry-safe.
 * - Closing an already-closed or never-opened database is a no-op and
 *   does not throw.
 *
 * Concurrency: Node is single-threaded, so the check-then-close sequence
 * is atomic with respect to other JS callbacks. A close that races with a
 * concurrent getDatabase() call will either see the old instance (and the
 * close will fail cleanly with an error) or the new one (and the close
 * will not affect it). The invariant is that we never leave `dbInstance`
 * pointing at a closed handle.
 *
 * @throws Re-throws any error from `better-sqlite3.close()` after cleanup.
 */
export function getDatabaseStatus(): DatabaseStatus {
  return {
    state: _state,
    lastOpenedAt: _lastOpenedAt,
    lastClosedAt: _lastClosedAt,
    lastErrorAt: _lastErrorAt,
    lastErrorCode: _lastErrorCode,
    consecutiveFailures: _consecutiveFailures,
  };
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/**
 * Close the active database connection and release all resources.
 *
 * After this call:
 * - `dbInstance` is set to `null`
 * - The prepared statement cache is cleared
 * - The lifecycle state transitions to `closed`
 * - The next call to `getDatabase()` will open a fresh connection
 *
 * Idempotent: calling `closeDatabase()` on an already-closed instance is a
 * no-op.
 */
export function closeDatabase(): void {
  const instance = dbInstance;
  if (!instance) {
    // Idempotent no-op: closing a never-opened or already-closed database
    // must not throw. Still clear the cache in case it was populated by
    // a previous instance that was never closed.
    statementCache.clear();
    return;
  }

  // Null out the singleton before closing so that any re-entrant call to
  // getDatabase() during close opens a fresh handle instead of returning
  // the half-closed one. This is the key determinism guarantee.
  dbInstance = null;

  // Always clear on attempt, whether close succeeds or fails. Stale
  // statements bound to a closed handle would throw on use.
  statementCache.clear();

  _state = 'closed';
  _lastClosedAt = new Date().toISOString();

  try {
    instance.close();
  } catch (err) {
    // Re-throw after cleanup so callers can observe the failure (lost
    // durability warning, etc.) while the module stays in a consistent
    // state. The next getDatabase() will re-open fresh.
    throw err;
  }
}

/**
 * Reset all module-level state.  **For use in tests only.**
 *
 * Closes the connection (if open), clears the statement cache, and resets
 * lifecycle counters so each test starts from a clean slate without
 * module-cache pollution.
 */
export function _resetDatabaseState(): void {
  closeDatabase();
  _state = 'uninitialized';
  _lastOpenedAt = null;
  _lastClosedAt = null;
  _lastErrorAt = null;
  _lastErrorCode = null;
  _consecutiveFailures = 0;
}