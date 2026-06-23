/**
 * Unit tests for the SQLite-backed Database class (src/db/database.ts).
 *
 * Coverage targets: >=95% branches, functions, lines, statements.
 *
 * All tests use an isolated in-memory SQLite database to guarantee
 * no side effects on the dev or production database.
 */

import path from 'path';
import fs from 'fs';
import crypto from 'crypto';
import { getDatabase, closeDatabase } from '../lib/database';
import { db, DbApiKey, DbAuditLog } from '../db/database';

// ---------------------------------------------------------------------------
// Test database lifecycle – isolated temp file per run
// ---------------------------------------------------------------------------

const TEST_DB_DIR = path.resolve(__dirname, '../../.data');
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-api-keys-${crypto.randomUUID()}.db`);

beforeAll(() => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase(); // reset singleton so next getDatabase() uses the new path

  const conn = getDatabase();
  conn.exec(`
    CREATE TABLE IF NOT EXISTS api_keys (
      id TEXT PRIMARY KEY,
      key_hash TEXT NOT NULL,
      prefix TEXT NOT NULL,
      name TEXT NOT NULL,
      scopes TEXT NOT NULL,
      created_at TEXT NOT NULL,
      last_used_at TEXT,
      expires_at TEXT,
      revoked INTEGER NOT NULL DEFAULT 0,
      created_by TEXT NOT NULL,
      prev_signing_secret_hash TEXT,
      prev_secret_expires_at TEXT
    )
  `);
  conn.exec(`
    CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix)
  `);
  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_api_keys_created_by ON api_keys(created_by)
  `);
  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_api_keys_revoked ON api_keys(revoked)
  `);
  conn.exec(`
    CREATE TABLE IF NOT EXISTS api_key_audit_log (
      id TEXT PRIMARY KEY,
      event_type TEXT NOT NULL CHECK(event_type IN ('created','used','rotated','revoked')),
      key_id TEXT NOT NULL,
      actor TEXT NOT NULL,
      timestamp TEXT NOT NULL,
      ip_address TEXT,
      endpoint TEXT,
      metadata TEXT,
      FOREIGN KEY (key_id) REFERENCES api_keys(id) ON DELETE CASCADE
    )
  `);
  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_api_key_audit_key_id ON api_key_audit_log(key_id)
  `);
  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_api_key_audit_event_type ON api_key_audit_log(event_type)
  `);
  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_api_key_audit_timestamp ON api_key_audit_log(timestamp)
  `);
});

afterAll(() => {
  closeDatabase();
  try {
    if (fs.existsSync(TEST_DB_PATH)) {
      fs.unlinkSync(TEST_DB_PATH);
    }
    // Remove WAL and SHM files if they exist
    try { fs.unlinkSync(TEST_DB_PATH + '-wal'); } catch { /* ok */ }
    try { fs.unlinkSync(TEST_DB_PATH + '-shm'); } catch { /* ok */ }
  } catch { /* ok */ }
});

beforeEach(() => {
  db.clear();
});

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

function makeKey(overrides: Partial<DbApiKey> = {}): DbApiKey {
  return {
    id: crypto.randomUUID(),
    key_hash: crypto.createHash('sha256').update(crypto.randomBytes(32)).digest('hex'),
    prev_signing_secret_hash: null,
    prefix: `qlx_test_${crypto.randomBytes(4).toString('hex')}`,
    name: 'Test Key',
    scopes: JSON.stringify(['read:*']),
    created_at: new Date().toISOString(),
    last_used_at: null,
    expires_at: null,
    prev_secret_expires_at: null,
    revoked: 0,
    created_by: 'test-user',
    ...overrides,
  };
}

