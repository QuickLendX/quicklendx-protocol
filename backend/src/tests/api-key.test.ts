import request from 'supertest';
import app from '../app';
import { db } from '../db/database';
import { apiKeyService } from '../services/api-key-service';
import { auditLogService } from '../services/audit-log';
import { generateApiKey, hashApiKey, timingSafeCompare } from '../models/api-key';

describe('API Key System', () => {
  beforeEach(() => {
    // Clear database before each test
    db.clear();
  });

  describe('Key Generation and Storage', () => {
    it('should generate key with correct format', () => {
      const { key, prefix, hash } = generateApiKey();

      // Check format: qlx_<env>_<random>
      expect(key).toMatch(/^qlx_(test|live)_[A-Za-z0-9_-]+$/);
      expect(prefix).toHaveLength(15);
      expect(hash).toHaveLength(64); // SHA-256 produces 64 hex characters
    });

    it('should never store plaintext keys', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Test Key',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      const storedKey = await apiKeyService.getApiKeyById(apiKey.id);
      expect(storedKey).toBeDefined();
      expect(storedKey!.key_hash).not.toBe(apiKey.plaintext_key);
      expect(storedKey!.key_hash).toHaveLength(64);
    });

    it('should generate unique keys', () => {
      const key1 = generateApiKey();
      const key2 = generateApiKey();

      expect(key1.key).not.toBe(key2.key);
      expect(key1.hash).not.toBe(key2.hash);
    });

    it('should use timing-safe comparison', () => {
      const hash1 = hashApiKey('test-key-1');
      const hash2 = hashApiKey('test-key-2');
      const hash1Copy = hashApiKey('test-key-1');

      expect(timingSafeCompare(hash1, hash1Copy)).toBe(true);
      expect(timingSafeCompare(hash1, hash2)).toBe(false);
    });

    it('should handle different length strings in timing-safe compare', () => {
      const short = 'abc';
      const long = 'abcdef';

      expect(timingSafeCompare(short, long)).toBe(false);
    });
  });

  describe('Scope Validation', () => {
    it('should reject invalid scopes', async () => {
      await expect(
        apiKeyService.createApiKey({
          name: 'Invalid Scope Key',
          scopes: ['invalid:scope'],
          created_by: 'test-user',
        })
      ).rejects.toThrow('Invalid scopes');
    });

    it('should accept valid scopes', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Valid Scope Key',
        scopes: ['read:users', 'write:jobs'],
        created_by: 'test-user',
      });

      expect(apiKey.scopes).toEqual(['read:users', 'write:jobs']);
    });

    it('should accept wildcard scopes', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Wildcard Key',
        scopes: ['read:*', 'admin:*'],
        created_by: 'test-user',
      });

      expect(apiKey.scopes).toContain('read:*');
      expect(apiKey.scopes).toContain('admin:*');
    });

    it('should require at least one scope', async () => {
      await expect(
        apiKeyService.createApiKey({
          name: 'No Scope Key',
          scopes: [],
          created_by: 'test-user',
        })
      ).rejects.toThrow();
    });
  });

  describe('Key Expiration', () => {
    it('should reject expired keys', async () => {
      const pastDate = new Date(Date.now() - 1000).toISOString();

      await expect(
        apiKeyService.createApiKey({
          name: 'Expired Key',
          scopes: ['read:users'],
          created_by: 'test-user',
          expires_at: pastDate,
        })
      ).rejects.toThrow('expires_at must be in the future');
    });

    it('should accept future expiration dates', async () => {
      const futureDate = new Date(Date.now() + 86400000).toISOString();

      const apiKey = await apiKeyService.createApiKey({
        name: 'Future Expiry Key',
        scopes: ['read:users'],
        created_by: 'test-user',
        expires_at: futureDate,
      });

      expect(apiKey.expires_at).toBe(futureDate);
    });

    it('should reject keys past their expiration', async () => {
      // Create a key that expires in 100ms
      const expiresAt = new Date(Date.now() + 100).toISOString();
      const apiKey = await apiKeyService.createApiKey({
        name: 'Soon Expired',
        scopes: ['read:users'],
        created_by: 'test-user',
        expires_at: expiresAt,
      });

      // Wait for expiration
      await new Promise(resolve => setTimeout(resolve, 150));

      // Try to verify the expired key
      const verified = await apiKeyService.verifyApiKey(apiKey.plaintext_key);
      expect(verified).toBeNull();
    });
  });

  describe('Key Revocation', () => {
    it('should reject revoked keys', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'To Be Revoked',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      await apiKeyService.revokeApiKey(apiKey.id, 'admin');

      const verified = await apiKeyService.verifyApiKey(apiKey.plaintext_key);
      expect(verified).toBeNull();
    });

    it('should not allow revoking already revoked keys', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Revoked Key',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      await apiKeyService.revokeApiKey(apiKey.id, 'admin');

      await expect(
        apiKeyService.revokeApiKey(apiKey.id, 'admin')
      ).rejects.toThrow('already revoked');
    });
  });

  describe('Key Rotation', () => {
    it('should create new key and invalidate old key', async () => {
      const oldKey = await apiKeyService.createApiKey({
        name: 'To Rotate',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      const newKey = await apiKeyService.rotateApiKey(oldKey.id, 'admin');

      // New key should work
      const verifiedNew = await apiKeyService.verifyApiKey(newKey.plaintext_key);
      expect(verifiedNew).toBeDefined();
      expect(verifiedNew!.id).toBe(newKey.id);

      // Old key should not work
      const verifiedOld = await apiKeyService.verifyApiKey(oldKey.plaintext_key);
      expect(verifiedOld).toBeNull();
    });

    it('should preserve scopes and name during rotation', async () => {
      const oldKey = await apiKeyService.createApiKey({
        name: 'Original Key',
        scopes: ['read:users', 'write:jobs'],
        created_by: 'test-user',
      });

      const newKey = await apiKeyService.rotateApiKey(oldKey.id, 'admin');

      expect(newKey.name).toBe(oldKey.name);
      expect(newKey.scopes).toEqual(oldKey.scopes);
      expect(newKey.created_by).toBe(oldKey.created_by);
    });

    it('should not allow rotating revoked keys', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Revoked Key',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      await apiKeyService.revokeApiKey(apiKey.id, 'admin');

      await expect(
        apiKeyService.rotateApiKey(apiKey.id, 'admin')
      ).rejects.toThrow('Cannot rotate a revoked key');
    });

    it('should log rotation event in audit log', async () => {
      const oldKey = await apiKeyService.createApiKey({
        name: 'Rotation Test',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      const newKey = await apiKeyService.rotateApiKey(oldKey.id, 'admin', '127.0.0.1');

      // Wait for async audit log
      await new Promise(resolve => setTimeout(resolve, 50));

      const logs = auditLogService.getLogsForKey(oldKey.id);
      const rotationLog = logs.find(l => l.event_type === 'rotated');

      expect(rotationLog).toBeDefined();
      expect(rotationLog!.actor).toBe('admin');
      expect(rotationLog!.ip_address).toBe('127.0.0.1');
    });
  });

  describe('Audit Logging', () => {
    it('should log key creation', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Audit Test',
        scopes: ['read:users'],
        created_by: 'test-user',
      }, '192.168.1.1');

      // Wait for async audit log
      await new Promise(resolve => setTimeout(resolve, 50));

      const logs = auditLogService.getLogsForKey(apiKey.id);
      const createdLog = logs.find(l => l.event_type === 'created');

      expect(createdLog).toBeDefined();
      expect(createdLog!.actor).toBe('test-user');
      expect(createdLog!.ip_address).toBe('192.168.1.1');
    });

    it('should log key usage', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Usage Test',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      await apiKeyService.updateLastUsed(apiKey.id, '/api/v1/users', '10.0.0.1');

      // Wait for async audit log
      await new Promise(resolve => setTimeout(resolve, 50));

      const logs = auditLogService.getLogsForKey(apiKey.id);
      const usedLog = logs.find(l => l.event_type === 'used');

      expect(usedLog).toBeDefined();
      expect(usedLog!.endpoint).toBe('/api/v1/users');
      expect(usedLog!.ip_address).toBe('10.0.0.1');
    });

    it('should log key revocation', async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Revoke Test',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      await apiKeyService.revokeApiKey(apiKey.id, 'admin', '172.16.0.1');

      // Wait for async audit log
      await new Promise(resolve => setTimeout(resolve, 50));

      const logs = auditLogService.getLogsForKey(apiKey.id);
      const revokedLog = logs.find(l => l.event_type === 'revoked');

      expect(revokedLog).toBeDefined();
      expect(revokedLog!.actor).toBe('admin');
      expect(revokedLog!.ip_address).toBe('172.16.0.1');
    });
  });

  describe('Authentication Middleware', () => {
    let validKey: string;
    let validKeyId: string;

    beforeEach(async () => {
      const apiKey = await apiKeyService.createApiKey({
        name: 'Test Auth Key',
        scopes: ['read:users', 'write:users'],
        created_by: 'test-user',
      });
      validKey = apiKey.plaintext_key;
      validKeyId = apiKey.id;
    });

    it('should accept valid key with correct scope', async () => {
      const response = await request(app)
        .get('/api/v1/keys/scopes')
        .set('Authorization', `Bearer ${validKey}`);

      expect(response.status).toBe(200);
    });

    it('should reject request without authorization header', async () => {
      // Create a key with admin:keys scope for testing
      const adminKey = await apiKeyService.createApiKey({
        name: 'Admin Key',
        scopes: ['admin:keys'],
        created_by: 'admin',
      });

      const response = await request(app)
        .get('/api/v1/keys');

      expect(response.status).toBe(401);
      expect(response.body.error.code).toBe('UNAUTHORIZED');
    });

    it('should reject invalid authorization format', async () => {
      const response = await request(app)
        .get('/api/v1/keys/scopes')
        .set('Authorization', 'InvalidFormat');

      expect(response.status).toBe(401);
      expect(response.body.error.code).toBe('INVALID_AUTH_FORMAT');
    });

    it('should reject malformed API key', async () => {
      const response = await request(app)
        .get('/api/v1/keys/scopes')
        .set('Authorization', 'Bearer invalid_key_format');

      expect(response.status).toBe(401);
      expect(response.body.error.code).toBe('INVALID_API_KEY');
    });

    it('should reject non-existent key', async () => {
      const response = await request(app)
        .get('/api/v1/keys/scopes')
        .set('Authorization', 'Bearer qlx_test_nonexistentkey123456789');

      expect(response.status).toBe(401);
      expect(response.body.error.code).toBe('INVALID_API_KEY');
    });

    it('should reject key with insufficient scopes', async () => {
      // Create key without admin:keys scope
      const limitedKey = await apiKeyService.createApiKey({
        name: 'Limited Key',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      const response = await request(app)
        .get('/api/v1/keys')
        .set('Authorization', `Bearer ${limitedKey.plaintext_key}`);

      expect(response.status).toBe(403);
      expect(response.body.error.code).toBe('FORBIDDEN');
    });

    it('should update last_used_at on successful auth', async () => {
      await request(app)
        .get('/api/v1/keys/scopes')
        .set('Authorization', `Bearer ${validKey}`);

      // Wait for async update
      await new Promise(resolve => setTimeout(resolve, 50));

      const key = await apiKeyService.getApiKeyById(validKeyId);
      expect(key!.last_used_at).not.toBeNull();
    });
  });

  describe('API Endpoints', () => {
    let adminKey: string;

    beforeEach(async () => {
      const key = await apiKeyService.createApiKey({
        name: 'Admin Key',
        scopes: ['admin:keys'],
        created_by: 'admin',
      });
      adminKey = key.plaintext_key;
    });

    describe('POST /api/v1/keys', () => {
      it('should create a new API key', async () => {
        const response = await request(app)
          .post('/api/v1/keys')
          .set('Authorization', `Bearer ${adminKey}`)
          .send({
            name: 'New Test Key',
            scopes: ['read:users'],
            created_by: 'test-user',
          });

        expect(response.status).toBe(201);
        expect(response.body.data.name).toBe('New Test Key');
        expect(response.body.data.key).toBeDefined();
        expect(response.body.data.key).toMatch(/^qlx_(test|live)_/);
        expect(response.body.data.warning).toContain('Store this key securely');
      });

      it('should reject invalid scopes', async () => {
        const response = await request(app)
          .post('/api/v1/keys')
          .set('Authorization', `Bearer ${adminKey}`)
          .send({
            name: 'Invalid Key',
            scopes: ['invalid:scope'],
            created_by: 'test-user',
          });

        expect(response.status).toBe(400);
      });

      it('should reject empty name', async () => {
        const response = await request(app)
          .post('/api/v1/keys')
          .set('Authorization', `Bearer ${adminKey}`)
          .send({
            name: '',
            scopes: ['read:users'],
            created_by: 'test-user',
          });

        expect(response.status).toBe(400);
        expect(response.body.error.code).toBe('VALIDATION_ERROR');
      });
    });

    describe('GET /api/v1/keys', () => {
      it('should list all API keys', async () => {
        await apiKeyService.createApiKey({
          name: 'Key 1',
          scopes: ['read:users'],
          created_by: 'user1',
        });

        await apiKeyService.createApiKey({
          name: 'Key 2',
          scopes: ['write:jobs'],
          created_by: 'user2',
        });

        const response = await request(app)
          .get('/api/v1/keys')
          .set('Authorization', `Bearer ${adminKey}`);

        expect(response.status).toBe(200);
        expect(response.body.data.length).toBeGreaterThanOrEqual(2);
        expect(response.body.count).toBeGreaterThanOrEqual(2);
      });

      it('should filter by created_by', async () => {
        await apiKeyService.createApiKey({
          name: 'User1 Key',
          scopes: ['read:users'],
          created_by: 'user1',
        });

        const response = await request(app)
          .get('/api/v1/keys?created_by=user1')
          .set('Authorization', `Bearer ${adminKey}`);

        expect(response.status).toBe(200);
        expect(response.body.data.every((k: any) => k.created_by === 'user1')).toBe(true);
      });
    });

    describe('GET /api/v1/keys/:id', () => {
      it('should get a specific key', async () => {
        const key = await apiKeyService.createApiKey({
          name: 'Specific Key',
          scopes: ['read:users'],
          created_by: 'test-user',
        });

        const response = await request(app)
          .get(`/api/v1/keys/${key.id}`)
          .set('Authorization', `Bearer ${adminKey}`);

        expect(response.status).toBe(200);
        expect(response.body.data.id).toBe(key.id);
        expect(response.body.data.name).toBe('Specific Key');
      });

      it('should return 404 for non-existent key', async () => {
        const response = await request(app)
          .get('/api/v1/keys/non-existent-id')
          .set('Authorization', `Bearer ${adminKey}`);

        expect(response.status).toBe(404);
        expect(response.body.error.code).toBe('KEY_NOT_FOUND');
      });
    });

    describe('POST /api/v1/keys/:id/rotate', () => {
      it('should rotate a key', async () => {
        const oldKey = await apiKeyService.createApiKey({
          name: 'Rotate Test',
          scopes: ['read:users'],
          created_by: 'test-user',
        });

        const response = await request(app)
          .post(`/api/v1/keys/${oldKey.id}/rotate`)
          .set('Authorization', `Bearer ${adminKey}`)
          .send({ actor: 'admin' });

        expect(response.status).toBe(200);
        expect(response.body.data.key).toBeDefined();
        expect(response.body.data.key).not.toBe(oldKey.plaintext_key);
        expect(response.body.data.old_key_id).toBe(oldKey.id);
      });

      it('should reject rotation without actor', async () => {
        const key = await apiKeyService.createApiKey({
          name: 'Test Key',
          scopes: ['read:users'],
          created_by: 'test-user',
        });

        const response = await request(app)
          .post(`/api/v1/keys/${key.id}/rotate`)
          .set('Authorization', `Bearer ${adminKey}`)
          .send({});

        expect(response.status).toBe(400);
        expect(response.body.error.code).toBe('VALIDATION_ERROR');
      });
    });

    describe('POST /api/v1/keys/:id/revoke', () => {
      it('should revoke a key', async () => {
        const key = await apiKeyService.createApiKey({
          name: 'Revoke Test',
          scopes: ['read:users'],
          created_by: 'test-user',
        });

        const response = await request(app)
          .post(`/api/v1/keys/${key.id}/revoke`)
          .set('Authorization', `Bearer ${adminKey}`)
          .send({ actor: 'admin' });

        expect(response.status).toBe(200);
        expect(response.body.data.message).toContain('revoked successfully');
      });
    });

    describe('GET /api/v1/keys/:id/audit-logs', () => {
      it('should get audit logs for a key', async () => {
        const key = await apiKeyService.createApiKey({
          name: 'Audit Test',
          scopes: ['read:users'],
          created_by: 'test-user',
        });

        // Wait for async audit log
        await new Promise(resolve => setTimeout(resolve, 50));

        const response = await request(app)
          .get(`/api/v1/keys/${key.id}/audit-logs`)
          .set('Authorization', `Bearer ${adminKey}`);

        expect(response.status).toBe(200);
        expect(response.body.data.length).toBeGreaterThan(0);
        expect(response.body.data[0].event_type).toBe('created');
      });
    });

    describe('GET /api/v1/keys/scopes', () => {
      it('should return available scopes without auth', async () => {
        const response = await request(app)
          .get('/api/v1/keys/scopes');

        expect(response.status).toBe(200);
        expect(response.body.data.length).toBeGreaterThan(0);
        expect(response.body.data[0]).toHaveProperty('scope');
        expect(response.body.data[0]).toHaveProperty('description');
        expect(response.body.data[0]).toHaveProperty('category');
      });
    });
  });

  describe('Security Validation', () => {
    it('should not leak key existence in error messages', async () => {
      const response = await request(app)
        .get('/api/v1/keys/scopes')
        .set('Authorization', 'Bearer qlx_test_nonexistentkey123456789');

      expect(response.status).toBe(401);
      expect(response.body.error.message).toBe('Invalid API key');
      expect(response.body.error.message).not.toContain('not found');
      expect(response.body.error.message).not.toContain('does not exist');
    });

    it('should not return key_hash in API responses', async () => {
      const adminKey = await apiKeyService.createApiKey({
        name: 'Admin Key',
        scopes: ['admin:keys'],
        created_by: 'admin',
      });

      const testKey = await apiKeyService.createApiKey({
        name: 'Test Key',
        scopes: ['read:users'],
        created_by: 'test-user',
      });

      const response = await request(app)
        .get(`/api/v1/keys/${testKey.id}`)
        .set('Authorization', `Bearer ${adminKey.plaintext_key}`);

      expect(response.status).toBe(200);
      expect(response.body.data).not.toHaveProperty('key_hash');
      expect(response.body.data).not.toHaveProperty('plaintext_key');
    });

    it('should use CSPRNG for key generation', () => {
      const keys = new Set();
      for (let i = 0; i < 100; i++) {
        const { key } = generateApiKey();
        keys.add(key);
      }

      // All keys should be unique
      expect(keys.size).toBe(100);
    });
  });
});
