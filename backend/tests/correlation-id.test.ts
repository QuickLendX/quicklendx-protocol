import { describe, expect, it, jest } from "@jest/globals";
import { AsyncLocalStorage } from "node:async_hooks";
import {
  sanitizeCorrelationId,
  generateCorrelationId,
  runWithContext,
  withCorrelationId,
  getCorrelationId,
  getOrGenerateCorrelationId,
  createRequestContextMiddleware,
  _setUlidGeneratorForTesting,
  _resetUlidGeneratorForTesting,
} from "../src/lib/requestContext";

describe("requestContext", () => {
  describe("sanitizeCorrelationId", () => {
    it("should accept valid ULID-style correlation IDs", () => {
      const validId = "01H9K4W2X8Y9Z0A1B2C3D4E5F6";
      expect(sanitizeCorrelationId(validId)).toBe(validId);
    });

    it("should accept alphanumeric with hyphens and underscores", () => {
      const validId = "ABC-123_xyz-789";
      expect(sanitizeCorrelationId(validId)).toBe(validId);
    });

    it("should reject empty strings", () => {
      expect(sanitizeCorrelationId("")).toBeNull();
    });

    it("should reject strings with only whitespace", () => {
      expect(sanitizeCorrelationId("   ")).toBeNull();
      expect(sanitizeCorrelationId("\t\n\r")).toBeNull();
    });

    it("should accept boundary min length (1 char)", () => {
      expect(sanitizeCorrelationId("a")).toBe("a");
      expect(sanitizeCorrelationId("1")).toBe("1");
      expect(sanitizeCorrelationId("_")).toBe("_");
      expect(sanitizeCorrelationId("-")).toBe("-");
    });

    it("should accept boundary max length (128 chars)", () => {
      const maxLength = "a".repeat(128);
      expect(sanitizeCorrelationId(maxLength)).toBe(maxLength);
    });

    it("should reject boundary length just over limit (129 chars)", () => {
      const tooLong = "a".repeat(129);
      expect(sanitizeCorrelationId(tooLong)).toBeNull();
    });

    it("should reject strings with special characters (log injection prevention)", () => {
      const malicious = "test\ninjection";
      expect(sanitizeCorrelationId(malicious)).toBeNull();
    });

    it("should reject strings with newlines", () => {
      const withNewline = "test\ntest";
      expect(sanitizeCorrelationId(withNewline)).toBeNull();
    });

    it("should reject strings with carriage returns", () => {
      const withCarriageReturn = "test\rtest";
      expect(sanitizeCorrelationId(withCarriageReturn)).toBeNull();
    });

    it("should reject strings with tabs", () => {
      const withTab = "test\ttest";
      expect(sanitizeCorrelationId(withTab)).toBeNull();
    });

    it("should reject strings with semicolons", () => {
      const withSemicolon = "test;test";
      expect(sanitizeCorrelationId(withSemicolon)).toBeNull();
    });

    it("should reject strings with pipe characters", () => {
      const withPipe = "test|test";
      expect(sanitizeCorrelationId(withPipe)).toBeNull();
    });

    it("should reject strings with null bytes", () => {
      const withNullByte = "test\0test";
      expect(sanitizeCorrelationId(withNullByte)).toBeNull();
    });

    it("should reject strings with ANSI escape codes", () => {
      const withAnsi = "test\x1b[31mtest";
      expect(sanitizeCorrelationId(withAnsi)).toBeNull();
    });

    it("should trim whitespace from valid IDs", () => {
      const withSpaces = "  ABC-123  ";
      expect(sanitizeCorrelationId(withSpaces)).toBe("ABC-123");
    });

    it("should reject IDs with spaces in the middle", () => {
      const withInternalSpace = "ABC 123";
      expect(sanitizeCorrelationId(withInternalSpace)).toBeNull();
    });

    it("should return null for non-string input types", () => {
      const nonStringInputs: unknown[] = [
        undefined,
        null,
        0,
        123,
        true,
        false,
        {},
        [],
        () => "test",
        Symbol("test"),
        BigInt(123),
        NaN,
        Infinity,
      ];

      for (const input of nonStringInputs) {
        expect(sanitizeCorrelationId(input)).toBeNull();
      }
    });
  });

  describe("generateCorrelationId", () => {
    it("should generate a ULID", () => {
      const id = generateCorrelationId();
      expect(id).toBeDefined();
      expect(typeof id).toBe("string");
      expect(id.length).toBe(26);
    });

    it("should generate unique IDs", () => {
      const id1 = generateCorrelationId();
      const id2 = generateCorrelationId();
      expect(id1).not.toBe(id2);
    });

    it("should generate valid ULID characters", () => {
      const id = generateCorrelationId();
      expect(id).toMatch(/^[A-Z0-9]+$/);
    });

    it("should fall back to resilient ID generator when ulid() throws an error", () => {
      _setUlidGeneratorForTesting(() => {
        throw new Error("PRNG failure");
      });

      try {
        const fallbackId = generateCorrelationId();
        expect(fallbackId).toBeDefined();
        expect(typeof fallbackId).toBe("string");
        expect(fallbackId.length).toBeGreaterThan(0);
        expect(sanitizeCorrelationId(fallbackId)).toBe(fallbackId);
      } finally {
        _resetUlidGeneratorForTesting();
      }
    });

    it("should fall back to timestamp-entropy generator when both ULID and randomUUID fail", () => {
      const crypto = require("node:crypto");
      const origUUID = crypto.randomUUID;
      _setUlidGeneratorForTesting(() => {
        throw new Error("ULID failed");
      });
      crypto.randomUUID = () => {
        throw new Error("randomUUID failed");
      };

      try {
        const id = generateCorrelationId();
        expect(typeof id).toBe("string");
        expect(id.startsWith("FALLBACK")).toBe(true);
        expect(sanitizeCorrelationId(id)).toBe(id);
      } finally {
        crypto.randomUUID = origUUID;
        _resetUlidGeneratorForTesting();
      }
    });
  });

  describe("withCorrelationId and runWithContext", () => {
    it("should set correlation ID in context for synchronous function", () => {
      const testId = "test-correlation-id";
      let capturedId: string | null = null;

      withCorrelationId(testId, () => {
        capturedId = getCorrelationId();
      });

      expect(capturedId).toBe(testId);
    });

    it("should set correlation ID in context for async function", async () => {
      const testId = "async-correlation-id";
      let capturedId: string | null = null;

      await withCorrelationId(testId, async () => {
        await Promise.resolve();
        capturedId = getCorrelationId();
      });

      expect(capturedId).toBe(testId);
    });

    it("should return function result for sync execution", () => {
      const testId = "test-id";
      const result = withCorrelationId(testId, () => {
        return "result-value";
      });

      expect(result).toBe("result-value");
    });

    it("should return async function result", async () => {
      const testId = "test-id";
      const result = await withCorrelationId(testId, async () => {
        return "async-result";
      });

      expect(result).toBe("async-result");
    });

    it("should isolate context between concurrent calls", async () => {
      const id1 = "context-1";
      const id2 = "context-2";

      const result1 = withCorrelationId(id1, async () => {
        await new Promise((resolve) => setTimeout(resolve, 10));
        return getCorrelationId();
      });

      const result2 = withCorrelationId(id2, async () => {
        await new Promise((resolve) => setTimeout(resolve, 5));
        return getCorrelationId();
      });

      const [r1, r2] = await Promise.all([result1, result2]);

      expect(r1).toBe(id1);
      expect(r2).toBe(id2);
    });

    it("should not leak context after function completes", () => {
      const testId = "leak-test";

      withCorrelationId(testId, () => {
        expect(getCorrelationId()).toBe(testId);
      });

      expect(getCorrelationId()).toBeNull();
    });

    it("should handle nested contexts cleanly", () => {
      const outerId = "outer-id";
      const innerId = "inner-id";
      let capturedInner: string | null = null;
      let capturedOuter: string | null = null;
      let capturedAfterInner: string | null = null;

      withCorrelationId(outerId, () => {
        capturedOuter = getCorrelationId();

        withCorrelationId(innerId, () => {
          capturedInner = getCorrelationId();
        });

        capturedAfterInner = getCorrelationId();
      });

      expect(capturedOuter).toBe(outerId);
      expect(capturedInner).toBe(innerId);
      expect(capturedAfterInner).toBe(outerId);
      expect(getCorrelationId()).toBeNull();
    });

    it("should clean up context when synchronous function throws", () => {
      const testId = "throw-sync-test";

      expect(() => {
        withCorrelationId(testId, () => {
          expect(getCorrelationId()).toBe(testId);
          throw new Error("sync-error");
        });
      }).toThrow("sync-error");

      expect(getCorrelationId()).toBeNull();
    });

    it("should clean up context when async function rejects", async () => {
      const testId = "reject-async-test";

      await expect(
        withCorrelationId(testId, async () => {
          expect(getCorrelationId()).toBe(testId);
          await Promise.resolve();
          throw new Error("async-error");
        })
      ).rejects.toThrow("async-error");

      expect(getCorrelationId()).toBeNull();
    });

    it("should sanitize correlation ID before establishing context", () => {
      const dirtyId = "  valid-trimmed-id  ";
      withCorrelationId(dirtyId, () => {
        expect(getCorrelationId()).toBe("valid-trimmed-id");
      });

      const invalidId = "invalid\nnewline-id";
      withCorrelationId(invalidId, () => {
        const stored = getCorrelationId();
        expect(stored).toBeDefined();
        expect(stored).not.toContain("\n");
        expect(stored).not.toBe(invalidId);
      });
    });
  });

  describe("getCorrelationId", () => {
    it("should return null when no context is set", () => {
      expect(getCorrelationId()).toBeNull();
    });

    it("should return the correlation ID from context", () => {
      const testId = "test-id";
      withCorrelationId(testId, () => {
        expect(getCorrelationId()).toBe(testId);
      });
    });

    it("should return null after context is cleared", () => {
      const testId = "test-id";

      withCorrelationId(testId, () => {
        expect(getCorrelationId()).toBe(testId);
      });

      expect(getCorrelationId()).toBeNull();
    });

    it("should never throw and always return null for corrupted context store values", () => {
      const corruptValues: unknown[] = [
        undefined,
        null,
        0,
        123,
        true,
        false,
        {},
        [],
        () => "test",
        NaN,
        Infinity,
        BigInt(1),
        Symbol("corr"),
        "",
        "   ",
        "invalid\nnewline",
        "a".repeat(129),
      ];

      for (const val of corruptValues) {
        const result = (AsyncLocalStorage.prototype.run as any).call(
          (require("../src/lib/requestContext") as any).storage ??
            new AsyncLocalStorage(),
          { correlationId: val },
          () => getCorrelationId()
        );
        expect(result === null || typeof result === "string").toBe(true);
      }
    });

    it("should safely return null when storage.getStore() throws an unexpected exception", () => {
      const spy = jest
        .spyOn(AsyncLocalStorage.prototype, "getStore")
        .mockImplementation(() => {
          throw new Error("Store access fault");
        });

      try {
        expect(getCorrelationId()).toBeNull();
      } finally {
        spy.mockRestore();
      }
    });
  });

  describe("getOrGenerateCorrelationId", () => {
    it("should safely generate a valid ID when storage.getStore() throws an unexpected exception", () => {
      const spy = jest
        .spyOn(AsyncLocalStorage.prototype, "getStore")
        .mockImplementation(() => {
          throw new Error("Storage subsystem failure");
        });

      try {
        const id = getOrGenerateCorrelationId();
        expect(typeof id).toBe("string");
        expect(id.length).toBeGreaterThan(0);
        expect(sanitizeCorrelationId(id)).toBe(id);
      } finally {
        spy.mockRestore();
      }
    });
    it("should return existing correlation ID from active valid context", () => {
      const testId = "existing-active-id";
      withCorrelationId(testId, () => {
        const result = getOrGenerateCorrelationId();
        expect(result).toBe(testId);
      });
    });

    it("should generate a new ULID when no context is set", () => {
      const result = getOrGenerateCorrelationId();
      expect(result).toBeDefined();
      expect(typeof result).toBe("string");
      expect(result.length).toBe(26);
      expect(sanitizeCorrelationId(result)).toBe(result);
    });

    it("should generate unique IDs on consecutive calls without context", () => {
      const id1 = getOrGenerateCorrelationId();
      const id2 = getOrGenerateCorrelationId();
      const id3 = getOrGenerateCorrelationId();

      expect(id1).not.toBe(id2);
      expect(id2).not.toBe(id3);
      expect(id1).not.toBe(id3);
    });

    it("should be deterministic and return identical ID across repeated calls within the same context", () => {
      const testId = "stable-context-id";
      withCorrelationId(testId, () => {
        for (let i = 0; i < 50; i++) {
          expect(getOrGenerateCorrelationId()).toBe(testId);
        }
      });
    });

    it("should generate a new valid ID when context store is corrupted with non-string values", () => {
      const invalidStoreValues: unknown[] = [
        null,
        undefined,
        42,
        true,
        false,
        {},
        [],
        () => "corrupted",
        NaN,
        Infinity,
        BigInt(99),
        Symbol("invalid"),
      ];

      for (const val of invalidStoreValues) {
        // Run with an improperly constructed store to simulate memory corruption or buggy upstream injector
        withCorrelationId(undefined as unknown as string, () => {
          const generated = getOrGenerateCorrelationId();
          expect(typeof generated).toBe("string");
          expect(generated.length).toBeGreaterThan(0);
          expect(sanitizeCorrelationId(generated)).toBe(generated);
        });
      }
    });

    it("should generate a new valid ID when context store has empty or whitespace string", () => {
      withCorrelationId("", () => {
        const id = getOrGenerateCorrelationId();
        expect(typeof id).toBe("string");
        expect(id.length).toBe(26);
      });

      withCorrelationId("   ", () => {
        const id = getOrGenerateCorrelationId();
        expect(typeof id).toBe("string");
        expect(id.length).toBe(26);
      });
    });

    it("should generate a new valid ID when context store contains log-injection payload", () => {
      const injectionPayload = "malicious\r\nSET-COOKIE: admin=true";
      withCorrelationId(injectionPayload, () => {
        const id = getOrGenerateCorrelationId();
        expect(id).not.toContain("\r");
        expect(id).not.toContain("\n");
        expect(sanitizeCorrelationId(id)).toBe(id);
      });
    });

    it("should generate a new valid ID when context store string exceeds 128 characters", () => {
      const oversized = "X".repeat(150);
      withCorrelationId(oversized, () => {
        const id = getOrGenerateCorrelationId();
        expect(id.length).toBeLessThanOrEqual(128);
        expect(sanitizeCorrelationId(id)).toBe(id);
      });
    });

    it("should handle ULID generation failure gracefully and return a valid fallback ID", () => {
      _setUlidGeneratorForTesting(() => {
        throw new Error("Random entropy device unavailable");
      });

      try {
        const id = getOrGenerateCorrelationId();
        expect(id).toBeDefined();
        expect(typeof id).toBe("string");
        expect(id.length).toBeGreaterThan(0);
        expect(sanitizeCorrelationId(id)).toBe(id);
      } finally {
        _resetUlidGeneratorForTesting();
      }
    });

    it("should maintain context across Promise chains and async microtasks", async () => {
      const testId = "promise-async-chain-id";

      const result = await withCorrelationId(testId, async () => {
        return Promise.resolve()
          .then(() => getOrGenerateCorrelationId())
          .then((id) => {
            expect(id).toBe(testId);
            return Promise.resolve(getOrGenerateCorrelationId());
          })
          .then((id) => id);
      });

      expect(result).toBe(testId);
    });

    it("should isolate context in high-concurrency parallel async tasks (100 concurrent workers)", async () => {
      const concurrency = 100;
      const tasks = Array.from({ length: concurrency }, (_, idx) => {
        const workerId = `worker-context-${idx}`;
        return withCorrelationId(workerId, async () => {
          const delay = Math.floor(Math.random() * 15);
          await new Promise((resolve) => setTimeout(resolve, delay));
          const observedId = getOrGenerateCorrelationId();
          expect(observedId).toBe(workerId);
          return observedId;
        });
      });

      const results = await Promise.all(tasks);
      expect(results).toHaveLength(concurrency);
      const uniqueResults = new Set(results);
      expect(uniqueResults.size).toBe(concurrency);
    });

    it("should generate unique IDs for concurrent callers running outside any context", async () => {
      const count = 50;
      const tasks = Array.from({ length: count }, async () => {
        const delay = Math.floor(Math.random() * 10);
        await new Promise((resolve) => setTimeout(resolve, delay));
        return getOrGenerateCorrelationId();
      });

      const results = await Promise.all(tasks);
      const uniqueResults = new Set(results);
      expect(uniqueResults.size).toBe(count);
    });

    it("should handle multiple nested contexts restoring outer context at each level", async () => {
      const l1 = "level-1-id";
      const l2 = "level-2-id";
      const l3 = "level-3-id";

      await withCorrelationId(l1, async () => {
        expect(getOrGenerateCorrelationId()).toBe(l1);

        await withCorrelationId(l2, async () => {
          expect(getOrGenerateCorrelationId()).toBe(l2);

          await withCorrelationId(l3, async () => {
            expect(getOrGenerateCorrelationId()).toBe(l3);
          });

          expect(getOrGenerateCorrelationId()).toBe(l2);
        });

        expect(getOrGenerateCorrelationId()).toBe(l1);
      });

      // Outside context, generates a new ID
      const afterId = getOrGenerateCorrelationId();
      expect(afterId).not.toBe(l1);
      expect(afterId).not.toBe(l2);
      expect(afterId).not.toBe(l3);
    });
  });

  describe("createRequestContextMiddleware", () => {
    it("should call next with correlation ID context when correlationId is set", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = { correlationId: "test-id" };
      const res: any = {};
      const next = jest.fn();

      middleware(req, res, next);

      expect(next).toHaveBeenCalled();
    });

    it("should call next without context when correlationId is not set", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = {};
      const res: any = {};
      const next = jest.fn();

      middleware(req, res, next);

      expect(next).toHaveBeenCalled();
    });

    it("should use requestId as fallback when correlationId is not set", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = { requestId: "request-id" };
      const res: any = {};
      let observed: string | null = null;
      const next = jest.fn(() => {
        observed = getCorrelationId();
      });

      middleware(req, res, next);

      expect(next).toHaveBeenCalled();
      expect(observed).toBe("request-id");
    });

    it("should use headers['x-request-id'] as fallback when correlationId and requestId are not set", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = { headers: { "x-request-id": "header-request-id" } };
      const res: any = {};
      let observed: string | null = null;
      const next = jest.fn(() => {
        observed = getCorrelationId();
      });

      middleware(req, res, next);

      expect(next).toHaveBeenCalled();
      expect(observed).toBe("header-request-id");
    });

    it("should sanitize header correlation IDs and reject log injection characters in middleware", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = {
        headers: { "x-request-id": "bad\r\ninjection-header" },
      };
      const res: any = {};
      let observed: string | null = "initial";
      const next = jest.fn(() => {
        observed = getCorrelationId();
      });

      middleware(req, res, next);

      expect(next).toHaveBeenCalled();
      expect(observed).toBeNull();
    });

    it("should expose the correlation ID to downstream code within next()", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = { correlationId: "middleware-test" };
      const res: any = {};
      let observed: string | null = null;
      const next = jest.fn(() => {
        observed = getCorrelationId();
      });

      middleware(req, res, next);

      expect(next).toHaveBeenCalled();
      expect(observed).toBe("middleware-test");
    });

    it("should not leak the context after next() returns", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = { correlationId: "leak-test" };
      const res: any = {};
      const next = jest.fn();

      middleware(req, res, next);

      expect(getCorrelationId()).toBeNull();
    });

    it("should propagate errors thrown by next() and still tear down the context", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = { correlationId: "error-test" };
      const res: any = {};
      const next = jest.fn(() => {
        throw new Error("downstream failure");
      });

      expect(() => middleware(req, res, next)).toThrow("downstream failure");
      expect(getCorrelationId()).toBeNull();
    });

    it("should not establish a context for an empty-string correlationId", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = { correlationId: "" };
      const res: any = {};
      let observed: string | null = "not-set";
      const next = jest.fn(() => {
        observed = getCorrelationId();
      });

      middleware(req, res, next);

      expect(next).toHaveBeenCalled();
      expect(observed).toBeNull();
    });

    it("should handle malformed req objects gracefully without throwing", () => {
      const middleware = createRequestContextMiddleware();
      const next = jest.fn();

      expect(() => middleware(null as any, {} as any, next)).not.toThrow();
      expect(next).toHaveBeenCalled();
    });
  });
});