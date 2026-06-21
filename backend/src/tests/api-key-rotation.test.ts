import crypto from 'crypto';
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
import { getDatabase, closeDatabase } from '../lib/database';

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
      scopes: ['read:*'],
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
      scopes: ['read:*'],
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

});
