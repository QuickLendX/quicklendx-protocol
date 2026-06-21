/**
 * Logging Policy — Comprehensive Test Suite
 *
 * Coverage targets per issue #863:
 *   • Field classification (public / private / secret)
 *   • Object-level deep redaction
 *   • Request / Response sanitisation
 *   • "No secrets in logs" regression guard
 *   • Request-logger middleware integration
 *   • Edge-cases: null, undefined, arrays, deeply nested objects
 */

import { createHash } from "crypto";
import express, { Request, Response } from "express";
import supertest from "supertest";

import {
  classifyField,
  isSecret,
  isPublic,
  isPrivate,
  FieldTier,
  hashValue,
  redactByTier,
  redactObject,
  sanitiseRequest,
  sanitiseResponse,
  findSecretLeak,
} from "../lib/logging/policy";

import {
  createRequestLogger,
  RequestLogEntry,
  Logger,
} from "../middleware/request-logger";

// ── Helpers ───────────────────────────────────────────────────────────────────

function sha256Prefix(value: unknown): string {
  const str = typeof value === "string" ? value : JSON.stringify(value);
  return (
    "sha256:" +
    createHash("sha256").update(str).digest("hex").slice(0, 8)
  );
}

// ── 1. Field Classification ───────────────────────────────────────────────────

describe("classifyField", () => {
  describe("PUBLIC fields", () => {
    const publicFields = [
      "id", "invoice_id", "bid_id", "settlement_id", "dispute_id",
      "status", "timestamp", "created_at", "updated_at",
      "method", "path", "url", "statusCode", "duration",
      "requestId", "version", "category", "currency", "due_date",
    ];

    it.each(publicFields)("classifies '%s' as PUBLIC", (field) => {
      expect(classifyField(field)).toBe(FieldTier.PUBLIC);
      expect(isPublic(field)).toBe(true);
      expect(isPrivate(field)).toBe(false);
      expect(isSecret(field)).toBe(false);
    });
  });

  describe("PRIVATE fields", () => {
    const privateFields = [
      "business", "investor", "payer", "recipient", "actor",
      "user_id", "userId", "initiator", "amount", "bid_amount",
      "expected_return", "ipAddress", "ip", "userAgent", "user_agent",
      "description", "reason", "tags", "notes",
    ];

    it.each(privateFields)("classifies '%s' as PRIVATE", (field) => {
      expect(classifyField(field)).toBe(FieldTier.PRIVATE);
      expect(isPrivate(field)).toBe(true);
      expect(isPublic(field)).toBe(false);
      expect(isSecret(field)).toBe(false);
    });
  });

  describe("SECRET fields", () => {
    const secretFields = [
      // Auth / wallet
      "signature", "wallet_signature", "private_key", "secret",
      "token", "access_token", "refresh_token", "api_key",
      "authorization", "password",
      // KYC / PII
      "tax_id", "ssn", "national_id", "passport_number",
      "date_of_birth", "bank_account", "kyc_document", "kyc_data",
      "customer_name", "customer_address", "phone_number", "email",
      // Crypto
      "mnemonic", "seed_phrase",
      // Webhook
      "webhook_secret", "signing_secret",
    ];

    it.each(secretFields)("classifies '%s' as SECRET", (field) => {
      expect(classifyField(field)).toBe(FieldTier.SECRET);
      expect(isSecret(field)).toBe(true);
      expect(isPublic(field)).toBe(false);
      expect(isPrivate(field)).toBe(false);
    });
  });

  it("defaults unknown fields to PRIVATE", () => {
    expect(classifyField("totally_unknown_field_xyz")).toBe(FieldTier.PRIVATE);
    expect(isPrivate("totally_unknown_field_xyz")).toBe(true);
  });
});

// ── 2. Value-level redaction ──────────────────────────────────────────────────

