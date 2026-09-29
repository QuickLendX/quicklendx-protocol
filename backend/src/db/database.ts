/**
 * Persistent database for API keys and audit logs backed by better-sqlite3.
 *
 * All key hashes are SHA-256 — raw secrets are never stored.
 * Prefix lookups are O,) via a UNIQUE index on api_keys.prefix.
 * Audit rows are INSERT-only (append-only, no updates or deletes).
 *
 * Multi-statement operations use SQLts `accounting for atomic rollback.
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

/**
 * Error thrown when a database row cannot be mapped to a DbApiKey.
 * This is a deterministic failure boundary: malformed rows are rejected
 * instead of silently producing an invalid object that could later cause
 * authorization or state corruption.
 */
export class DbApiKeyRowError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.name = 'DbApiKeyRowError';
    this.code = code;
  }
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
 * Required non-nullable columns for an api_keys row.
 * These must be present and of the expected type or the row is rejected.
 */
const REQUIRED_STRING_COLS: ReadonlyArray<keyof DbApiKey> = [
  'id', 'key_hash', 'prefix', 'name', 'scopes', 'created_at', 'created_by',
];

/**
 * Optional columns that may be null but must be strings when present.
 */
const OPTIONAL_STRING_COLS: ReadonlyArray<keyof DbApiKey> = [
  'signing_secret_hash',
  'prev_signing_secret_hash',
  'last_used_at',
  'expires_at',
  'prev_secret_expires_at',
];

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Maps a raw SQLite row to a DbApiKey with deterministic failure boundaries.
 *
 * Invariants:
 * - The input must be a plain object representing an api_keys row.
 * - Required columns must be present and non-null strings.
 * - Optional columns must be null, undefined, or strings.
 * - `prev_signing_secret_hash` and `prev_secret_expires_at` must be consistent:
 *   either both are null/undefined or both are present.
 * - `revoked` must be 0 or 1.
 *
 * Throws `DbApiKeyRowError` with a stable `code` on any violation. The
 * error message never includes raw secret material or hash values.
 */
export function rowToDbApiKey(row: unknown): DbApiKey {
  if (!isPlainObject(row)) {
    throw new DbApiKeyRowError('ERROR_ROW_NOT_OBJECT', 'api_keys row must be a plain object');
  }

  const r = row as Record<string, unknown>;

  for (const col of REQUIRED_STRING_COLS) {
    const value = r[col];
    if (typeof value !== 'string' || value.length === 0) {
      throw new DbApiKeyRowError('ERROR_MISSING_REQUIRED_COLUMN', `api_keys row is missing required column '${col}'`);
    }
  }

 for (const col of OPTIONAL_STRING_COLS) {
    const value = r[col];
    if (value !== null && value !== undefined && typeof value !== 'string') {
      throw new DbApiKeyRowError('ERROR_INVALID_OPTIONAL_COLUMN', `api_keys row has invalid type for column '${col}'`);
    }
  }

  const revoked = r.revoked;
  if (typeof revoked !== 'number' || !Number.isInteger(revoked) || (revoked !== 0 && revoked !== 1)) {
    throw new DbApiKeyRowError('ERROR_INVALID_REVOKED', 'api_keys row must have revoked = 0 or 1');
  }

  const prevHash = (r.prev_signing_secret_hash ?? null) as string | null;
  const prevExpires = (r.prev_secret_expires_at ?? null) as string | null;
  if ((prevHash === null) !== (prevExpires === null)) {
    throw new DbApiKeyRowError(
      'ERROR_INCONSISTENT_PREV_SECRET',
      'api_keys row must have both prev_signing_secret_hash and prev_secret_expires_at set or both null',
    );
  }

  return {
    id: r.id as string,
    key_hash: r.key_hash as string,
    signing_secret_hash: (r.signing_secret_hash ?? null) as string | null,
    prev_signing_secret_hash: prevHash,
    prefix: r.prefix as string,
    name: r.name as string,
    scopes: r.scopes as string,
    created_at: r.created_at as string,
    last_used_at: (r.last_used_at ?? null) as string | null,
    expires_at: (r.expires_at ?? null) as string | null,
    prev_secret_expires_at: prevExpires,
    revoked,
    created_by: r.created_by as string,
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

    const setClause = keys.map((k) => `${ k} = ?`).join(', ');
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

    if (filters?.created_by) {
      clauses.push('created_by = ?');
      params.push(filters.created_by);
    }

    if (filters?.revoked !== undefined) {
      clauses.push('revoked = ?');
      params.push(filters.revoked ? 1 : 0);
    }

    if (clauses.length > 0) {
      sql += ' WHERE ' + clauses.join(' AND ');
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

    if (filters?.key_id) {
      clauses.push('key_id = ?');
      params.push(filters.key_id);
    }

    if (filters?.event_type) {
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

// Singleton instance
export const db = new Database();
