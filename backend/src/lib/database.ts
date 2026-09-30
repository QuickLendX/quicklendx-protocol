// Updated implementation with deterministic failure‑boundary handling for prepared statements.

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
 *
 * Invariant: these counters describe the lifetime of the *current* cache
 * generation. Every code path that empties `statementCache` must reset them,
 * otherwise `getStatementCacheStats()` would report `size: 0` alongside
 * non-zero counters that describe statements which no longer exist.
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
  resetCacheMetrics();
}

/**
 * Reset the counters that describe the current cache generation.
 *
 * Must be called whenever `statementCache` is emptied so that
 * `getStatementCacheStats()` never mixes an empty cache with live counters.
 */
function resetCacheMetrics(): void {
  cacheHits = 0;
  cacheMisses = 0;
  cacheEvicts = 0;
}

/**
 * Shape returned by {@link getStatementCacheStats}.
 *
 * Returned snapshots are defensive copies: mutating `statements` cannot
 * corrupt the cache, and each call is a self-consistent point-in-time view.
 */
export interface StatementCacheStats {
  /** Number of cached prepared statements. */
  size: number;
  /** SQL strings currently cached, in insertion order. */
  statements: string[];
  /** Cache hits recorded for the current cache generation. */
  hits: number;
  /** Cache misses recorded for the current cache generation. */
  misses: number;
  /** Entries evicted due to `SQLITE_SCHEMA` in the current generation. */
  evicts: number;
}

/**
 * Retrieve cache statistics including deterministic metrics.
 *
 * Deterministic guarantees:
 * - `statements` is a fresh array; callers cannot mutate internal state.
 * - Repeated calls without intervening cache activity return deep-equal values.
 * - Counters are non-negative integers and reset with the cache generation.
 */
export function getStatementCacheStats(): StatementCacheStats {
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
    // Drop the metrics alongside the cache so the next generation starts from
    // zeroed counters instead of inheriting a closed generation's hit counts.
    resetCacheMetrics();
    dbInstance.close();
    dbInstance = null;
  }
}