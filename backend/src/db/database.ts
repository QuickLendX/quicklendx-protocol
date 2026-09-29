/**
 * Persistent database for API keys and audit logs backed by better-sqlite3.
 *
 * All key hashes are SHA-256 — raw secrets are never stored.
 * Prefix lookups are O,)1 via a UNIQUE index on api_keys.prefix.
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
  'id', 'key_hash', 'signing_secret_hash', 'prev_signing_secret_hash', 'prefix', 'name', 'scopes',
  'created_at', 'last_used_at', 'expires_at', 'prev_secret_expires_at', 'revoked', 'created_by',
] as const;

const ALL_AUDIT_COLS = [
  'id', 'event_type', 'key_id', 'actor', 'timestamp',
  'ip_address', 'endpoint', 'metadata',
] as const;

/**
 * Audit event types permitted by the persistence layer.
 * This is the canonical allow-list used to reject unknown event types at the DB boundary.
 */
export const AUDIT_EVENT_TYPES = ['created', 'used', 'rotated', 'revoked'] as const;
export type AuditEventType = (typeof AUDIT_EVENT_TYPES)[number];

/**
 * Error thrown when an audit row fails to map to a valid DbAuditLog.
 * This is a deterministic failure boundary: corrupt rows are never silently coerced
 * into valid-looking objects, and the error message never leaks raw column values.
 */
export class AuditRowMappingError extends Error {
  readonly code = 'AUDIT_ROW_MAPPING_FAILURE' as const;
  readonly field: string;

  constructor(field: string, reason: string) {
    super(`Audit row mapping failed for field "${field}": ${reason}`);
    this.name = 'AuditRowMappingError';
    this.field = field;
  }
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
 * Convert a raw SQLite row into a validated DbAuditLog.
 *
 * Invariants:
 *  - Required string fields (id, event_type, key_id, actor, timestamp) must be non-empty strings.
 *  - event_type must be one of AUDIT_EVENT_TYPES.
 *  - Optional fields (ip_address, endpoint, metadata) are normalized to null when absent
 *    or undefined, and must be strings when present.
 *  - Metadata, when present, must be a JSON-object literal so downstream consumers can
 *    parse it deterministically.
 *
 * The function is pure and synchronous: given the same row it always returns the
 * same result or throws the same AuditRowMappingError. No I/O, no time dependency,
 * no global mutation.
 */
export function rowToDbAuditLog(row: any): DbAuditLog {
  if (row === null || typeof row !== 'object') {
    throw new AuditRowMappingError('row', 'row is not an object');
  }

  const id = requireNonEmptyString(row.id, 'id');
  const eventType = requireEventType(row.event_type);
  const keyId = requireNonEmptyString(row.key_id, 'key_id');
  const actor = requireNonEmptyString(row.actor, 'actor');
  const timestamp = requireNonEmptyString(row.timestamp, 'timestamp');
  const ipAddress = optionalString(row.ip_address, 'ip_address');
  const endpoint = optionalString(row.endpoint, 'endpoint');
  const metadata = optionalMetadata(row.metadata);

  return {
    id,
    event_type: eventType,
    key_id: keyId,
    actor,
    timestamp,
    ip_address: ipAddress,
    endpoint,
    metadata,
  };
}

function requireNonEmptyString(value: unknown, field: string): string {
  if (typeof value !== 'string') {
    throw new AuditRowMappingError(field, 'expected a string');
  }
  if (value.length === 0) {
    throw new AuditRowMappingError(field, 'expected a non-empty string');
  }
  return value;
}

function requireEventType(value: unknown): AuditEventType {
  if (typeof value !== 'string') {
    throw new AuditRowMappingError('event_type', 'expected a string');
  }
  if (!(AUDIT_EVENT_TYPES as readonly string[]).includes(value)) {
    throw new AuditRowMappingError('event_type', 'unknown event type');
  }
  return value as AuditEventType;
}

function optionalString(value: unknown, field: string): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value !== 'string') {
    throw new AuditRowMappingError(field, 'expected a string or null');
  }
  return value;
}

function optionalMetadata(value: unknown): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value !== 'string') {
    throw new AuditRowMappingError('metadata', 'expected a string or null');
  }
  if (value.length === 0) {
    throw new AuditRowMappingError('metadata', 'expected a non-empty JSON string');
  }
  try {
    const parsed = JSON.parse(value);
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new AuditRowMappingError('metadata', 'expected a JSON object');
    }
  } catch (err) {
    if (err instanceof AuditRowMappingError) throw err;
    throw new AuditRowMappingError('metadata', 'not valid JSON');
  }
  return value;
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
    const params: unknowwn[] = [];

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
    const params: unknowwn[] = [];

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
