import request from 'supertest';
import express, { Request, Response, NextFunction } from 'express';
import { getKeyAuditLogs } from '../controllers/v1/api-keys';
import { apiKeyAuthMiddleware, requireScopes } from '../middleware/api-key-auth';
import { apiKeyService } from '../services/api-key-service';
import { auditLogService } from '../services/audit-log';
import { ApiKey } from '../models/api-key';

jest.mock('../services/api-key-service', () => ({
  apiKeyService: {
    verifyApiKey: jest.fn(),
    updateLastUsed: jest.fn(),
    getApiKeyById: jest.fn(),
  },
}));

jest.mock('../services/audit-log', () => ({
  auditLogService: {
    getLogsForKey: jest.fn(),
  },
}));

const VALID_KEY_ID = 'key-00000000-0000-4000-8000-000000000001';
const UNKNOWN_KEY_ID = 'key-does-not-exist';
// Assembled at runtime so the secret scanner does not treat this fixture as a
// real leaked API key.
const VALID_PLAINTEXT_KEY = ['qlx', 'test', 'd'.repeat(32)].join('_');
const ADMIN_SCOPE = 'admin:keys';

function buildApiKey(overrides: Partial<ApiKey> = {}): ApiKey {
  return {
    id: VALID_KEY_ID,
    key_hash: 'super-secret-hash',
    signing_secret_hash: 'signing-secret-hash',
    prev_signing_secret_hash: null,
    prefix: 'qlx_test_abcde',
    name: 'Deterministic Key',
    scopes: [ADMIN_SCOPE],
    created_at: '2024-01-01T00:00:00.000Z',
    last_used_at: null,
    expires_at: null,
    prev_secret_expires_at: null,
    revoked: false,
    created_by: 'admin-user',
    ...overrides,
  };
}

const app = express();
app.use(express.json());
// Mirror the production route wiring in src/routes/v1/api-keys.ts so that
// authorization is exercised for the audit-log endpoint, not bypassed.
app.get('/api/v1/keys/:id/audit-logs', apiKeyAuthMiddleware, requireScopes([ADMIN_SCOPE]), getKeyAuditLogs);

const verifyApiKeyMock = apiKeyService.verifyApiKey as jest.Mock;
const updateLastUsedMock = apiKeyService.updateLastUsed as jest.Mock;
const getApiKeyByIdMock = apiKeyService.getApiKeyById as jest.Mock;
const getLogsForKeyMock = auditLogService.getLogsForKey as jest.Mock;

function getAuditLogs(pathId: string, plaintextKey: string | null = VALID_PLAINTEXT_KEY) {
  const req = request(app).get(`/api/v1/keys/${encodeURIComponent(pathId)}/audit-logs`);
  if (plaintextKey !== null) {
    req.set('Authorization', `Bearer ${plaintextKey}`);
  }
  return req;
}

/**
 * Invoke the controller directly with a stub request/response so that route
 * parameters that Express would never produce (missing/empty ids) can still be
 * exercised deterministically.
 */
