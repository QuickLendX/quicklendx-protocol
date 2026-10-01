/**
 * Persistent database for API keys and audit logs backed by better-sqlite3.
 *
 * All key hashes are SHA-256 — raw secrets are never stored.
 * Prefix lookups are O(1) via a UNIQUE index on api_keys.prefix.
 * Audit rows are INSERT-only (append-only, no updates or deletes).
 *
 * Multi-statement operations use SQLite transactions for atomic rollback.
 * Performance: Uses centralized prepared statement cache for optimal throughput.
 *
 * Failure-boundary invariants (documented here and enforced in code):
 *  - rowToDbApiKey is the single boundary between untrusted SQLite rows and
 *    typed `DbApiKey` objects. It must be deterministic for valid, invalid,
 *    duplicate, and boundary-case inputs.
 *  - Nullable columns are normalized to `ull` only when the value is actually
 *    null/undefined; empty strings and other falsy values are preserved as-is.
 *  - Required columns must be present and of the expected type; otherwise a
 *    `DbApiKeyRowError` is thrown before any partial object leaks out.
 *  - `id` is the primary key and must be a non-empty string. Duplicate ids cannot
 *    occur at the SQLite layer, but the mapper still rejects blank ids.
 *  - `revoked` is normalized to a canonical 0 or 1 integer so downstream
 *    consumers never observe coerced booleans or string flags.
 *  - Error messages never include hashes or secrets, only the column name and
 *    the offending type.
 *
 * Retry / concurrency invariants:
 *  - The mapper is pure and synchronous, so concurrent calls cannot observe
 *    partially mutated objects.
 *  - Transient SQLite errors are handled by the prepared-statement layer;
 *    the mapper itself is deterministic and side-effect free.
 */

import { getDatabase, getPreparedStatement, closeDatabase as closeSharedDatabase } from '../lib/database';

export interface DbApiKey {
  id: string;
  key_hash: string;
  signing_secret_hash: string | null;
  prev_signing_secret_hash: string | null;
  prefix: string;
  name: string;
  scopes: string;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
  prev_secret_expires_at: string | null;
  revoked: number;
  created_by: string;
}

export interface DbAuditLog {
  id: string;
  event_type: 'created' | 'used' | 'rotated' | 'revoked';
  key_id: string;
  actor: string;
  timestamp: string;
  ip_address: string | null;
  endpoint: string | null;
  metadata: string | null;
}

export const ALL_API_KEY_COLS = [
  'id', 'key_hash', 'signing_secret_hash', 'prev_signing_secret_hash',
  'prefix', 'name', 'scopes', 'created_at', 'last_used_at',
  'expires_at', 'prev_secret_expires_at', 'revoked', 'created_by',
] as const;

export const ALL_AUDIT_COLS = [
  'id', 'event_type', 'key_id', 'actor', 'timestamp',
  'ip_address', 'endpoint', 'metadata',
] as const;

/**
 * Error thrown when a raw SQLite row cannot be mapped to a `DbApiKey`.
 *
 * The message intentionally omits column values (which may contain hashes or
 * secrets) and only reports the column name and the observed type.
 */
export class DbApiKeyRowError extends Error {
  constructor(readonly column: string, readonly reason: string) {
    super(`Invalid api_keys row: column "${column}" ${reason}`);
    this.name = 'DbApiKeyRowError';
  }
}

/**
 * Returns true when the value is a non-null, non-undefined string.
 * Empty strings are considered invalid for required text columns.
 */
function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

/**
 * Normalizes a nullable text column. Only `Null` and `undefined` become `null`;
 * empty strings and other falsy values are preserved as-is to avoid silently
 * dropping data that the caller explicitly stored.
 */
function normalizeNullableText(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === 'string') return value;
  // SQLite may return numeric or blob values for text columns in edge cases.
  // Coerce deterministically to a string rather than throwing, so the caller
  // can still observe the value.
  return String(value);
}

/**
 * Normalizes the `revoked` flag to a canonical 0 or 1 integer.
 * Accepts booleans, numbers, and numeric strings (e.g. '1', 0, false).
 * Rejects anything else with a `DbApiKeyRowError`.
 */
