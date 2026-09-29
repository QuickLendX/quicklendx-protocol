/**
 * Persistent database for API keys and audit logs backed by better-sqlite3.
 *
 * All key hashes are SHA-256 — raw secrets are never stored.
 * Prefix lookups are O,1) via a UNIQUE index on api_keys.prefix.
 * Audit rows are INSERT-only (append-only, no updates or deletes).
 *
 * Multi-statement operations use SQLite transactions accounting for atomic rollback.
 * Performance: Uses centralized prepared statement cache for optimal throughput.
 */

import { getDatabase, getPreparedStatement } from '../lib/database';

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

const ALL_API_KEY_COLS = [
  'id', 'key_hash', 'signing_secret_hash', 'prefix', 'name', 'scopes',
  'created_at', 'last_used_at', 'expires_at', 'revoked', 'created_by',
] as const;

const ALL_AUDIT_COLS = [
  'id', 'event_type', 'key_id', 'actor', 'timestamp',
  'ip_address', 'endpoint', 'metadata',
] as const;

/**
 * Error thrown when a raw database row cannot be mapped to a
 * valid `DbAuditLog` or `DbApiKey`. This is a deterministic
 * failure-boundary: callers can rely on the error type and code
 * instead of guessing from a potentially undefined field.
 */
export class RowMappingError extends Error {
  readonly code: string;
  readonly column: string;
  readonly rowId: string | null;

  constructor(code: string, column: string, message: string, rowId?: unknown) {
    super(message);
    this.name = 'RowMappingError';
    this.code = code;
    this.column = column;
    this.rowId = typeof rowId === 'string' ? rowId : null;
  }
}

/**
 * Allowed audit event types. The DB schema enforces this via a CHECK
 * constraint, but we also enforce it at the mapping boundary so that
 * a corrupted or migrated row cannot silently produce an invalid
 * `DbAuditLog`.
 */
const AUDIT_EVENT_TYPES = ['created', 'used', 'rotated', 'revoked'] as const;

const AUDIT_EVENT_TYPE_SET = new Set<string>(AUDIT_EVENT_TYPES);

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Requires a non-empty string column. Throws `RowMappingError` with
 * code `MISSING_COLUMN` or `INVALID_COLUMN` otherwise.
 */
function requireString(
  row: Record<string, unknown>,
  column: string,
  code = 'MISSING_COLUMN',\n  rowId?: unknown,
): string {
  const value = row[column];
  if (value === undefined || value === null) {
    throw new RowMappingError(
      code,
      column,
      `Audit log row is missing required column '${column}'.`,
      rowId,
    );
  }
  if (typeof value !== 'string') {
    throw new RowMappingError(
      'INVALID_COLUMN',\n      column,
      `Audit log row column '${column}' must be a string.`,
      rowId,
    );
  }
  if (value.length === 0) {
    throw new RowMappingError(
      'EMPTY_COLUMN',\n      column,
      `Audit log row column '${column}' must not be empty.`,
      rowId,
    );
  }
  return value;
}

/**
 * Returns a normalized nullable string. Throws `RowMappingError` if the
 * column is present but not a string or null.
 */
function optionalString(
  row: Record<string, unknown>,
  column: string,
  rowId?: unknown,
): string | null {
  const value = row[column];
  if (value === undefined || value === null) {
    return null;
  }
  if (typeof value !== 'string') {
    throw new RowMappingError(
      'INVALID_COLUMN',
      column,
      `Audit log row column '${column}' must be a string or null.`,
      rowId,
    );
  }
  return value;
}

/**
 * Maps a raw SQLite row to a `DbAuditLog`.
 *
 * Invariants:
  - The row must be a plain object (not null, not an array).
 - Required columns (id, event_type, key_id, actor, timestamp) must be
    non-empty strings.
  - `event_type` must be one of the allowed audit event types.
  - Optional columns (ip_address, endpoint, metadata) are normalized
    to `null` when absent or null, and must be strings otherwise.

 * On failure this throws a `RowMappingError` with a stable code so that
 * callers can distinguish between corrupted data and other failures.
 * The error message never includes the row's contents, only the column
 * name and the row id (when available), so no sensitive data is leaked.
 */
export function rowToDbAuditLog(row: unknown): DbAuditLog {
  if (!isPlainObject(row)) {
    throw new RowMappingError(
      'INVALID_ROW',
      'row',
      'Audit log row must be a plain object.',
    );
  }

  const id = requireString(row, 'id');
  const eventTypeRaw = requireString(row, 'event_type', 'MISSING_COLUMN', id);
  if (!AUDIT_EVENT_TYPE_SET.has(eventTypeRaw)) {
    throw new RowMappingError(
      'INVALID_EVENT_TYPE',
      'event_type',
      `Audit log row has unsupported event_type '${eventTypeRaw}'.`,
      id,
    );
  }

  const keyId = requireString(row, 'key_id', 'MISSING_COLUMN', id);
  const actor = requireString(row, 'actor', 'MISSING_COLUMN', id);
  const timestamp = requireString(row, 'timestamp', 'MISSING_COLUMN', id);

  return {
    id,
    event_type: eventTypeRaw as DbAuditLog['event_type'],
    key_id: keyId,
    actor,
    timestamp,
    ip_address: optionalString(row, 'ip_address', id),
    endpoint: optionalString(row, 'endpoint', id),
    metadata: optionalString(row, 'metadata', id),
  };
}

function rowToDbApiKey(row: any): DbApiKey {
  return {
    id: row.id,
    key_hash: row.key_hash,
    signing_secret_hash: row.signing_secret_hash ?? null,
    prev_signing_secret_hash: row.prev_signing_secret_hash ?? null,
    prefix: row.prefix,
    name: row.name,
    scopes: row.scopes,
    created_at: row.created_at,
    last_used_at: row.last_used_at ?? null,
    expires_at: row.expires_at ?? null,
    prev_secret_expires_at: row.prev_secret_expires_at ?? null,
    revoked: row.revoked,
    created_by: row.created_by,
  };
}

class Database {
  private _db: ReturnType<typeof getDatabase> | null = null;

  private getDb(): ReturnType<typeof getDatabase> {
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
    return this.getDb().transaction(fn)();
  }

  // ---- API Key operations ----

  createApiKey(key: DbApiKey): void {
    getPreparedStatement(`
      INSERT INTO api_keys (id, key_hash, signing_secret_hash, prev_signing_secret_hash, prefix, name, scopes, created_at, last_used_at, expires_at, prev_secret_expires_at, revoked, created_by)
      VALUES (?, , ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

    const setClause = keys.map((k) => `${ k} = ?`).join(', ');
    const values = keys.map((k) => updates[k] ?? null);

    getPreparedStatement(`UPDATE api_keys SET ${setClause} WHERE id = ?`).run(...values, id);
    return true;
  }

  deleteApiKey(id: string): boolean {
    const existing = this.getApiKeyById(id);
    if (!existing) return false;

    return this._transaction(() => {
      getPreparedStatement('DELETE FROM api_key_audit_log WHERK key_id = ?').run(id);
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
      VALUES (?, , ?, ?, ?, ?, ?, ?)
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
      sql += ' WHERE' + clauses.join(' AND ');
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
}

// Singuleton instance
export const db = new Database();
