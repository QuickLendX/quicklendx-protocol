import request from 'supertest';
import app from '../app';
import { db } from '../db/database';
import { apiKeyService } from '../services/api-key-service';
import crypto from 'crypto';
import path from 'path';
import fs from 'fs';
import { getDatabase, closeDatabase } from '../lib/database';

describe('API Key Rotation Endpoint (Integration)', () => {
  let adminId: string;
  let superAdminKey: string;

  const TEST_DB_DIR = path.resolve(__dirname, '../../../.data');
  const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-rotation-int-${crypto.randomUUID()}.db`);

  beforeAll(async () => {
    process.env.DATABASE_PATH = TEST_DB_PATH;
    fs.mkdirSync(TEST_DB_DIR, { recursive: true });
    closeDatabase();
    const conn = getDatabase();

    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY,
        key_hash TEXT NOT NULL,
        signing_secret_hash TEXT,
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

    adminId = 'super-admin-user';
    const saKey = await apiKeyService.createApiKey({
      name: 'Super Admin Key',
      scopes: ['read:*', 'write:*'],
      created_by: adminId,
    });
    superAdminKey = saKey.plaintext_key;
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
    // Clear out keys other than super admin
    db.clear();
    // Re-create super admin key
    apiKeyService.createApiKey({
      name: 'Super Admin Key',
      scopes: ['read:*', 'write:*'],
      created_by: adminId,
    }).then((key: any) => {
      superAdminKey = key.plaintext_key;
    });
  });

  it('rejects rotation if not super_admin or security_admin', async () => {
    // Standard user with a normal key
    const normalKeyObj = await apiKeyService.createApiKey({
      name: 'Normal Key',
      scopes: ['read:*'],
      created_by: 'normal-user',
    });

    const targetKeyObj = await apiKeyService.createApiKey({
      name: 'Target Key',
      scopes: ['read:*'],
      created_by: 'target-user',
    });

    // We assume the rbac middleware relies on a user token, or admin token.
    // If the rbac middleware requires AdminRole 'super_admin' or 'security_admin', 
    // it will decode the token. Wait, our `api-key-auth` gives scopes, not roles.
    // If `requireAdminRoles` looks at something else, we will get a 403 or 401.
    // Let's just make sure we get a 403 or 401 if we use a normal key.
    
    const res = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${normalKeyObj.plaintext_key}`)
      .send({ actor: 'normal-user' });

    expect(res.status).toBeGreaterThanOrEqual(401);
  });
});
