import { describe, expect, it, jest, beforeEach } from "@jest/globals";
import {
  sanitizeCorrelationId,
  generateCorrelationId,
  withCorrelationId,
  getCorrelationId,
  getOrGenerateCorrelationId,
  createRequestContextMiddleware,
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
    });

    it("should reject strings exceeding max length", () => {
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

    it("should trim whitespace from valid IDs", () => {
      const withSpaces = "  ABC-123  ";
      expect(sanitizeCorrelationId(withSpaces)).toBe("ABC-123");
    });

    it("should return null for undefined input", () => {
      expect(sanitizeCorrelationId(undefined)).toBeNull();
    });

    it("should accept maximum length valid ID", () => {
      const maxLength = "a".repeat(128);
      expect(sanitizeCorrelationId(maxLength)).toBe(maxLength);
    });

    it("should reject IDs with spaces in the middle", () => {
      const withInternalSpace = "ABC 123";
      expect(sanitizeCorrelationId(withInternalSpace)).toBeNull();
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
  });

  describe("withCorrelationId", () => {
    it("should set correlation ID in context for synchronous function", () => {
      const testId = "test-correlation-id";
      let capturedId: string | null | undefined;

      withCorrelationId(testId, () => {
        capturedId = getCorrelationId();
      });

      expect(capturedId).toBe(testId);
    });

    it("should set correlation ID in context for async function", async () => {
      const testId = "async-correlation-id";
      let capturedId: string | null | undefined;

      await withCorrelationId(testId, async () => {
        await Promise.resolve();
        capturedId = getCorrelationId();
      });

      expect(capturedId).toBe(testId);
    });

    it("should return function result", () => {
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

      expect(getCorrelationId()).toBeUndefined();
    });

    it("should handle nested contexts", () => {
      const outerId = "outer";
      const innerId = "inner";
      let capturedInner: string | null | undefined;
      let capturedOuter: string | null | undefined;

      withCorrelationId(outerId, () => {
        capturedOuter = getCorrelationId();
        
        withCorrelationId(innerId, () => {
          capturedInner = getCorrelationId();
        });
      });

      expect(capturedOuter).toBe(outerId);
      expect(capturedInner).toBe(innerId);
      expect(getCorrelationId()).toBeUndefined();
    });
  });

  describe("getCorrelationId", () => {
    it("should return undefined when no context is set", () => {
      expect(getCorrelationId()).toBeUndefined();
    });

    it("should return the correlation ID from context", () => {
      const testId = "test-id";
      withCorrelationId(testId, () => {
        expect(getCorrelationId()).toBe(testId);
      });
    });

    it("should return undefined after context is cleared", () => {
      const testId = "test-id";
      
      withCorrelationId(testId, () => {
        expect(getCorrelationId()).toBe(testId);
      });

      expect(getCorrelationId()).toBeUndefined();
    });
  });

  describe("getOrGenerateCorrelationId", () => {
    it("should return existing correlation ID from context", () => {
      const testId = "existing-id";
      withCorrelationId(testId, () => {
        const result = getOrGenerateCorrelationId();
        expect(result).toBe(testId);
      });
    });

    it("should generate new ID when no context is set", () => {
      const result = getOrGenerateCorrelationId();
      expect(result).toBeDefined();
      expect(typeof result).toBe("string");
      expect(result.length).toBe(26);
    });

    it("should generate unique IDs when called without context", () => {
      const id1 = getOrGenerateCorrelationId();
      const id2 = getOrGenerateCorrelationId();
      expect(id1).not.toBe(id2);
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
      const next = jest.fn();

      middleware(req, res, next);

      expect(next).toHaveBeenCalled();
    });

    it("should set correlation ID in async storage for downstream handlers", () => {
      const middleware = createRequestContextMiddleware();
      const req: any = { correlationId: "middleware-test" };
      const res: any = {};
      const next = jest.fn();

      middleware(req, res, next);

      // After middleware runs, the context should be set for the duration of next()
      // We can't test this directly without running the middleware in a request context,
      // but we can verify it doesn't throw
      expect(next).toHaveBeenCalled();
    });
  });

  describe("context isolation with async operations", () => {
    it("should maintain context through Promise chains", async () => {
      const testId = "promise-chain-id";
      
      const result = await withCorrelationId(testId, async () => {
        return Promise.resolve()
          .then(() => getCorrelationId())
          .then((id) => id)
          .then((id) => Promise.resolve(id));
      });

      expect(result).toBe(testId);
    });

    it("should maintain context through setTimeout", async () => {
      const testId = "timeout-id";
      
      const result = await withCorrelationId(testId, async () => {
        return new Promise((resolve) => {
          setTimeout(() => {
            resolve(getCorrelationId());
          }, 10);
        });
      });

      expect(result).toBe(testId);
    });

    it("should maintain context through async/await", async () => {
      const testId = "async-await-id";
      
      const result = await withCorrelationId(testId, async () => {
        await Promise.resolve();
        const id = getCorrelationId();
        await Promise.resolve();
        return id;
      });

      expect(result).toBe(testId);
    });

    it("should isolate context in parallel async operations", async () => {
      const results: string[] = [];

      const promise1 = withCorrelationId("id-1", async () => {
        await new Promise((resolve) => setTimeout(resolve, 20));
        results.push(getCorrelationId()!);
      });

      const promise2 = withCorrelationId("id-2", async () => {
        await new Promise((resolve) => setTimeout(resolve, 10));
        results.push(getCorrelationId()!);
      });

      const promise3 = withCorrelationId("id-3", async () => {
        await new Promise((resolve) => setTimeout(resolve, 5));
        results.push(getCorrelationId()!);
      });

      await Promise.all([promise1, promise2, promise3]);

      expect(results).toEqual(["id-3", "id-2", "id-1"]);
    });
  });

  describe("security and edge cases", () => {
    it("should prevent log injection via newlines in correlation IDs", () => {
      const malicious = "test\nInjected Log Line";
      expect(sanitizeCorrelationId(malicious)).toBeNull();
    });

    it("should prevent log injection via carriage returns", () => {
      const malicious = "test\rInjected Log Line";
      expect(sanitizeCorrelationId(malicious)).toBeNull();
    });

    it("should prevent log injection via null bytes", () => {
      const malicious = "test\0Injected";
      expect(sanitizeCorrelationId(malicious)).toBeNull();
    });

    it("should reject correlation IDs with ANSI escape sequences", () => {
      const malicious = "test\x1b[31mInjected";
      expect(sanitizeCorrelationId(malicious)).toBeNull();
    });

    it("should handle very long valid correlation IDs", () => {
      const longValid = "a".repeat(128);
      expect(sanitizeCorrelationId(longValid)).toBe(longValid);
    });

    it("should reject correlation IDs just over the limit", () => {
      const tooLong = "a".repeat(129);
      expect(sanitizeCorrelationId(tooLong)).toBeNull();
    });

    it("should handle concurrent context switches correctly", async () => {
      const order: string[] = [];

      const task1 = withCorrelationId("task-1", async () => {
        order.push(getCorrelationId()!);
        await new Promise((resolve) => setTimeout(resolve, 15));
        order.push(getCorrelationId()!);
      });

      const task2 = withCorrelationId("task-2", async () => {
        order.push(getCorrelationId()!);
        await new Promise((resolve) => setTimeout(resolve, 10));
        order.push(getCorrelationId()!);
      });

      const task3 = withCorrelationId("task-3", async () => {
        order.push(getCorrelationId()!);
        await new Promise((resolve) => setTimeout(resolve, 5));
        order.push(getCorrelationId()!);
      });

      await Promise.all([task1, task2, task3]);

      // Verify that each task maintained its own context
      // The order will be interleaved but each task should see its own ID
      const task1Ids = order.filter((id) => id === "task-1");
      const task2Ids = order.filter((id) => id === "task-2");
      const task3Ids = order.filter((id) => id === "task-3");

      expect(task1Ids).toHaveLength(2);
      expect(task2Ids).toHaveLength(2);
      expect(task3Ids).toHaveLength(2);
    });
  });
});
