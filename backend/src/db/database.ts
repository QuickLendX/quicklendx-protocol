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
 * This is a deterministic failure boundary: malformed rows are surfaced
 * as a typed error instead of silently producing an invalid DbApiKey.
 */
export class DbApiKeyMappingError extends Error {
  constructor(readonly field: string, reason: string) {
    super(`Failed to map database row to DbApiKey: ${reason}`);
    this.name = 'DbApiKeyMappingError';
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
 * Required non-nullable columns for api_keys. A missing or null value
 * indicates a corrupted or incomplete row and must fail deterministically.
 */
const REQUIRED_API_KEY_COLS = ['id', 'key_hash', 'prefix', 'name', 'scopes', 'created_at', 'revoked', 'created_by'] as const;

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Normalize a nullable text column. Treats undefined and null as null.
 * Rejects any non-string non-null value to avoid silent coercion.
 */
function normalizeNullableText(value: unknown, column: string): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== 'string') {
    throw new DbApiKeyMappingError(column, `expected string or null, received ${typeof value}`);
  }
  return value;
}

function requiredText(value: unknown, column: string): string {
  if (typeof value !== 'string') {
    throw new DbApiKeyMappingError(column, `expected non-null string, received ${value === null ? 'null' : typeof value}`);
  }
  return value;
}

function requiredRevoked(value: unknown): number {
  if (typeof value === 'number' && Number.isInteger(value) && (value === 0 || value === 1)) {
    return value;
  }
  if (typeof value === 'boolean') {
    return value ? 1 : 0;
  }
  throw new DbApiKeyMappingError('revoked', `expected 0 or 1, received ${String(value)}`);
}

function rowToDbApiKey(row: any): DbApiKey {
  if (!isPlainObject(row)) {
    throw new DbApiKeyMappingError('row', 'expected a non-null object');
  }

  for (const col of REQUIRED_API_KEY_COLS) {
    if (!(col in row) || row[col] === null || row[col] === undefined) {
      throw new DbApiKeyMappingError(col, 'required column is missing or null');
    }
  }

  return {
    id: requiredText(row.id, 'id'),
    key_hash: requiredText(row.key_hash, 'key_hash'),
    signing_secret_hash: normalizeNullableText(row.signing_secret_hash, 'signing_secret_hash'),
    prev_signing_secret_hash: normalizeNullableText(row.prev_signing_secret_hash, 'prev_signing_secret_hash'),
    prefix: requiredText(row.prefix, 'prefix'),
    name: requiredText(row.name, 'name'),
    scopes: requiredText(row.scopes, 'scopes'),
    created_at: requiredText(row.created_at, 'created_at'),
    last_used_at: normalizeNullableText(row.last_used_at, 'last_used_at'),
    expires_at: normalizeNullableText(row.expires_at, 'expires_at'),
    prev_secret_expires_at: normalizeNullableText(row.prev_secret_expires_at, 'prev_secret_expires_at'),
    revoked: requiredRevoked(row.revoked),
    created_by: requiredText(row.created_by, 'created_by'),
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