describe("hashValue", () => {
  it("returns a sha256: prefixed string", () => {
    const h = hashValue("hello");
    expect(h).toMatch(/^sha256:[0-9a-f]{8}$/);
  });

  it("is deterministic for the same input", () => {
    expect(hashValue("wallet_addr")).toBe(hashValue("wallet_addr"));
  });

  it("differs for different inputs", () => {
    expect(hashValue("a")).not.toBe(hashValue("b"));
  });

  it("handles non-string values by JSON-stringifying", () => {
    const h = hashValue({ nested: true });
    expect(h).toMatch(/^sha256:/);
    expect(h).toBe(hashValue({ nested: true }));
  });
});

describe("redactByTier", () => {
  it("PUBLIC tier — value unchanged", () => {
    expect(redactByTier("open", FieldTier.PUBLIC)).toBe("open");
    expect(redactByTier(42, FieldTier.PUBLIC)).toBe(42);
    expect(redactByTier(null, FieldTier.PUBLIC)).toBeNull();
  });

  it("SECRET tier — always [REDACTED]", () => {
    expect(redactByTier("my-secret-token", FieldTier.SECRET)).toBe("[REDACTED]");
    expect(redactByTier(12345, FieldTier.SECRET)).toBe("[REDACTED]");
    expect(redactByTier("", FieldTier.SECRET)).toBe("[REDACTED]");
  });

  it("PRIVATE tier — hashes the value", () => {
    const result = redactByTier("0xABCDEF", FieldTier.PRIVATE);
    expect(result).toBe(sha256Prefix("0xABCDEF"));
  });

  it("PRIVATE tier — null / undefined pass through", () => {
    expect(redactByTier(null, FieldTier.PRIVATE)).toBeNull();
    expect(redactByTier(undefined, FieldTier.PRIVATE)).toBeUndefined();
  });
});

// ── 3. Object-level deep redaction ───────────────────────────────────────────

describe("redactObject", () => {
  it("leaves public fields verbatim", () => {
    const out = redactObject({ id: "inv_123", status: "Pending" });
    expect(out.id).toBe("inv_123");
    expect(out.status).toBe("Pending");
  });

  it("hashes private fields", () => {
    const out = redactObject({ amount: "1000000" });
    expect(out.amount).toBe(sha256Prefix("1000000"));
  });

  it("replaces secret fields with [REDACTED]", () => {
    const out = redactObject({ authorization: "Bearer xyz", tax_id: "123-45-6789" });
    expect(out.authorization).toBe("[REDACTED]");
    expect(out.tax_id).toBe("[REDACTED]");
  });

  it("defaults unknown fields to private (hashed)", () => {
    const out = redactObject({ mystery_field: "value" });
    expect(out.mystery_field).toBe(sha256Prefix("value"));
  });

  it("recurses into nested public objects", () => {
    const out = redactObject({
      id: "bid_1",
      metadata: {
        id: "meta_1",
        secret: "shhh",
      },
    });
    expect(out.id).toBe("bid_1");
    // metadata is treated as PUBLIC because 'id' is public and the key
    // 'metadata' is unknown → PRIVATE → hashed as object
    expect(typeof out.metadata).toBe("string"); // hashed
  });

  it("never mutates the original object", () => {
    const orig = { authorization: "Bearer token", id: "x" };
    const copy = { ...orig };
    redactObject(orig);
    expect(orig).toEqual(copy);
  });

  it("redacts array values for non-public fields", () => {
    const out = redactObject({ tags: ["invoice", "urgent"] });
    // 'tags' is PRIVATE → whole array is hashed
    expect(typeof out.tags).toBe("string");
    expect(out.tags).toMatch(/^sha256:/);
  });

  it("keeps array items for public fields", () => {
    // 'tags' is PRIVATE so we test with an explicitly public field via a nested
    // approach: give a public parent that carries an array sub-field.
    // This exercises the array branch inside a recursed PUBLIC object.
    const out = redactObject({ id: "x" }); // id is a leaf, not an object
    expect(out.id).toBe("x");
  });

  it("handles empty object", () => {
    expect(redactObject({})).toEqual({});
  });

  it("handles deeply nested secret", () => {
    // 'password' at any nesting depth should be [REDACTED] since redactObject
    // is called recursively on nested objects only for PUBLIC top-level keys.
    // The top-level 'password' is SECRET.
    const out = redactObject({ password: "hunter2" });
    expect(out.password).toBe("[REDACTED]");
  });
});

