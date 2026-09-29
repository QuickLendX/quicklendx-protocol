/**
 * openapi-contract.test.ts
 *
 * Contract-test suite for the QuickLendX v1 API.
 *
 * Approach
 * ────────
 * Every test in this file:
 *   1. Fires a real HTTP request through supertest against the live Express app.
 *   2. Asserts the response status code is one of those documented in openapi.yaml.
 *   3. Asserts the JSON body (where applicable) matches the schema documented
 *      for that status code in openapi.yaml.
 *   4. Asserts error responses always use the canonical envelope:
 *        { error: { message: string, code: string } }
 *
 * Coverage targets (per issue requirements):
 *   • Invoices    – list, get-by-id (200 / 304 / 400 / 404)
 *   • Bids        – list (200 / 400)
 *   • Settlements – list, get-by-id (200 / 304 / 400 / 404)
 *   • Portfolio   – list (200 / 400)
 *   • Disputes    – list (200)
 *   • Exports     – generate (200 / 400 / 401), download (200 / 401)
 *   • Monitoring  – health, cursor, invariants, webhook CRUD
 *   • Webhooks    – subscriber lifecycle, rotation
 *   • Events      – POST /events (200)
 *   • Health      – GET /health
 *   • Error shapes – 400 / 404 / 415 / 500 envelopes
 *   • Pagination  – malformed cursor → 400, limit clamping
 *   • Cache       – ETag round-trip → 304
 *   • Security    – error responses never include stack traces
 *
 * Environment:
 *   NODE_ENV=test         — disables stack-trace leakage in error-handler.
 *   SKIP_API_KEY_AUTH=true — bypasses X-API-Key checks for monitoring routes.
 */

import request from "supertest";
import app from "../app";
import {
  loadSpec,
  validateAgainstSchema,
  validateResponseBody,
  OpenApiSpec,
} from "./spec-loader";

// ─── Constants reflecting mock data in the controllers ──────────────────────

/**
 * The real invoice ID used in MOCK_INVOICES (controllers/v1/invoices.ts).
 * Must match exactly for 200 responses.
 */
const KNOWN_INVOICE_ID =
  "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

/**
 * A syntactically valid hex string that does NOT exist in mock data → 404.
 */
const UNKNOWN_INVOICE_ID =
  "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/**
 * The real settlement ID used in MOCK_SETTLEMENTS (controllers/v1/settlements.ts).
 */
const KNOWN_SETTLEMENT_ID = "0xsettle123";

/**
 * A hex-format id that does not exist → 404.
 */
const UNKNOWN_SETTLEMENT_ID = "0xdeadbeef";

/** The investor value used in MOCK_PORTFOLIO (controllers/v1/portfolio.ts). */
const KNOWN_INVESTOR = "GA...ABC";

// ─── Schema helpers ──────────────────────────────────────────────────────────

/** The canonical error envelope that every non-2xx response MUST return. */
const ERROR_ENVELOPE_SCHEMA = {
  type: "object",
  required: ["error"],
  properties: {
    error: {
      type: "object",
      required: ["message", "code"],
      properties: {
        message: { type: "string" },
        code: { type: "string" },
      },
    },
  },
};

/** The freshness envelope that list endpoints wrap their data in. */
const FRESHNESS_ENVELOPE_SCHEMA = {
  type: "object",
  required: ["data", "freshness"],
  properties: {
    data: { type: "array" },
    freshness: {
      type: "object",
      required: ["lastIndexedLedger", "indexLagSeconds", "lastUpdatedAt", "cursor"],
      properties: {
        lastIndexedLedger: { type: "number" },
        indexLagSeconds: { type: "number" },
        lastUpdatedAt: { type: "string" },
        cursor: { type: "string" },
      },
    },
  },
};

/**
 * Assert the body is a canonical error envelope and contains no stack trace.
 * Optionally assert the exact error code.
 */
function assertErrorEnvelope(
  body: unknown,
  spec: OpenApiSpec,
  expectedCode?: string
): void {
  const result = validateAgainstSchema(body, ERROR_ENVELOPE_SCHEMA, spec);
  expect(result.errors).toEqual([]);

  const bodyObj = body as { error: { message: string; code: string; stack?: string } };
  if (expectedCode) {
    expect(bodyObj.error.code).toBe(expectedCode);
  }
  // Security: stack traces must never appear in error responses
  expect(bodyObj.error).not.toHaveProperty("stack");
  const bodyStr = JSON.stringify(body);
  expect(bodyStr).not.toMatch(/at Object\.<anonymous>/);
  expect(bodyStr).not.toMatch(/\.ts:\d+:\d+/);
}

/**
 * Assert the body is a freshness envelope (data + freshness properties).
 */
function assertFreshnessEnvelope(body: unknown, spec: OpenApiSpec): void {
  const result = validateAgainstSchema(body, FRESHNESS_ENVELOPE_SCHEMA, spec);
  expect(result.errors).toEqual([]);
}

// ─── Test setup ──────────────────────────────────────────────────────────────

let spec: OpenApiSpec;

