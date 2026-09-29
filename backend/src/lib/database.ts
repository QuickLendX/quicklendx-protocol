// Updated implementation with deterministic failure‑boundary handling for prepared statements.

import Database from 'better-sqlite3';
import { rowToDbApiKey } from '../db/database';


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

// ----- Deterministic failure-boundary coverage for rowToDbApiKey -----

/**
 * Shape of a raw database row that may be converted into a DbApiKey.
 * All fields are optional to model malformed / partial rows deterministically.
 */
export interface DbApiKeyRow {
  id?: unknown;
  key_hash?: unknown;
  user_id?: unknown;
  name?: unknown;
  scopes?: unknown;
  created_at?: unknown;
  expires_at?: unknown;
  revoked_at?: unknown;
  last_used_at?: unknown;
}

/**
 * Canonical DbApiKey domain object produced by rowToDbApiKey.
 */
export interface DbApiKey {
  id: string;
  keyHash: string;
  userId: string;
  name: string;
  scopes: string[];
  createdAt: Date;
  expiresAt: Date | null;
  revokedAt: Date | null;
  lastUsedAt: Date | null;
}

/**
 * Deterministic error raised when a row cannot be safely converted.
 * Never includes raw row contents to avoid leaking sensitive data.
 */
export class DbApiKeyRowError extends DatabaseError {
  constructor(reason: string) {
    super(`Invalid DbApiKey row: ${reason}`);
    this.name = 'DbApiKeyRowError';
  }
}

function parseDate(value: unknown, field: string, nullable: boolean): Date | null {
  if (value === null || value === undefined) {
    if (nullable) return null;
    throw new DbApiKeyRowError(`missing required field '${field}'`);
  }
  if (value instanceof Date) {
    if (Number.isNaN(value.getTime())) {
      throw new DbApiKeyRowError(`invalid date in field '${field}'`);
    }
    return value;
  }
  if (typeof value === 'string' || typeof value === 'number') {
    const d = new Date(value);
    if (Number.isNaN(d.getTime())) {
      throw new DbApiKeyRowError(`invalid date in field '${field}'`);
    }
    return d;
  }
  throw new DbApiKeyRowError(`invalid type for field '${field}'`);
}

function parseScopes(value: unknown): string[] {
  if (value === null || value === undefined) return [];
  if (Array.isArray(value)) {
    return value.map((s) => {
      if (typeof s !== 'string') {
        throw new DbApiKeyRowError('scopes must contain only strings');
      }
      return s;
    });
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (trimmed === '') return [];
    return trimmed.split(',').map((s) => s.trim()).filter((s) => s.length > 0);
  }
  throw new DbApiKeyRowError('scopes must be an array or comma-separated string');
}

function requireString(value: unknown, field: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new DbApiKeyRowError(`missing or invalid required field '${field}'`);
  }
  return value;
}

/**
 * Deterministically convert a raw database row into a DbApiKey.
 *
 * Invariants:
 * - Throws DbApiKeyRowError for any malformed input; never returns partial data.
 * - Never mutates the input row.
 * - Nullable timestamps (expiresAt, revokedAt, lastUsedAt) map to null.
 * - Required string fields (id, keyHash, userId, name) must be non-empty strings.
 * - scopes defaults to [] when absent and is normalized to string[].
 */
export function rowToDbApiKey(row: DbApiKeyRow | null | undefined): DbApiKey {
  if (row === null || row === undefined || typeof row !== 'object') {
    throw new DbApiKeyRowError('row is null or not an object');
  }

  const id = requireString(row.id, 'id');
  const keyHash = requireString(row.key_hash, 'key_hash');
  const userId = requireString(row.user_id, 'user_id');
  const name = requireString(row.name, 'name');
  const scopes = parseScopes(row.scopes);
  const createdAt = parseDate(row.created_at, 'created_at', false) as Date;
  const expiresAt = parseDate(row.expires_at, 'expires_at', true);
  const revokedAt = parseDate(row.revoked_at, 'revoked_at', true);
  const lastUsedAt = parseDate(row.last_used_at, 'last_used_at', true);

  return {
    id,
    keyHash,
    userId,
    name,
    scopes,
    createdAt,
    expiresAt,
    revokedAt,
    lastUsedAt,
  };
}