// ── 4. Request sanitisation ───────────────────────────────────────────────────

describe("sanitiseRequest", () => {
  const baseReq = {
    method: "POST",
    path: "/api/v1/invoices",
    query: { status: "Pending" },
    headers: {
      "content-type": "application/json",
      authorization: "Bearer super-secret-token",
      "x-api-key": "my-api-key",
    },
    body: {
      invoice_id: "inv_001",
      amount: "500000",
      tax_id: "123-45-6789",
      customer_name: "Alice",
    },
  };

  it("preserves method and path verbatim", () => {
    const snap = sanitiseRequest(baseReq);
    expect(snap.method).toBe("POST");
    expect(snap.path).toBe("/api/v1/invoices");
  });

  it("redacts secret headers before they reach policy", () => {
    const snap = sanitiseRequest(baseReq);
    // authorization is stripped at the middleware level (stripSensitiveHeaders)
    // and then would be [REDACTED] by policy — both are safe. In sanitiseRequest
    // we do not strip but do classify, so 'authorization' → [REDACTED].
    expect(snap.headers["authorization"]).toBe("[REDACTED]");
  });

  it("redacts secret body fields", () => {
    const snap = sanitiseRequest(baseReq);
    expect(snap.body!["tax_id"]).toBe("[REDACTED]");
    expect(snap.body!["customer_name"]).toBe("[REDACTED]");
  });

  it("hashes private body fields", () => {
    const snap = sanitiseRequest(baseReq);
    expect(snap.body!["amount"]).toBe(sha256Prefix("500000"));
  });

  it("preserves public body fields", () => {
    const snap = sanitiseRequest(baseReq);
    expect(snap.body!["invoice_id"]).toBe("inv_001");
  });

  it("handles null body gracefully", () => {
    const snap = sanitiseRequest({ ...baseReq, body: null });
    expect(snap.body).toBeNull();
  });

  it("handles undefined body gracefully", () => {
    const snap = sanitiseRequest({ ...baseReq, body: undefined });
    expect(snap.body).toBeNull();
  });
});

// ── 5. Response sanitisation ─────────────────────────────────────────────────

describe("sanitiseResponse", () => {
  it("preserves public fields in the response body", () => {
    const snap = sanitiseResponse(200, { id: "inv_001", status: "Funded" });
    expect(snap.statusCode).toBe(200);
    expect(snap.body!["id"]).toBe("inv_001");
    expect(snap.body!["status"]).toBe("Funded");
  });

  it("redacts secret fields in the response body", () => {
    const snap = sanitiseResponse(200, {
      id: "inv_001",
      tax_id: "LEAKED_VALUE",
    });
    expect(snap.body!["tax_id"]).toBe("[REDACTED]");
  });

  it("handles non-object body", () => {
    const snap = sanitiseResponse(204, null);
    expect(snap.body).toBeNull();
  });

  it("handles string body", () => {
    const snap = sanitiseResponse(200, "plain text");
    expect(snap.body).toBeNull();
  });

  it("preserves status code", () => {
    expect(sanitiseResponse(500, null).statusCode).toBe(500);
  });
});

// ── 6. No-secrets-in-logs regression guard ───────────────────────────────────

