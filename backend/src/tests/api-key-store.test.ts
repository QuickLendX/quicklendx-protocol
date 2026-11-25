/**
 * Unit tests for the SQLite-backed Database class (src/db/database.ts).
 *
 * Coverage targets: >=95% branches, functions, lines, statements.
 *
 * All tests use an isolated in-memory SQLite database to guarantee
 * no side effects on the dev or production database.
 */

import path from 'path';
import os from 'os';
import fs from 'fs';
import crypto from 'crypto';
import { config } from '../config';
import { getDatabase, closeDatabase } from '../lib/database';
import { db, DbApiKey, DbAuditLog } from '../db/database';
import { createApiKey, listApiKeys, getApiKey } from '../controllers/v1/api-keys';
import v008 from '../migrations/v008_create_api_keys';
import v014 from '../migrations/v014_add_api_key_rotation';
import v015 from '../migrations/v015_add_signing_secret_hash';
import v013Versions from '../migrations/v013_api_key_versions';
import type { MigrationContext } from '../lib/migrations/types';

// ---------------------------------------------------------------------------
// Test database lifecycle – isolated temp file per run
// ---------------------------------------------------------------------------

const ORIGINAL_DATABASE_PATH = config.DATABASE_PATH;
const TEST_DB_PATH = path.join(os.tmpdir(), `test-api-keys-${crypto.randomUUID()}.db`);

beforeAll(async () => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  (config as { DATABASE_PATH: string }).DATABASE_PATH = TEST_DB_PATH;
  closeDatabase(); // reset singleton so next getDatabase() uses the new path

  const conn = getDatabase();
  const ctx = {
    db: {
      exec: async (sql: string) => {
        conn.exec(sql);
        return [];
      },
      get: async <T = unknown>(sql: string, params: unknown[] = []) =>
        conn.prepare(sql).get(...params) as T | undefined,
      run: async (sql: string, params: unknown[] = []) => {
        const res = conn.prepare(sql).run(...params);
        return { lastInsertRowId: Number(res.lastInsertRowid), changes: res.changes };
      },
      transaction: <T>(fn: (d: MigrationContext['db']) => T) => conn.transaction(() => fn(ctx.db))(),
    },
    env: process.env,
    isProduction: false,
    isTest: true,
  } as MigrationContext;

  for (const migration of [v008, v014, v015, v013Versions]) {
    await migration.up(ctx);
  }
});

afterAll(async () => {
  await new Promise((resolve) => setImmediate(resolve));
  closeDatabase();
  (config as { DATABASE_PATH: string }).DATABASE_PATH = ORIGINAL_DATABASE_PATH;
  for (const suffix of ['', '-wal', '-shm']) {
    try {
      if (fs.existsSync(TEST_DB_PATH + suffix)) {
        fs.unlinkSync(TEST_DB_PATH + suffix);
      }
    } catch { /* ok */ }
  }
});

beforeEach(() => {
  db.clear();
});

afterEach(async () => {
  await new Promise((resolve) => setImmediate(resolve));
});

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

