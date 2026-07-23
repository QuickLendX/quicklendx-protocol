import request from 'supertest';
import app from '../app';
import crypto from 'crypto';
import { db } from '../db/database';
import { getDatabase } from '../lib/database';
import { apiKeyService } from '../services/api-key-service';
import { nonceStore } from '../middleware/request-signing';

describe('Request Signing Middleware', () => {
  let apiKeyInfo: any;

  beforeAll(async () => {
    // Setup test DB schema for SQLite
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
      );
      CREATE TABLE IF NOT EXISTS api_key_audit_log (
        id TEXT PRIMARY KEY,
        key_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        actor TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        ip_address TEXT,
        endpoint TEXT,
        metadata TEXT
      );
    `);
    
    // Generate a test API key with correct scopes
    apiKeyInfo = await apiKeyService.createApiKey({
      name: 'Test Key for Signing',
      scopes: ['write:bids', 'read:bids'],
      created_by: 'test-admin',
    });
  });

  afterAll(() => {
    db.deleteApiKey(apiKeyInfo.id);
  });

  beforeEach(() => {
    nonceStore._clearForTest();
  });

  function generateSignatureHeaders(method: string, path: string, body: any, skewMs = 0, nonceOverride?: string) {
    const timestamp = Date.now() + skewMs;
    const nonce = nonceOverride || crypto.randomBytes(8).toString('hex');
    const bodyStr = JSON.stringify(body);
    const bodySha256 = crypto.createHash('sha256').update(bodyStr).digest('hex');
    const payload = `${method.toUpperCase()}${path}${bodySha256}${timestamp}${nonce}`;
    const signature = crypto.createHmac('sha256', apiKeyInfo.plaintext_signing_secret).update(payload).digest('hex');

    return {
      'Authorization': `Bearer ${apiKeyInfo.plaintext_key}`,
      'X-Timestamp': timestamp.toString(),
      'X-Nonce': nonce,
      'X-Signature': signature,
    };
  }

  it('valid signature accepted', async () => {
    const body = { invoice_id: 'inv_123', bid_amount: 1000, expected_return: 1050, expiration_timestamp: Date.now() + 86400000 };
    const headers = generateSignatureHeaders('POST', '/api/v1/bids', body);
    
    const res = await request(app)
      .post('/api/v1/bids')
      .set(headers)
      .send(body);

    expect(res.status).not.toBe(401);
    expect(res.status).not.toBe(403);
  });

  it('body tampering rejected', async () => {
    const body = { invoice_id: 'inv_123', bid_amount: 1000, expected_return: 1050, expiration_timestamp: Date.now() + 86400000 };
    const headers = generateSignatureHeaders('POST', '/api/v1/bids', body);
    
    const tamperedBody = { ...body, bid_amount: 2000 };

    const res = await request(app)
      .post('/api/v1/bids')
      .set(headers)
      .send(tamperedBody);

    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe('INVALID_SIGNATURE');
  });

  it('timestamp outside window rejected', async () => {
    const body = { invoice_id: 'inv_123' };
    const headers = generateSignatureHeaders('POST', '/api/v1/bids', body, - (6 * 60 * 1000)); // 6 mins ago

    const res = await request(app)
      .post('/api/v1/bids')
      .set(headers)
      .send(body);

    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe('EXPIRED_SIGNATURE');
  });

  it('replay of same signature within window rejected (nonce required)', async () => {
    const body = { invoice_id: 'inv_123' };
    const nonce = crypto.randomBytes(8).toString('hex');
    const headers = generateSignatureHeaders('POST', '/api/v1/bids', body, 0, nonce);

    const res1 = await request(app)
      .post('/api/v1/bids')
      .set(headers)
      .send(body);

    expect(res1.status).not.toBe(401);

    const res2 = await request(app)
      .post('/api/v1/bids')
      .set(headers)
      .send(body);

    expect(res2.status).toBe(401);
    expect(res2.body.error.code).toBe('REPLAY_DETECTED');
  });

  it('missing header on write endpoint rejected', async () => {
    const body = { invoice_id: 'inv_123' };
    const headers = { 'Authorization': `Bearer ${apiKeyInfo.plaintext_key}` };

    const res = await request(app)
      .post('/api/v1/bids')
      .set(headers)
      .send(body);

    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe('MISSING_SIGNATURE');
  });

  it('Confirm a leaked bearer alone cannot call a signed endpoint', async () => {
    const body = { invoice_id: 'inv_123' };
    
    // An attacker has the token but not the signing secret
    const attackerHeaders = {
      'Authorization': `Bearer ${apiKeyInfo.plaintext_key}`,
      'X-Timestamp': Date.now().toString(),
      'X-Nonce': crypto.randomBytes(8).toString('hex'),
      'X-Signature': 'fake_signature_hash_here'
    };

    const res = await request(app)
      .post('/api/v1/bids')
      .set(attackerHeaders)
      .send(body);

    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe('INVALID_SIGNATURE');
  });

  it('missing header on read endpoint allowed', async () => {
    const headers = { 'Authorization': `Bearer ${apiKeyInfo.plaintext_key}` };

    const res = await request(app)
      .get('/api/v1/bids?invoice_id=inv_123')
      .set(headers);

    expect(res.status).not.toBe(401);
  });
});