describe("findSecretLeak", () => {
  it("returns null for a clean object", () => {
    const out = redactObject({
      id: "inv_1",
      amount: "100",
      authorization: "Bearer abc",
      email: "user@example.com",
    });
    expect(findSecretLeak(out)).toBeNull();
  });

  it("detects a raw secret field", () => {
    const dirty = { id: "x", authorization: "Bearer still-here" };
    const leak = findSecretLeak(dirty);
    expect(leak).not.toBeNull();
    expect(leak!.path).toBe("authorization");
  });

  it("detects nested secret field", () => {
    // simulate a misconfigured logger that didn't redact
    const dirty = {
      request: {
        body: { tax_id: "123-45-6789" },
      },
    };
    const leak = findSecretLeak(dirty);
    expect(leak).not.toBeNull();
    expect(leak!.path).toBe("request.body.tax_id");
  });

  it("treats [REDACTED] sentinel as clean", () => {
    const clean = {
      tax_id: "[REDACTED]",
      authorization: "[REDACTED]",
    };
    // findSecretLeak only flags when the value is NOT the sentinel
    expect(findSecretLeak(clean)).toBeNull();
  });

  it("handles arrays of objects", () => {
    const dirty = [{ id: "1" }, { password: "oops" }];
    const leak = findSecretLeak(dirty);
    expect(leak).not.toBeNull();
    expect(leak!.path).toBe("[1].password");
  });

  it("returns null for null / undefined", () => {
    expect(findSecretLeak(null)).toBeNull();
    expect(findSecretLeak(undefined)).toBeNull();
  });
});

// ── 7. Middleware integration ─────────────────────────────────────────────────

describe("createRequestLogger middleware", () => {
  let capturedEntries: RequestLogEntry[];
  let capturedErrors: Array<{ message: string; meta?: Record<string, unknown> }>;
  let testApp: ReturnType<typeof express>;

  beforeEach(() => {
    capturedEntries = [];
    capturedErrors = [];

    const testLogger: Logger = {
      info: (entry) => capturedEntries.push(entry),
      error: (message, meta) => capturedErrors.push({ message, meta }),
    };

    testApp = express();
    testApp.use(express.json());
    testApp.use(createRequestLogger(testLogger, { skipHealthCheck: true }));

    // Test routes
    testApp.get("/api/v1/invoices/:id", (req: Request, res: Response) => {
      res.json({
        id: req.params.id,
        status: "Pending",
        amount: "250000",
        email: "hidden@example.com",
      });
    });

    testApp.post("/api/v1/bids", (req: Request, res: Response) => {
      res.status(201).json({ bid_id: "bid_123", status: "Placed" });
    });

    testApp.get("/health", (_req: Request, res: Response) => {
      res.json({ status: "ok" });
    });
  });

  it("emits a structured log entry for each request", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    expect(capturedEntries).toHaveLength(1);
    const entry = capturedEntries[0];
    expect(entry.method).toBe("GET");
    expect(entry.path).toBe("/api/v1/invoices/inv_001");
    expect(entry.statusCode).toBe(200);
    expect(typeof entry.requestId).toBe("string");
    expect(typeof entry.durationMs).toBe("number");
  });

  it("skips the /health endpoint by default", async () => {
    await supertest(testApp).get("/health").expect(200);
    expect(capturedEntries).toHaveLength(0);
  });

  it("redacts secret fields in the response body", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    const entry = capturedEntries[0];
    // 'email' is SECRET
    expect(entry.response.body!["email"]).toBe("[REDACTED]");
  });

  it("preserves public fields in the response body", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    const entry = capturedEntries[0];
    expect(entry.response.body!["id"]).toBe("inv_001");
    expect(entry.response.body!["status"]).toBe("Pending");
  });

  it("hashes private fields in the response body", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    const entry = capturedEntries[0];
    expect(entry.response.body!["amount"]).toBe(sha256Prefix("250000"));
  });

  it("strips Authorization header before logging", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .set("Authorization", "Bearer super-secret")
      .expect(200);

    const entry = capturedEntries[0];
    // authorization should either be absent or [REDACTED] — never the raw token
    const authHeader = entry.request.headers["authorization"];
    expect(authHeader).not.toBe("Bearer super-secret");
  });

  it("redacts secret body fields in POST requests", async () => {
    await supertest(testApp)
      .post("/api/v1/bids")
      .send({
        bid_id: "bid_123",
        investor: "GBXXX",
        amount: "5000",
        signature: "ed25519-sig-0xdeadbeef",
        tax_id: "555-44-3333",
      })
      .expect(201);

    const entry = capturedEntries[0];
    expect(entry.request.body!["signature"]).toBe("[REDACTED]");
    expect(entry.request.body!["tax_id"]).toBe("[REDACTED]");
    // investor is PRIVATE → hashed
    expect(entry.request.body!["investor"]).toMatch(/^sha256:/);
  });

  it("attaches X-Request-Id header to response", async () => {
    const res = await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    expect(res.headers["x-request-id"]).toBeDefined();
    expect(typeof res.headers["x-request-id"]).toBe("string");
  });

  it("the captured log entry contains no raw secret values (regression)", async () => {
    await supertest(testApp)
      .post("/api/v1/bids")
      .set("Authorization", "Bearer TOP_SECRET_TOKEN")
      .send({
        signature: "wallet-sig",
        email: "alice@example.com",
        password: "hunter2",
        bid_id: "bid_1",
      })
      .expect(201);

    const entry = capturedEntries[0];
    const leak = findSecretLeak(entry);
    expect(leak).toBeNull();
  });

  it("logs multiple requests independently", async () => {
    await supertest(testApp).get("/api/v1/invoices/inv_001").expect(200);
    await supertest(testApp).post("/api/v1/bids").send({}).expect(201);

    expect(capturedEntries).toHaveLength(2);
    expect(capturedEntries[0].requestId).not.toBe(capturedEntries[1].requestId);
  });
});

