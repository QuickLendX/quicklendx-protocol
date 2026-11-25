import { Request, Response } from 'express';
import path from 'path';
import fs from 'fs';
import crypto from 'crypto';
import { config } from '../config';
import { getDatabase, closeDatabase } from '../lib/database';
import { apiKeyService } from '../services/api-key-service';
import { auditLogService } from '../services/auditLogService';
import { db } from '../db/database';
import { requireAdminRoles, getAdminContext } from '../middleware/rbac';
import {
  ADMIN_ROLES,
  OPERATIONS_WRITE_ROLES,
  SUPER_ADMIN_ONLY_ROLES,
  SUPPORT_READ_ROLES,
} from '../types/rbac';

describe('rbac middleware (API-key backed)', () => {
  const next = jest.fn();
  const flushAudit = () => new Promise<void>((resolve) => setImmediate(resolve));

  const buildRes = () => {
    const res: Partial<Response> = {};
    res.status = jest.fn().mockReturnValue(res as Response);
    res.json = jest.fn().mockReturnValue(res as Response);
    return res as Response;
  };

  // Use an isolated test database per run to avoid interfering with dev DB
  const TEST_DB_DIR = path.resolve(__dirname, '../../.data');
  const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-rbac-${crypto.randomUUID()}.db`);
  const ORIGINAL_DATABASE_PATH = config.DATABASE_PATH;

  beforeAll(() => {
    fs.mkdirSync(TEST_DB_DIR, { recursive: true });
    process.env.DATABASE_PATH = TEST_DB_PATH;
    (config as { DATABASE_PATH: string }).DATABASE_PATH = TEST_DB_PATH;
    closeDatabase();
    const conn = getDatabase();

    // Create minimal tables needed by api-key service
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
        created_by TEXT NOT NULL,
        version INTEGER NOT NULL DEFAULT 1,
        revoked_at INTEGER,
        revoked_by TEXT
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

  afterAll(async () => {
    await flushAudit();
    closeDatabase();
    (config as { DATABASE_PATH: string }).DATABASE_PATH = ORIGINAL_DATABASE_PATH;
    try {
      if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
      try { fs.unlinkSync(TEST_DB_PATH + '-wal'); } catch {}
      try { fs.unlinkSync(TEST_DB_PATH + '-shm'); } catch {}
    } catch {}
  });

  beforeEach(async () => {
    await flushAudit();
    next.mockReset();
    auditLogService.clear();
    db.clear();
  });

  it('exports expected role groupings in types/rbac', () => {
    expect(ADMIN_ROLES).toEqual([
      'support',
      'operations_admin',
      'security_admin',
      'super_admin',
    ]);
    expect(SUPPORT_READ_ROLES).toEqual([
      'support',
      'operations_admin',
      'super_admin',
    ]);
    expect(OPERATIONS_WRITE_ROLES).toEqual(['operations_admin', 'super_admin']);
    expect(SUPER_ADMIN_ONLY_ROLES).toEqual(['super_admin']);
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

    const handler = requireAdminRoles(SUPPORT_READ_ROLES, 'test_action');
    await handler(req, res, next);

    expect(next).toHaveBeenCalled();
    const ctx = getAdminContext(req);
    expect(ctx.role).toBe('support');
    expect(ctx.envName).toBe(`api_key:${created.id}`);
  });

  it('allows operations_admin and super_admin keys for their respective role gates', async () => {
    const opsKey = await apiKeyService.createApiKey({
      name: 'ops-key',
      scopes: ['write:*'],
      created_by: 'tests',
    });
    const superKey = await apiKeyService.createApiKey({
      name: 'super-key',
      scopes: ['admin:*'],
      created_by: 'tests',
    });

    const opsReq = {
      headers: { authorization: `Bearer ${opsKey.plaintext_key}` },
      method: 'POST',
      path: '/admin/ops',
      ip: '127.0.0.1',
    } as unknown as Request;
    await requireAdminRoles(OPERATIONS_WRITE_ROLES, 'ops_action')(opsReq, buildRes(), next);
    expect(next).toHaveBeenCalledTimes(1);
    expect(getAdminContext(opsReq)).toEqual({
      role: 'operations_admin',
      envName: `api_key:${opsKey.id}`,
    });

    const superReq = {
      headers: { authorization: `Bearer ${superKey.plaintext_key}` },
      method: 'POST',
      path: '/admin/super',
      ip: '127.0.0.1',
    } as unknown as Request;
    await requireAdminRoles(SUPER_ADMIN_ONLY_ROLES, 'super_action')(superReq, buildRes(), next);
    expect(next).toHaveBeenCalledTimes(2);
    expect(getAdminContext(superReq)).toEqual({
      role: 'super_admin',
      envName: `api_key:${superKey.id}`,
    });
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
    expect(res.json).toHaveBeenCalledWith({
      error: {
        message: 'The authenticated admin role does not have access to this action.',
        code: 'INSUFFICIENT_ROLE',
      },
    });
    expect(next).not.toHaveBeenCalled();
  });

  it('denies valid API keys whose scopes do not map to an admin role', async () => {
    const created = await apiKeyService.createApiKey({
      name: 'service-key',
      scopes: ['service:ingest'],
      created_by: 'tests',
    });

    const req = {
      headers: { authorization: `Bearer ${created.plaintext_key}` },
      method: 'GET',
      path: '/admin/test',
      ip: '127.0.0.1',
    } as unknown as Request;

    const res = buildRes();
    const handler = requireAdminRoles(SUPPORT_READ_ROLES, 'test_action');
    await handler(req, res, next);

    expect(res.status).toHaveBeenCalledWith(403);
    expect(res.json).toHaveBeenCalledWith({
      error: {
        message: 'The provided API key does not grant administrative privileges.',
        code: 'INSUFFICIENT_ROLE',
      },
    });
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
    expect(res.json).toHaveBeenCalledWith({
      error: {
        message: 'The provided admin credential is not valid for this environment.',
        code: 'FORBIDDEN',
      },
    });
    expect(next).not.toHaveBeenCalled();
  });

  it('returns 500 VERIFY_ERROR when key verification throws', async () => {
    const spy = jest
      .spyOn(apiKeyService, 'verifyApiKey')
      .mockRejectedValueOnce(new Error('db failure'));

    const req = {
      headers: { authorization: 'Bearer qlx_test_token' },
      method: 'GET',
      path: '/admin/test',
      ip: '127.0.0.1',
    } as unknown as Request;

    const res = buildRes();
    await requireAdminRoles(['support'], 'test_action')(req, res, next);

    expect(res.status).toHaveBeenCalledWith(500);
    expect(res.json).toHaveBeenCalledWith({
      error: {
        message: 'Failed to verify admin credential.',
        code: 'VERIFY_ERROR',
      },
    });
    expect(next).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('returns 401 when missing or malformed bearer token', async () => {
    const req = { headers: {}, method: 'GET', path: '/', ip: '127.0.0.1' } as unknown as Request;
    const res = buildRes();
    const handler = requireAdminRoles(['support'], 'mismatch');
    await handler(req, res, next);
    expect(res.status).toHaveBeenCalledWith(401);
    expect(res.json).toHaveBeenCalledWith({
      error: {
        message: 'Bearer authentication is required for admin endpoints.',
        code: 'AUTH_REQUIRED',
      },
    });

    const malformedReq = {
      headers: { authorization: 'Basic abc123' },
      method: 'GET',
      path: '/',
    } as unknown as Request;
    const malformedRes = buildRes();
    await handler(malformedReq, malformedRes, next);
    expect(malformedRes.status).toHaveBeenCalledWith(401);
  });

  it('throws when getAdminContext is called on a request without adminContext', () => {
    const req = { headers: {} } as unknown as Request;
    expect(() => getAdminContext(req)).toThrow(
      'Admin context is not available on this request.',
    );
  });
});
