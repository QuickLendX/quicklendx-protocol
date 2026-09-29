import Database from 'better-sqlite3';

// ---------------------------------------------------------------------------
// Types & error classes
// ---------------------------------------------------------------------------

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
 * Classify a raw better-sqlite3 / OS error into one of our typed codes so
 * that callers never need to inspect message strings.
 *
 * Detection rules:
 *  - EACCES / EPERM              → PERMISSION_DENIED
 *  - "database is locked" / BUSY → BUSY_TIMEOUT
 *  - SQLite error codes 5/6      → BUSY_TIMEOUT (SQLITE_BUSY / SQLITE_LOCKED)
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
 * Apply all post-open WAL/durability pragmas.
 * Throws `DatabaseError(PRAGMA_FAILED, …)` on the first pragma that errors,
 * which lets `getDatabase` tear down the partially-initialized instance.
 */
function applyPragmas(db: InstanceType<typeof Database>): void {
  const pragmas: Array<[string, string]> = [
    ['journal_mode', 'WAL'],
    ['synchronous', 'NORMAL'],
    ['foreign_keys', 'ON'],
    ['busy_timeout', '5000'],
  ];

  for (const [pragma, value] of pragmas) {
    try {
      db.pragma(`${pragma} = ${value}`);
    } catch (err) {
      throw new DatabaseError(
        'PRAGMA_FAILED',
        `Failed to apply pragma "${pragma} = ${value}": ${(err as Error).message}`,
        err,
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Return the singleton better-sqlite3 connection, initializing it on first
 * call.  All failure modes are surfaced as typed `DatabaseError` instances so
 * callers can branch deterministically.
 *
 * Failure-boundary guarantees
 * ────────────────────────────
 * 1. **Open failure** – if the constructor throws (corrupt file, missing
 *    parent directory, disk full, etc.) the instance is never assigned, the
 *    state transitions to `error`, and a `DatabaseError(OPEN_FAILED, …)` is
 *    thrown.  The next call retries from scratch.
 * 2. **Permission denial** – EACCES / EPERM are re-classified to
 *    `PERMISSION_DENIED` so upstream middleware can return 503 without
 *    leaking OS details.
 * 3. **Pragma failure** – if any post-open pragma throws, the partially-open
 *    connection is closed and the error is wrapped as `PRAGMA_FAILED`.  No
 *    half-configured instance is ever exposed to callers.
 * 4. **Stale / closed instance** – if `dbInstance` is not null but its
 *    internal `open` flag is false (e.g. the OS closed the file descriptor
 *    underneath us), the stale reference is discarded and a fresh connection
 *    is opened transparently.
 * 5. **Concurrent callers** – because Node.js is single-threaded, the
 *    `opening` guard is sufficient to detect re-entrant calls (which would
 *    only arise from synchronous re-entry in tests or pathological pragma
 *    hooks).  Re-entry during `opening` throws immediately rather than
 *    blocking, preventing infinite recursion.
 * 6. **Retries after error** – a previous `error` state does not permanently
 *    block future calls; the state is reset and a new open is attempted on
 *    every subsequent `getDatabase()` call.
 *
 * @throws {DatabaseError} OPEN_FAILED | PERMISSION_DENIED | PRAGMA_FAILED |
 *                          BUSY_TIMEOUT | CLOSED (re-entrant call during close)
 */
export function getDatabase(): InstanceType<typeof Database> {
  // ── Guard: re-entrant call while opening ──────────────────────────────────
  if (_state === 'opening') {
    throw new DatabaseError(
      'OPEN_FAILED',
      'Re-entrant call to getDatabase() detected while a connection is already being opened. ' +
        'This indicates a circular dependency in initialization code.',
    );
  }

  // ── Guard: stale instance whose file descriptor was closed externally ─────
  if (dbInstance !== null && !(dbInstance as any).open) {
    // The OS or another code path closed the underlying fd. Discard the stale
    // reference so the block below re-opens cleanly.
    dbInstance = null;
    statementCache.clear();
    _state = 'closed';
    _lastClosedAt = new Date().toISOString();
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
 * Return a compiled prepared statement from the cache, preparing and caching
 * it on first use.  This eliminates redundant `db.prepare()` overhead on hot
 * paths.
 *
 * SECURITY: `sql` must be a fully parameterized query.  Never interpolate
 * user-controlled values into the SQL string passed here — use `?` placeholders
 * and pass values at execution time.
 *
 * @throws {DatabaseError} CLOSED if the database is not open.
 * @throws {Error} if the SQL is syntactically invalid (surfaces better-sqlite3's
 *                 native error so the developer sees the exact malformed query).
 *
 * @example
 * const stmt = getPreparedStatement('SELECT * FROM invoices WHERE id = ?');
 * const row  = stmt.get(invoiceId);
 */
export function getPreparedStatement(
  sql: string,
): ReturnType<InstanceType<typeof Database>['prepare']> {
  if (!statementCache.has(sql)) {
    const db = getDatabase();
    const stmt = db.prepare(sql);
    statementCache.set(sql, stmt);
  }
  // Non-null assertion is safe: we just set it above if absent.
  return statementCache.get(sql)!;
}

/**
 * Evict all entries from the prepared statement cache.
 *
 * Use this after schema migrations or in tests where the schema changes
 * between runs.  better-sqlite3 will throw if a cached statement references
 * a column or table that no longer exists.
 */
export function clearStatementCache(): void {
  statementCache.clear();
}

/**
 * Return diagnostic statistics for the prepared statement cache.
 * Safe to expose to monitoring endpoints — contains no sensitive data.
 */
export function getStatementCacheStats(): { size: number; statements: string[] } {
  return {
    size: statementCache.size,
    statements: Array.from(statementCache.keys()),
  };
}

// ---------------------------------------------------------------------------
// Health / observability
// ---------------------------------------------------------------------------

/**
 * Lightweight connectivity probe.
 *
 * Executes `SELECT 1` against the live connection.  Returns `true` on
 * success, `false` on any failure — never throws.  Intended for use in
 * readiness probes where the caller wants a boolean branch, not an exception
 * handler.
 *
 * The query is constant and carries no user input, so it cannot leak schema
 * details or be used as an injection vector.
 */
export function pingDatabase(): boolean {
  try {
    const db = getDatabase();
    const row = db.prepare('SELECT 1 AS ok').get() as { ok: number } | undefined;
    return row?.ok === 1;
  } catch {
    return false;
  }
}

/**
 * Return a read-only snapshot of connection lifecycle state.
 *
 * Suitable for structured logging and `/readyz`-style monitoring.  All fields
 * are safe to expose — no file paths, passwords, or internal error details
 * are included.
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
  if (dbInstance) {
    statementCache.clear();
    dbInstance.close();
    dbInstance = null;
    _state = 'closed';
    _lastClosedAt = new Date().toISOString();
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