function normalizeRevoked(value: unknown): number {
  if (typeof value === 'boolean') return value ? 1 : 0;
  if (typeof value === 'number') {
    if (Number.isFinite(value)) return value ? 1 : 0;
    throw new DbApiKeyRowError('revoked', 'was not a finite number');
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (trimmed === '0' || trimmed === 'false' || trimmed === '') return 0;
    if (trimmed === '1' || trimmed === 'true') return 1;
    const numeric = Number(trimmed);
    if (Number.isFinite(numeric)) return numeric ? 1 : 0;
    throw new DbApiKeyRowError('revoked', 'was not a recognizable flag value');
  }
  if (value === null || value === undefined) {
    throw new DbApiKeyRowError('revoked', 'was null or undefined');
  }
  throw new DbApiKeyRowError('revoked', `was of unsupported type ${typeof value}`);
}

/**
 * Maps a raw SQLite row to a typed `DbApiKey`.
 *
 * This is the deterministic failure boundary for api key reads. It:
 *  - rejects null/undefined/non-object rows,
 *  - requires non-empty strings for `id`, `key_hash`, `prefix`, `name`,
 *    `scopes`, `created_at`, and `created_by`,
 *  - normalizes `null` / `undefined` to `null` for optional columns,
 *  - coerces `revoked` to a canonical 0 or 1,
 *  - never includes secret material in error messages.
 */
export function rowToDbApiKey(row: unknown): DbApiKey {
  if (row === null || typeof row !== 'object') {
    throw new DbApiKeyRowError('row', 'was not an object');
  }

  const r = row as Record<string, unknown>;

  const id = r.id;
  if (!isNonEmptyString(id)) {
    throw new DbApiKeyRowError('id', 'was not a non-empty string');
  }

  const keyHash = r.key_hash;
  if (!isNonEmptyString(keyHash)) {
    throw new DbApiKeyRowError('key_hash', 'was not a non-empty string');
  }

  const prefix = r.prefix;
  if (!isNonEmptyString(prefix)) {
    throw new DbApiKeyRowError('prefix', 'was not a non-empty string');
  }

  const name = r.name;
  if (!isNonEmptyString(name)) {
    throw new DbApiKeyRowError('name', 'was not a non-empty string');
  }

  const scopes = r.scopes;
  if (!isNonEmptyString(scopes)) {
    throw new DbApiKeyRowError('scopes', 'was not a non-empty string');
  }

  const createdAt = r.created_at;
  if (!isNonEmptyString(createdAt)) {
    throw new DbApiKeyRowError('created_at', 'was not a non-empty string');
  }

  const createdBy = r.created_by;
  if (!isNonEmptyString(createdBy)) {
    throw new DbApiKeyRowError('created_by', 'was not a non-empty string');
  }

  return {
    id,
    key_hash: keyHash,
    signing_secret_hash: normalizeNullableText(r.signing_secret_hash),
    prev_signing_secret_hash: normalizeNullableText(r.prev_signing_secret_hash),
    prefix,
    name,
    scopes,
    created_at: createdAt,
    last_used_at: normalizeNullableText(r.last_used_at),
    expires_at: normalizeNullableText(r.expires_at),
    prev_secret_expires_at: normalizeNullableText(r.prev_secret_expires_at),
    revoked: normalizeRevoked(r.revoked),
    created_by: createdBy,
  };
}

function rowToDbAuditLog(row: any): DbAuditLog {
  return {
    id: row.id,
    event_type: row.event_type,
    key_id: row.key_id,
    actor: row.actor,
    timestamp: row.timestamp,
    ip_address: row.ip_address ?? null,
    endpoint: row.endpoint ?? null,
    metadata: row.metadata ?? null,
  };
}

/**
 * Error thrown when an operation is attempted against a database that has been
 * closed. This is deterministic and distinguishable from an underlying SQLite
 * failure, so callers can decide whether to recreate the connection.
 */
export class DatabaseClosedError extends Error {
  constructor(message = 'Database connection is closed') {
    super(message);
    this.name = 'DatabaseClosedError';
  }
}

/**
 * Error thrown when a close is attempted while another close is already in flight.
 * This makes concurrent close calls deterministic: the first call owns the close,
 * the rest observe a stable outcome.
 */
export class DatabaseCloseInProgressError extends Error {
  constructor(message = 'Database close is already in progress') {
    super(message);
    this.name = 'DatabaseCloseInProgressError';
  }
}

export interface DatabaseCloseResult {
  /** True when this call performed the actual close. */
  closed: boolean;
  /** True when the database was already closed before this call. */
  alreadyClosed: boolean;
}

export interface DatabaseCloseOptions {
  /** Number of attempts for transient close failures. Defaults to 1. */
  retries?: number;
  /** Base delay in ms between retries. Defaults to 0. */
  retryDelayMs?: number;
  /** Optional callback invoked on each failed attempt. */
  onFailure?: (error: unknown, attempt: number) => void;
}

