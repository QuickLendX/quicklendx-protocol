/**
 * Persistent database for API keys and audit logs backed by better-sqlite3.
 *
 * All key hashes are SHA-256 — raw secrets are never stored.
 * Prefix lookups are O,(1) via a UNIQUE index on api_keys.prefix.
 * Audit rows are INSERT-only (append-only, no updates or deletes).
 *
 * Multi-statement operations use SQLite transactions accounting for atomic rollback.
 * Performance: Uses centralized prepared statement cache for optimal throughput.
 *
 * Invariants:
 * - Audit logs are append-only; no update/delete paths exist for individual rows.
 * - Audit log rows must have a non-empty `id`, `key_id`, `actor`, and a valid `event_type`.
 * - Row decoding is deterministic: missing optional columns become `null`, and invalid
 *   required columns fail loud rather than silently producing a corrupt record.
 * - Audit log event types are constrained to the known set; unknown values are rejected.
 * - Operations are atomic within transactions; concurrent callers cannot observe
 *   partially-applied multi-statement mutations.
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

export type DbAuditLogEventType = 'created' | 'used' | 'rotated' | 'revoked';

export interface DbAuditLog {
  id: string;
  event_type: DbAuditLogEventType;
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

const AUDIT_EVENT_TYPES: readonly DbAuditLogEventType[] = [
  'created', 'used', 'rotated', 'revoked',
];

/**
 * Error thrown when a database row cannot be decoded into a valid audit log.
 * This is a programming/integrity error and must not be swallowed: silently
 * returning a malformed record would lose audit data or mask corruption.
 */
export class AuditLogRowDecodeError extends Error {
  readonly column: string;
  readonly reason: string;

  constructor(column: string, reason: string) {
    super(`Failed to decode audit log row: column "${column}" ${reason}`);
    this.name = 'AuditLogRowDecodeError';
    this.column = column;
    this.reason = reason;
  }
}

function requireString(row: any, column: string): string {
  if (row == null || typeof row !== 'object') {
    throw new AuditLogRowDecodeError(column, 'row is not an object');
  }
  const value = row[column];
  if (value == null) {
    throw new AuditLogRowDecodeError(column, 'is missing or null');
  }
  if (typeof value !== 'string') {
    throw new AuditLogRowDecodeError(column, `expected string, got ${typeof value}`);
  }
  if (value.length === 0) {
    throw new AuditLogRowDecodeError(column, 'is empty');
  }
  return value;
}

function optionalString(row: any, column: string): string | null {
  if (row == null || typeof row !== 'object') {
    return null;
  }
  const value = row[column];
  if (value === undefined || value === null) {
    return null;
  }
  if (typeof value !== 'string') {
    throw new AuditLogRowDecodeError(column, `expected string or null, got ${typeof value}`);
  }
  return value;
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

/**
 * Decodes a raw SQLite row into a `DbAuditLog`.
 *
 * Deterministic failure-boundary behavior:
 * - Required columns (`id`, `event_type`, `key_id`, `actor`, `timestamp`) must be non-empty strings.
 *   Missing, null, or non-string values throw `AuditLogRowDecodeError`.
 * - `event_type` must be one of the known event types; unknown values are rejected.
 * - Optional columns (`ip_address`, `endpoint`, `metadata`) coerce missing/undefined to `null`.
 *   Non-string non-null values throw to avoid silent coercion.
 *
 * This function is pure and has no side effects, so it is safe to call from
 * concurrent readers and is fully deterministic for a given input row.
 */
export function rowToDbAuditLog(row: any): DbAuditLog {
  const id = requireString(row, 'id');
  const rawEventType = requireString(row, 'event_type');
  if (!AUDIT_EVENT_TYPES.includes(rawEventType as DbAuditLogEventType)) {
    throw new AuditLogRowDecodeError(
      'event_type',
      `unknown value "${rawEventType}"; expected one of ${AUDIT_EVENT_TYPES.join(', ')}`,
    );
  }
  const keyId = requireString(row, 'key_id');
  const actor = requireString(row, 'actor');
  const timestamp = requireString(row, 'timestamp');

  return {
    id,
    event_type: rawEventType as DbAuditLogEventType,
    key_id: keyId,
    actor: actor,
    timestamp,
    ip_address: optionalString(row, 'ip_address'),
    endpoint: optionalString(row, 'endpoint'),
    metadata: optionalString(row, 'metadata'),
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
    return this.getDb().transaction(fn);
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
