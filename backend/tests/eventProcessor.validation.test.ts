import express from 'express';
import request from 'supertest';
import { mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import { join } from 'path';
import { afterEach, describe, it, expect, jest } from '@jest/globals';
import { validateEvent, validateEventBatch } from '../src/services/eventValidator';

const validInvoiceSettled = {
  id: 'evt1',
  ledger: 1,
  txHash: 'tx1',
  type: 'InvoiceSettled',
  payload: {
    invoice_id: 'inv1',
    business: 'biz1',
    investor: 'invstr1',
    amount: '1000',
  },
  timestamp: 1234567890,
  complianceHold: false,
  indexedAt: new Date().toISOString(),
};

const validPaymentRecorded = {
  id: 'evt2',
  ledger: 2,
  txHash: 'tx2',
  type: 'PaymentRecorded',
  payload: {
    invoice_id: 'inv2',
    payer: 'payer1',
    amount: '500',
  },
  timestamp: 1234567891,
  complianceHold: true,
  indexedAt: new Date().toISOString(),
};

const validDisputeCreated = {
  id: 'evt3',
  ledger: 3,
  txHash: 'tx3',
  type: 'DisputeCreated',
  payload: {
    invoice_id: 'inv3',
    initiator: 'user1',
  },
  timestamp: 1234567892,
  complianceHold: false,
  indexedAt: new Date().toISOString(),
};

const validDisputeResolved = {
  id: 'evt4',
  ledger: 4,
  txHash: 'tx4',
  type: 'DisputeResolved',
  payload: {
    invoice_id: 'inv4',
    resolved_by: 'admin1',
  },
  timestamp: 1234567893,
  complianceHold: true,
  indexedAt: new Date().toISOString(),
};

describe('eventValidator', () => {
  it('accepts valid InvoiceSettled', () => {
    const result = validateEvent(validInvoiceSettled);
    expect(result.success).toBe(true);
  });

  it('accepts valid PaymentRecorded', () => {
    const result = validateEvent(validPaymentRecorded);
    expect(result.success).toBe(true);
  });

  it('accepts valid DisputeCreated', () => {
    const result = validateEvent(validDisputeCreated);
    expect(result.success).toBe(true);
  });

  it('accepts valid DisputeResolved', () => {
    const result = validateEvent(validDisputeResolved);
    expect(result.success).toBe(true);
  });

  it('rejects unknown event type', () => {
    const bad = { ...validInvoiceSettled, type: 'UnknownType' };
    const result = validateEvent(bad);
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.errors).toContain('type is not an accepted Soroban event value');
    }
  });

  it('rejects missing required fields', () => {
    const bad = { ...validInvoiceSettled };
    if ('id' in bad) delete (bad as any).id;
    const result = validateEvent(bad);
    expect(result.success).toBe(false);
  });

  it('rejects missing timestamp', () => {
    const bad = { ...validInvoiceSettled };
    delete (bad as any).timestamp;
    const result = validateEvent(bad);
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.errors).toContain('timestamp is invalid');
    }
  });

  it('rejects malformed batch', () => {
    const batch = [validInvoiceSettled, { foo: 'bar' }];
    const { success, results } = validateEventBatch(batch);
    expect(success).toBe(false);
    expect(results[1].success).toBe(false);
  });

  it('rejects oversized batch', () => {
    const batch = Array(101).fill(validInvoiceSettled);
    const { success, errors } = validateEventBatch(batch);
    expect(success).toBe(false);
    expect(errors?.[0]).toMatch(/Batch size exceeds/);
  });

  it('does not echo raw payload in error', () => {
    const bad = { ...validInvoiceSettled, payload: undefined };
    const result = validateEvent(bad);
    expect(result.success).toBe(false);
    // Should not contain the payload value
    expect(JSON.stringify(result)).not.toContain('undefined');
  });
});

describe('POST /events validation and idempotency', () => {
  const originalCwd = process.cwd();
  let tempDir: string | null = null;

  afterEach(async () => {
    process.chdir(originalCwd);
    jest.resetModules();
    jest.dontMock('../src/services/notificationService');

    if (tempDir) {
      await rm(tempDir, { recursive: true, force: true });
      tempDir = null;
    }
  });

  async function createTestApp(
    processNotification: jest.Mock = jest.fn(async () => undefined)
  ) {
    tempDir = await mkdtemp(join(tmpdir(), 'quicklendx-events-'));
    process.chdir(tempDir);
    jest.resetModules();
    jest.doMock('../src/services/notificationService', () => ({
      notificationService: {
        processNotification,
      },
    }));
    jest.doMock(
      'pg',
      () => ({
        Pool: jest.fn(() => ({
          query: jest.fn(),
          connect: jest.fn(),
          end: jest.fn(),
        })),
      }),
      { virtual: true }
    );

    const routes = (await import('../src/routes/v1')).default;
    const app = express();
    app.use(express.json());
    app.use('/api/v1', routes);

    return { app, processNotification };
  }

  it('processes a valid event once and no-ops a duplicate id', async () => {
    const { app, processNotification } = await createTestApp();

    await request(app)
      .post('/api/v1/events')
      .send(validInvoiceSettled)
      .expect(200)
      .expect((res) => {
        expect(res.body.success).toBe(true);
        expect(res.body.results[0]).toMatchObject({
          id: 'evt1',
          type: 'InvoiceSettled',
          status: 'processed',
        });
      });

    await request(app)
      .post('/api/v1/events')
      .send(validInvoiceSettled)
      .expect(200)
      .expect((res) => {
        expect(res.body.success).toBe(true);
        expect(res.body.results[0]).toMatchObject({
          id: 'evt1',
          type: 'InvoiceSettled',
          status: 'duplicate',
        });
      });

    expect(processNotification).toHaveBeenCalledTimes(1);
  });

  it('returns per-event results for mixed valid and invalid batches', async () => {
    const { app, processNotification } = await createTestApp();
    const invalid = { ...validPaymentRecorded };
    delete (invalid as any).timestamp;

    await request(app)
      .post('/api/v1/events')
      .send([validInvoiceSettled, invalid])
      .expect(400)
      .expect((res) => {
        expect(res.body.success).toBe(false);
        expect(res.body.results).toHaveLength(2);
        expect(res.body.results[0]).toMatchObject({ status: 'processed', id: 'evt1' });
        expect(res.body.results[1]).toMatchObject({ status: 'rejected' });
        expect(JSON.stringify(res.body.results[1])).not.toContain('inv2');
      });

    expect(processNotification).toHaveBeenCalledTimes(1);
  });
});
