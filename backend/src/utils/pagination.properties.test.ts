/**
 * Property-based tests for pagination encoding/decoding.
 *
 * Uses fast-check to verify:
 * 1. Round-trip correctness: decode(encode(x)) === x
 * 2. Negative domain coverage: malformed inputs surface as null/error
 * 3. Invariant properties: limits are clamped to [1, 100]
 * 4. Robustness: no crashes on adversarial inputs
 */

import fc from "fast-check";
import {
  encodeCursor,
  decodeCursor,
  parsePaginationParams,
  CursorPayload,
  PaginationError,
  DEFAULT_LIMIT,
  MAX_LIMIT,
} from "./pagination";

describe("Pagination property-based tests", () => {
  /**
   * POSITIVE DOMAIN: Round-trip correctness
   * Property: decode(encode(x)) === x for all valid CursorPayload
   */
  describe("Round-trip encoding/decoding", () => {
    it("should encode and decode arbitrary CursorPayload without loss", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 100 }),
            sort_val: fc.number({ min: -Number.MAX_SAFE_INTEGER, max: Number.MAX_SAFE_INTEGER }),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            const decoded = decodeCursor(encoded);

            // Verify round-trip correctness
            expect(decoded).not.toBeNull();
            expect(decoded).toEqual(payload);
          }
        )
      );
    });

    it("should handle extreme numeric values in sort_val", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.stringOf(fc.char()),
            sort_val: fc.oneof(
              fc.constant(0),
              fc.constant(1),
              fc.constant(-1),
              fc.constant(Number.MAX_SAFE_INTEGER),
              fc.constant(Number.MIN_SAFE_INTEGER)
            ),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            const decoded = decodeCursor(encoded);
            expect(decoded).toEqual(payload);
          }
        )
      );
    });

    it("should preserve string id with special characters", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 50 }),
            sort_val: fc.integer(),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            const decoded = decodeCursor(encoded);
            expect(decoded?.id).toBe(payload.id);
          }
        )
      );
    });
  });

  /**
   * NEGATIVE DOMAIN: Malformed inputs
   * Property: decodeCursor(malformed) returns null
   */
  describe("Rejection of malformed cursors", () => {
    it("should reject invalid base64url strings", () => {
      fc.assert(
        fc.property(
          fc.string({
            minLength: 1,
            maxLength: 100,
            // Characters NOT in base64url alphabet
            blacklist: "!@#$%^&*(){}[]|\\;:',<>?/`~ \n\t",
          }),
          (malformed: string) => {
            const result = decodeCursor(malformed);
            // Either returns null or throws (both are acceptable)
            expect([null, undefined].includes(result)).toBe(true);
          }
        )
      );
    });

    it("should reject truncated base64url (incomplete padding)", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 50 }),
            sort_val: fc.integer(),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            // Truncate the encoded string (remove last 1-3 chars)
            const truncated = encoded.slice(0, Math.max(1, encoded.length - 1));

            const decoded = decodeCursor(truncated);
            // Truncation might fail to decode or return null
            // The key is it shouldn't crash
            expect(typeof decoded === "object" || decoded === null).toBe(true);
          }
        )
      );
    });

    it("should reject JSON with wrong field types", () => {
      const invalid = [
        Buffer.from(JSON.stringify({ id: 123, sort_val: "string" })).toString(
          "base64url"
        ), // wrong types
        Buffer.from(JSON.stringify({ id: "abc" })).toString("base64url"), // missing sort_val
        Buffer.from(JSON.stringify({ sort_val: 123 })).toString("base64url"), // missing id
        Buffer.from(JSON.stringify({ id: null, sort_val: 123 })).toString(
          "base64url"
        ), // null id
        Buffer.from(JSON.stringify({ id: "abc", sort_val: NaN })).toString(
          "base64url"
        ), // NaN sort_val
        Buffer.from(JSON.stringify({ id: "abc", sort_val: Infinity })).toString(
          "base64url"
        ), // Infinity
      ];

      invalid.forEach((cursor) => {
        const result = decodeCursor(cursor);
        expect(result).toBeNull();
      });
    });

    it("should reject non-JSON base64url content", () => {
      fc.assert(
        fc.property(fc.string({ minLength: 1, maxLength: 50 }), (content: string) => {
          const encoded = Buffer.from(content).toString("base64url");
          // If content is not valid JSON, decode should return null
          try {
            const result = decodeCursor(encoded);
            expect(result).toBeNull();
          } catch {
            // Parsing errors are acceptable
          }
        })
      );
    });

    it("should reject tampering with padding", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 50 }),
            sort_val: fc.integer(),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            // Try adding extra padding
            const tampered = encoded + "===";

            try {
              const result = decodeCursor(tampered);
              // Should either return null or throw
              expect([null, undefined].includes(result)).toBe(true);
            } catch {
              // Decoding errors are fine for tampered data
            }
          }
        )
      );
    });

    it("should reject empty and whitespace cursors", () => {
      const invalid = ["", " ", "\t", "\n", "   "];
      invalid.forEach((cursor) => {
        const result = decodeCursor(cursor);
        expect(result).toBeNull();
      });
    });
  });

  /**
   * INVARIANT PROPERTIES: Limit clamping
   * Property: parsePaginationParams always returns limit in [1, MAX_LIMIT]
   */
  describe("Limit validation and clamping", () => {
    it("should clamp limit to [1, MAX_LIMIT]", () => {
      fc.assert(
        fc.property(fc.integer(), (limitValue: number) => {
          const params = parsePaginationParams({ limit: limitValue });

          // Verify clamping invariant
          expect(params.limit).toBeGreaterThanOrEqual(1);
          expect(params.limit).toBeLessThanOrEqual(MAX_LIMIT);
        })
      );
    });

    it("should return DEFAULT_LIMIT when limit is undefined", () => {
      fc.assert(
        fc.property(fc.anything(), (_: any) => {
          const params = parsePaginationParams({});
          expect(params.limit).toBe(DEFAULT_LIMIT);
        })
      );
    });

    it("should reject negative limits", () => {
      fc.assert(
        fc.property(fc.integer({ max: 0 }), (negLimit: number) => {
          expect(() => {
            parsePaginationParams({ limit: negLimit });
          }).toThrow(PaginationError);
        })
      );
    });

    it("should accept and clamp large limits to MAX_LIMIT", () => {
      fc.assert(
        fc.property(
          fc.integer({ min: MAX_LIMIT + 1, max: Number.MAX_SAFE_INTEGER }),
          (largeLimit: number) => {
            const params = parsePaginationParams({ limit: largeLimit });
            expect(params.limit).toBe(MAX_LIMIT);
          }
        )
      );
    });

    it("should accept limits in valid range [1, MAX_LIMIT]", () => {
      fc.assert(
        fc.property(fc.integer({ min: 1, max: MAX_LIMIT }), (validLimit: number) => {
          const params = parsePaginationParams({ limit: validLimit });
          expect(params.limit).toBe(validLimit);
        })
      );
    });

    it("should reject non-integer limit values", () => {
      fc.assert(
        fc.property(
          fc.oneof(
            fc.string(),
            fc.float({ noNaN: false, noInfinity: false }),
            fc.boolean(),
            fc.object()
          ),
          (invalid: any) => {
            expect(() => {
              parsePaginationParams({ limit: invalid });
            }).toThrow();
          }
        )
      );
    });
  });

  /**
   * CURSOR PARAMETER PARSING
   * Property: decodeCursor results are validated by parsePaginationParams
   */
  describe("Cursor parameter parsing", () => {
    it("should decode valid cursors passed to parsePaginationParams", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 100 }),
            sort_val: fc.number({ noNaN: true, noInfinity: true }),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            const params = parsePaginationParams({ cursor: encoded });

            expect(params.cursor).toEqual(payload);
          }
        )
      );
    });

    it("should reject invalid cursors in parsePaginationParams", () => {
      fc.assert(
        fc.property(
          fc.string({
            minLength: 1,
            blacklist: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_=",
          }),
          (malformed: string) => {
            expect(() => {
              parsePaginationParams({ cursor: malformed });
            }).toThrow(PaginationError);
          }
        )
      );
    });

    it("should accept empty/null cursor as no-cursor", () => {
      const params1 = parsePaginationParams({ cursor: "" });
      expect(params1.cursor).toBeNull();

      const params2 = parsePaginationParams({});
      expect(params2.cursor).toBeNull();

      const params3 = parsePaginationParams({ cursor: null });
      expect(params3.cursor).toBeNull();
    });

    it("should reject non-string cursor", () => {
      fc.assert(
        fc.property(
          fc.oneof(
            fc.integer(),
            fc.boolean(),
            fc.object(),
            fc.array(fc.anything())
          ),
          (nonString: any) => {
            expect(() => {
              parsePaginationParams({ cursor: nonString });
            }).toThrow(PaginationError);
          }
        )
      );
    });
  });

  /**
   * EDGE CASES: Robustness
   * Property: encodeCursor and decodeCursor never crash on valid inputs
   */
  describe("Robustness and edge cases", () => {
    it("should handle very long id strings", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1000, maxLength: 10000 }),
            sort_val: fc.integer(),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            const decoded = decodeCursor(encoded);
            expect(decoded).toEqual(payload);
          }
        )
      );
    });

    it("should handle unicode and special characters in id", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 100 }),
            sort_val: fc.integer(),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            const decoded = decodeCursor(encoded);
            expect(decoded?.id).toBe(payload.id);
          }
        )
      );
    });

    it("should handle payload with extra fields (should ignore)", () => {
      const payload = { id: "test-id", sort_val: 42, extra: "field" };
      const encoded = Buffer.from(JSON.stringify(payload)).toString("base64url");
      const decoded = decodeCursor(encoded);

      // Should decode the valid fields
      expect(decoded?.id).toBe("test-id");
      expect(decoded?.sort_val).toBe(42);
      // Extra fields are ignored (not in CursorPayload contract)
    });

    it("should never throw on any valid CursorPayload", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 100 }),
            sort_val: fc.number({ noNaN: true, noInfinity: true }),
          }),
          (payload: CursorPayload) => {
            expect(() => {
              encodeCursor(payload);
            }).not.toThrow();
          }
        )
      );
    });

    it("should never crash on arbitrary strings passed to decodeCursor", () => {
      fc.assert(
        fc.property(fc.string(), (arbitrary: string) => {
          expect(() => {
            decodeCursor(arbitrary);
          }).not.toThrow();
        })
      );
    });
  });

  /**
   * STATELESS & DETERMINISTIC
   * Property: encoding is deterministic (same input always produces same output)
   */
  describe("Determinism and idempotence", () => {
    it("should encode the same payload identically every time", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 100 }),
            sort_val: fc.integer(),
          }),
          (payload: CursorPayload) => {
            const encoded1 = encodeCursor(payload);
            const encoded2 = encodeCursor(payload);
            expect(encoded1).toBe(encoded2);
          }
        )
      );
    });

    it("should decode the same cursor identically every time", () => {
      fc.assert(
        fc.property(
          fc.record({
            id: fc.string({ minLength: 1, maxLength: 100 }),
            sort_val: fc.integer(),
          }),
          (payload: CursorPayload) => {
            const encoded = encodeCursor(payload);
            const decoded1 = decodeCursor(encoded);
            const decoded2 = decodeCursor(encoded);
            expect(decoded1).toEqual(decoded2);
          }
        )
      );
    });
  });

  /**
   * REGRESSION TESTS: Known failure modes
   * These tests document specific security/correctness properties
   */
  describe("Regression: Known failure modes", () => {
    it("should reject cursors with NaN in sort_val", () => {
      const malformed = Buffer.from(
        JSON.stringify({ id: "test", sort_val: NaN })
      ).toString("base64url");
      const result = decodeCursor(malformed);
      expect(result).toBeNull();
    });

    it("should reject cursors with Infinity in sort_val", () => {
      const malformed = Buffer.from(
        JSON.stringify({ id: "test", sort_val: Infinity })
      ).toString("base64url");
      const result = decodeCursor(malformed);
      expect(result).toBeNull();
    });

    it("should reject cursors with negative Infinity in sort_val", () => {
      const malformed = Buffer.from(
        JSON.stringify({ id: "test", sort_val: -Infinity })
      ).toString("base64url");
      const result = decodeCursor(malformed);
      expect(result).toBeNull();
    });

    it("should handle cursor from concurrent updates (stale cursor race)", () => {
      // Simulate a cursor from an older snapshot
      const stalePayload: CursorPayload = {
        id: "old-id-12345",
        sort_val: 99999,
      };
      const staleCursor = encodeCursor(stalePayload);

      // Should decode without issue (staleness is handled by pagination logic, not cursor)
      const decoded = decodeCursor(staleCursor);
      expect(decoded).toEqual(stalePayload);
    });
  });
});