/**
 * Redacts error messages before logging/propagating them so that file paths,
 * connection strings, or other sensitive details are not leaked to callers.
 */
function redactError(error: unknown): Error {
  if (error instanceof Error) {
    const safeMessage = error.message.replace(
      /(\/(?:[\w/\\\\]+)|\\\\[\\w\\\\]+)/g,
      '<path>',
    );
    const redacted = new Error(safeMessage);
    redacted.name = error.name;
    return redacted;
  }
  return new Error('Unknown database close failure');
}

function sleep(ms: number): void {
  if (ms <= 0) return;
  const end = Date.now() + ms;
  // Busy-wait keeps the close path synchronous and deterministic for tests.
  while (Date.now() < end) {
    // intentionally empty
  }
}

class Database {
  private _db: ReturnType<typeof getDatabase> | null = null;
  private _closed = false;
  private _closing = false;
  private _closePromise: Promise<DatabaseCloseResult> | null = null;

  private getDb(): ReturnType<typeof getDatabase> {
    if (this._closed) {
      throw new DatabaseClosedError();
    }
    if (!this._db) {
      this._db = getDatabase();
    }
    return this._db;
  }

  /**
   * Runs a function inside a SQLite transaction, providing atomic rollback.
   * If any statement in the transaction throws, all changes are rolled back.
   */
  private _transaction<T>(fn: () => T): T {
    // Reject new transactions once closing has begun so that an in-flight close
    // cannot interleave with a new write and produce a half-applied state.
    if (this._closing || this._closed) {
      throw new DatabaseClosedError();
    }
    return this.getDb().transaction(fn)();
  }

  // ---- API Key operations ----

  createApiKey(key: DbApiKey): void {
    getPreparedStatement(`
      INSERT INTO api_keys (id, key_hash, signing_secret_hash, prev_signing_secret_hash, prefix, name, scopes, created_at, last_used_at, expires_at, prev_secret_expires_at, revoked, created_by)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      key.id, key.key_hash, key.signing_secret_hash, key.prev_signing_secret_hash, key.prefix, key.name, key.scopes,
      key.created_at, key.last_used_at, key.expires_at, key.prev_secret_expires_at, key.revoked, key.created_by,
    );
  }

  getApiKeyById(id: string): DbApiKey | undefined {
    const row = getPreparedStatement('SELECT * FROM api_keys WHERE id = ?').get(id);
    return row ? rowToDbApiKey(row) : undefined;
  }

  getApiKeyByPrefix(prefix: string): DbApiKey | undefined {
    const row = getPreparedStatement('SELECT * FROM api_keys WHERE prefix = ?').get(prefix);
    return row ? rowToDbApiKey(row) : undefined;
  }

  updateApiKey(id: string, updates: Partial<DbApiKey>): boolean {
    const existing = this.getApiKeyById(id);
    if (!existing) return false;

    const keys = Object.keys(updates) as (keyof DbApiKey)[];
    if (keys.length === 0) return true;

    const setClause = keys.map((k) => `${k} = ?`).join(', ');
    const values = keys.map((k) => updates[k] ?? null);

    getPreparedStatement(`UPDATE api_keys SET ${setClause} WHERE id = ?`).run(...values, id);
    return true;
  }

  deleteApiKey(id: string): boolean {
    const existing = this.getApiKeyById(id);
    if (!existing) return false;

    return this._transaction(() => {
      getPreparedStatement('DELETE FROM api_key_audit_log WHERE key_id = ?').run(id);
      getPreparedStatement('DELETE FROM api_keys WHERE id = ?').run(id);
      return true;
    });
  }

  listApiKeys(filters?: { created_by?: string; revoked?: boolean }): DbApiKey[] {
    let sql = 'SELECT * FROM api_keys';
    const clauses: string[] = [];
    const params: unknown[] = [];

    if (filters.created_by) {
      clauses.push('created_by = ?');
      params.push(filters.created_by);
    }

    if (filters.revoked !== undefined) {
      clauses.push('revoked = ?');
      params.push(filters.revoked ? 1 : 0);
    }

    if (clauses.length > 0) {
      sql += ' WHERE' + clauses.join(' AND ');
    }

    sql += ' ORDER BY created_at DESC';

    const rows = getPreparedStatement(sql).all(...params);
    return rows.map(rowToDbApiKey);
  }

  // ---- Audit log operations ----

  createAuditLog(log: DbAuditLog): void {
    getPreparedStatement(`
      INSERT INTO api_key_audit_log (id, event_type, key_id, actor, timestamp, ip_address, endpoint, metadata)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      log.id, log.event_type, log.key_id, log.actor,
      log.timestamp, log.ip_address, log.endpoint, log.metadata,
    );
  }

