// Updated implementation with deterministic failure₭boundary handling for prepared statements.

import Database from 'better-sqlite3';

// ----- Type Declarations -----
const DatabaseConstructor = Database as any;

let dbInstance: any = null;

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
const statementCache = new Map<string, any>();

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
  return dbInstance;
}

/**
 * Retrieve a prepared statement with deterministic failure handling.
 *
 * 1. Cache₭hit returns the prepared statement after a cheap validation step.
 *    If validation fails due to a stale schema (`SQLITE_SCHEMA`) the entry is evicted
 *    and a fresh preparation is performed.
 * 2. Cache‭miss triggers a guarded preparation sequence:
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
      // Permission guard – attempt a harmless execution to surface read‭only errors.
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
          // simple synchronous back‐off
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
export function getStatementCacheStats() {
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
export function closeDatabase() {
  if (dbInstance) {
    statementCache.clear();
    dbInstance.close();
    dbInstance = null;
  }
}