function makeKey(overrides: Partial<DbApiKey> = {}): DbApiKey {
  return {
    id: crypto.randomUUID(),
    key_hash: crypto.createHash('sha256').update(crypto.randomBytes(32)).digest('hex'),
    signing_secret_hash: null,
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
    version: 1,
    revoked_at: null,
    revoked_by: null,
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
import { ApiKeyService, InMemoryApiKeyService } from '../services/api-key-service';
import { ApiKeyError } from '../errors/api-key-error';
import type { ApiKey } from '../models/api-key';

// ---------------------------------------------------------------------------
// ApiKeyService & InMemoryApiKeyService contract coverage
// ---------------------------------------------------------------------------

describe('ApiKeyService & InMemoryApiKeyService', () => {
  test('createApiKey, verifyApiKey, getApiKeyById, listApiKeys, updateLastUsed', async () => {
    const service = new ApiKeyService();
    const created = await service.createApiKey({
      name: 'Service Key',
      scopes: ['read:invoices', 'write:bids'],
      created_by: 'svc-owner',
    });

    expect(created.plaintext_key).toMatch(/^qlx_/);
    expect(created.plaintext_signing_secret.length).toBeGreaterThan(0);
    expect(created.status).toBe('active');
    expect(created.version).toBe(1);

    const verified = await service.verifyApiKey(created.plaintext_key);
    expect(verified).not.toBeNull();
    expect(verified!.id).toBe(created.id);

    expect(await service.verifyApiKey('invalid-token')).toBeNull();

    const fetched = await service.getApiKey(created.id);
    expect(fetched).not.toBeNull();
    expect(fetched!.id).toBe(created.id);

    const listed = await service.listApiKeys({ created_by: 'svc-owner' });
    expect(listed.map((k) => k.id)).toEqual([created.id]);

    await service.updateLastUsed(created.id, '/api/v1/invoices', '127.0.0.1');
    const afterUsed = await service.getApiKeyById(created.id);
    expect(afterUsed!.last_used_at).not.toBeNull();
  });

  test('revokeApiKey enforces OCC expectedVersion and is idempotent once revoked', async () => {
    const service = new ApiKeyService();
    const created = await service.createApiKey({
      name: 'Revokable Key',
      scopes: ['read:*'],
      created_by: 'owner-1',
    });

    await expect(service.revokeApiKey(created.id, 'owner-1', 999)).rejects.toMatchObject({
      code: 'CONFLICT',
    });

    const revoked = await service.revokeApiKey(created.id, 'owner-1', 1);
    expect(revoked.status).toBe('revoked');
    expect(revoked.revoked).toBe(true);
    expect(revoked.version).toBe(2);
    expect(revoked.revokedBy).toBe('owner-1');
    expect(typeof revoked.revokedAt).toBe('number');

    // Idempotent repeat revocation
    const again = await service.revokeApiKey(created.id, 'owner-1');
    expect(again.status).toBe('revoked');
    expect(again.revokedAt).toBe(revoked.revokedAt);

    expect(await service.verifyApiKey(created.plaintext_key)).toBeNull();
  });

  test('revokeApiKey throws NOT_FOUND for missing key', async () => {
    const service = new ApiKeyService();
    await expect(service.revokeApiKey(crypto.randomUUID(), 'actor-1')).rejects.toThrow(ApiKeyError);
    await expect(service.revokeApiKey(crypto.randomUUID(), 'actor-1')).rejects.toMatchObject({
      code: 'NOT_FOUND',
    });
  });

  test('rotateApiKey issues new key and revokes old key', async () => {
    const service = new ApiKeyService();
    const created = await service.createApiKey({
      name: 'Rotate Key',
      scopes: ['read:*'],
      created_by: 'owner-1',
    });

    const rotated = await service.rotateApiKey(created.id, 'owner-1', '127.0.0.1');
    expect(rotated.id).not.toBe(created.id);
    expect(rotated.plaintext_key).not.toBe(created.plaintext_key);

    expect(await service.verifyApiKey(created.plaintext_key)).toBeNull();
    expect((await service.verifyApiKey(rotated.plaintext_key))!.id).toBe(rotated.id);
  });

  test('rotateSigningSecret enforces grace window and increments version', async () => {
    const service = new ApiKeyService();
    const created = await service.createApiKey({
      name: 'Signing Key',
      scopes: ['read:*'],
      created_by: 'owner-1',
    });

    const rotated = await service.rotateSigningSecret(created.id, 'owner-1', '127.0.0.1', 24);
    expect(rotated.version).toBe(2);
    expect(rotated.prev_signing_secret_hash).toBe(created.signing_secret_hash);
    expect(rotated.prev_secret_expires_at).not.toBeNull();

    await expect(service.rotateSigningSecret(created.id, 'owner-1', '127.0.0.1', 24)).rejects.toMatchObject({
      code: 'CONFLICT',
    });
  });

  test('InMemoryApiKeyService works without touching SQLite', async () => {
    const mem = new InMemoryApiKeyService();
    const created = await mem.createApiKey({
      name: 'InMem Key',
      scopes: ['read:bids'],
      created_by: 'mem-user',
    });

    expect(mem.size).toBe(1);
    expect((await mem.verifyApiKey(created.plaintext_key))!.id).toBe(created.id);

    mem.create({ id: crypto.randomUUID(), name: 'Seeded', ownerId: 'mem-user', permissions: ['read:*'] });
    expect(mem.size).toBe(2);
    expect(db.getStats().apiKeys).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Controller helpers
// ---------------------------------------------------------------------------

function makeApiKeyContext(overrides: Partial<ApiKey> = {}): ApiKey {
  return {
    id: crypto.randomUUID(),
    prefix: 'qlx_admin_test0',
    name: 'Admin Key',
    scopes: ['admin:keys'],
    created_by: 'user-1',
    created_at: new Date().toISOString(),
    last_used_at: null,
    expires_at: null,
    revoked: false,
    signing_secret_hash: null,
    prev_signing_secret_hash: null,
    prev_secret_expires_at: null,
    ownerId: 'user-1',
    permissions: ['admin:keys'],
    status: 'active',
    version: 1,
    createdAt: Date.now(),
    revokedAt: null,
    revokedBy: null,
    ...overrides,
  };
}

function makeControllerRes() {
  const res: any = { statusCode: 200 };
  res.status = jest.fn((code: number) => {
    res.statusCode = code;
    return res;
  });
  res.json = jest.fn().mockReturnValue(res);
  return res;
}

// ---------------------------------------------------------------------------
// createApiKey controller – deterministic failure-boundary coverage
// ---------------------------------------------------------------------------

describe('createApiKey controller failure boundaries', () => {
  function makeReq(bodyOverrides: Record<string, unknown> = {}, reqOverrides: Record<string, unknown> = {}) {
    return {
      body: {
        name: 'CI Key',
        scopes: ['read:*'],
        ...bodyOverrides,
      },
      apiKey: makeApiKeyContext(),
      ip: '127.0.0.1',
      headers: {},
      ...reqOverrides,
    } as any;
  }

  test('rejects when required name is missing', async () => {
    const res = makeControllerRes();
    await createApiKey(makeReq({ name: undefined }), res);
    expect(res.status).toHaveBeenCalledWith(400);
  });

  test('rejects when scopes is not an array', async () => {
    const res = makeControllerRes();
    await createApiKey(makeReq({ scopes: 'read:*' }), res);
    expect(res.status).toHaveBeenCalledWith(400);
  });

  test('rejects when scopes contains an unknown scope', async () => {
    const res = makeControllerRes();
    await createApiKey(makeReq({ scopes: ['unknown:scope'] }), res);
    expect(res.status).toHaveBeenCalledWith(400);
  });

  test('rejects when caller is unauthenticated', async () => {
    const res = makeControllerRes();
    await createApiKey(makeReq({}, { apiKey: undefined }), res);
    expect(res.status).toHaveBeenCalledWith(401);
  });

  test('rejects when caller lacks admin:keys scope', async () => {
    const res = makeControllerRes();
    await createApiKey(
      makeReq({}, { apiKey: makeApiKeyContext({ id: 'user-2', created_by: 'user-2', scopes: ['read:*'] }) }),
      res,
    );
    expect(res.status).toHaveBeenCalledWith(401);
  });

  test('boundary: empty scopes array is accepted deterministically', async () => {
    const res = makeControllerRes();
    await createApiKey(makeReq({ scopes: [] }), res);
    expect(res.status).toHaveBeenCalledWith(201);
    const payload = res.json.mock.calls[0][0];
    expect(payload.data.scopes).toEqual([]);
  });

  test('boundary: extremely long name is rejected without throwing', async () => {
    const res = makeControllerRes();
    await createApiKey(makeReq({ name: 'x'.repeat(10_000) }), res);
    expect(res.status).toHaveBeenCalledWith(400);
  });

  test('concurrent createApiKey calls do not corrupt state', async () => {
    const responses = await Promise.all(
      Array.from({ length: 10 }, (_, i) => {
        const res = makeControllerRes();
        return createApiKey(makeReq({ name: `Concurrent-${i}` }), res).then(() => res);
      }),
    );
    responses.forEach((res) => expect(res.status).toHaveBeenCalledWith(201));
    expect(db.getStats().apiKeys).toBe(10);
  });

  test('responses do not leak internal hash fields', async () => {
    const okRes = makeControllerRes();
    await createApiKey(makeReq(), okRes);
    const okPayload = JSON.stringify(okRes.json.mock.calls[0]?.[0] ?? {});
    expect(okPayload).not.toMatch(/key_hash|signing_secret_hash|prev_signing_secret_hash/i);

    const errRes = makeControllerRes();
    await createApiKey(makeReq({ name: undefined }), errRes);
    const errPayload = JSON.stringify(errRes.json.mock.calls[0]?.[0] ?? {});
    expect(errPayload).not.toMatch(/key_hash|signing_secret/i);
  });
});

// ---------------------------------------------------------------------------
// listApiKeys controller – deterministic failure-boundary coverage
// ---------------------------------------------------------------------------

describe('listApiKeys controller – failure boundaries', () => {
  function makeReq(overrides: Record<string, unknown> = {}) {
    return {
      query: {},
      headers: {},
      apiKey: makeApiKeyContext({ created_by: 'admin-1' }),
      ...overrides,
    } as any;
  }

  test('success: admin sees all keys deterministically ordered by created_at DESC', async () => {
    const a = makeKey({ name: 'A', created_at: '2024-01-01T00:00:00.000Z' });
    const b = makeKey({ name: 'B', created_at: '2025-01-01T00:00:00.000Z' });
    db.createApiKey(a);
    db.createApiKey(b);

    const res = makeControllerRes();
    await listApiKeys(makeReq(), res);

    expect(res.statusCode).toBe(200);
    const payload = res.json.mock.calls[0][0];
    expect(payload.data.map((k: any) => k.id)).toEqual([b.id, a.id]);
  });

  test('success: filters by created_by query param', async () => {
    const mine = makeKey({ created_by: 'user-1' });
    const theirs = makeKey({ created_by: 'user-2' });
    db.createApiKey(mine);
    db.createApiKey(theirs);

    const res = makeControllerRes();
    await listApiKeys(makeReq({ query: { created_by: 'user-1' } }), res);

    expect(res.statusCode).toBe(200);
    expect(res.json.mock.calls[0][0].data.map((k: any) => k.id)).toEqual([mine.id]);
  });

  test('rejection: unauthenticated actor is denied', async () => {
    db.createApiKey(makeKey());
    const res = makeControllerRes();
    await listApiKeys(makeReq({ apiKey: undefined }), res);

    expect(res.status).toHaveBeenCalledWith(401);
    expect(res.json.mock.calls[0][0].error.code).toBe('UNAUTHORIZED');
  });

  test('rejection: non-admin key is denied', async () => {
    db.createApiKey(makeKey());
    const res = makeControllerRes();
    await listApiKeys(makeReq({ apiKey: makeApiKeyContext({ scopes: ['read:*'] }) }), res);

    expect(res.status).toHaveBeenCalledWith(401);
    expect(res.json.mock.calls[0][0].error.code).toBe('UNAUTHORIZED');
  });

  test('boundary: empty result set is a success, not an error', async () => {
    const res = makeControllerRes();
    await listApiKeys(makeReq(), res);

    expect(res.statusCode).toBe(200);
    expect(res.json.mock.calls[0][0]).toEqual({ data: [] });
  });

  test('boundary: revoked filter is honored deterministically', async () => {
    const active = makeKey({ revoked: 0 });
    const revoked = makeKey({ revoked: 1 });
    db.createApiKey(active);
    db.createApiKey(revoked);

    const resRevoked = makeControllerRes();
    await listApiKeys(makeReq({ query: { revoked: 'true' } }), resRevoked);
    expect(resRevoked.json.mock.calls[0][0].data.map((k: any) => k.id)).toEqual([revoked.id]);

    const resActive = makeControllerRes();
    await listApiKeys(makeReq({ query: { revoked: 'false' } }), resActive);
    expect(resActive.json.mock.calls[0][0].data.map((k: any) => k.id)).toEqual([active.id]);
  });

  test('boundary: invalid revoked query parameter is rejected with 400', async () => {
    const res = makeControllerRes();
    await listApiKeys(makeReq({ query: { revoked: 'invalid' } }), res);

    expect(res.status).toHaveBeenCalledWith(400);
    expect(res.json.mock.calls[0][0].error.code).toBe('VALIDATION_ERROR');
  });

  test('retry: repeated identical calls return identical results (idempotent read)', async () => {
    const k1 = makeKey({ created_at: '2024-01-01T00:00:00.000Z' });
    const k2 = makeKey({ created_at: '2025-01-01T00:00:00.000Z' });
    db.createApiKey(k1);
    db.createApiKey(k2);

    const res1 = makeControllerRes();
    const res2 = makeControllerRes();
    await listApiKeys(makeReq(), res1);
    await listApiKeys(makeReq(), res2);
    expect(res1.json.mock.calls[0][0]).toEqual(res2.json.mock.calls[0][0]);
  });

  test('concurrency: interleaved writes do not corrupt listApiKeys output', async () => {
    const res1 = makeControllerRes();
    await listApiKeys(makeReq(), res1);
    expect(res1.json.mock.calls[0][0].data).toEqual([]);

    const k1 = makeKey();
    db.createApiKey(k1);
    const res2 = makeControllerRes();
    await listApiKeys(makeReq(), res2);
    expect(res2.json.mock.calls[0][0].data.map((k: any) => k.id)).toEqual([k1.id]);

    const k2 = makeKey();
    db.createApiKey(k2);
    const res3 = makeControllerRes();
    await listApiKeys(makeReq(), res3);
    expect(res3.json.mock.calls[0][0].data).toHaveLength(2);
  });

  test('regression: listApiKeys controller does not mutate persisted state', async () => {
    const key = makeKey();
    db.createApiKey(key);
    const before = db.getApiKeyById(key.id);

    await listApiKeys(makeReq(), makeControllerRes());

    const after = db.getApiKeyById(key.id);
    expect(after).toEqual(before);
  });

  test('regression: responses and errors never leak key material', async () => {
    const key = makeKey({ signing_secret_hash: 'secret-signing-hash' });
    db.createApiKey(key);

    const okRes = makeControllerRes();
    await listApiKeys(makeReq(), okRes);
    const okSerialized = JSON.stringify(okRes.json.mock.calls[0][0]);
    expect(okSerialized).not.toMatch(/key_hash|signing_secret_hash|prev_signing_secret_hash/i);
    expect(okSerialized).not.toContain('secret-signing-hash');

    const errRes = makeControllerRes();
    await listApiKeys(makeReq({ apiKey: undefined }), errRes);
    const errSerialized = JSON.stringify(errRes.json.mock.calls[0][0]);
    expect(errSerialized).not.toMatch(/key_hash|prev_signing_secret_hash|qlx_/i);
  });
});

// ---------------------------------------------------------------------------
// getApiKey failure-boundary coverage
// ---------------------------------------------------------------------------

describe('getApiKey failure boundaries', () => {
  function makeReq(overrides: Record<string, unknown> = {}) {
    return {
      params: {},
      query: {},
      headers: {},
      apiKey: makeApiKeyContext({ created_by: 'test-user' }),
      ...overrides,
    } as any;
  }

  test('returns 400 for missing id', async () => {
    const req = makeReq({ params: {} });
    const res = makeControllerRes();
    await getApiKey(req, res);
    expect(res.status).toHaveBeenCalledWith(400);
  });

  test('returns 400 for non-UUIDv4 id', async () => {
    const req = makeReq({ params: { id: 'not-a-uuid' } });
    const res = makeControllerRes();
    await getApiKey(req, res);
    expect(res.status).toHaveBeenCalledWith(400);
  });

  test('returns 404 for unknown UUIDv4 id', async () => {
    const req = makeReq({ params: { id: crypto.randomUUID() } });
    const res = makeControllerRes();
    await getApiKey(req, res);
    expect(res.status).toHaveBeenCalledWith(404);
  });

  test('returns 401 when caller lacks admin:keys scope', async () => {
    const key = makeKey({ created_by: 'someone-else' });
    db.createApiKey(key);
    const req = makeReq({
      params: { id: key.id },
      apiKey: makeApiKeyContext({ created_by: 'test-user', scopes: ['read:*'] }),
    });
    const res = makeControllerRes();
    await getApiKey(req, res);
    expect(res.status).toHaveBeenCalledWith(401);
  });

  test('returns 200 for authorized admin', async () => {
    const key = makeKey({ created_by: 'test-user' });
    db.createApiKey(key);
    const req = makeReq({ params: { id: key.id } });
    const res = makeControllerRes();
    await getApiKey(req, res);
    expect(res.statusCode).toBe(200);
    expect(res.json).toHaveBeenCalled();
    expect(res.json.mock.calls[0][0].data.id).toBe(key.id);
  });

  test('does not expose key_hash or signing_secret_hash in response', async () => {
    const key = makeKey({ created_by: 'test-user', signing_secret_hash: 'secret-hmac-hash' });
    db.createApiKey(key);
    const req = makeReq({ params: { id: key.id } });
    const res = makeControllerRes();
    await getApiKey(req, res);
    const payload = JSON.stringify(res.json.mock.calls[0][0]);
    expect(payload).not.toContain(key.key_hash);
    expect(payload).not.toContain('secret-hmac-hash');
    expect(payload).not.toMatch(/key_hash|signing_secret_hash|prev_signing_secret_hash/i);
  });

  test('deterministic across repeated calls', async () => {
    const key = makeKey({ created_by: 'test-user' });
    db.createApiKey(key);
    const req = makeReq({ params: { id: key.id } });
    const res1 = makeControllerRes();
    const res2 = makeControllerRes();
    await getApiKey(req, res1);
    await getApiKey(req, res2);
    expect(res1.statusCode).toBe(200);
    expect(res2.statusCode).toBe(200);
    expect(res1.json.mock.calls[0][0]).toEqual(res2.json.mock.calls[0][0]);
  });
});