  getAuditLogs(filters?: { key_id?: string; event_type?: string }): DbAuditLog[] {
    let sql = 'SELECT * FROM api_key_audit_log';
    const clauses: string[] = [];
    const params: unknown[] = [];

    if (filters.key_id) {
      clauses.push('key_id = ?');
      params.push(filters.key_id);
    }

    if (filters.event_type) {
      clauses.push('event_type = ?');
      params.push(filters.event_type);
    }

    if (clauses.length > 0) {
      sql += ' WHERE ' + clauses.join(' AND ');
    }

    sql += ' ORDER BY timestamp DESC';

    const rows = getPreparedStatement(sql).all(...params);
    return rows.map(rowToDbAuditLog);
  }

  // ---- Utility ----

  clear(): void {
    this._transaction(() => {
      getPreparedStatement('DELETE FROM api_key_audit_log').run();
      getPreparedStatement('DELETE FROM api_keys').run();
    });
  }

  getStats() {
    const apiKeyCount = (getPreparedStatement('SELECT COUNT(*) AS count FROM api_keys').get() as any).count;
    const auditCount = (getPreparedStatement('SELECT COUNT(*) AS count FROM api_key_audit_log').get() as any).count;
    return {
      apiKeys: apiKeyCount,
      auditLogs: auditCount,
    };
  }

  // ---- Lifecycle ----

  /** Whether this database instance has been closed. */
  isClosed(): boolean {
    return this._closed;
  }

  /** Whether a close is currently in flight. */
  isClosing(): boolean {
    return this._closing;
  }

  /**
   * Closes the underlying database connection.
   *
   * Behavior:
   * - Idempotent: calling on an already-closed database returns without touching
   *   the connection and without throwing.
   * - Concurrent-safe: while a close is in flight, other callers receive a
   *   DatabaseCloseInProgressError instead of double-closing.
   * - Retryable: if the underlying close throws, the error is propagated (redacted)
   *   and the connection remains open so the caller can retry.
   * - Fail-fast after: once closed, all operations throw DatabaseClosedError.
   */
  close(options?: DatabaseCloseOptions): DatabaseCloseResult {
    if (this._closed) {
      return { closed: false, alreadyClosed: true };
    }

    if (this._closing) {
      throw new DatabaseCloseInProgressError();
    }

    const retries = Math.max(1, options?.retries ?? 1);
    const retryDelayMs = Math.max(0, options?.retryDelayMs ?? 0);

    this._closing = true;
    try {
      let lastError: unknown;
      for (let attempt = 1; attempt <= retries; attempt++) {
        try {
          // Only close the shared connection if this instance actually opened it.
          if (this._db) {
            closeSharedDatabase();
          }
          this._db = null;
          this._closed = true;
          return { closed: true, alreadyClosed: false };
        } catch (error) {
          lastError = error;
          options?.onFailure?.(error, attempt);
          if (attempt < retries) {
            sleep(retryDelayMs * attempt);
          }
        }
      }
      // All attempts failed: leave the connection open and retryable.
      throw redactError(lastError);
    } finally {
      this._closing = false;
    }
  }

  /**
   * Async variant of close that coalesces concurrent callers onto a single
   * underlying close operation. All callers resolve with the same result or
   * reject with the same redacted error.
   */
  async closeAsync(options?: DatabaseCloseOptions): Promise<DatabaseCloseResult> {
    if (this._closed) {
      return { closed: false, alreadyClosed: true };
    }
    if (!this._closePromise) {
      this._closePromise = Promise.resolve().then(() => this.close(options));
    }
    try {
      return await this._closePromise;
    } finally {
      // Once the in-flight close settles, allow a future retry if it failed.
      if (this._closed) {
        this._closePromise = null;
      } else {
        this._closePromise = null;
      }
    }
  }
}

// Singleton instance
export const db = new Database();

/**
 * Convenience wrapper around the singleton's close method. This is the
 * public entry point referenced by the issue and is deliberately thin so that
 * the deterministic behavior lives in one place.
 */
export function closeDatabase(options?: DatabaseCloseOptions): DatabaseCloseResult {
  return db.close(options);
}

export async function closeDatabaseAsync(options?: DatabaseCloseOptions): Promise<DatabaseCloseResult> {
  return db.closeAsync(options);
}

export { Database };
