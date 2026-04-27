/**
 * Security regression tests for QuickLendX backend.
 *
 * These tests verify the guards described in backend/docs/security-checklist.md
 * and act as a regression suite so that future changes cannot silently remove
 * a security control without a test failure.
 *
 * Coverage areas:
 *   1. Query-parameter validation (injection, oversized values)
 *   2. Admin-endpoint authentication (missing key, wrong key, correct key)
 *   3. Request body size limit
 *   4. Rate-limit middleware
 *   5. Error-handler log redaction
 *   6. Security headers (helmet)
 *   7. Admin-auth helper: timingSafeStringEqual
 */

import { describe, it, expect, beforeEach, afterEach } from "@jest/globals";
import request from "supertest";
import app from "../src/app";
import indexApp from "../src/index";
import { rateLimiter } from "../src/middleware/rate-limit";
import {
  redactSensitiveFields,
} from "../src/middleware/error-handler";
import {
  isSafeQueryValue,
  MAX_QUERY_PARAM_LENGTH,
} from "../src/middleware/validate-query";
import {
  timingSafeStringEqual,
  ADMIN_KEY_HEADER,
} from "../src/middleware/admin-auth";

// ---------------------------------------------------------------------------
// 1. Query-parameter validation
// ---------------------------------------------------------------------------
describe("Query-parameter validation", () => {
  it("passes clean query params through", async () => {
    const res = await request(app).get("/api/v1/invoices?status=Verified");
    expect(res.status).toBe(200);
  });

  it("rejects a query param containing a null byte", async () => {
    const res = await request(app).get("/api/v1/invoices?status=Ver%00ified");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_QUERY_PARAM");
  });

  it("rejects a query param containing a newline (log injection)", async () => {
    // %0A = \n
    const res = await request(app).get(
      "/api/v1/invoices?status=ok%0AINJECTED"
    );
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_QUERY_PARAM");
  });

  it("rejects a query param containing a carriage return (CRLF injection)", async () => {
    const res = await request(app).get(
      "/api/v1/invoices?status=ok%0DINJECTED"
    );
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_QUERY_PARAM");
  });

  it("rejects a query param containing an angle bracket (HTML injection)", async () => {
    const res = await request(app).get(
      "/api/v1/invoices?status=%3Cscript%3E"
    );
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_QUERY_PARAM");
  });

  it("rejects a query param that exceeds the maximum length", async () => {
    const longValue = "A".repeat(MAX_QUERY_PARAM_LENGTH + 1);
    const res = await request(app).get(`/api/v1/invoices?status=${longValue}`);
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_QUERY_PARAM");
  });

  it("accepts a query param exactly at the maximum length", async () => {
    const maxValue = "A".repeat(MAX_QUERY_PARAM_LENGTH);
    // The value won't match any invoice but the request itself should be accepted.
    const res = await request(app).get(`/api/v1/invoices?status=${maxValue}`);
    expect(res.status).toBe(200);
  });

  // Unit tests for the helper function
  describe("isSafeQueryValue unit tests", () => {
    it("returns true for a normal string", () => {
      expect(isSafeQueryValue("Verified")).toBe(true);
    });

    it("returns false for a string with a null byte", () => {
      expect(isSafeQueryValue("foo\x00bar")).toBe(false);
    });

    it("returns false for a string with a newline", () => {
      expect(isSafeQueryValue("foo\nbar")).toBe(false);
    });

    it("returns false for a string with a carriage return", () => {
      expect(isSafeQueryValue("foo\rbar")).toBe(false);
    });

    it("returns false for a string with an angle bracket", () => {
      expect(isSafeQueryValue("<script>")).toBe(false);
    });

    it("returns false for a string that exceeds max length", () => {
      expect(isSafeQueryValue("A".repeat(MAX_QUERY_PARAM_LENGTH + 1))).toBe(
        false
      );
    });

    it("returns true for a string exactly at max length", () => {
      expect(isSafeQueryValue("A".repeat(MAX_QUERY_PARAM_LENGTH))).toBe(true);
    });
  });
});

