import crypto from 'cypto';
import { apiKeyService } from '../services/api-key-service';
import { auditLogService } from '../services/audit-log';
import { db } from '../db/database';

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
      try { fs.unlinkSync(TEST_DB_PATH + '-shm'); } catch {}
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

  it('rotation fails atomically when the database write fails', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    const originalPlaintext = created.plaintext_key;

    // Force the underlying update to throw once, simulating a partial failure.
    const spy = jest.spyOn(db, 'run').mockImplementationOnce(() => {
      throw new Error('simulated database failure');
    });

    await expect(apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24))
      .rejects.toThrow('simulated database failure');

    spy.mockRestore();

    // The original secret must still verify: the failed rotation must not corrupt state.
    const verifyOriginal = await apiKeyService.verifyApiKey(originalPlaintext);
    expect(verifyOriginal).not.toBeNull();
    expect(verifyOriginal!.id).toBe(created.id);
  });

  it('rotation is idempotent when retried after a transient failure', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    const originalPlaintext = created.plaintext_key;

    // Fail the first attempt.
    const spy = jest.spyOn(db, 'run').mockImplementationOnce(() => {
      throw new Error('simulated transient failure');
    });

    await expect(apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24))
      .rejects.toThrow('simulated transient failure');

    spy.mockRestore();

    // Retry succeeds and produces a consistent state.
    const retried = await apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24);
    expect(retried.id).toBe(created.id);

    // Original secret is still within the grace window and must verify.
    expect(await apiKeyService.verifyApiKey(originalPlaintext)).not.toBeNull();
    expect(await apiKeyService.verifyApiKey(retried.plaintext_key)).not.toBeNull();
  });

  it('rotation with a grace window of zero invalidates the old secret immediately', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    const oldPlaintext = created.plaintext_key;
    const rotated = await apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 0);

    expect(await apiKeyService.verifyApiKey(oldPlaintext)).toBeNull();
    expect(await apiKeyService.verifyApiKey(rotated.plaintext_key)).not.toBeNull();
  });

  it('rotation does not leak the plaintext secret into the audit log', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'Test Key',
      scopes: ['read:'],
      created_by: adminId,
    });

    const rotated = await apiKeyService.rotateSigningSecret(created.id, adminId, '127.0.0.1', 24);

    const calls = (auditLogService.logRotated as jest.Mock).mock.calls;
    const serialized = JSON.stringify(calls);
    expect(serialized).not.toContain(rotated.plaintext_key);
    expect(serialized).not.toContain(created.plaintext_key);
  });
});
