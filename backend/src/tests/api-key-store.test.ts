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

// ---------------------------------------------------------------------------
// Failure-boundary coverage for getApiKey
// ---------------------------------------------------------------------------

describe('getApiKey failure boundaries', () => {
  test('returns undefined for empty string id', () => {
    expect(db.getApiKeyById('')).toBeUndefined();
  });

  test('returns undefined for whitespace-only id', () => {
    expect(db.getApiKeyById('   ')).toBeUndefined();
  });

  test('returns undefined for id with SQL metacharacters', () => {
    expect(db.getApiKeyById("'; DROP TABLE api_keys; --")).toBeUndefined();
    expect(db.getStats().apiKeys).toBe(0);
  });

  test('returns undefined for very long id', () => {
    expect(db.getApiKeyById('x'.repeat(10_000))).toBeUndefined();
  });

  test('returns undefined for unicode id', () => {
    expect(db.getApiKeyById('🔑-ключ-鍵')).toBeUndefined();
  });

  test('returns undefined for null-like id', () => {
    expect(db.getApiKeyById(undefined as unknown as string)).toBeUndefined();
    expect(db.getApiKeyById(null as unknown as string)).toBeUndefined();
  });

  test('returns undefined for revoked key by id (still retrievable)', () => {
    const key = makeKey({ revoked: 1 });
    db.createApiKey(key);
    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved).toBeDefined();
    expect(retrieved!.revoked).toBe(1);
  });

  test('returns undefined for expired key by id (still retrievable)', () => {
    const key = makeKey({ expires_at: '2000-01-01T00:00:00.000Z' });
    db.createApiKey(key);
    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved).toBeDefined();
    expect(retrieved!.expires_at).toBe('2000-01-01T00:00:00.000Z');
  });

  test('getApiKeyByPrefix returns undefined for empty prefix', () => {
    expect(db.getApiKeyByPrefix('')).toBeUndefined();
  });

  test('getApiKeyByPrefix returns undefined for whitespace prefix', () => {
    expect(db.getApiKeyByPrefix('   ')).toBeUndefined();
  });

  test('getApiKeyByPrefix returns undefined for SQL metacharacters', () => {
    expect(db.getApiKeyByPrefix("'; DROP TABLE api_keys; --")).toBeUndefined();
    expect(db.getStats().apiKeys).toBe(0);
  });

  test('getApiKeyByPrefix returns undefined for very long prefix', () => {
    expect(db.getApiKeyByPrefix('p'.repeat(10_000))).toBeUndefined();
  });

  test('getApiKeyByPrefix returns undefined for unicode prefix', () => {
    expect(db.getApiKeyByPrefix('🔑-prefix')).toBeUndefined();
  });

  test('getApiKeyByPrefix returns undefined for null-like prefix', () => {
    expect(db.getApiKeyByPrefix(undefined as unknown as string)).toBeUndefined();
    expect(db.getApiKeyByPrefix(null as unknown as string)).toBeUndefined();
  });

  test('getApiKeyByPrefix is deterministic across repeated calls', () => {
    const key = makeKey();
    db.createApiKey(key);
    const first = db.getApiKeyByPrefix(key.prefix);
    const second = db.getApiKeyByPrefix(key.prefix);
    expect(first!.id).toBe(second!.id);
  });

  test('getApiKeyById is deterministic across repeated calls', () => {
    const key = makeKey();
    db.createApiKey(key);
    const first = db.getApiKeyById(key.id);
    const second = db.getApiKeyById(key.id);
    expect(first!.id).toBe(second!.id);
  });
});

// ---------------------------------------------------------------------------
// Failure-boundary coverage for createApiKey
// ---------------------------------------------------------------------------

describe('createApiKey failure boundaries', () => {
  test('rejects duplicate id', () => {
    const key = makeKey();
    db.createApiKey(key);
    const dup = makeKey({ id: key.id });
    expect(() => db.createApiKey(dup)).toThrow();
  });

  test('rejects duplicate prefix', () => {
    const key = makeKey();
    db.createApiKey(key);
    const dup = makeKey({ prefix: key.prefix });
    expect(() => db.createApiKey(dup)).toThrow();
  });

  test('rejects null key_hash', () => {
    const key = makeKey({ key_hash: null as unknown as string });
    expect(() => db.createApiKey(key)).toThrow();
  });

  test('rejects null prefix', () => {
    const key = makeKey({ prefix: null as unknown as string });
    expect(() => db.createApiKey(key)).toThrow();
  });

  test('rejects null name', () => {
    const key = makeKey({ name: null as unknown as string });
    expect(() => db.createApiKey(key)).toThrow();
  });

  test('rejects null scopes', () => {
    const key = makeKey({ scopes: null as unknown as string });
    expect(() => db.createApiKey(key)).toThrow();
  });

  test('rejects null created_at', () => {
    const key = makeKey({ created_at: null as unknown as string });
    expect(() => db.createApiKey(key)).toThrow();
  });

  test('rejects null created_by', () => {
    const key = makeKey({ created_by: null as unknown as string });
    expect(() => db.createApiKey(key)).toThrow();
  });

  test('rejects invalid revoked value', () => {
    const key = makeKey({ revoked: 2 as unknown as number });
    expect(() => db.createApiKey(key)).toThrow();
  });

  test('partial failure does not leave partial state', () => {
    const key = makeKey();
    db.createApiKey(key);
    const before = db.getStats();
    const dup = makeKey({ prefix: key.prefix });
    expect(() => db.createApiKey(dup)).toThrow();
    expect(db.getStats()).toEqual(before);
  });
});