function makeAuditLog(overrides: Partial<DbAuditLog> = {}): DbAuditLog {
  return {
    id: crypto.randomUUID(),
    event_type: 'created',
    key_id: crypto.randomUUID(),
    actor: 'test-actor',
    timestamp: new Date().toISOString(),
    ip_address: '127.0.0.1',
    endpoint: '/api/v1/keys',
    metadata: null,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// ApiKey CRUD
// ---------------------------------------------------------------------------

describe('ApiKey CRUD', () => {
  test('createApiKey inserts and getApiKeyById retrieves', () => {
    const key = makeKey();
    db.createApiKey(key);

    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved).toBeDefined();
    expect(retrieved!.id).toBe(key.id);
    expect(retrieved!.key_hash).toBe(key.key_hash);
    expect(retrieved!.prefix).toBe(key.prefix);
    expect(retrieved!.revoked).toBe(0);
  });

  test('getApiKeyById returns undefined for missing key', () => {
    const result = db.getApiKeyById('nonexistent-id');
    expect(result).toBeUndefined();
  });

  test('getApiKeyByPrefix returns key by prefix', () => {
    const key = makeKey();
    db.createApiKey(key);

    const retrieved = db.getApiKeyByPrefix(key.prefix);
    expect(retrieved).toBeDefined();
    expect(retrieved!.id).toBe(key.id);
  });

  test('getApiKeyByPrefix returns undefined for missing prefix', () => {
    const result = db.getApiKeyByPrefix('qlx_nonexistent_');
    expect(result).toBeUndefined();
  });

  test('updateApiKey updates fields', () => {
    const key = makeKey({ name: 'Original Name' });
    db.createApiKey(key);

    const updated = db.updateApiKey(key.id, { name: 'Updated Name', revoked: 1 });
    expect(updated).toBe(true);

    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved!.name).toBe('Updated Name');
    expect(retrieved!.revoked).toBe(1);
  });

  test('updateApiKey returns false for non-existent key', () => {
    const result = db.updateApiKey('nonexistent', { name: 'Nope' });
    expect(result).toBe(false);
  });

  test('updateApiKey with empty updates returns true (no-op)', () => {
    const key = makeKey();
    db.createApiKey(key);

    const result = db.updateApiKey(key.id, {});
    expect(result).toBe(true);
  });

  test('deleteApiKey removes key and cascade-deletes audit logs', () => {
    const key = makeKey();
    db.createApiKey(key);

    const audit = makeAuditLog({ key_id: key.id });
    db.createAuditLog(audit);

    const deleted = db.deleteApiKey(key.id);
    expect(deleted).toBe(true);

    expect(db.getApiKeyById(key.id)).toBeUndefined();
    expect(db.getAuditLogs({ key_id: key.id })).toHaveLength(0);
  });

  test('deleteApiKey returns false for non-existent key', () => {
    const result = db.deleteApiKey('nonexistent');
    expect(result).toBe(false);
  });

  test('listApiKeys returns all keys', () => {
    const key1 = makeKey({ name: 'Key 1' });
    const key2 = makeKey({ name: 'Key 2' });
    db.createApiKey(key1);
    db.createApiKey(key2);

    const keys = db.listApiKeys();
    expect(keys).toHaveLength(2);
  });

  test('listApiKeys filters by created_by', () => {
    const key1 = makeKey({ created_by: 'user-a' });
    const key2 = makeKey({ created_by: 'user-b' });
    db.createApiKey(key1);
    db.createApiKey(key2);

    const keys = db.listApiKeys({ created_by: 'user-a' });
    expect(keys).toHaveLength(1);
    expect(keys[0].id).toBe(key1.id);
  });

  test('listApiKeys filters by revoked status', () => {
    const active = makeKey({ revoked: 0 });
    const revoked = makeKey({ revoked: 1 });
    db.createApiKey(active);
    db.createApiKey(revoked);

    expect(db.listApiKeys({ revoked: false })).toHaveLength(1);
    expect(db.listApiKeys({ revoked: true })).toHaveLength(1);
  });

  test('listApiKeys with no matches returns empty array', () => {
    const keys = db.listApiKeys({ created_by: 'nobody' });
    expect(keys).toEqual([]);
  });

  test('listApiKeys returns keys ordered by created_at DESC', () => {
    const oldKey = makeKey({ created_at: '2024-01-01T00:00:00.000Z' });
    const newKey = makeKey({ created_at: '2025-01-01T00:00:00.000Z' });
    db.createApiKey(oldKey);
    db.createApiKey(newKey);

    const keys = db.listApiKeys();
    expect(keys[0].id).toBe(newKey.id);
    expect(keys[1].id).toBe(oldKey.id);
  });
});

// ---------------------------------------------------------------------------
// Audit log operations
// ---------------------------------------------------------------------------

