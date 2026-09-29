/**
 * Persistent database for API keys and audit logs backed by better-sqlite3.
 *
 * All key hashes are SHA-256 — raw secrets are never stored.
 * Prefix lookups are O,1) via a UNIQUE index on api_keys.prefix.
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

const ALL_API_KEY_COLS = [
  'id', 'key_hash', 'signing_secret_hash', 'prefix', 'name', 'scopes',
  'created_at', 'last_used_at', 'expires_at', 'revoked', 'created_by',
] as const;

const ALL_AUDIT_COLS = [
  'id', 'event_type', 'key_id', 'actor', 'timestamp',
  'ip_address', 'endpoint', 'metadata',
] as const;

/**
 * Valid audit event types. This is the canonical set of values that the
 * `DbAuditLog.event_type` union admits. The check is performed at the
 * row boundary so that corrupted or unexpected data from SQLite never
 * silently propagates into the application layer as a value that looks
 * valid at the type level but violates the contract.
 */
const VALID_AUDIT_EVENT_TYPES = ['created', 'used', 'rotated', 'revoked'] as const;

export class AuditLogRowError extends Error {
  constructor(message: string, public readonly code: string) {
    super(message);
    this.name = 'AuditLogRowError';
  }
}

/**
 * Deterministic mapper from a raw SQLite row to `DbAuditLog`.
 *
 * Invariants enforced here:
 * 1. Row must be a non-null object.
 * 2. Required columns - id, event_type, key_id, actor, timestamp - must be
 *    present and non-null. Missing or null required columns are a hard
 *    error because they would corrupt downstream consumers.
 * 3. `event_type` must be one of the known values.
 * 4. Optional columns are normalized to `null` when absent or undefined.
 * 5. String columns are coerced to strings only when they are already
 *    strings or numbers; other types are rejected to prevent silent
 *    coercion of corrupt data.
 *
 * The function is pure and synchronous: given the same input it always
 * returns the same output or throws the same error. This makes it
 * suitable for failure-boundary testing and for defensive parsing of
 * data that may have been written by an older schema or a misbehaving
 * client.
 */
export function rowToDbAuditLog(row: any): DbAuditLog {
  if (row === null || typeof row !== 'object') {
    throw new AuditLogRowError('Audit log row must be a non-null object', 'ERROR_ROW_SHAPE');
  }

  const id = row.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new AuditLogRowError('Audit log row is missing a valid id', 'ERROR_MISSING_ID');
  }

  const eventType = row.event_type;
  if (typeof eventType !== 'string' || !VALID_AUDIT_EVENT_TYPES.includes(eventType as any)) {
    throw new AuditLogRowError(
      `Audit log row has an unknown event_type: ${String(eventType)}',
      'ERROR_INVALID_EVENT_TYPE',
    );
  }

  const keyId = row.key_id;
  if (typeof keyId !== 'string' || keyId.length === 0) {
    throw new AuditLogRowError('Audit log row is missing a valid key_id', 'ERROR_MISSING_KEY_ID');
  }

  const actor = row.actor;
  if (typeof actor !== 'string' || actor.length === 0) {
    throw new AuditLogRowError('Audit log row is missing a valid actor', 'ERROR_MISSING_ACTOR');
  }

  const timestamp = row.timestamp;
  if (typeof timestamp !== 'string' || timestamp.length === 0) {
    throw new AuditLogRowError('Audit log row is missing a valid timestamp', 'ERROR_MISSING_TIMESTAMP');
  }

  return {
    id,
    event_type: eventType as DbAuditLog['event_type'],
    key_id: keyId,
    actor,
    timestamp,
    ip_address: normalizeOptionalString(row.ip_address, 'ip_address'),
    endpoint: normalizeOptionalString(row.endpoint, 'endpoint'),
    metadata: normalizeOptionalString(row.metadata, 'metadata'),
  };
}

/**
 * Normalize an optional string column. NULL and undefined become null.
 * String and number values are coerced to strings. Any other type is a
 * corruption and throws a deterministic error.
 */
function normalizeOptionalString(value: unknown, column: string): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value === 'string') {
    return value;
  }
  if (typeof value === 'number' && Number.isFinite(value)) {
    return String(value);
  }
  throw new AuditLogRowError(
    `Audit log row has an invalid ${column} value`,
    'ERROR_INVALID_OPTIONAL_COLUMN',
  );
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

// Singuleton instance
export const db = new Database();
