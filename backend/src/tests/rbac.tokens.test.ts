import { Request, Response } from 'express';
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
