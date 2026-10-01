import {
  sanitiseResponse,
  sanitiseRequest,
  redactObject,
  hashValue,
  redactByTier,
  FieldTier,
  findSecretLeak
} from '../policy';

describe('Logging Policy', () => {
  describe('sanitiseResponse', () => {
    it('returns a log-safe snapshot of an outgoing HTTP response body', () => {
      const response = sanitiseResponse(200, { data: 'hello' });
      expect(response.statusCode).toBe(200);
      expect(response.body).toEqual(expect.any(Object));
    });

    it('handles null body', () => {
      const response = sanitiseResponse(204, null);
      expect(response.statusCode).toBe(204);
      expect(response.body).toBeNull();
    });

    it('handles non-object body', () => {
      const response = sanitiseResponse(200, 'string body');
      expect(response.statusCode).toBe(200);
      expect(response.body).toBeNull();
    });

    it('redacts sensitive fields in body deterministically', () => {
      // Assuming 'password' is a SECRET or PRIVATE field based on deny-by-default
      const response = sanitiseResponse(200, { password: 'secret123', publicField: 'value' });
      expect(response.body?.password).not.toBe('secret123');
      expect(findSecretLeak(response.body)).toBeNull();
    });

    it('safely handles circular references in response body without crashing', () => {
      const body: any = { a: 1 };
      body.self = body;
      const response = sanitiseResponse(200, body);
      // The circular ref should be caught by hashValue / redactObject and replaced with [REDACTED]
      expect(response.statusCode).toBe(200);
      expect(response.body).toBeDefined();
    });
  });

  describe('sanitiseRequest', () => {
    it('redacts request queries, headers, and bodies', () => {
      const req = {
        method: 'POST',
        path: '/test',
        query: { secret_token: '123' },
        headers: { authorization: 'Bearer token', 'x-custom': 'val' },
        body: { password: 'pass', email: 'test@test.com' }
      };
      const safeReq = sanitiseRequest(req);
      
      expect(safeReq.method).toBe('POST');
      expect(safeReq.path).toBe('/test');
      expect(safeReq.headers.authorization).not.toBe('Bearer token');
      expect(safeReq.body?.password).not.toBe('pass');
      expect(findSecretLeak(safeReq)).toBeNull();
    });
    
    it('handles requests with no body', () => {
      const req = {
        method: 'GET',
        path: '/',
        query: {},
        headers: {}
      };
      const safeReq = sanitiseRequest(req);
      expect(safeReq.body).toBeNull();
    });
  });

  describe('redactObject', () => {
    it('redacts unknown fields as PRIVATE by default', () => {
      const obj = { unknown_field_123: 'value' };
      const redacted = redactObject(obj);
      expect(redacted.unknown_field_123).not.toBe('value');
      expect(typeof redacted.unknown_field_123).toBe('string');
      expect((redacted.unknown_field_123 as string).startsWith('sha256:')).toBe(true);
    });

    it('redacts arrays of objects', () => {
      // The array elements should be redacted properly based on their object keys
      const obj = { data: [{ unknown_field: 'secret' }] };
      const redacted = redactObject(obj);
      // Depending on whether 'data' is PUBLIC, it might recurse or hash the whole array
      expect(findSecretLeak(redacted)).toBeNull();
    });
  });

  describe('hashValue determinism and failure boundaries', () => {
    it('produces the same hash for structurally identical objects regardless of key order', () => {
      const hash1 = hashValue({ a: 1, b: 2 });
      const hash2 = hashValue({ b: 2, a: 1 });
      expect(hash1).toBe(hash2);
    });

    it('distinguishes different types with the same string representation', () => {
      const hashString = hashValue('1');
      const hashNumber = hashValue(1);
      expect(hashString).not.toBe(hashNumber);
    });

    it('throws on cyclic structures', () => {
      const obj: any = {};
      obj.self = obj;
      expect(() => hashValue(obj)).toThrow('hashValue: cyclic structure is not supported');
    });
  });

  describe('redactByTier', () => {
    it('returns original value for PUBLIC', () => {
      expect(redactByTier('value', FieldTier.PUBLIC)).toBe('value');
    });

    it('returns [REDACTED] for SECRET', () => {
      expect(redactByTier('value', FieldTier.SECRET)).toBe('[REDACTED]');
    });

    it('hashes value for PRIVATE', () => {
      const redacted = redactByTier('value', FieldTier.PRIVATE);
      expect(redacted).not.toBe('value');
      expect(typeof redacted).toBe('string');
      expect((redacted as string).startsWith('sha256:')).toBe(true);
    });

    it('returns [REDACTED] for PRIVATE if hashing fails (e.g. cyclic structure)', () => {
      const obj: any = {};
      obj.self = obj;
      expect(redactByTier(obj, FieldTier.PRIVATE)).toBe('[REDACTED]');
    });
  });
});
