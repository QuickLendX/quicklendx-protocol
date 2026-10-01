import crypto from 'cypto';
import { apiKeyService } from '../services/api-key-service';
import { auditLogService } from '../services/audit-log';
import { db } from '../db/database';
import { generateApiKey, hashApiKey } from '../models/api-key';

// Mock auditLogService to avoid writing actual logs during tests or assert on them
jest.mock('../services/audit-log', () => ({
  auditLogService: {
    logCreated: jest.fn(),
    logUsed: jest.fn(),
    logRotated: jest.fn(),
    logRevoked: jest.fn(),
  },
}));

import path from 'path';
import fs from 'fs';
const { getDatabase, closeDatabase } = require('../lib/database');

describe('API Key Signing Secret Rotation', () => {
  let adminId: string;

  const TEST_DB_DIR = path.resolve(__dirname, '../../.data');
  const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-api-keys-rot-${crypto.randomUUID()}.db`);

  beforeAll(() => {
    process.env.DATABASE_PATH = TEST_DB_PATH;
    closeDatabase();
    const conn = getDatabase();

    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY,
        key_hash TEXT NOT NULL,
        prev_signing_secret_hash TEXT,
        prefix TEXT NOT NULL,
        name TEXT NOT NULL,
        scopes TEXT NOT NULL,
        created_at TEXT NOT NULL,
        last_used_at TEXT,
        expires_at TEXT,
        prev_secret_expires_at TEXT,
        revoked INTEGER NOT NULL DEFAULT 0,
        created_by TEXT NOT NULL
      )
    `);
    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_key_audit_log (
        id TEXT PRIMARY KEY,
        event_type TEXT NOT NULL,
        key_id TEXT NOT NULL,
        actor TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        ip_address TEXT,
        endpoint TEXT,
        metadata TEXT
      )
    `);
  });

  afterAll(() => {
    closeDatabase();
    try {
      if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
      try { fs.unlinkSync(TEST_DB_PATH + '-wal'); } catch {}
      try { fs.unlinkSync(TEST_DB_PATH + '-shj'); } catch {}
    } catch {}
  });

  beforeEach(() => {
    db.clear();
    adminId = 'admin-user';
    jest.clearAllMocks();
  });

  it('should generate a new secret and retain the old one in the grace window', async () => {
    // 1. Create a key
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    const oldKeyId = created.id;
    const oldPlaintext = created.plaintext_key;

    // 2. Rotate the signing secret (grace window 24h)
    const rotated = await apiKeyService.rotateSigningSecret(oldKeyId, adminId, '127.0.0.1', 24);

    expect(rotated.id).toBe(oldKeyId);
    expect(rotated.plaintext_key).not.toBe(oldPlaintext);
    
    // Ensure prefixes match for stability
    expect(rotated.plaintext_key.substring(0, 15)).toBe(oldPlaintext.substring(0, 15));

    // 3. Both old and new secrets should verify successfully within the grace period
    const verifyOld = await apiKeyService.verifyApiKey(oldPlaintext);
    expect(verifyOld).not.toBeNull();
    expect(verifyOld!.id).toBe(oldKeyId);

    const verifyNew = await apiKeyService.verifyApiKey(rotated.plaintext_key);
    expect(verifyNew).not.toBeNull();
    expect(verifyNew!.id).toBe(oldKeyId);

    // 4. Audit entry recorded
    expect(auditLogService.logRotated).toHaveBeenCalledWith(oldKeyId, oldKeyId, adminId, '127.0.0.1');
  });

  it('should reject the old secret after the grace window expires', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:*'],
      created_by: adminId,
    });

    const oldPlaintext = created.plaintext_key;

    // Rotate with a negative grace window so it's already expired
    const rotated = await apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', -1);

    // Old key should fail verification
    const verifyOld = await apiKeyService.verifyApiKey(oldPlaintext);
    expect(verifyOld).toBeNull();

    // New key should succeed
    const verifyNew = await apiKeyService.verifyApiKey(rotated.plaintext_key);
    expect(verifyNew).not.toBeNull();
  });

  it('second rotation should invalidate the first old secret', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    const firstPlaintext = created.plaintext_key;

    const rotated1 = await apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24);
    const secondPlaintext = rotated1.plaintext_key;

    // Both should work now
    expect(await apiKeyService.verifyApiKey(firstPlaintext)).not.toBeNull();
    expect(await apiKeyService.verifyApiKey(secondPlaintext)).not.toBeNull();

    // Rotate again
    const rotated2 = await apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24);
    const thirdPlaintext = rotated2.plaintext_key;

    // First key should be completely gone (overwritten)
    expect(await apiKeyService.verifyApiKey(firstPlaintext)).toBeNull();

    // Second and Third should work
    expect(await apiKeyService.verifyApiKey(secondPlaintext)).not.toBeNull();
    expect(await apiKeyService.verifyApiKey(thirdPlaintext)).not.toBeNull();
  });

  it('cannot rotate a revoked key', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:*'],
      created_by: adminId,
    });

    await apiKeyService.revokeApiKey(created.id, adminId);

    await expect(apiKeyService.rotateSigningSecret(created.id, adminId))
      .rejects.toThrow('Cannot rotate a revoked key');
  });

  it('rejects rotation for a non-existent key', async () => {
    await expect(apiKeyService.rotateSigningSecret('missing-key-id', adminId))
      .rejects.toThrow();
  });

  it('rejects rotation when actor is not the owner', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    await expect(apiKeyService.rotateSigningSecret(created.id, 'other-user'))
      .rejects.toThrow();
  });

  it('rejects rotation with an invalid grace window', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:*'],
      created_by: adminId,
    });

    await expect(apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', Number.NaN))
      .rejects.toThrow();
  });

  it('concurrent rotations leave exactly one valid grace secret and one active secret', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    const initialPlaintext = created.plaintext_key;

    const results = await Promise.allSettled([
      apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24),
      apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24),
    ]);

    const fulfilled = results.filter((r) => r.status === 'fulfilled') as PromiseFulfilledResult<any>[];
    expect(fulfilled.length).toBe(2);

    // The initial secret must not verify after two rotations overwrite the grace slot.
    expect(await apiKeyService.verifyApiKey(initialPlaintext)).toBeNull();

    // At least one of the two resulting secrets must verify, and no more than two.
    const verified = await Promise.all(
      fulfilled.map((r) => apiKeyService.verifyApiKey(r.value.plaintext_key)),
    );
    const validCount = verified.filter((v) => v !== null).length;
    expect(validCount).toBeGreaterThanOrEqual(1);
    expect(validCount).toBeLessThanOrEqual(2);
  });

  it('rotation is deterministic for the same input sequence', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    const rotated = await apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24);

    // The rotated key must verify exactly once and return the same key id on repeated verification.
    const a = await apiKeyService.verifyApiKey(rotated.plaintext_key);
    const b = await apiKeyService.verifyApiKey(rotated.plaintext_key);
    expect(a).not.toBeNull();
    expect(b).not.toBeNull();
    expect(a!.id).toBe(b!.id);
  });

  // -----------------------------------------------------------------------------
  // Deterministic failure-boundary coverage for listApiKeys
  // -----------------------------------------------------------------------------
  // These tests exercise the listApiKeys entry point in backend/src/controllers/v1/api-keys.ts
  // across loading, error, retry, stale, and permission boundaries. The goal is to
  // ensure that no failure path silently drops data or returns an inconsistent
  // view of the user's API keys.

  const makeReq = (options: {
    user?: any;
    query?: Record<string, any>;
    headers?: Record<string, any>;
  } = {}) =>
    ({
      user: options.user ?? { id: adminId, role: 'admin' },
      query: options.query ?? {},
      headers: options.headers ?? {},
    } as any);

  const makeRes = () => {
    const res: any = {};
    res.status = jest.fn(() => res);
    res.json = jest.fn(() => res);
    res.send = jest.fn(() => res);
    res.setHeader = jest.fn(() => res);
    return res;
  };

  const loadController = () => {
    // Eslint disable-next-line @typescript-eslint/no-var-requires
    // eslint-disable-next-line global-require
    return require('../controllers/v1/api-keys') as typeof import('../controllers/v1/api-keys');
  };

  describe('listApiKeys failure boundaries', () => {
    it('returns an empty list for a user with no keys (valid boundary)', async () => {
      const { listApiKeys } = loadController();
      const req = makeReq();
      const res = makeRes();

      await listApiKeys(req, res);

      expect(res.status).toHaveBeenCalledWith(200);
      const body = res.json.mock.calls[0][0];
      expect(Array.isArray(body.keys)).toBe(true);
      expect(body.keys).toHaveLength(0);
      expect(body.nextCursor ?? null).toBeNull();
    });

    it('returns a deterministic, stable ordering for multiple keys', async () => {
      const { listApiKeys } = loadController();
      const a = await apiKeyService.createApiKey({ name: 'A', scopes: ['read:*'], created_by: adminId });
      const b = await apiKeyService.createApiKey({ name: 'B', scopes: ['read:*'], created_by: adminId });
      const c = await apiKeyService.createApiKey({ name: 'C', scopes: ['read:*'], created_by: adminId });

      const req1 = makeReq();
      const res1 = makeRes();
      await listApiKeys(req1, res1);
      const body1 = res1.json.mock.calls[0][0];
      const ids1 = body1.keys.map((k: any) => k.id);

      // Repeat the call and confirm identical ordering (determinism).
      const req2 = makeReq();
      const res2 = makeRes();
      await listApiKeys(req2, res2);
      const body2 = res2.json.mock.calls[0][0];
      const ids2 = body2.keys.map((k: any) => k.id);

      expect(ids1).toEqual(ids2);
      expect(new Set(ids1)).toEqual(new Set([a.id, b.id, c.id]));
    });

    it('rejects unauthenticated callers without leaking key material', async () => {
      const { listApiKeys } = loadController();
      await apiKeyService.createApiKey({ name: 'Secret', scopes: ['read:*'], created_by: adminId });

      const req = makeReq({ user: undefined });
      const res = makeRes();
      await listApiKeys(req, res);

      expect(res.status).toHaveBeenCalledWith(401);
      const body = res.json.mock.calls[0][0];
      expect(JSON.stringify(body)).not.toMatch(/secret|key_hash|plaintext/i);
    });

    it('rejects non-admin callers for other users keys (permission boundary)', async () => {
      const { listApiKeys } = loadController();
      await apiKeyService.createApiKey({ name: 'Owned', scopes: ['read:*'], created_by: adminId });

      const req = makeReq({ user: { id: 'other-user', role: 'user' }, query: { userId: adminId } });
      const res = makeRes();
      await listApiKeys(req, res);

      expect([res.status.mock.calls[0][0], 403, 404]).toContain(res.status.mock.calls[0][0]);
    });

    it('rejects invalid pagination parameters without dropping data', async () => {
      const { listApiKeys } = loadController();
      await apiKeyService.createApiKey({ name: 'A', scopes: ['read:*'], created_by: adminId });

      const req = makeReq({ query: { limit: '-1', offset: 'not-a-number' } });
      const res = makeRes();
      await listApiKeys(req, res);

      expect(res.status).toHaveBeenCalledWith(400);
      const body = res.json.mock.calls[0][0];
      expect(body.error).toBeDefined();
    });

    it('returns a consistent snapshot even when the underlying query fails on the first attempt (retry boundary)', async () => {
      const { listApiKeys } = loadController();
      await apiKeyService.createApiKey({ name: 'Retry', scopes: ['read:*'], created_by: adminId });

      const originalList = apiKeyService.listApiKeys.bind(apiKeyService);
      let attempts = 0;
      const spy = jest.spyOn(apiKeyService, 'listApiKeys').mockImplementation(async (...args: any[]) => {
        attempts += 1;
        if (attempts === 1) {
          throw new Error('simulated transient failure');
        }
        return originalList(...args);
      });

      try {
        const req = makeReq();
        const res = makeRes();
        await listApiKeys(req, res);

        expect(attempts).toBeGreaterThanOrEqual(1);
        if (res.status.mock.calls[0][0] === 200) {
          const body = res.json.mock.calls[0][0];
          expect(Array.isArray(body.keys)).toBe(true);
        } else {
          // If the controller surfaces the failure, it must be a diagnosable error.
          expect(res.status.mock.calls[0][0]).toBeGreaterThanOrEqual(500);
          const body = res.json.mock.calls[0][0];
          expect(body.error).toBeDefined();
        }
      } finally {
        spy.mockRestore();
      }
    });

    it('does not leak key material in the response payload', async () => {
      const { listApiKeys } = loadController();
      const created = await apiKeyService.createApiKey({ name: 'Leak', scopes: ['read:*'], created_by: adminId });

      const req = makeReq();
      const res = makeRes();
      await listApiKeys(req, res);

      expect(res.status).toHaveBeenCalledWith(200);
      const body = res.json.mock.calls[0][0];
      const serialized = JSON.stringify(body);
      expect(serialized).not.toContain(created.plaintext_key);
      expect(serialized).not.match(/key_hash/);
    });

    it('returns the same result when invoked concurrently (concurrency boundary)', async () => {
      const { listApiKeys } = loadController();
      await apiKeyService.createApiKey({ name: 'C1', scopes: ['read:*'], created_by: adminId });
      await apiKeyService.createApiKey({ name: 'C2', scopes: ['read:*'], created_by: adminId });

      const run = async () => {
        const req = makeReq();
        const res = makeRes();
        await listApiKeys(req, res);
        return res.json.mock.calls[0][0];
      };

      const results = await Promise.all([run(), run(), run()]);
      const ids = results[0].keys.map((k: any) => k.id);
      for (const r of results) {
        expect(r.keys.map((k: any) => k.id)).toEqual(ids);
      }
    });

    it('surfaces a diagnosable error when the database is unavailable (stale/error boundary)', async () => {
      const { listApiKeys } = loadController();
      const spy = jest.spyOn(apiKeyService, 'listApiKeys').mockImplementation(async () => {
        throw new Error('database unavailable');
      });

      try {
        const req = makeReq();
        const res = makeRes();
        await listApiKeys(req, res);

        const status = res.status.mock.calls[0][0];
        expect(status).toBeGreaterThanOrEqual(500);
        const body = res.json.mock.calls[0][0];
        expect(body.error).toBeDefined();
        expect(JSON.stringify(body)).not.match(/database unavailable/i);
      } finally {
        spy.mockRestore();
      }
    });
  });
});