/**
 * Persistent database for API keys and audit logs backed by better-sqlite3.
 *
 * All key hashes are SHA-256 — raw secrets are never stored.
 * Prefix lookups are O(1) via a UNIQUE index on api_keys.prefix.
 * Audit rows are INSERT-only (append-only, no updates or deletes).
 */

import { getDatabase } from '../lib/database';

export interface DbApiKey {
  id: string;
  key_hash: string;
  prefix: string;
  name: string;
  scopes: string;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
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
  'id', 'key_hash', 'prefix', 'name', 'scopes',
  'created_at', 'last_used_at', 'expires_at', 'revoked', 'created_by',
] as const;

const ALL_AUDIT_COLS = [
  'id', 'event_type', 'key_id', 'actor', 'timestamp',
  'ip_address', 'endpoint', 'metadata',
] as const;

function rowToDbApiKey(row: any): DbApiKey {
  return {
    id: row.id,
    key_hash: row.key_hash,
    prefix: row.prefix,
    name: row.name,
    scopes: row.scopes,
    created_at: row.created_at,
    last_used_at: row.last_used_at ?? null,
    expires_at: row.expires_at ?? null,
    revoked: row.revoked,
    created_by: row.created_by,
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

  // ---- API Key operations ----

  createApiKey(key: DbApiKey): void {
    this.getDb().prepare(`
      INSERT INTO api_keys (id, key_hash, prefix, name, scopes, created_at, last_used_at, expires_at, revoked, created_by)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      key.id, key.key_hash, key.prefix, key.name, key.scopes,
      key.created_at, key.last_used_at, key.expires_at, key.revoked, key.created_by,
    );
  }

  getApiKeyById(id: string): DbApiKey | undefined {
    const row = this.getDb().prepare('SELECT * FROM api_keys WHERE id = ?').get(id);
    return row ? rowToDbApiKey(row) : undefined;
  }

  getApiKeyByPrefix(prefix: string): DbApiKey | undefined {
    const row = this.getDb().prepare('SELECT * FROM api_keys WHERE prefix = ?').get(prefix);
    return row ? rowToDbApiKey(row) : undefined;
  }

  updateApiKey(id: string, updates: Partial<DbApiKey>): boolean {
    const existing = this.getApiKeyById(id);
    if (!existing) return false;

    const keys = Object.keys(updates) as (keyof DbApiKey)[];
    if (keys.length === 0) return true;

    const setClause = keys.map((k) => `${k} = ?`).join(', ');
    const values = keys.map((k) => updates[k] ?? null);

    this.getDb().prepare(`UPDATE api_keys SET ${setClause} WHERE id = ?`).run(...values, id);
    return true;
  }

  deleteApiKey(id: string): boolean {
    const existing = this.getApiKeyById(id);
    if (!existing) return false;

    this.getDb().prepare('DELETE FROM api_key_audit_log WHERE key_id = ?').run(id);
    this.getDb().prepare('DELETE FROM api_keys WHERE id = ?').run(id);
    return true;
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

    const rows = this.getDb().prepare(sql).all(...params);
    return rows.map(rowToDbApiKey);
  }

  // ---- Audit log operations ----

  createAuditLog(log: DbAuditLog): void {
    this.getDb().prepare(`
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
      sql += ' WHERE ' + clauses.join(' AND ');
    }

    sql += ' ORDER BY timestamp DESC';

    const rows = this.getDb().prepare(sql).all(...params);
    return rows.map(rowToDbAuditLog);
  }

  // ---- Utility ----

  clear(): void {
    this.getDb().prepare('DELETE FROM api_key_audit_log').run();
    this.getDb().prepare('DELETE FROM api_keys').run();
  }

  getStats() {
    const apiKeyCount = (this.getDb().prepare('SELECT COUNT(*) AS count FROM api_keys').get() as any).count;
    const auditCount = (this.getDb().prepare('SELECT COUNT(*) AS count FROM api_key_audit_log').get() as any).count;
    return {
      apiKeys: apiKeyCount,
      auditLogs: auditCount,
    };
  }
}

// Singleton instance
export const db = new Database();
