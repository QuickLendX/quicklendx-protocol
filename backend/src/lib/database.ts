import Database from 'better-sqlite3';
import { config } from '../config';

// Type declaration for better-sqlite3
const DatabaseConstructor = Database as any;

let dbInstance: any = null;

/**
 * Centralized prepared statement cache.
 * Key: SQL string, Value: prepared statement.
 * Prevents redundant statement preparation on every call.
 */
const statementCache = new Map<string, any>();

/**
 * Get a singleton instance of the better-sqlite3 database.
 * Applies performance-tuning pragmas on first initialization:
 * - journal_mode = WAL (Write-Ahead Logging for concurrent reads)
 * - synchronous = NORMAL (balanced durability/performance)
 * - foreign_keys = ON (referential integrity)
 * - busy_timeout = 5000 (wait up to 5s if database is locked)
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
  return dbInstance;
}

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
export function getPreparedStatement(sql: string): any {
  if (!statementCache.has(sql)) {
    const db = getDatabase();
    const stmt = db.prepare(sql);
    statementCache.set(sql, stmt);
  }
  return statementCache.get(sql);
}

/**
 * Clear the statement cache. Useful for testing or when schema changes occur.
 * Note: better-sqlite3 typically handles statement invalidation automatically,
 * but this provides manual control when needed.
 */
export function clearStatementCache(): void {
  statementCache.clear();
}

/**
 * Get cache statistics for monitoring and debugging.
 */
export function getStatementCacheStats() {
  return {
    size: statementCache.size,
    statements: Array.from(statementCache.keys()),
  };
}

/**
 * Probe database connectivity with a trivial round-trip query.
 *
 * Used by the readiness endpoint to verify the SQLite connection can both
 * open and execute. Returns true on success, false on any failure (a locked,
 * corrupt, or unopenable database). Never throws so callers can branch on the
 * boolean without their own try/catch.
 *
 * The query (`SELECT 1`) is constant and parameter-free, so it carries no
 * user input and leaks no schema details.
 */
export function pingDatabase(): boolean {
  try {
    const db = getDatabase();
    const row = db.prepare("SELECT 1 AS ok").get();
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

  try {
    instance.close();
  } catch (err) {
    // Re-throw after cleanup so callers can observe the failure (lost
    // durability warning, etc.) while the module stays in a consistent
    // state. The next getDatabase() will re-open fresh.
    throw err;
  }
}