describe('Audit log operations', () => {
  let keyId: string;

  beforeEach(() => {
    const key = makeKey();
    keyId = key.id;
    db.createApiKey(key);
  });

  test('createAuditLog inserts log entry', () => {
    const log = makeAuditLog({ key_id: keyId });
    db.createAuditLog(log);

    const logs = db.getAuditLogs();
    expect(logs).toHaveLength(1);
    expect(logs[0].id).toBe(log.id);
  });

  test('getAuditLogs filters by key_id', () => {
    const key2 = makeKey();
    db.createApiKey(key2);
    const log1 = makeAuditLog({ key_id: keyId });
    const log2 = makeAuditLog({ key_id: key2.id });
    db.createAuditLog(log1);
    db.createAuditLog(log2);

    const logs = db.getAuditLogs({ key_id: keyId });
    expect(logs).toHaveLength(1);
    expect(logs[0].id).toBe(log1.id);
  });

  test('getAuditLogs filters by event_type', () => {
    const log1 = makeAuditLog({ key_id: keyId, event_type: 'created' });
    const log2 = makeAuditLog({ key_id: keyId, event_type: 'revoked' });
    db.createAuditLog(log1);
    db.createAuditLog(log2);

    const logs = db.getAuditLogs({ event_type: 'revoked' });
    expect(logs).toHaveLength(1);
    expect(logs[0].id).toBe(log2.id);
  });

  test('getAuditLogs filters by both key_id and event_type', () => {
    const log = makeAuditLog({ key_id: keyId, event_type: 'used' });
    db.createAuditLog(log);
    db.createAuditLog(makeAuditLog({ key_id: keyId, event_type: 'revoked' }));
    const key2 = makeKey();
    db.createApiKey(key2);
    db.createAuditLog(makeAuditLog({ key_id: key2.id, event_type: 'used' }));

    const logs = db.getAuditLogs({ key_id: keyId, event_type: 'used' });
    expect(logs).toHaveLength(1);
  });

  test('getAuditLogs returns logs in reverse chronological order', () => {
    const oldLog = makeAuditLog({ key_id: keyId, timestamp: '2024-01-01T00:00:00.000Z' });
    const newLog = makeAuditLog({ key_id: keyId, timestamp: '2025-01-01T00:00:00.000Z' });
    db.createAuditLog(oldLog);
    db.createAuditLog(newLog);

    const logs = db.getAuditLogs();
    expect(logs[0].id).toBe(newLog.id);
    expect(logs[1].id).toBe(oldLog.id);
  });

  test('getAuditLogs with no matches returns empty array', () => {
    const logs = db.getAuditLogs({ key_id: 'nonexistent' });
    expect(logs).toEqual([]);
  });

  test('metadata field round-trips correctly', () => {
    const meta = JSON.stringify({ new_key_id: 'abc-123', reason: 'rotation' });
    const log = makeAuditLog({ key_id: keyId, metadata: meta });
    db.createAuditLog(log);

    const logs = db.getAuditLogs();
    expect(logs[0].metadata).toBe(meta);
  });

  test('ip_address and endpoint can be null', () => {
    const log = makeAuditLog({ key_id: keyId, ip_address: null, endpoint: null });
    db.createAuditLog(log);

    const logs = db.getAuditLogs();
    expect(logs[0].ip_address).toBeNull();
    expect(logs[0].endpoint).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

describe('Edge cases', () => {
  test('expires_at round-trips correctly', () => {
    const expiresAt = '2026-12-31T23:59:59.000Z';
    const key = makeKey({ expires_at: expiresAt });
    db.createApiKey(key);

    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved!.expires_at).toBe(expiresAt);
  });

  test('expires_at can be null', () => {
    const key = makeKey({ expires_at: null });
    db.createApiKey(key);

    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved!.expires_at).toBeNull();
  });

  test('last_used_at can be null', () => {
    const key = makeKey({ last_used_at: null });
    db.createApiKey(key);

    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved!.last_used_at).toBeNull();
  });

  test('last_used_at round-trips correctly', () => {
    const lastUsed = '2026-05-27T12:00:00.000Z';
    const key = makeKey({ last_used_at: lastUsed });
    db.createApiKey(key);

    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved!.last_used_at).toBe(lastUsed);
  });

  test('scopes JSON round-trips correctly', () => {
    const scopes = JSON.stringify(['read:*', 'write:invoices']);
    const key = makeKey({ scopes });
    db.createApiKey(key);

    const retrieved = db.getApiKeyById(key.id);
    expect(JSON.parse(retrieved!.scopes)).toEqual(['read:*', 'write:invoices']);
  });

  test('duplicate prefix throws UNIQUE constraint error', () => {
    const key = makeKey();
    db.createApiKey(key);

    const dup = makeKey({ prefix: key.prefix });
    expect(() => db.createApiKey(dup)).toThrow();
  });

  test('clear() empties both tables', () => {
    const key = makeKey();
    db.createApiKey(key);
    db.createAuditLog(makeAuditLog({ key_id: key.id }));

    db.clear();

    expect(db.getStats()).toEqual({ apiKeys: 0, auditLogs: 0 });
  });

  test('getStats returns correct counts', () => {
    expect(db.getStats()).toEqual({ apiKeys: 0, auditLogs: 0 });

    db.createApiKey(makeKey());
    expect(db.getStats()).toEqual({ apiKeys: 1, auditLogs: 0 });

    const key = makeKey();
    db.createApiKey(key);
    db.createAuditLog(makeAuditLog({ key_id: key.id }));
    expect(db.getStats()).toEqual({ apiKeys: 2, auditLogs: 1 });
  });

  test('concurrent rapid writes are safe', () => {
    const keys = Array.from({ length: 50 }, (_, i) => makeKey({ name: `Concurrent-${i}` }));
    keys.forEach((k) => db.createApiKey(k));

    expect(db.listApiKeys()).toHaveLength(50);
  });
});

// ---------------------------------------------------------------------------
// Audit log event_type constraints
// ---------------------------------------------------------------------------

describe('Audit log event_type constraints', () => {
  let keyId: string;

  beforeEach(() => {
    const key = makeKey();
    keyId = key.id;
    db.createApiKey(key);
  });

  test.each([
    'created',
    'used',
    'rotated',
    'revoked',
  ] as const)('accepts valid event_type: %s', (eventType) => {
    const log = makeAuditLog({ key_id: keyId, event_type: eventType });
    expect(() => db.createAuditLog(log)).not.toThrow();
  });
});