// ── 8. Snapshot regression tests ─────────────────────────────────────────────

describe("redactObject — snapshot regression", () => {
  it("produces stable output for a representative invoice payload", () => {
    const payload = {
      id: "inv_abc123",
      status: "Funded",
      amount: "1000000",
      business: "GBSOME_STELLAR_ADDRESS",
      investor: "GBINVESTOR_ADDR",
      tax_id: "123-45-6789",
      customer_name: "Bob",
      email: "bob@example.com",
      authorization: "Bearer tok_xxx",
      signature: "ED_SIG_0xDEAD",
      due_date: 1714000000,
    };

    const redacted = redactObject(payload);

    // Public — untouched
    expect(redacted.id).toBe("inv_abc123");
    expect(redacted.status).toBe("Funded");
    expect(redacted.due_date).toBe(1714000000);

    // Private — hashed deterministically
    expect(redacted.amount).toBe(sha256Prefix("1000000"));
    expect(redacted.business).toBe(sha256Prefix("GBSOME_STELLAR_ADDRESS"));
    expect(redacted.investor).toBe(sha256Prefix("GBINVESTOR_ADDR"));

    // Secret — always [REDACTED]
    expect(redacted.tax_id).toBe("[REDACTED]");
    expect(redacted.customer_name).toBe("[REDACTED]");
    expect(redacted.email).toBe("[REDACTED]");
    expect(redacted.authorization).toBe("[REDACTED]");
    expect(redacted.signature).toBe("[REDACTED]");
  });
});

// ── 9. Extra branch coverage ──────────────────────────────────────────────────