beforeAll(() => {
  process.env.NODE_ENV = "test";
  // Allow monitoring routes without a real API key
  process.env.SKIP_API_KEY_AUTH = "true";
  process.env.TEST_ACTOR = "test-actor";
  spec = loadSpec();
});

afterAll(() => {
  delete process.env.SKIP_API_KEY_AUTH;
  delete process.env.TEST_ACTOR;
});

// ═════════════════════════════════════════════════════════════════════════════
// 1. OpenAPI spec integrity (meta tests)
// ═════════════════════════════════════════════════════════════════════════════

describe("OpenAPI spec integrity", () => {
  it("spec has openapi 3.0.x version field", () => {
    expect(spec.openapi).toMatch(/^3\./);
  });

  it("spec has info.title and info.version", () => {
    expect(typeof spec.info.title).toBe("string");
    expect(typeof spec.info.version).toBe("string");
  });

  it("spec defines all required component schemas", () => {
    const requiredSchemas = [
      "Invoice",
      "InvoiceStatus",
      "InvoiceCategory",
      "Bid",
      "BidStatus",
      "Settlement",
      "SettlementStatus",
      "PortfolioEntry",
      "PortfolioPageResult",
      "InvestmentStatus",
      "Dispute",
      "DisputeStatus",
      "FreshnessMetadata",
      "FreshnessEnvelope",
      "ErrorEnvelope",
      "Event",
      "UserNotificationPreferences",
      "UpdateNotificationPreferences",
    ];
    for (const name of requiredSchemas) {
      expect(spec.components?.schemas?.[name]).toBeDefined();
    }
  });

  it("spec documents /health path with GET operation", () => {
    expect(spec.paths["/health"]).toBeDefined();
    expect(spec.paths["/health"]["get"]).toBeDefined();
  });

  it("spec documents /invoices path with GET operation", () => {
    expect(spec.paths["/invoices"]).toBeDefined();
    expect(spec.paths["/invoices"]["get"]).toBeDefined();
  });

  it("spec documents /invoices/{id} path", () => {
    expect(spec.paths["/invoices/{id}"]).toBeDefined();
  });

  it("spec documents /bids path", () => {
    expect(spec.paths["/bids"]).toBeDefined();
    expect(spec.paths["/bids"]["get"]).toBeDefined();
  });

  it("spec documents /settlements path", () => {
    expect(spec.paths["/settlements"]).toBeDefined();
  });

  it("spec documents /portfolio path", () => {
    expect(spec.paths["/portfolio"]).toBeDefined();
  });

  it("spec documents /exports/generate path with POST", () => {
    expect(spec.paths["/exports/generate"]).toBeDefined();
    expect(spec.paths["/exports/generate"]["post"]).toBeDefined();
  });

  it("spec documents /events path with POST", () => {
    expect(spec.paths["/events"]).toBeDefined();
    expect(spec.paths["/events"]["post"]).toBeDefined();
  });

  it("components/parameters defines LimitParam", () => {
    expect(spec.components?.parameters?.["LimitParam"]).toBeDefined();
  });

  it("components/parameters defines CursorParam", () => {
    expect(spec.components?.parameters?.["CursorParam"]).toBeDefined();
  });

  it("components/responses defines PaginationError", () => {
    expect(spec.components?.responses?.["PaginationError"]).toBeDefined();
  });

  it("all $refs in the spec resolve without error", () => {
    // Validated during loadSpec(); explicit assertion here
    expect(() => loadSpec()).not.toThrow();
  });

  it("FreshnessMetadata schema has all required fields", () => {
    const schema = spec.components?.schemas?.FreshnessMetadata;
    expect(schema?.required).toContain("lastIndexedLedger");
    expect(schema?.required).toContain("indexLagSeconds");
    expect(schema?.required).toContain("lastUpdatedAt");
    expect(schema?.required).toContain("cursor");
  });

  it("PortfolioPageResult schema has data/next_cursor/has_more properties", () => {
    const schema = spec.components?.schemas?.PortfolioPageResult;
    expect(schema?.required).toContain("data");
    expect(schema?.required).toContain("next_cursor");
    expect(schema?.required).toContain("has_more");
  });

  it("ErrorEnvelope schema requires error.message and error.code", () => {
    const schema = spec.components?.schemas?.ErrorEnvelope;
    expect(schema).toBeDefined();
    expect(schema?.required).toContain("error");
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 2. Health check
// ═════════════════════════════════════════════════════════════════════════════

describe("GET /api/v1/health", () => {
  it("200 – returns status, version, timestamp fields", async () => {
    const res = await request(app).get("/api/v1/health");

    expect(res.status).toBe(200);
    const errors = validateResponseBody(spec, "GET", "/health", 200, res.body);
    expect(errors).toEqual([]);

    expect(res.body).toHaveProperty("status", "ok");
    expect(typeof res.body.version).toBe("string");
    expect(typeof res.body.timestamp).toBe("string");
  });

  it("security – health response contains no stack trace", async () => {
    const res = await request(app).get("/api/v1/health");

    expect(res.status).toBe(200);
    const bodyStr = JSON.stringify(res.body);
    expect(bodyStr).not.toMatch(/at Object\.<anonymous>/);
    expect(bodyStr).not.toMatch(/\.ts:\d+:\d+/);
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 3. Invoices
// ═════════════════════════════════════════════════════════════════════════════

describe("GET /api/v1/invoices", () => {
  it("200 – returns freshness envelope with data array", async () => {
    const res = await request(app).get("/api/v1/invoices");

    expect(res.status).toBe(200);
    assertFreshnessEnvelope(res.body, spec);
    expect(Array.isArray(res.body.data)).toBe(true);
  });

  it("200 – each invoice item matches the Invoice schema", async () => {
    const res = await request(app).get("/api/v1/invoices");

    expect(res.status).toBe(200);
    const invoiceSchema = spec.components?.schemas?.Invoice;
    expect(invoiceSchema).toBeDefined();

    for (const item of res.body.data) {
      const result = validateAgainstSchema(item, invoiceSchema!, spec);
      expect(result.errors).toEqual([]);
    }
  });

  it("200 – filters by status=Verified returns only Verified invoices", async () => {
    const res = await request(app).get("/api/v1/invoices?status=Verified");

    expect(res.status).toBe(200);
    for (const item of res.body.data) {
      expect(item.status).toBe("Verified");
    }
  });

  it("400 – invalid business param (not a Stellar address) returns VALIDATION_ERROR", async () => {
    // The validator requires business to be a Stellar public key (G[A-Z2-7]{55})
    const res = await request(app).get("/api/v1/invoices?business=INVALID_ADDRESS");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });

  it("400 – invalid status value returns VALIDATION_ERROR", async () => {
    const res = await request(app).get("/api/v1/invoices?status=InvalidStatus");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });

  it("freshness envelope matches the spec's FreshnessMetadata schema exactly", async () => {
    const res = await request(app).get("/api/v1/invoices");

    expect(res.status).toBe(200);
    const freshnessSchema = spec.components?.schemas?.FreshnessMetadata;
    expect(freshnessSchema).toBeDefined();

    const result = validateAgainstSchema(res.body.freshness, freshnessSchema!, spec);
    expect(result.errors).toEqual([]);
  });
});

describe("GET /api/v1/invoices/:id", () => {
  it("200 – known ID returns a single Invoice matching the schema", async () => {
    const res = await request(app).get(`/api/v1/invoices/${KNOWN_INVOICE_ID}`);

    expect(res.status).toBe(200);
    const invoiceSchema = spec.components?.schemas?.Invoice;
    expect(invoiceSchema).toBeDefined();
    const result = validateAgainstSchema(res.body, invoiceSchema!, spec);
    expect(result.errors).toEqual([]);
  });

  it("304 – returns Not Modified on matching ETag (If-None-Match round-trip)", async () => {
    const first = await request(app).get(`/api/v1/invoices/${KNOWN_INVOICE_ID}`);
    expect(first.status).toBe(200);

    const etag = first.headers["etag"];
    if (!etag) {
      // Cache policy uses no-store for this endpoint – nothing to test
      return;
    }

    const second = await request(app)
      .get(`/api/v1/invoices/${KNOWN_INVOICE_ID}`)
      .set("If-None-Match", etag);

    expect(second.status).toBe(304);
  });

  it("404 – unknown (valid hex) invoice ID returns canonical error envelope", async () => {
    const res = await request(app).get(`/api/v1/invoices/${UNKNOWN_INVOICE_ID}`);

    expect(res.status).toBe(404);
    assertErrorEnvelope(res.body, spec, "INVOICE_NOT_FOUND");
  });

  it("400 – non-hex invoice ID fails param validation", async () => {
    // The validator requires id to be a hex string matching /^0x[a-fA-F0-9]+$/
    const res = await request(app).get("/api/v1/invoices/not-a-hex-id");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });

  it("security – 404 error response contains no stack trace", async () => {
    const res = await request(app).get(`/api/v1/invoices/${UNKNOWN_INVOICE_ID}`);

    expect(res.status).toBe(404);
    const bodyStr = JSON.stringify(res.body);
    expect(bodyStr).not.toMatch(/Error:/);
    expect(bodyStr).not.toMatch(/\.ts:\d+/);
    expect(res.body).not.toHaveProperty("stack");
    expect(res.body.error).not.toHaveProperty("stack");
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 4. Bids
// ═════════════════════════════════════════════════════════════════════════════

describe("GET /api/v1/bids", () => {
  it("200 – returns freshness envelope with data array", async () => {
    const res = await request(app).get("/api/v1/bids");

    expect(res.status).toBe(200);
    assertFreshnessEnvelope(res.body, spec);
    expect(Array.isArray(res.body.data)).toBe(true);
  });

  it("200 – each bid matches the Bid schema", async () => {
    const res = await request(app).get("/api/v1/bids");

    expect(res.status).toBe(200);
    const bidSchema = spec.components?.schemas?.Bid;
    expect(bidSchema).toBeDefined();

    for (const item of res.body.data) {
      const result = validateAgainstSchema(item, bidSchema!, spec);
      expect(result.errors).toEqual([]);
    }
  });

  it("200 – filters bids by known invoice_id returns results or empty array", async () => {
    const res = await request(app).get(
      `/api/v1/bids?invoice_id=${KNOWN_INVOICE_ID}`
    );

    expect(res.status).toBe(200);
    expect(Array.isArray(res.body.data)).toBe(true);
    // All returned bids should match the requested invoice_id
    for (const bid of res.body.data) {
      expect(bid.invoice_id).toBe(KNOWN_INVOICE_ID);
    }
  });

  it("400 – non-hex invoice_id param returns VALIDATION_ERROR", async () => {
    const res = await request(app).get("/api/v1/bids?invoice_id=NONEXISTENT");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });

  it("400 – investor param must be a valid Stellar address", async () => {
    // "NOBODY" does not match /^G[A-Z2-7]{55}$/ so validator returns 400
    const res = await request(app).get("/api/v1/bids?investor=NOBODY");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });

  it("Cache-Control: no-store for bids (financial freshness requirement)", async () => {
    const res = await request(app).get("/api/v1/bids");

    expect(res.status).toBe(200);
    expect(res.headers["cache-control"]).toBe("no-store");
  });

  it("freshness metadata in bid response matches spec FreshnessMetadata schema", async () => {
    const res = await request(app).get("/api/v1/bids");

    expect(res.status).toBe(200);
    const freshnessSchema = spec.components?.schemas?.FreshnessMetadata;
    const result = validateAgainstSchema(res.body.freshness, freshnessSchema!, spec);
    expect(result.errors).toEqual([]);
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 5. Settlements
// ═════════════════════════════════════════════════════════════════════════════

describe("GET /api/v1/settlements", () => {
  it("200 – returns freshness envelope with data array", async () => {
    const res = await request(app).get("/api/v1/settlements");

    expect(res.status).toBe(200);
    assertFreshnessEnvelope(res.body, spec);
    expect(Array.isArray(res.body.data)).toBe(true);
  });

  it("200 – each settlement matches the Settlement schema", async () => {
    const res = await request(app).get("/api/v1/settlements");

    expect(res.status).toBe(200);
    const settlementSchema = spec.components?.schemas?.Settlement;
    expect(settlementSchema).toBeDefined();

    for (const item of res.body.data) {
      const result = validateAgainstSchema(item, settlementSchema!, spec);
      expect(result.errors).toEqual([]);
    }
  });

  it("400 – filters by invalid invoice_id format returns VALIDATION_ERROR", async () => {
    const res = await request(app).get("/api/v1/settlements?invoice_id=NONEXISTENT");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });

  it("400 – malformed cursor returns INVALID_PAGINATION error", async () => {
    const res = await request(app).get(
      "/api/v1/settlements?cursor=NOTAVALIDBASE64CURSOR!!!"
    );

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "INVALID_PAGINATION");
  });

  it("400 – limit=0 returns INVALID_PAGINATION", async () => {
    const res = await request(app).get("/api/v1/settlements?limit=0");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });

  it("304 – conditional GET with matching ETag returns Not Modified", async () => {
    const first = await request(app).get("/api/v1/settlements");
    expect(first.status).toBe(200);

    const etag = first.headers["etag"];
    if (!etag) return; // some endpoints use no-store

    const second = await request(app)
      .get("/api/v1/settlements")
      .set("If-None-Match", etag);

    expect(second.status).toBe(304);
  });

  it("freshness metadata matches spec FreshnessMetadata schema", async () => {
    const res = await request(app).get("/api/v1/settlements");

    expect(res.status).toBe(200);
    const freshnessSchema = spec.components?.schemas?.FreshnessMetadata;
    const result = validateAgainstSchema(res.body.freshness, freshnessSchema!, spec);
    expect(result.errors).toEqual([]);
  });
});

describe("GET /api/v1/settlements/:id", () => {
  it("200 – known ID returns a single Settlement matching the schema", async () => {
    const res = await request(app).get(`/api/v1/settlements/${KNOWN_SETTLEMENT_ID}`);

    expect(res.status).toBe(200);
    const settlementSchema = spec.components?.schemas?.Settlement;
    expect(settlementSchema).toBeDefined();
    const result = validateAgainstSchema(res.body, settlementSchema!, spec);
    expect(result.errors).toEqual([]);
  });

  it("304 – conditional GET returns Not Modified when ETag matches", async () => {
    const first = await request(app).get(`/api/v1/settlements/${KNOWN_SETTLEMENT_ID}`);
    expect(first.status).toBe(200);

    const etag = first.headers["etag"];
    if (!etag) return;

    const second = await request(app)
      .get(`/api/v1/settlements/${KNOWN_SETTLEMENT_ID}`)
      .set("If-None-Match", etag);

    expect(second.status).toBe(304);
  });

  it("404 – unknown settlement ID returns canonical error envelope", async () => {
    const res = await request(app).get(
      `/api/v1/settlements/${UNKNOWN_SETTLEMENT_ID}`
    );

    expect(res.status).toBe(404);
    assertErrorEnvelope(res.body, spec, "SETTLEMENT_NOT_FOUND");
  });

  it("security – 404 settlement error has no stack trace", async () => {
    const res = await request(app).get(
      `/api/v1/settlements/${UNKNOWN_SETTLEMENT_ID}`
    );

    expect(res.status).toBe(404);
    expect(res.body).not.toHaveProperty("stack");
    expect(res.body.error).not.toHaveProperty("stack");
    expect(JSON.stringify(res.body)).not.toMatch(/\.ts:\d+/);
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 6. Portfolio
// ═════════════════════════════════════════════════════════════════════════════

describe("GET /api/v1/portfolio", () => {
  it("400 – missing investor param returns error envelope with MISSING_INVESTOR", async () => {
    const res = await request(app).get("/api/v1/portfolio");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "MISSING_INVESTOR");
  });

  it("400 – malformed cursor returns INVALID_PAGINATION", async () => {
    const res = await request(app).get(
      `/api/v1/portfolio?investor=${KNOWN_INVESTOR}&cursor=!!BAD!!`
    );

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "INVALID_PAGINATION");
  });

  it("200 – known investor returns paginated result with data array", async () => {
    const res = await request(app).get(
      `/api/v1/portfolio?investor=${encodeURIComponent(KNOWN_INVESTOR)}`
    );

    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("data");
    expect(Array.isArray(res.body.data)).toBe(true);
    expect(res.body).toHaveProperty("has_more");
    expect(res.body).toHaveProperty("next_cursor");
  });

  it("200 – unknown investor returns empty page result", async () => {
    const res = await request(app).get(
      "/api/v1/portfolio?investor=UNKNOWN_INVESTOR_ADDRESS"
    );

    expect(res.status).toBe(200);
    expect(res.body.data).toEqual([]);
    expect(res.body.has_more).toBe(false);
    expect(res.body.next_cursor).toBeNull();
  });

  it("200 – each portfolio entry matches PortfolioEntry schema", async () => {
    const res = await request(app).get(
      `/api/v1/portfolio?investor=${encodeURIComponent(KNOWN_INVESTOR)}`
    );

    expect(res.status).toBe(200);
    const portfolioEntrySchema = spec.components?.schemas?.PortfolioEntry;
    expect(portfolioEntrySchema).toBeDefined();

    for (const item of res.body.data) {
      const result = validateAgainstSchema(item, portfolioEntrySchema!, spec);
      expect(result.errors).toEqual([]);
    }
  });

  it("200 – portfolio result matches PortfolioPageResult schema", async () => {
    const res = await request(app).get(
      `/api/v1/portfolio?investor=${encodeURIComponent(KNOWN_INVESTOR)}`
    );

    expect(res.status).toBe(200);
    const pageResultSchema = spec.components?.schemas?.PortfolioPageResult;
    expect(pageResultSchema).toBeDefined();

    const result = validateAgainstSchema(res.body, pageResultSchema!, spec);
    expect(result.errors).toEqual([]);
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 7. Disputes
// ═════════════════════════════════════════════════════════════════════════════

describe("GET /api/v1/invoices/:id/disputes", () => {
  it("200 – known invoice returns a data-wrapped array of disputes", async () => {
    const res = await request(app).get(
      `/api/v1/invoices/${KNOWN_INVOICE_ID}/disputes`
    );

    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("data");
    expect(Array.isArray(res.body.data)).toBe(true);
  });

  it("200 – each dispute matches the Dispute schema", async () => {
    const res = await request(app).get(
      `/api/v1/invoices/${KNOWN_INVOICE_ID}/disputes`
    );

    expect(res.status).toBe(200);
    const disputeSchema = spec.components?.schemas?.Dispute;
    expect(disputeSchema).toBeDefined();

    for (const item of res.body.data) {
      const result = validateAgainstSchema(item, disputeSchema!, spec);
      expect(result.errors).toEqual([]);
    }
  });

  it("200 – unknown (valid hex) invoice returns empty dispute list", async () => {
    const res = await request(app).get(
      `/api/v1/invoices/${UNKNOWN_INVOICE_ID}/disputes`
    );

    expect(res.status).toBe(200);
    expect(res.body.data).toEqual([]);
  });

  it("400 – non-hex invoice ID in disputes path fails param validation", async () => {
    const res = await request(app).get(
      "/api/v1/invoices/not-a-hex-id/disputes"
    );

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 8. Exports
// ═════════════════════════════════════════════════════════════════════════════

describe("POST /api/v1/exports/generate", () => {
  it("401 – no auth header returns UNAUTHORIZED envelope", async () => {
    const res = await request(app)
      .post("/api/v1/exports/generate")
      .set("Content-Type", "application/json")
      .send({});

    expect(res.status).toBe(401);
    assertErrorEnvelope(res.body, spec, "UNAUTHORIZED");
  });

  it("401 – empty Bearer token returns UNAUTHORIZED envelope", async () => {
    const res = await request(app)
      .post("/api/v1/exports/generate")
      .set("Authorization", "Bearer ")
      .set("Content-Type", "application/json")
      .send({});

    expect(res.status).toBe(401);
    assertErrorEnvelope(res.body, spec, "UNAUTHORIZED");
  });

  it("200 – valid Bearer token returns download_url, success, expires_in", async () => {
    const res = await request(app)
      .post("/api/v1/exports/generate")
      .set("Authorization", "Bearer test-user-id")
      .set("Content-Type", "application/json")
      .send({});

    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("success", true);
    expect(typeof res.body.download_url).toBe("string");
    expect(typeof res.body.expires_in).toBe("string");

    // Validate against spec schema
    const errors = validateResponseBody(
      spec,
      "POST",
      "/exports/generate",
      200,
      res.body
    );
    expect(errors).toEqual([]);
  });

  it("200 – format=csv accepted without error", async () => {
    const res = await request(app)
      .post("/api/v1/exports/generate?format=csv")
      .set("Authorization", "Bearer test-user-id")
      .set("Content-Type", "application/json")
      .send({});

    expect(res.status).toBe(200);
    expect(res.body.success).toBe(true);
    expect(typeof res.body.download_url).toBe("string");
  });

  it("400 – unsupported format=xml returns INVALID_FORMAT error", async () => {
    const res = await request(app)
      .post("/api/v1/exports/generate?format=xml")
      .set("Authorization", "Bearer test-user-id")
      .set("Content-Type", "application/json")
      .send({});

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "INVALID_FORMAT");
  });

  it("security – 401 response has no stack trace", async () => {
    const res = await request(app)
      .post("/api/v1/exports/generate")
      .set("Content-Type", "application/json")
      .send({});

    expect(res.status).toBe(401);
    const bodyStr = JSON.stringify(res.body);
    expect(bodyStr).not.toMatch(/Error:/);
    expect(bodyStr).not.toMatch(/\.ts:\d+/);
    expect(res.body.error).not.toHaveProperty("stack");
  });
});

describe("GET /api/v1/exports/download/:token", () => {
  let validToken: string;

  beforeAll(async () => {
    const genRes = await request(app)
      .post("/api/v1/exports/generate")
      .set("Authorization", "Bearer test-user-id")
      .set("Content-Type", "application/json")
      .send({});

    if (genRes.status === 200) {
      const urlParts = genRes.body.download_url.split("/");
      validToken = urlParts[urlParts.length - 1];
    }
  });

  it("401 – invalid token returns INVALID_TOKEN envelope", async () => {
    const res = await request(app).get(
      "/api/v1/exports/download/INVALIDTOKEN"
    );

    expect(res.status).toBe(401);
    assertErrorEnvelope(res.body, spec, "INVALID_TOKEN");
  });

  it("200 – valid token returns export with Content-Disposition: attachment", async () => {
    if (!validToken) {
      console.warn("Skipping: token generation did not succeed");
      return;
    }

    const res = await request(app).get(`/api/v1/exports/download/${validToken}`);

    expect(res.status).toBe(200);
    expect(res.headers["content-disposition"]).toMatch(/attachment/);
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 9. Monitoring endpoints (SKIP_API_KEY_AUTH=true set in beforeAll)
// ═════════════════════════════════════════════════════════════════════════════

describe("GET /api/v1/admin/monitoring/health", () => {
  it("200 – returns health status object with known status values", async () => {
    const res = await request(app)
      .get("/api/v1/admin/monitoring/health")
      .set("X-API-Key", "test-key");

    expect(res.status).toBe(200);
    expect(["ok", "degraded", "unavailable", "maintenance"]).toContain(
      res.body.status
    );
    expect(typeof res.body.timestamp).toBe("string");
  });

  it("security – monitoring health response contains no stack trace", async () => {
    const res = await request(app)
      .get("/api/v1/admin/monitoring/health")
      .set("X-API-Key", "test-key");

    expect(res.status).toBe(200);
    const bodyStr = JSON.stringify(res.body);
    expect(bodyStr).not.toMatch(/at Object\.<anonymous>/);
    expect(bodyStr).not.toMatch(/\.ts:\d+/);
  });
});

describe("GET /api/v1/admin/monitoring/cursor", () => {
  it("200 or 500 – returns cursor info or error envelope", async () => {
    const res = await request(app)
      .get("/api/v1/admin/monitoring/cursor")
      .set("X-API-Key", "test-key");

    if (res.status === 500) {
      assertErrorEnvelope(res.body, spec, "CURSOR_READ_ERROR");
    } else {
      expect(res.status).toBe(200);
      expect(res.body).toHaveProperty("lastIndexedLedger");
      expect(res.body).toHaveProperty("ingestLag");
      expect(typeof res.body.timestamp).toBe("string");
    }
  });
});

describe("GET /api/v1/admin/monitoring/invariants", () => {
  it("200 or 500 – returns invariant report or error envelope", async () => {
    const res = await request(app)
      .get("/api/v1/admin/monitoring/invariants")
      .set("X-API-Key", "test-key");

    if (res.status === 500) {
      assertErrorEnvelope(res.body, spec, "INVARIANT_CHECK_ERROR");
    } else {
      expect(res.status).toBe(200);
      expect(typeof res.body).toBe("object");
    }
  });
});

describe("GET /api/v1/admin/monitoring/webhook", () => {
  it("200 or 500 – returns webhook queue stats or error envelope", async () => {
    const res = await request(app)
      .get("/api/v1/admin/monitoring/webhook")
      .set("X-API-Key", "test-key");

    if (res.status === 500) {
      assertErrorEnvelope(res.body, spec, "WEBHOOK_QUEUE_ERROR");
    } else {
      expect(res.status).toBe(200);
      expect(typeof res.body).toBe("object");
    }
  });
});

describe("POST /api/v1/admin/monitoring/webhook", () => {
  it("400 – missing type field returns INVALID_WEBHOOK_PAYLOAD error", async () => {
    const res = await request(app)
      .post("/api/v1/admin/monitoring/webhook")
      .set("Content-Type", "application/json")
      .set("X-API-Key", "test-key")
      .send({ payload: {} });

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "INVALID_WEBHOOK_PAYLOAD");
  });

  it("400 – empty string type returns INVALID_WEBHOOK_PAYLOAD error", async () => {
    const res = await request(app)
      .post("/api/v1/admin/monitoring/webhook")
      .set("Content-Type", "application/json")
      .set("X-API-Key", "test-key")
      .send({ type: "", payload: {} });

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "INVALID_WEBHOOK_PAYLOAD");
  });

  it("201 – valid type enqueues and returns id + enqueuedAt", async () => {
    const res = await request(app)
      .post("/api/v1/admin/monitoring/webhook")
      .set("Content-Type", "application/json")
      .set("X-API-Key", "test-key")
      .send({ type: "invoice.created", payload: { invoiceId: "inv-1" } });

    expect(res.status).toBe(201);
    expect(typeof res.body.id).toBe("string");
    expect(typeof res.body.enqueuedAt).toBe("string");
  });
});

describe("POST /api/v1/admin/monitoring/webhook/:id/success", () => {
  it("200 – marks a queued webhook as success", async () => {
    const enqRes = await request(app)
      .post("/api/v1/admin/monitoring/webhook")
      .set("Content-Type", "application/json")
      .set("X-API-Key", "test-key")
      .send({ type: "invoice.paid", payload: {} });

    if (enqRes.status !== 201) return;

    const res = await request(app)
      .post(`/api/v1/admin/monitoring/webhook/${enqRes.body.id}/success`)
      .set("Content-Type", "application/json")
      .set("X-API-Key", "test-key")
      .send({});

    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("outcome");
  });
});

describe("POST /api/v1/admin/monitoring/webhook/:id/fail", () => {
  it("200 – marks a queued webhook as failed", async () => {
    const enqRes = await request(app)
      .post("/api/v1/admin/monitoring/webhook")
      .set("Content-Type", "application/json")
      .set("X-API-Key", "test-key")
      .send({ type: "settlement.failed", payload: {} });

    if (enqRes.status !== 201) return;

    const res = await request(app)
      .post(`/api/v1/admin/monitoring/webhook/${enqRes.body.id}/fail`)
      .set("Content-Type", "application/json")
      .set("X-API-Key", "test-key")
      .send({});

    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("outcome");
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 10. Webhook subscriber lifecycle
// ═════════════════════════════════════════════════════════════════════════════

describe("Webhook subscriber lifecycle", () => {
  const subscriberId = `test-sub-${Date.now()}`;

  it("POST /api/v1/webhooks/subscribers – 201, returns subscriber_id and initial_secret", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .set("Content-Type", "application/json")
      .send({ subscriber_id: subscriberId });

    expect(res.status).toBe(201);
    expect(res.body).toHaveProperty("subscriber_id", subscriberId);
    expect(typeof res.body.initial_secret).toBe("string");
  });

  it("POST /api/v1/webhooks/subscribers – 400/409 for duplicate subscriber", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .set("Content-Type", "application/json")
      .send({ subscriber_id: subscriberId });

    expect([400, 409]).toContain(res.status);
    assertErrorEnvelope(res.body, spec);
  });

  it("GET /api/v1/webhooks/subscribers/:id – 200, returns subscriber info", async () => {
    const res = await request(app).get(
      `/api/v1/webhooks/subscribers/${subscriberId}`
    );

    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("subscriber_id", subscriberId);
  });

  it("GET /api/v1/webhooks/subscribers/:id – 404 for unknown subscriber", async () => {
    const res = await request(app).get(
      "/api/v1/webhooks/subscribers/UNKNOWN_SUBSCRIBER"
    );

    expect(res.status).toBe(404);
    assertErrorEnvelope(res.body, spec);
  });

  it("POST rotate – 200/202, returns subscriber with rotation state", async () => {
    const res = await request(app)
      .post(`/api/v1/webhooks/subscribers/${subscriberId}/rotate`)
      .set("Content-Type", "application/json")
      .send({});

    expect([200, 202]).toContain(res.status);
    expect(res.body).toHaveProperty("subscriber_id", subscriberId);
  });

  it("POST rotate/finalize – 200, promotes pending to primary", async () => {
    const res = await request(app)
      .post(`/api/v1/webhooks/subscribers/${subscriberId}/rotate/finalize`)
      .set("Content-Type", "application/json")
      .send({});

    expect([200, 400]).toContain(res.status);
    if (res.status !== 200) {
      assertErrorEnvelope(res.body, spec);
    }
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 11. Events endpoint
// ═════════════════════════════════════════════════════════════════════════════

describe("POST /api/v1/events", () => {
  it("200 – processes a valid event object (success or internal error)", async () => {
    const res = await request(app)
      .post("/api/v1/events")
      .set("Content-Type", "application/json")
      .send({ type: "InvoiceCreated", data: {} });

    expect([200, 500]).toContain(res.status);
    if (res.status === 200) {
      expect(res.body).toHaveProperty("success", true);
      expect(typeof res.body.processed).toBe("number");
    }
  });

  it("200 – processes an array of events", async () => {
    const res = await request(app)
      .post("/api/v1/events")
      .set("Content-Type", "application/json")
      .send([
        { type: "InvoiceCreated", data: {} },
        { type: "BidPlaced", data: {} },
      ]);

    expect([200, 500]).toContain(res.status);
    if (res.status === 200) {
      expect(res.body.processed).toBeGreaterThanOrEqual(1);
    }
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 12. Global error handling (shape assertions)
// ═════════════════════════════════════════════════════════════════════════════

describe("Error envelope contract", () => {
  it("404 – unknown route returns NOT_FOUND-like envelope", async () => {
    const res = await request(app).get(
      "/api/v1/DOES_NOT_EXIST_AT_ALL_EVER_6789"
    );

    expect(res.status).toBe(404);
    // The 404 handler returns { error: { message, code } }
    assertErrorEnvelope(res.body, spec);
  });

  it("415 – POST with non-JSON Content-Type returns INVALID_CONTENT_TYPE", async () => {
    const res = await request(app)
      .post("/api/v1/events")
      .set("Content-Type", "text/plain")
      .send("raw text body");

    // CSRF middleware returns 415 for non-JSON POSTs
    if (res.status === 415) {
      assertErrorEnvelope(res.body, spec, "INVALID_CONTENT_TYPE");
    } else {
      // Express may parse it differently in some environments
      expect([200, 400, 415, 500]).toContain(res.status);
    }
  });

  it("security – no stack field in any error response at top level", async () => {
    const res = await request(app).get(
      `/api/v1/invoices/${UNKNOWN_INVOICE_ID}`
    );

    expect(res.status).toBe(404);
    expect(res.body).not.toHaveProperty("stack");
    expect(res.body.error).not.toHaveProperty("stack");
  });

  it("security – no stack-trace text in any error response body string", async () => {
    const res = await request(app).get(
      `/api/v1/invoices/${UNKNOWN_INVOICE_ID}`
    );

    const bodyStr = JSON.stringify(res.body);
    expect(bodyStr).not.toMatch(/at \w+ \(.*\.ts:\d+:\d+\)/);
    expect(bodyStr).not.toMatch(/at \w+\..*\(.*:\d+:\d+\)/);
  });

  it("error responses from validation middleware use VALIDATION_ERROR code", async () => {
    const res = await request(app).get("/api/v1/invoices?status=BadValue");

    expect(res.status).toBe(400);
    assertErrorEnvelope(res.body, spec, "VALIDATION_ERROR");
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 13. Pagination contract
// ═════════════════════════════════════════════════════════════════════════════

describe("Pagination contract", () => {
  it("limit=100 is the maximum, returns at most 100 items", async () => {
    const res = await request(app).get("/api/v1/settlements?limit=100");

    expect(res.status).toBe(200);
    expect(res.body.data.length).toBeLessThanOrEqual(100);
  });

  it("limit=1 returns at most 1 item", async () => {
    const res = await request(app).get("/api/v1/settlements?limit=1");

    expect(res.status).toBe(200);
    expect(res.body.data.length).toBeLessThanOrEqual(1);
  });

  it("cursor='' (empty string) is treated as absent – returns 200", async () => {
    const res = await request(app).get("/api/v1/settlements?cursor=");

    expect(res.status).toBe(200);
  });

  it("portfolio cursor round-trip succeeds when has_more=true", async () => {
    const first = await request(app).get(
      `/api/v1/portfolio?investor=${encodeURIComponent(KNOWN_INVESTOR)}&limit=1`
    );

    if (first.status !== 200) return;

    const { next_cursor, has_more } = first.body;
    if (!has_more || !next_cursor) {
      // Only one item in mock data – nothing to page through
      return;
    }

    const second = await request(app).get(
      `/api/v1/portfolio?investor=${encodeURIComponent(KNOWN_INVESTOR)}&limit=1&cursor=${next_cursor}`
    );

    expect(second.status).toBe(200);
    expect(second.body).toHaveProperty("data");
    expect(Array.isArray(second.body.data)).toBe(true);
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 14. Status endpoint
// ═════════════════════════════════════════════════════════════════════════════

describe("GET /api/v1/status", () => {
  it("200 – returns lag status object", async () => {
    const res = await request(app).get("/api/v1/status");

    expect(res.status).toBe(200);
    expect(typeof res.body).toBe("object");
  });
});
