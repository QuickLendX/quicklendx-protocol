import request from 'supertest';
import app from '../app';
import { db } from '../db/database';
import { apiKeyService } from '../services/api-key-service';
import crypto from 'crypto';
import path from 'path';
import fs from 'fs';
import { getDatabase, closeDatabase } from '../lib/database';
import { sign } from 'jsonwebtoken';

describe('API Key Signing Secret Controller Failure Boundaries', () => {
  let adminId: string;
  let superAdminToken: string;
  
  const TEST_DB_DIR = path.resolve(__dirname, '../../../.data');
  const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-signing-secret-fail-${crypto.randomUUID()}.db`);

  beforeAll(async () => {
    process.env.DATABASE_PATH = TEST_DB_PATH;
    process.env.JWT_SECRET = 'test-secret';
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
    superAdminToken = sign({
      sub: adminId,
      roles: ['super_admin']
    }, 'test-secret', { expiresIn: '1h' });
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
  });

  it('rejects rotation if key not found (404 KEY_NOT_FOUND)', async () => {
    const res = await request(app)
      .post(`/api/v1/keys/non-existent-id/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminToken}`)
      .send({ actor: adminId });

    expect(res.status).toBe(404);
    expect(res.body.error.code).toBe('KEY_NOT_FOUND');
  });

  it('rejects rotation if key is revoked (403 KEY_REVOKED)', async () => {
    const key = await apiKeyService.createApiKey({
      name: 'Revoked Key',
      scopes: ['read:*'],
      created_by: adminId,
    });
    await apiKeyService.revokeApiKey(key.id, adminId);

    const res = await request(app)
      .post(`/api/v1/keys/${key.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminToken}`)
      .send({ actor: adminId });

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe('KEY_REVOKED');
  });

  it('rejects rotation if grace window is still active (409 GRACE_WINDOW_CONFLICT)', async () => {
    const key = await apiKeyService.createApiKey({
      name: 'Active Grace Window Key',
      scopes: ['read:*'],
      created_by: adminId,
    });

    // Rotate once to start grace window
    await request(app)
      .post(`/api/v1/keys/${key.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminToken}`)
      .send({ actor: adminId, grace_window_hours: 24 });

    // Rotate again immediately -> should fail
    const res = await request(app)
      .post(`/api/v1/keys/${key.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminToken}`)
      .send({ actor: adminId, grace_window_hours: 24 });

    expect(res.status).toBe(409);
    expect(res.body.error.code).toBe('GRACE_WINDOW_CONFLICT');
  });

  it('allows rotation after grace window expires', async () => {
    const key = await apiKeyService.createApiKey({
      name: 'Expired Grace Window Key',
      scopes: ['read:*'],
      created_by: adminId,
    });

    // Rotate once to start a very short grace window (simulate expiry)
    // We will bypass by manually setting the expiry in DB
    const firstRotation = await request(app)
      .post(`/api/v1/keys/${key.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminToken}`)
      .send({ actor: adminId, grace_window_hours: 1 });

    expect(firstRotation.status).toBe(200);

    // Mock date backwards so it appears expired
    const pastDate = new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString();
    db.updateApiKey(key.id, { prev_secret_expires_at: pastDate });

    // Rotate again -> should succeed
    const res = await request(app)
      .post(`/api/v1/keys/${key.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminToken}`)
      .send({ actor: adminId, grace_window_hours: 24 });

    expect(res.status).toBe(200);
    expect(res.body.data.id).toBe(key.id);
  });

  it('handles database errors gracefully (500 ROTATE_SECRET_DB_ERROR)', async () => {
    const key = await apiKeyService.createApiKey({
      name: 'DB Error Key',
      scopes: ['read:*'],
      created_by: adminId,
    });

    // Mock db.updateApiKey to throw a DB error
    const originalUpdate = db.updateApiKey;
    db.updateApiKey = jest.fn().mockImplementation(() => {
      throw new Error('UNIQUE constraint failed: api_keys.database_error');
    });

    const res = await request(app)
      .post(`/api/v1/keys/${key.id}/rotate-signing-secret`)
      .set('Authorization', `Bearer ${superAdminToken}`)
      .send({ actor: adminId });

    expect(res.status).toBe(500);
    expect(res.body.error.code).toBe('ROTATE_SECRET_DB_ERROR');

    // Restore original DB function
    db.updateApiKey = originalUpdate;
  });

});