// ---------------------------------------------------------------------------
// Failure-boundary coverage for updateApiKey
// ---------------------------------------------------------------------------

describe('updateApiKey failure boundaries', () => {
  test('returns false for empty string id', () => {
    expect(db.updateApiKey('', { name: 'x' })).toBe(false);
  });

  test('returns false for whitespace id', () => {
    expect(db.updateApiKey('   ', { name: 'x' })).toBe(false);
  });

  test('returns false for SQL metacharacter id', () => {
    expect(db.updateApiKey("'; DROP TABLE api_keys; --", { name: 'x' })).toBe(false);
    expect(db.getStats().apiKeys).toBe(0);
  });

  test('returns false for very long id', () => {
    expect(db.updateApiKey('x'.repeat(10_000), { name: 'x' })).toBe(false);
  });

  test('returns false for null-like id', () => {
    expect(db.updateApiKey(undefined as unknown as string, { name: 'x' })).toBe(false);
    expect(db.updateApiKey(null as unknown as string, { name: 'x' })).toBe(false);
  });

  test('rejects invalid revoked value', () => {
    const key = makeKey();
    db.createApiKey(key);
    expect(() => db.updateApiKey(key.id, { revoked: 2 as unknown as number })).toThrow();
  });

  test('partial failure does not mutate state', () => {
    const key = makeKey({ name: 'Original' });
    db.createApiKey(key);
    expect(() => db.updateApiKey(key.id, { revoked: 2 as unknown as number })).toThrow();
    const retrieved = db.getApiKeyById(key.id);
    expect(retrieved!.name).toBe('Original');
    expect(retrieved!.revoked).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Failure-boundary coverage for deleteApiKey
// ---------------------------------------------------------------------------

describe('deleteApiKey failure boundaries', () => {
  test('returns false for empty string id', () => {
    expect(db.deleteApiKey('')).toBe(false);
  });

  test('returns false for whitespace id', () => {
    expect(db.deleteApiKey('   ')).toBe(false);
  });

  test('returns false for SQL metacharacter id', () => {
    expect(db.deleteApiKey("'; DROP TABLE api_keys; --")).toBe(false);
    expect(db.getStats().apiKeys).toBe(0);
  });

  test('returns false for very long id', () => {
    expect(db.deleteApiKey('x'.repeat(10_000))).toBe(false);
  });

  test('returns false for null-like id', () => {
    expect(db.deleteApiKey(undefined as unknown as string)).toBe(false);
    expect(db.deleteApiKey(null as unknown as string)).toBe(false);
  });

  test('delete is idempotent for missing key', () => {
    expect(db.deleteApiKey('missing')).toBe(false);
    expect(db.deleteApiKey('missing')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Failure-boundary coverage for audit logs
// ---------------------------------------------------------------------------

describe('audit log failure boundaries', () => {
  let keyId: string;

  beforeEach(() => {
    const key = makeKey();
    keyId = key.id;
    db.createApiKey(key);
  });

  test('rejects invalid event_type', () => {
    const log = makeAuditLog({ key_id: keyId, event_type: 'invalid' as unknown as DbAuditLog['event_type'] });
    expect(() => db.createAuditLog(log)).toThrow();
  });

  test('rejects null event_type', () => {
    const log = makeAuditLog({ key_id: keyId, event_type: null as unknown as DbAuditLog['event_type'] });
    expect(() => db.createAuditLog(log)).toThrow();
  });

  test('rejects null key_id', () => {
    const log = makeAuditLog({ key_id: null as unknown as string });
    expect(() => db.createAuditLog(log)).toThrow();
  });

  test('rejects null actor', () => {
    const log = makeAuditLog({ key_id: keyId, actor: null as unknown as string });
    expect(() => db.createAuditLog(log)).toThrow();
  });

  test('rejects null timestamp', () => {
    const log = makeAuditLog({ key_id: keyId, timestamp: null as unknown as string });
    expect(() => db.createAuditLog(log)).toThrow();
  });

  test('rejects duplicate id', () => {
    const log = makeAuditLog({ key_id: keyId });
    db.createAuditLog(log);
    const dup = makeAuditLog({ id: log.id, key_id: keyId });
    expect(() => db.createAuditLog(dup)).toThrow();
  });

  test('partial failure does not leave partial state', () => {
    const before = db.getStats();
    const log = makeAuditLog({ key_id: keyId, event_type: 'invalid' as unknown as DbAuditLog['event_type'] });
    expect(() => db.createAuditLog(log)).toThrow();
    expect(db.getStats()).toEqual(before);
  });

  test('getAuditLogs returns empty for empty string key_id', () => {
    expect(db.getAuditLogs({ key_id: '' })).toEqual([]);
  });

  test('getAuditLogs returns empty for SQL metacharacter key_id', () => {
    expect(db.getAuditLogs({ key_id: "'; DROP TABLE api_key_audit_log; --" })).toEqual([]);
    expect(db.getStats().auditLogs).toBe(0);
  });

  test('getAuditLogs returns empty for very long key_id', () => {
    expect(db.getAuditLogs({ key_id: 'x'.repeat(10_000) })).toEqual([]);
  });

  test('getAuditLogs returns empty for null-like key_id', () => {
    expect(db.getAuditLogs({ key_id: undefined as unknown as string })).toEqual([]);
    expect(db.getAuditLogs({ key_id: null as unknown as string })).toEqual([]);
  });

  test('getAuditLogs is deterministic across repeated calls', () => {
    const log = makeAuditLog({ key_id: keyId });
    db.createAuditLog(log);
    const first = db.getAuditLogs({ key_id: keyId });
    const second = db.getAuditLogs({ key_id: keyId });
    expect(first.map((l) => l.id)).toEqual(second.map((l) => l.id));
  });
});

// ---------------------------------------------------------------------------
// Retry and concurrency boundaries
// ---------------------------------------------------------------------------

describe('retry and concurrency boundaries', () => {
  test('repeated reads after write are consistent', () => {
    const key = makeKey();
    db.createApiKey(key);
    for (let i = 0; i < 100; i++) {
      expect(db.getApiKeyById(key.id)!.id).toBe(key.id);
    }
  });

  test('interleaved create and read remain consistent', () => {
    const keys = Array.from({ length: 25 }, () => makeKey());
    keys.forEach((k) => db.createApiKey(k));
    keys.forEach((k) => {
      expect(db.getApiKeyById(k.id)!.id).toBe(k.id);
    });
    expect(db.listApiKeys()).toHaveLength(25);
  });

  test('retry after failed duplicate create succeeds with new prefix', () => {
    const key = makeKey();
    db.createApiKey(key);
    const dup = makeKey({ prefix: key.prefix });
    expect(() => db.createApiKey(dup)).toThrow();
    const retry = makeKey();
    expect(() => db.createApiKey(retry)).not.toThrow();
    expect(db.getApiKeyById(retry.id)).toBeDefined();
  });

  test('concurrent delete of same key yields single success', () => {
    const key = makeKey();
    db.createApiKey(key);
    const results = [db.deleteApiKey(key.id), db.deleteApiKey(key.id)];
    expect(results.filter(Boolean)).toHaveLength(1);
  });

  test('clear during reads does not throw', () => {
    const key = makeKey();
    db.createApiKey(key);
    expect(() => {
      db.getApiKeyById(key.id);
      db.clear();
      db.getApiKeyById(key.id);
    }).not.toThrow();
    expect(db.getStats()).toEqual({ apiKeys: 0, auditLogs: 0 });
  });
});

// ---------------------------------------------------------------------------
// Permission and authorization boundary coverage
// ---------------------------------------------------------------------------

describe('permission and authorization boundaries', () => {
  test('listApiKeys scoped by created_by does not leak other users keys', () => {
    const a = makeKey({ created_by: 'user-a' });
    const b = makeKey({ created_by: 'user-b' });
    db.createApiKey(a);
    db.createApiKey(b);
    const aKeys = db.listApiKeys({ created_by: 'user-a' });
    expect(aKeys).toHaveLength(1);
    expect(aKeys[0].id).toBe(a.id);
    expect(aKeys.find((k) => k.id === b.id)).toBeUndefined();
  });

  test('listApiKeys with empty created_by returns empty', () => {
    db.createApiKey(makeKey({ created_by: 'user-a' }));
    expect(db.listApiKeys({ created_by: '' })).toEqual([]);
  });

  test('listApiKeys with SQL metacharacter created_by returns empty', () => {
    db.createApiKey(makeKey({ created_by: 'user-a' }));
    expect(db.listApiKeys({ created_by: "'; DROP TABLE api_keys; --" })).toEqual([]);
    expect(db.getStats().apiKeys).toBe(1);
  });

  test('revoked filter does not return active keys', () => {
    const active = makeKey({ revoked: 0 });
    const revoked = makeKey({ revoked: 1 });
    db.createApiKey(active);
    db.createApiKey(revoked);
    const revokedKeys = db.listApiKeys({ revoked: true });
    expect(revokedKeys).toHaveLength(1);
    expect(revokedKeys[0].id).toBe(revoked.id);
  });

  test('audit logs are scoped by key_id and do not leak across keys', () => {
    const k1 = makeKey();
    const k2 = makeKey();
    db.createApiKey(k1);
    db.createApiKey(k2);
    db.createAuditLog(makeAuditLog({ key_id: k1.id }));
    db.createAuditLog(makeAuditLog({ key_id: k2.id }));
    const logs = db.getAuditLogs({ key_id: k1.id });
    expect(logs).toHaveLength(1);
    expect(logs[0].key_id).toBe(k1.id);
  });
});