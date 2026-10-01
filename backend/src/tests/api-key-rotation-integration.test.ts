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

  const TEST_DB_DIR = path.resolve(__dirname, '../../.data');
  const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-rotation-int-${crypto.randomUUID()}.db`);

  beforeAll(async () => {
    fs.mkdirSync(TEST_DB_DIR, { recursive: true });
    process.env.DATABASE_PATH = TEST_DB_PATH;
    fs.mkdirSync(TEST_DB_DIR, { recursive: true });
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

  beforeEach(async () => {
    // Clear out keys other than super admin
    dbClear();
    // Re-create super admin key
    const key = await apiKeyService.createApiKey({
      name: 'Super Admin Key',
      scopes: ['read:*', 'write:*'],
      created_by: adminId,
    });
    superAdminKey = key.plaintext_key;
  });

  // Helper to avoid duplicating the clean-up logic and keep tests deterministic.
  async function createTargetKey(overrides: Partial<any> = {}) {
    return apiKeyService.createApiKey({
      name: overrides.name ?? 'Target Key',
      scopes: overrides.scopes ?? ['read:*'],
      created_by: overrides.created_by ?? 'target-user',
      expires_at: overrides.expires_at,
    });
  }

  function dbClear() {
    const conn = getDatabase();
    conn.exec('DELETE FROM api_keys');
    conn.exec('DELETE FROM api_key_audit_log');
  }

  function getAuditEvents() {
    const conn = getDatabase();
    return conn.prepare('SELECT * FROM api_key_audit_log ORDER BY
        timestamp ASC');
  }

  it('rejects rotation if not super_admin or security_admin', async () => {
    // Standard user with a normal key
    const normalKeyObj = await apiKeyService.createApiKey({
      name: 'Normal Key',
      scopes: ['read:*'],
      created_by: 'normal-user',
    });

    const targetKeyObj = await createTargetKey();

    const res = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${normalKeyObj.plaintext_key}`)
      .send({ actor: 'normal-user' });

    expect(res.status).beGreaterThanOrEqual(401);
  });

  it('rotates signing secret for authorized admin and returns new secret once', async () => {
    const targetKeyObj = await createTargetKey();

    const res = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({ actor: adminId });

    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty('secret');
    expect(typeof res.body.secret).toBe('string');
    expect(res.body.secret.length).toBeGreaterThan(0);
  });

  it('returns 404 for unknown key id', async () => {
    const res = await request(app)
      .post('/api/v1/keys/non-existent-key/rotate-signing-secret')
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({ actor: adminId });

    expect(res.status).toBe(404);
  });

  it('rejects rotation for revoked keys', async () => {
    const targetKeyObj = await createTargetKey();
    await apiKeyService.revokeApiKey(targetKeyObj.id, adminId);

    const res = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({ actor: adminId });

    expect(res.status).toBeGreaterThanOrEqual(400);
  });

  it('rejects rotation for expired keys', async () => {
    const past = new Date(Date.now() - 60_000).toISOString();
    const targetKeyObj = await createTargetKey({ expires_at: past });

    const res = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({ actor: adminId });

    expect(res.status).toBeGreaterThanOrEqual(400);
  });

  it('returns 400 when actor is missing or invalid', async () => {
    const targetKeyObj = await createTargetKey();

    const resActorMissing = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({});

    expect(resActorMissing.status).toBeGreaterThanOrEqual(400);

    const resActorEmpty = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({ actor: '' });

    expect(resActorEmpty.status).toBeGreaterThanOrEqual(400);
  });

  it('preserves the old secret as prev secret and expires it after rotation', async () => {
    const targetKeyObj = await createTargetKey();

    const res = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({ actor: adminId });

    expect(res.status).toBe(200);

    const conn = getDatabase();
    const row = conn.prepare('SELECT * FROM api_keys WHERE id = ?').get(targetKeyObj.id);
    expect(row).toBeTruthy();
    expect(row.prev_signing_secret_hash).toBeTruthy();
    expect(row.prev_secret_expires_at).toBeTruthy();
  });

  it('ensures concurrent rotation requests do not produce inconsistent state', async () => {
    const targetKeyObj = await createTargetKey();

    const requests = Array.from({ length: 5 }, () =>
      request(app)
        .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
        .set('Authorization', `Bearer ${superAdminKey}`)
        .send({ actor: adminId })
    );

    const results = await Promise.all(requests);
    const successes = results.filter((r) => r.status === 200);
    const failures = results.filter((r) => r.status !== 200);

    // At least one succeeds.
    expect(successes.length).beGreaterThanOrEqual(1);
    // Any failure must be a client error or conflict, not a 5xx.
    for (const f of failures) {
      expect(f.status).toBeLessThan(500);
    }

    // The key must remain in a consistent state with a single active secret.
    const conn = getDatabase();
    const row = conn.prepare('SELECT * FROM api_keys WHERE id = ?').get(targetKeyObj.id);
    expect(row).toBeTruthy();
    expect(row.key_hash).toBeTruthy();
  });

  it('writes an audit log entry for each rotation attempt', async () => {
    const targetKeyObj = await createTargetKey();

    const resAuthorized = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({ actor: adminId });

    expect(resAuthorized.status).toBe(200);

    const auditRows = getAuditEvents().all();
    const rotationEvents = auditRows.filter(
      (r: any) => r.key_id === targetKeyObj.id && /rotate/i.test(r.event_type)
    );
    expect(rotationEvents.length).beGreaterThanOrEqual(1);
  });

  it('does not expose the hashed secret in response or audit logs', async () => {
    const targetKeyObj = await createTargetKey();

    const res = await request(app)
      .post(`/api/v1/keys/${targetKeyObj.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminKey}`)
      .send({ actor: adminId });

    expect(res.status).toBe(200);
    expect(JSON.stringify(res.body)).not.toMatch(/key_hash/);
    expect(JSON.stringify(res.body)).not.toMatch(/prev_signing_secret_hash/);

    const auditRows = getAuditEvents().all();
    for (const row of auditRows as any[]) {
      const serialized = JSON.stringify(row);
      expect(serialized).not.toMatch(/key_hash/);
    }
  });
});
