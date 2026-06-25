import { Request, Response } from 'express';
import path from 'path';
import fs from 'fs';
import crypto from 'crypto';
import { getDatabase, closeDatabase } from '../lib/database';
import { apiKeyService } from '../services/api-key-service';
import { db } from '../db/database';
import { requireAdminRoles, getAdminContext } from '../middleware/rbac';

describe('rbac middleware (API-key backed)', () => {
  const next = jest.fn();

  const buildRes = () => {
    const res: Partial<Response> = {};
    res.status = jest.fn().mockReturnValue(res as Response);
    res.json = jest.fn().mockReturnValue(res as Response);
    return res as Response;
  };

  // Use an isolated test database per run to avoid interfering with dev DB
  const TEST_DB_DIR = path.resolve(__dirname, '../../.data');
  const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-rbac-${crypto.randomUUID()}.db`);

  beforeAll(() => {
    process.env.DATABASE_PATH = TEST_DB_PATH;
    closeDatabase();
    const conn = getDatabase();

    // Create minimal tables needed by api-key service
    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY,
        key_hash TEXT NOT NULL,
        signing_secret_hash TEXT,
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
    next.mockReset();
    db.clear();
  });

  it('allows a support-scoped key for support actions', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'support-key',
      scopes: ['read:*'],
      created_by: 'tests',
    });

    const req = {
      headers: { authorization: `Bearer ${created.plaintext_key}` },
      method: 'GET',
      path: '/admin/test',
      ip: '127.0.0.1',
    } as unknown as Request;

    const res = buildRes();

    const handler = requireAdminRoles(['support'], 'test_action');
    await handler(req, res, next);

    expect(next).toHaveBeenCalled();
    const ctx = getAdminContext(req);
    expect(ctx.role).toBe('support');
  });

  it('denies insufficient role', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'support-key',
      scopes: ['read:*'],
      created_by: 'tests',
    });

    const req = {
      headers: { authorization: `Bearer ${created.plaintext_key}` },
      method: 'POST',
      path: '/admin/write',
      ip: '127.0.0.1',
    } as unknown as Request;

    const res = buildRes();

    const handler = requireAdminRoles(['operations_admin'], 'write_action');
    await handler(req, res, next);

    expect(res.status).toHaveBeenCalledWith(403);
    expect(next).not.toHaveBeenCalled();
  });

  it('rejects revoked keys', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'ops-key',
      scopes: ['write:*'],
      created_by: 'tests',
    });

    // Revoke the key
    await apiKeyService.revokeApiKey(created.id, 'tests');

    const req = {
      headers: { authorization: `Bearer ${created.plaintext_key}` },
      method: 'POST',
      path: '/admin/write',
      ip: '127.0.0.1',
    } as unknown as Request;

    const res = buildRes();

    const handler = requireAdminRoles(['operations_admin'], 'write_action');
    await handler(req, res, next);

    expect(res.status).toHaveBeenCalledWith(403);
    expect(next).not.toHaveBeenCalled();
  });

  it('returns 401 when missing bearer token', async () => {
    const req = { headers: {}, method: 'GET', path: '/', ip: '127.0.0.1' } as unknown as Request;
    const res = buildRes();
    const handler = requireAdminRoles(['support'], 'mismatch');
    await handler(req, res, next);
    expect(res.status).toHaveBeenCalledWith(401);
  });
});
