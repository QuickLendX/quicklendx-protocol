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
 * Specific error types for database ping failures with deterministic categorization.
 */
export class DatabasePingError extends DatabaseError {
  public readonly code: string;
  public readonly retryable: boolean;
  public readonly severity: 'warning' | 'error' | 'critical';
  
  constructor(message: string, code: string, retryable: boolean = false, severity: 'warning' | 'error' | 'critical' = 'error') {
    super(message);
    this.name = 'DatabasePingError';
    this.code = code;
    this.retryable = retryable;
    this.severity = severity;
  }
}

export class DatabasePingTimeoutError extends DatabasePingError {
  constructor(timeoutMs: number) {
    super(`Database ping timed out after ${timeoutMs}ms`, 'PING_TIMEOUT', true, 'warning');
    this.name = 'DatabasePingTimeoutError';
  }
}

export class DatabasePingConnectionError extends DatabasePingError {
  constructor(message: string) {
    super(`Database connection failed: ${message}`, 'PING_CONNECTION_FAILED', true, 'error');
    this.name = 'DatabasePingConnectionError';
  }
}

export class DatabasePingPermissionError extends DatabasePingError {
  constructor(message: string) {
    super(`Database ping permission denied: ${message}`, 'PING_PERMISSION_DENIED', false, 'critical');
    this.name = 'DatabasePingPermissionError';
  }
}

export class DatabasePingBusyError extends DatabasePingError {
  constructor(message: string, attempts: number) {
    super(`Database ping failed after ${attempts} attempts: ${message}`, 'PING_BUSY', true, 'warning');
    this.name = 'DatabasePingBusyError';
  }
}

export class DatabasePingCorruptionError extends DatabasePingError {
  constructor(message: string) {
    super(`Database corruption detected: ${message}`, 'PING_CORRUPTION', false, 'critical');
    this.name = 'DatabasePingCorruptionError';
  }
}

/**
 * Database ping result with comprehensive failure information.
 */
export type DatabasePingResult = 
  | { success: true; latencyMs: number; timestamp: number; attempts: number }
  | { success: false; error: DatabasePingError; latencyMs?: number; timestamp: number; attempts: number };

/**
 * Database ping state for deterministic tracking.
 */
export type DatabasePingState = 
  | 'healthy'
  | 'degraded' 
  | 'timeout'
  | 'busy'
  | 'permission_denied'
  | 'connection_failed'
  | 'corrupted'
  | 'unknown';

/**
 * Configuration for database ping behavior.
 */
export interface DatabasePingConfig {
  /** Maximum time to wait for ping response in milliseconds */
  timeoutMs: number;
  /** Maximum number of retry attempts for retryable errors */
  maxRetries: number;
  /** Base delay between retries in milliseconds (will use exponential backoff) */
  baseRetryDelayMs: number;
  /** SQL query to use for ping test */
  pingQuery: string;
  /** Whether to enable metrics collection */
  enableMetrics: boolean;
}

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
    // Apply performance pragmas.
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
 * Retrieve a prepared statement with deterministic failure handling.
 *
 * 1. Cache‑hit returns the prepared statement after a cheap validation step.
 *    If validation fails due to a stale schema (`SQLITE_SCHEMA`) the entry is evicted
 *    and a fresh preparation is performed.
 * 2. Cache‑miss triggers a guarded preparation sequence:
 *    - Concurrency guard ensures only one preparation per SQL string.
 *    - Retry loop (max 3 attempts) handles transient `SQLITE_BUSY` errors.
 *    - Permission checks surface a `DatabasePermissionError` without caching.
 *    - Any other preparation error surfaces a `DatabasePrepareError`.
 *
 * The public signature is unchanged – callers receive the prepared statement or
 * a thrown error they can handle deterministically.
 */
// Deterministic, synchronous prepared statement retrieval with failure handling.
export function getPreparedStatement(sql: string): any {
  // ----- Cache Hit Path -----
  if (statementCache.has(sql)) {
    cacheHits++;
    const cached = statementCache.get(sql);
    try {
      if (cached.reader) {
        cached.get();
      } else {
        cached.run();
      }
      return cached;
    } catch (e: any) {
      if (e.code === 'SQLITE_SCHEMA') {
        statementCache.delete(sql);
        cacheEvicts++;
        // fall through to preparation
      } else {
        throw e;
      }
    }
  }

  // ----- Cache Miss / Evicted Path -----
  cacheMisses++;
  const maxAttempts = 3;
  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    try {
      const db = getDatabase();
      const stmt = db.prepare(sql);
      // Permission guard – attempt a harmless execution to surface read‑only errors.
      try {
        if (stmt.reader) {
          stmt.get();
        } else {
          stmt.run();
        }
      } catch (permErr: any) {
        if (permErr.code === 'SQLITE_READONLY') {
          throw new DatabasePermissionError(sql, permErr);
        }
        // ignore other errors here
      }
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
function resetCacheMetrics(): void {
  cacheHits = 0;
  cacheMisses = 0;
  cacheEvicts = 0;
  // Also reset ping metrics when clearing cache
  resetPingMetrics();
}

/**
 * Get comprehensive database ping statistics.
 */
export function getPingStats() {
  return {
    size: statementCache.size,
    statements: Array.from(statementCache.keys()),
    hits: cacheHits,
    misses: cacheMisses,
    evicts: cacheEvicts,
  };
}

/**
 * Simple health probe – deterministic, never throws.
 */
export function pingDatabase(): boolean {
  try {
    // Use synchronous approach for backward compatibility
    const db = getDatabase();
    const row = db.prepare('SELECT 1 AS ok').get();
    return row?.ok === 1;
  } catch {
    return false;
  }
}

/**
 * Graceful shutdown.
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
    // Drop the metrics alongside the cache so the next generation starts from
    // zeroed counters instead of inheriting a closed generation's hit counts.
    resetCacheMetrics();
    dbInstance.close();
    dbInstance = null;
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
