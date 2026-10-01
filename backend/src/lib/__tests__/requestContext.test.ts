import {
  runWithContext,
  getCorrelationId,
  getOrGenerateCorrelationId,
  withCorrelationId,
  generateCorrelationId,
  sanitizeCorrelationId,
  createRequestContextMiddleware
} from '../requestContext';
import { AsyncLocalStorage } from 'node:async_hooks';

describe('requestContext', () => {
  describe('withCorrelationId and runWithContext', () => {
    it('propagates the correlationId to nested functions deterministically', async () => {
      const id = 'test-id';
      const result = await withCorrelationId(id, async () => {
        return getCorrelationId();
      });
      expect(result).toBe(id);
    });

    it('isolates correlation IDs across concurrent executions', async () => {
      const p1 = withCorrelationId('id-1', async () => {
        await new Promise(resolve => setTimeout(resolve, 10));
        return getCorrelationId();
      });
      const p2 = withCorrelationId('id-2', async () => {
        return getCorrelationId();
      });
      const [r1, r2] = await Promise.all([p1, p2]);
      expect(r1).toBe('id-1');
      expect(r2).toBe('id-2');
    });
  });

  describe('getCorrelationId', () => {
    it('returns null when outside of context', () => {
      expect(getCorrelationId()).toBeNull();
    });

    it('returns null when context is corrupted (e.g. empty string)', () => {
      withCorrelationId('', () => {
        expect(getCorrelationId()).toBeNull();
      });
    });

    it('returns null when context is corrupted (e.g. non-string)', () => {
      // @ts-expect-error Intentionally passing invalid type for failure boundary testing
      withCorrelationId(123, () => {
        expect(getCorrelationId()).toBeNull();
      });
    });
  });

  describe('getOrGenerateCorrelationId', () => {
    it('returns existing id when in context', () => {
      withCorrelationId('existing-id', () => {
        expect(getOrGenerateCorrelationId()).toBe('existing-id');
      });
    });

    it('generates a new id when outside of context', () => {
      const id = getOrGenerateCorrelationId();
      expect(typeof id).toBe('string');
      expect(id.length).toBeGreaterThan(0);
    });
  });

  describe('generateCorrelationId', () => {
    it('generates valid ULIDs', () => {
      const id1 = generateCorrelationId();
      const id2 = generateCorrelationId();
      expect(id1).not.toBe(id2);
      expect(id1).toMatch(/^[0-9A-Z]{26}$/); // Basic ULID check
    });
  });

  describe('sanitizeCorrelationId', () => {
    it('accepts valid correlation IDs', () => {
      expect(sanitizeCorrelationId('valid-ID_123')).toBe('valid-ID_123');
    });

    it('trims whitespace', () => {
      expect(sanitizeCorrelationId('  padded-id  ')).toBe('padded-id');
    });

    it('rejects empty strings', () => {
      expect(sanitizeCorrelationId('')).toBeNull();
      expect(sanitizeCorrelationId('   ')).toBeNull();
    });

    it('rejects oversized strings', () => {
      expect(sanitizeCorrelationId('a'.repeat(129))).toBeNull();
    });

    it('rejects invalid characters', () => {
      expect(sanitizeCorrelationId('invalid/id')).toBeNull();
      expect(sanitizeCorrelationId('invalid\nid')).toBeNull();
      expect(sanitizeCorrelationId('invalid\0id')).toBeNull();
    });

    it('rejects non-string values', () => {
      expect(sanitizeCorrelationId(123)).toBeNull();
      expect(sanitizeCorrelationId(null)).toBeNull();
      expect(sanitizeCorrelationId({})).toBeNull();
    });
  });

  describe('createRequestContextMiddleware', () => {
    it('uses req.correlationId if present', () => {
      const middleware = createRequestContextMiddleware();
      let capturedId: string | null = null;
      middleware({ correlationId: 'req-corr-id' }, {}, () => {
        capturedId = getCorrelationId();
      });
      expect(capturedId).toBe('req-corr-id');
    });

    it('falls back to req.requestId if correlationId is missing', () => {
      const middleware = createRequestContextMiddleware();
      let capturedId: string | null = null;
      middleware({ requestId: 'req-req-id' }, {}, () => {
        capturedId = getCorrelationId();
      });
      expect(capturedId).toBe('req-req-id');
    });

    it('proceeds without context if neither id is present', () => {
      const middleware = createRequestContextMiddleware();
      let capturedId: string | null = 'sentinel';
      middleware({}, {}, () => {
        capturedId = getCorrelationId();
      });
      expect(capturedId).toBeNull();
    });

    it('propagates errors from next() without crashing', () => {
      const middleware = createRequestContextMiddleware();
      const error = new Error('next error');
      expect(() => {
        middleware({ correlationId: 'id' }, {}, () => {
          throw error;
        });
      }).toThrow(error);
      // Context should be torn down implicitly by AsyncLocalStorage leaving scope
      expect(getCorrelationId()).toBeNull();
    });
  });
});