// ---------------------------------------------------------------------------
// 2. Admin-endpoint authentication
// ---------------------------------------------------------------------------
describe("Admin-endpoint authentication", () => {
  const VALID_KEY = "test-admin-secret-key";

  beforeEach(() => {
    process.env.ADMIN_API_KEY = VALID_KEY;
  });

  afterEach(() => {
    delete process.env.ADMIN_API_KEY;
  });

  it("returns 401 when the admin key header is absent", async () => {
    const res = await request(indexApp)
      .post("/api/admin/maintenance")
      .send({ enabled: true });
    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("UNAUTHORIZED");
  });

  it("returns 403 when the admin key is wrong", async () => {
    const res = await request(indexApp)
      .post("/api/admin/maintenance")
      .set(ADMIN_KEY_HEADER, "wrong-key")
      .send({ enabled: true });
    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("FORBIDDEN");
  });

  it("allows the request when the correct admin key is supplied", async () => {
    const res = await request(indexApp)
      .post("/api/admin/maintenance")
      .set(ADMIN_KEY_HEADER, VALID_KEY)
      .send({ enabled: false });
    expect(res.status).toBe(200);
    expect(res.body.success).toBe(true);
  });

  it("returns 503 when ADMIN_API_KEY is not configured", async () => {
    delete process.env.ADMIN_API_KEY;
    const res = await request(indexApp)
      .post("/api/admin/maintenance")
      .set(ADMIN_KEY_HEADER, "any-key")
      .send({ enabled: false });
    expect(res.status).toBe(503);
    expect(res.body.error.code).toBe("ADMIN_NOT_CONFIGURED");
  });

  it("returns 400 for invalid body even with correct key", async () => {
    const res = await request(indexApp)
      .post("/api/admin/maintenance")
      .set(ADMIN_KEY_HEADER, VALID_KEY)
      .send({ enabled: "not-a-boolean" });
    expect(res.status).toBe(400);
  });

  // Unit tests for the timing-safe comparison helper
  describe("timingSafeStringEqual unit tests", () => {
    it("returns true for identical strings", () => {
      expect(timingSafeStringEqual("abc", "abc")).toBe(true);
    });

    it("returns false for different strings of the same length", () => {
      expect(timingSafeStringEqual("abc", "xyz")).toBe(false);
    });

    it("returns false for strings of different lengths", () => {
      expect(timingSafeStringEqual("abc", "abcd")).toBe(false);
    });

    it("returns false for empty vs non-empty string", () => {
      expect(timingSafeStringEqual("", "a")).toBe(false);
    });

    it("returns true for two empty strings", () => {
      expect(timingSafeStringEqual("", "")).toBe(true);
    });
  });
});

// ---------------------------------------------------------------------------
// 3. Request body size limit
// ---------------------------------------------------------------------------
describe("Request body size limit", () => {
  it("accepts a request body within the 100 KB limit", async () => {
    // A small valid body — should pass through to the route handler.
    const res = await request(app)
      .post("/api/v1/invoices")
      .set("Content-Type", "application/json")
      .send({ data: "small" });
    // The route doesn't exist as a POST, so we expect 404 — not a body-size error.
    expect(res.status).not.toBe(413);
  });

  it("rejects a request body that exceeds 100 KB", async () => {
    // Build a JSON payload just over 100 KB.
    const bigPayload = JSON.stringify({ data: "x".repeat(110 * 1024) });
    const res = await request(app)
      .post("/api/v1/invoices")
      .set("Content-Type", "application/json")
      .send(bigPayload);
    expect(res.status).toBe(413);
  });
});