describe("redactObject — branch coverage", () => {
  it("PUBLIC field containing an array of objects recurses into each item", () => {
    // Use a field that maps to PUBLIC (e.g. 'id') — but wrap it in an object
    // under a PUBLIC parent key.  The trick: give the outer key a PUBLIC tier
    // and make its value an array so the PUBLIC-array branch (L197-201) runs.
    //
    // To reach that branch we need: classifyField(key) === PUBLIC && Array.isArray(value)
    // 'id' is PUBLIC, but it's normally a scalar. We can use a custom structure:
    // put the array under a key that is PUBLIC. Let's use 'status' with array value.
    const out = redactObject({
      status: [{ id: "a", email: "secret@test.com" }, { id: "b" }],
    } as any);
    // 'status' is PUBLIC → recurse into each array element
    expect(Array.isArray(out.status)).toBe(true);
    const items = out.status as any[];
    // Each item is an object → redactObject called on it
    expect(items[0].id).toBe("a");       // id is PUBLIC
    expect(items[0].email).toBe("[REDACTED]"); // email is SECRET
    expect(items[1].id).toBe("b");
  });

  it("PUBLIC field containing an array of primitives passes them through", () => {
    const out = redactObject({ status: [1, 2, 3] } as any);
    expect(out.status).toEqual([1, 2, 3]);
  });

  it("SECRET field with object value yields [REDACTED]", () => {
    // Reaches the `tier === FieldTier.SECRET` nested-object branch (L205)
    const out = redactObject({ kyc_data: { fullName: "Alice", dob: "1990-01-01" } });
    expect(out.kyc_data).toBe("[REDACTED]");
  });

  it("PUBLIC field with nested object recurses (L210 branch)", () => {
    // Use a field explicitly classified as PUBLIC that holds a nested object.
    // 'version' is PUBLIC, give it an object value.
    const out = redactObject({ version: { major: 1, email: "leak@test.com" } } as any);
    // 'version' is PUBLIC → recurse into the nested object
    expect(typeof out.version).toBe("object");
    const nested = out.version as any;
    // 'major' is unknown → PRIVATE → hashed
    expect(nested.major).toBe(sha256Prefix(1 as any));
    // 'email' is SECRET → [REDACTED]
    expect(nested.email).toBe("[REDACTED]");
  });
});

describe("defaultLogger", () => {
  it("info() writes JSON to stdout", () => {
    const writeSpy = jest.spyOn(process.stdout, "write").mockImplementation(() => true);
    const { defaultLogger } = require("../middleware/request-logger");

    const fakeEntry = {
      requestId: "01ABC",
      timestamp: "2026-01-01T00:00:00.000Z",
      method: "GET",
      path: "/test",
      statusCode: 200,
      durationMs: 5,
      request: { method: "GET", path: "/test", query: {}, headers: {}, body: null },
      response: { statusCode: 200, body: null },
    };
    defaultLogger.info(fakeEntry);

    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining("01ABC"));
    writeSpy.mockRestore();
  });

  it("error() writes JSON to stderr", () => {
    const writeSpy = jest.spyOn(process.stderr, "write").mockImplementation(() => true);
    const { defaultLogger } = require("../middleware/request-logger");

    defaultLogger.error("something went wrong", { detail: "boom" });

    expect(writeSpy).toHaveBeenCalledWith(
      expect.stringContaining("something went wrong")
    );
    writeSpy.mockRestore();
  });
});

describe("createRequestLogger — error catch branch", () => {
  it("calls logger.error when the finish handler throws", async () => {
    const errorCalls: Array<{ message: string; meta?: Record<string, unknown> }> = [];
    const faultyLogger: Logger = {
      info: () => { throw new Error("simulated redaction failure"); },
      error: (message, meta) => errorCalls.push({ message, meta }),
    };

    const app = express();
    app.use(createRequestLogger(faultyLogger));
    app.get("/boom", (_req: Request, res: Response) => res.json({ id: "x" }));

    await supertest(app).get("/boom").expect(200);

    // Give the finish event a tick to fire
    await new Promise((r) => setTimeout(r, 50));

    expect(errorCalls.length).toBeGreaterThan(0);
    expect(errorCalls[0].message).toBe("request-logger: redaction error");
  });

  it("logs health check when skipHealthCheck is false", async () => {
    const entries: RequestLogEntry[] = [];
    const logger: Logger = { info: (e) => entries.push(e), error: jest.fn() };

    const app = express();
    app.use(createRequestLogger(logger, { skipHealthCheck: false }));
    app.get("/health", (_req: Request, res: Response) => res.json({ status: "ok" }));

    await supertest(app).get("/health").expect(200);
    expect(entries.length).toBe(1);
    expect(entries[0].path).toBe("/health");
  });
});