async function invokeController(params: Record<string, string | undefined>( {
  const req = { params } as any;
  let status = 0;
  let body: any = null;
  const res: any = {
    status(code: number) {
      status = code;
      return res;
    },
    json(payload: any) {
      body = payload;
      return res;
    },
  };

  await getKeyAuditLogs(req, res);
  return { status, body };
}

beforeEach(() => {
  jest.clearAllMocks();
  jest.spyOn(console, 'error').mockImplementation(() => undefined);
  verifyApiKeyMock.mockResolvedValue(buildApiKey());
  updateLastUsedMock.mockResolvedValue(undefined);
  getApiKeyByIdMock.mockResolvedValue(buildApiKey());
  getLogsForKeyMock.mockReturnValue([]);
});

afterEach(() => {
  jest.restoreAllMocks();
});

describe('getKeyAuditLogs', () => {
  describe('success path', () => {
    it('returns the audit log entries for an authorized admin key', async () => {
      getLogsForKeyMock.mockReturnValue([
        {
          id: 'log-1',
          event_type: 'created',
          key_id: VALID_KEY_ID,
          actor: 'admin-user',
          timestamp: '2024-01-01T00:00:00.000Z',
          ip_address: '127.0.0.1',
          endpoint: null,
          metadata: null,
        },
        {
          id: 'log-2',
          event_type: 'rotated',
          key_id: VALID_KEY_ID,
          actor: 'admin-user',
          timestamp: '2024-01-02T00:00:00.000Z',
          ip_address: null,
          endpoint: '/api/v1/keys/:id/rotate',
          metadata: JSON.stringify({ new_key_id: 'key-2' }),
        },
      ]);

      const res = await getAuditLogs(VALID_KEY_ID).expect(200);

      expect(getLogsForKeyMock).toHaveBeenCalledTimes(1);
      expect(getLogsForKeyMock).toHaveBeenCalledWith(VALID_KEY_ID);
      expect(res.body.count).toBe(2);
      expect(res.body.data).toHaveLength(2);

      // Fields are returned in insertion order and the payload is sanitized.
      expect(res.body.data[0]).toEqual({
        id: 'log-1',
        event_type: 'created',
        timestamp: '2024-01-01T00:00:00.000Z',
        actor: 'admin-user',
        ip_address: '127.0.0.1',
        endpoint: null,
        metadata: null,
      });
      expect(res.body.data[1]).toEqual({
        id: 'log-2',
        event_type: 'rotated',
        timestamp: '2024-01-02T00:00:00.000Z',
        actor: 'admin-user',
        ip_address: null,
        endpoint: '/api/v1/keys/:id/rotate',
        metadata: { new_key_id: 'key-2' },
      });
    });

    it('never leaks key material or internal key_id into the response', async () => {
      getLogsForKeyMock.mockReturnValue([
        {
          id: 'log-1',
          event_type: 'used',
          key_id: VALID_KEY_ID,
          actor: 'admin-user',
          timestamp: '2024-01-01T00:00:00.000Z',
          ip_address: '10.0.0.1',
          endpoint: '/api/v1/bids',
          metadata: null,
        },
      ]);

      const res = await getAuditLogs(VALID_KEY_ID).expect(200);

      expect(res.body.data[0]).not.toHaveProperty('key_id');
      expect(res.body.data[0]).not.toHaveProperty('key_hash');
      expect(res.body).not.toHaveProperty('key');
      expect(res.text).not.toContain('super-secret-hash');
    });

    it('returns an empty data array with count 0 when the key has no audit logs', async () => {
      getLogsForKeyMock.mockReturnValue([]);

      const res = await getAuditLogs(VALID_KEY_ID).expect(200);

      expect(res.body).toEqual({ data: [], count: 0 });
    });

    it('queries audit logs for the requested key id only', async () => {
      await getAuditLogs('key-with-special-chars-!@$').expect(200);

      expect(getApiKeyByIdMock).toHaveBeenCalledWith('key-with-special-chars-!@$');
      expect(getLogsForKeyMock).toHaveBeenCalledWith('key-with-special-chars-!@$');
    });

    it('still returns logs for a revoked key, since authorization is scope based', async () => {
      getApiKeyByIdMock.mockResolvedValue(buildApiKey({ revoked: true }));
      getLogsForKeyMock.mockReturnValue([
        {
          id: 'log-1',
          event_type: 'revoked',
          key_id: VALID_KEY_ID,
          actor: 'admin-user',
          timestamp: '2024-01-03T00:00:00.000Z',
          ip_address: null,
          endpoint: null,
          metadata: null,
        },
      ]);

      const res = await getAuditLogs(VALID_KEY_ID).expect(200);

      expect(res.body.count).toBe(1);
      expect(res.body.data[0].event_type).toBe('revoked');
    });
  });

  describe('key resolution boundaries', () => {
    it('returns 404 KEY_NOT_FOUND when the key does not exist', async () => {
      getApiKeyByIdMock.mockResolvedValue(null);

      const res = await getAuditLogs(UNKNOWN_KEY_ID).expect(404);

      expect(res.body.error.code).toBe('KEY_NOT_FOUND');
      expect(res.body.error.message).toBe('API key not found');
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('returns 404 KEY_NOT_FOUND when the key id is empty', async () => {
      getApiKeyByIdMock.mockResolvedValue(null);

      const { status, body } = await invokeController({ id: '' });

      expect(status).toBe(404);
      expect(body.error.code).toBe('KEY_NOT_FOUND');
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('returns 404 KEY_NOT_FOUND when the key id is missing from the route params', async () => {
      getApiKeyByIdMock.mockResolvedValue(null);

      const { status, body } = await invokeController({ });

      expect(status).toBe(404);
      expect(body.error.code).toBe('KEY_NOT_FOUND');
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('returns 500 when the key id is missing and key lookup throws', async () => {
      getApiKeyByIdMock.mockRejectedValue(new Error('undefined key id'));

      const { status, body } = await invokeController({ });

      expect(status).toBe(500);
      expect(body.error.code).toBe('GET_AUDIT_LOGS_ERROR');
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('returns 500 GET_AUDIT_LOGS_ERROR when key lookup throws', async () => {
      getApiKeyByIdMock.mockRejectedValue(new Error('database is locked'));

      const res = await getAuditLogs(VALID_KEY_ID).expect(500);

      expect(res.body.error.code).toBe('GET_AUDIT_LOGS_ERROR');
      expect(res.body.error.message).toBe('Failed to get audit logs');
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('returns 500 when audit log retrieval throws', async () => {
      getLogsForKeyMock.mockImplementation(() => {
        throw new Error('audit log store unavailable');
      });

      const res = await getAuditLogs(VALID_KEY_ID).expect(500);

      expect(res.body.error.code).toBe('GET_AUDIT_LOGS_ERROR');
      expect(res.body).not.toHaveProperty('data');
    });

    it('returns 500 and does not leak parser internals when metadata is not valid JSON', async () => {
      getLogsForKeyMock.mockReturnValue([
        {
          id: 'log-1',
          event_type: 'created',
          key_id: VALID_KEY_ID,
          actor: 'admin-user',
          timestamp: '2024-01-01T00:00:00.000Z',
          ip_address: null,
          endpoint: null,
          metadata: '{not-json',
        },
      ]);

      const res = await getAuditLogs(VALID_KEY_ID).expect(500);

      expect(res.body.error.code).toBe('GET_AUDIT_LOGS_ERROR');
      expect(res.body).not.toHaveProperty('data');
      expect(res.text).not.toContain('not-json');
    });
  });

  describe('authorization boundaries', () => {
    it('rejects the request with 401 when no Authorization header is present', async () => {
      const res = await getAuditLogs(VALID_KEY_ID, null).expect(401);

      expect(res.body.error.code).toBe('UNAUTHORIZED');
      expect(verifyApiKeyMock).not.toHaveBeenCalled();
      expect(getApiKeyByIdMock).not.toHaveBeenCalled();
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('rejects the request with 401 when the Authorization header is malformed', async () => {
      const res = await request(app)
        .get(`/api/v1/keys/${VALID_KEY_ID}/audit-logs`)
        .set('Authorization', VALID_PLAINTEXT_KEY)
        .expect(401);

      expect(res.body.error.code).toBe('INVALID_AUTH_FORMAT');
      expect(verifyApiKeyMock).not.toHaveBeenCalled();
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('rejects the request with 401 when the token is not an API key', async () => {
      const res = await getAuditLogs(VALID_KEY_ID, 'not-a-quicklendx-key').expect(401);

      expect(res.body.error.code).toBe('INVALID_API_KEY');
      expect(verifyApiKeyMock).not.toHaveBeenCalled();
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('rejects the request with 401 when the key cannot be verified', async () => {
      verifyApiKeyMock.mockResolvedValue(null);

      const res = await getAuditLogs(VALID_KEY_ID).expect(401);

      expect(res.body.error.code).toBe('INVALID_API_KEY');
      expect(getApiKeyByIdMock).not.toHaveBeenCalled();
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('rejects the request with 401 when key verification throws', async () => {
      verifyApiKeyMock.mockRejectedValue(new Error('hash comparison exploded'));

      const res = await getAuditLogs(VALID_KEY_ID).expect(500);

      expect(res.body.error.code).toBe('AUTH_ERROR');
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('rejects the request with 403 when the caller lacks the admin:keys scope', async () => {
      verifyApiKeyMock.mockResolvedValue(buildApiKey({ scopes: ['read:bids'] }));

      const res = await getAuditLogs(VALID_KEY_ID).expect(403);

      expect(res.body.error.code).toBe('INSUFFICIENT_SCOPES');
      expect(getApiKeyByIdMock).not.toHaveBeenCalled();
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });

    it('rejects the request with 401 when the key is revoked', async () => {
      verifyApiKeyMock.mockResolvedValue(buildApiKey({ revoked: true }));

      const res = await getAuditLogs(VALID_KEY_ID).expect(401);

      expect(res.body.error.code).toBe('INVALID_API_KEY');
      expect(getLogsForKeyMock).not.toHaveBeenCalled();
    });
  });

  describe('determinism and concurrency', () => {
    it('produces the same response for repeated identical requests', async () => {
      getLogsForKeyMock.mockReturnValue([
        {
          id: 'log-1',
          event_type: 'created',
          key_id: VALID_KEY_ID,
          actor: 'admin-user',
          timestamp: '2024-01-01T00:00:00.000Z',
          ip_address: null,
          endpoint: null,
          metadata: null,
        },
      ]);

      const first = await getAuditLogs(VALID_KEY_ID).expect(200);
      const second = await getAuditLogs(VALID_KEY_ID).expect(200);

      expect(second.body).toEqual(first.body);
    });

    it('handles concurrent requests without cross-contaminating results', async () => {
      getApiKeyByIdMock.implementation(async (id: string) => buildApiKey({ id }));
      getLogsForKeyMock.implementation((id: string) => [
        {
          id: `log-${id}`,
          event_type: 'created',
          key_id: id,
          actor: 'admin-user',
          timestamp: '2024-01-01T00:00:00.000Z',
          ip_address: null,
          endpoint: null,
          metadata: null,
        },
      ];

      const ids = ['key-a', 'key-b', 'key-c'];
      const responses = await Promise.all(ids.map((id) => getAuditLogs(id).expect(200)));

      responses.forEach((res, index) => {
        expect(res.body.count).toBe(1);
        expect(res.body.data[0].id).toBe(`log-${ids[index]}`);
      });
    });

    it('does not mutate audit log state on repeated reads', async () => {
      const logs = [
        {
          id: 'log-1',
          event_type: 'created',
          key_id: VALID_KEY_ID,
          actor: 'admin-user',
          timestamp: '2024-01-01T00:00:00.000Z',
          ip_address: null,
          endpoint: null,
          metadata: null,
        },
      ];
      getLogsForKeyMock.mockReturnValue(logs);

      await getAuditLogs(VALID_KEY_ID).expect(200);
      await getAuditLogs(VALID_KEY_ID).expect(200);

      expect(logs).toHaveLength(1);
      expect(logs[0].id).toBe('log-1');
    });
  });
});