// ---------------------------------------------------------------------------
// 4. Rate-limit middleware (regression — ensure it still fires)
// ---------------------------------------------------------------------------
describe("Rate-limit middleware", () => {
  it("returns 429 after the limit is exhausted for an IP", async () => {
    const testIp = "10.0.0.99";
    // Exhaust the bucket for this IP.
    for (let i = 0; i < 1000; i++) {
      await rateLimiter.consume(testIp);
    }
    const res = await request(app)
      .get("/health")
      .set("X-Forwarded-For", testIp);
    expect(res.status).toBe(429);
    expect(res.body.error.code).toBe("RATE_LIMIT_EXCEEDED");
  });
});

// ---------------------------------------------------------------------------
// 5. Error-handler log redaction
// ---------------------------------------------------------------------------
describe("Error-handler log redaction", () => {
  describe("redactSensitiveFields unit tests", () => {
    it("redacts a top-level password field", () => {
      const result = redactSensitiveFields({ password: "s3cr3t", name: "Alice" }) as any;
      expect(result.password).toBe("[REDACTED]");
      expect(result.name).toBe("Alice");
    });

    it("redacts a top-level token field", () => {
      const result = redactSensitiveFields({ token: "jwt.abc.def" }) as any;
      expect(result.token).toBe("[REDACTED]");
    });

    it("redacts a top-level apiKey field", () => {
      const result = redactSensitiveFields({ apiKey: "key-123" }) as any;
      expect(result.apiKey).toBe("[REDACTED]");
    });

    it("redacts a top-level api_key field", () => {
      const result = redactSensitiveFields({ api_key: "key-456" }) as any;
      expect(result.api_key).toBe("[REDACTED]");
    });

    it("redacts a top-level secret field", () => {
      const result = redactSensitiveFields({ secret: "shh" }) as any;
      expect(result.secret).toBe("[REDACTED]");
    });

    it("redacts a top-level privateKey field", () => {
      const result = redactSensitiveFields({ privateKey: "-----BEGIN..." }) as any;
      expect(result.privateKey).toBe("[REDACTED]");
    });

    it("redacts a top-level mnemonic field", () => {
      const result = redactSensitiveFields({ mnemonic: "word1 word2" }) as any;
      expect(result.mnemonic).toBe("[REDACTED]");
    });

    it("redacts a top-level seed field", () => {
      const result = redactSensitiveFields({ seed: "0xdeadbeef" }) as any;
      expect(result.seed).toBe("[REDACTED]");
    });

    it("redacts nested sensitive fields", () => {
      const result = redactSensitiveFields({
        user: { password: "hunter2", email: "a@b.com" },
      }) as any;
      expect(result.user.password).toBe("[REDACTED]");
      expect(result.user.email).toBe("a@b.com");
    });

    it("redacts sensitive fields inside arrays", () => {
      const result = redactSensitiveFields([
        { token: "abc" },
        { name: "safe" },
      ]) as any[];
      expect(result[0].token).toBe("[REDACTED]");
      expect(result[1].name).toBe("safe");
    });

    it("passes through non-object primitives unchanged", () => {
      expect(redactSensitiveFields("hello")).toBe("hello");
      expect(redactSensitiveFields(42)).toBe(42);
      expect(redactSensitiveFields(null)).toBe(null);
      expect(redactSensitiveFields(true)).toBe(true);
    });

    it("does not mutate the original object", () => {
      const original = { password: "secret", name: "Bob" };
      redactSensitiveFields(original);
      expect(original.password).toBe("secret");
    });
  });
});

// ---------------------------------------------------------------------------
// 6. Security headers (helmet)
// ---------------------------------------------------------------------------
describe("Security headers", () => {
  it("sets X-Content-Type-Options: nosniff", async () => {
    const res = await request(app).get("/health");
    expect(res.headers["x-content-type-options"]).toBe("nosniff");
  });

  it("sets X-Frame-Options to deny clickjacking", async () => {
    const res = await request(app).get("/health");
    // helmet sets SAMEORIGIN or DENY depending on version
    expect(res.headers["x-frame-options"]).toBeDefined();
  });

  it("does not expose X-Powered-By header", async () => {
    const res = await request(app).get("/health");
    expect(res.headers["x-powered-by"]).toBeUndefined();
  });
});
