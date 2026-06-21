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
 */
export function closeDatabase() {
  if (dbInstance) {
    statementCache.clear();
    dbInstance.close();
    dbInstance = null;
  }
}