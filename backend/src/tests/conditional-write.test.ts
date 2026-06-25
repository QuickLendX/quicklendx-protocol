import { describe, it, expect, beforeEach, beforeAll, afterAll, afterEach, jest } from "@jest/globals";
import request from "supertest";
import app from "../app";
import { Request, Response } from "express";
import {
  assertConditionalWrite,
  computeETag,
  extractLastModified,
  isNotModified,
  applyCacheHeaders,
  CC_NO_STORE,
} from "../middleware/cache-headers";
import { invoiceStore } from "../services/invoiceStore";
import { createBid, getBids, getBestBid, getTopBids } from "../controllers/v1/bids";
import { MOCK_INVOICES } from "../controllers/v1/invoices";
import { bidStore } from "../services/bidStore";
import pool from "../services/database";
import { getDatabase, closeDatabase } from "../lib/database";
import { apiKeyService } from "../services/api-key-service";
import path from "path";
import fs from "fs";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function mockReq(headers: Record<string, string> = {}): Request {
  return { headers } as unknown as Request;
}

function mockRes(): Response & { _status: number; _json: any; _headers: Record<string, string> } {
  const res: any = {
    _status: 0,
    _json: null,
    _headers: {} as Record<string, string>,
    status(code: number) {
      res._status = code;
      return res;
    },
    json(body: any) {
      res._json = body;
      return res;
    },
    setHeader(name: string, value: string) {
      res._headers[name.toLowerCase()] = value;
    },
  };
  return res;
}

// ---------------------------------------------------------------------------
// Unit tests — assertConditionalWrite
// ---------------------------------------------------------------------------

describe("assertConditionalWrite (unit)", () => {
  describe("If-Match header present", () => {
    it("should return false when etag matches", () => {
      const etag = computeETag('{"id":"1"}');
      const req = mockReq({ "if-match": etag });
      const res = mockRes();
      expect(assertConditionalWrite(req, res, etag)).toBe(false);
    });

    it("should return true (412) when etag does not match", () => {
      const req = mockReq({ "if-match": '"stale-etag"' });
      const res = mockRes();
      expect(assertConditionalWrite(req, res, '"current-etag"')).toBe(true);
      expect(res._status).toBe(412);
      expect(res._json.error.code).toBe("PRECONDITION_FAILED");
      expect(res._headers["cache-control"]).toBe(CC_NO_STORE);
    });

    it("should support comma-separated list with a matching etag", () => {
      const etag = '"abc"';
      const req = mockReq({ "if-match": '"old", "abc", "other"' });
      const res = mockRes();
      expect(assertConditionalWrite(req, res, etag)).toBe(false);
    });

    it("should return true (412) for comma-separated list with no matching etag", () => {
      const req = mockReq({ "if-match": '"old", "other"' });
      const res = mockRes();
      expect(assertConditionalWrite(req, res, '"current"')).toBe(true);
      expect(res._status).toBe(412);
    });

    it("should pass with wildcard * when etag exists", () => {
      const req = mockReq({ "if-match": "*" });
      const res = mockRes();
      expect(assertConditionalWrite(req, res, '"any-etag"')).toBe(false);
    });

    it("should fail (412) with wildcard * when etag is null (resource does not exist)", () => {
      const req = mockReq({ "if-match": "*" });
      const res = mockRes();
      expect(assertConditionalWrite(req, res, null)).toBe(true);
      expect(res._status).toBe(412);
      expect(res._json.error.code).toBe("PRECONDITION_FAILED");
    });

    it("should fail (412) when etag is null regardless of If-Match value", () => {
      const req = mockReq({ "if-match": '"some-tag"' });
      const res = mockRes();
      expect(assertConditionalWrite(req, res, null)).toBe(true);
      expect(res._status).toBe(412);
    });
  });

  describe("required option (missing If-Match)", () => {
    it("should return true (400) when required and If-Match is missing", () => {
      const req = mockReq();
      const res = mockRes();
      expect(assertConditionalWrite(req, res, '"tag"', { required: true })).toBe(true);
      expect(res._status).toBe(400);
      expect(res._json.error.code).toBe("PRECONDITION_REQUIRED");
      expect(res._json.error.message).toBe("If-Match header is required");
      expect(res._headers["cache-control"]).toBe(CC_NO_STORE);
    });

    it("should return false when not required and If-Match is missing", () => {
      const req = mockReq();
      const res = mockRes();
      expect(assertConditionalWrite(req, res, '"tag"', { required: false })).toBe(false);
    });

    it("should return false when required is not specified and If-Match is missing", () => {
      const req = mockReq();
      const res = mockRes();
      expect(assertConditionalWrite(req, res, '"tag"')).toBe(false);
    });
  });

  describe("If-Unmodified-Since", () => {
    it("should return false when resource is older than If-Unmodified-Since", () => {
      const pastDate = new Date("2025-01-01T00:00:00Z");
      const futureHeader = new Date("2026-06-01T00:00:00Z").toUTCString();
      const req = mockReq({ "if-unmodified-since": futureHeader });
      const res = mockRes();
      expect(
        assertConditionalWrite(req, res, '"tag"', { lastModified: pastDate })
      ).toBe(false);
    });

    it("should return true (412) when resource is newer than If-Unmodified-Since", () => {
      const recentDate = new Date("2026-06-20T00:00:00Z");
      const oldHeader = new Date("2026-01-01T00:00:00Z").toUTCString();
      const req = mockReq({ "if-unmodified-since": oldHeader });
      const res = mockRes();
      expect(
        assertConditionalWrite(req, res, '"tag"', { lastModified: recentDate })
      ).toBe(true);
      expect(res._status).toBe(412);
      expect(res._json.error.code).toBe("PRECONDITION_FAILED");
    });

    it("should ignore If-Unmodified-Since when lastModified is not provided", () => {
      const req = mockReq({ "if-unmodified-since": new Date("2020-01-01").toUTCString() });
      const res = mockRes();
      expect(assertConditionalWrite(req, res, '"tag"')).toBe(false);
    });

    it("should ignore If-Unmodified-Since when lastModified is null", () => {
      const req = mockReq({ "if-unmodified-since": new Date("2020-01-01").toUTCString() });
      const res = mockRes();
      expect(
        assertConditionalWrite(req, res, '"tag"', { lastModified: null })
      ).toBe(false);
    });

    it("should ignore invalid If-Unmodified-Since date strings", () => {
      const req = mockReq({ "if-unmodified-since": "not-a-date" });
      const res = mockRes();
      expect(
        assertConditionalWrite(req, res, '"tag"', {
          lastModified: new Date("2026-01-01"),
        })
      ).toBe(false);
    });
  });

  describe("no headers present", () => {
    it("should return false (proceed) when no precondition headers are present", () => {
      const req = mockReq();
      const res = mockRes();
      expect(assertConditionalWrite(req, res, '"tag"')).toBe(false);
    });

    it("should return false when etag is null and no headers are present", () => {
      const req = mockReq();
      const res = mockRes();
      expect(assertConditionalWrite(req, res, null)).toBe(false);
    });
  });

  describe("If-Match takes precedence over If-Unmodified-Since", () => {
    it("should fail on If-Match mismatch even if If-Unmodified-Since would pass", () => {
      const futureHeader = new Date("2030-01-01T00:00:00Z").toUTCString();
      const req = mockReq({
        "if-match": '"wrong"',
        "if-unmodified-since": futureHeader,
      });
      const res = mockRes();
      expect(
        assertConditionalWrite(req, res, '"current"', {
          lastModified: new Date("2025-01-01"),
        })
      ).toBe(true);
      expect(res._status).toBe(412);
    });
  });
});

// ---------------------------------------------------------------------------
// Additional branch-coverage tests
// ---------------------------------------------------------------------------

describe("assertConditionalWrite — additional branch coverage", () => {
  it("should pass when If-Match has comma-separated list and current ETag is in the middle", () => {
    const etag = '"mid-tag"';
    const req = mockReq({ "if-match": '"first", "mid-tag", "last"' });
    const res = mockRes();
    expect(assertConditionalWrite(req, res, etag)).toBe(false);
  });

  it("should skip If-Unmodified-Since check when lastModified is null", () => {
    const req = mockReq({
      "if-unmodified-since": new Date("2020-01-01").toUTCString(),
    });
    const res = mockRes();
    expect(
      assertConditionalWrite(req, res, '"tag"', { lastModified: null })
    ).toBe(false);
  });

  it("should pass when If-Unmodified-Since date is exactly equal to lastModified", () => {
    const exactDate = new Date("2026-06-15T12:00:00Z");
    const req = mockReq({
      "if-unmodified-since": exactDate.toUTCString(),
    });
    const res = mockRes();
    // lastModified > since is false when exactly equal, so the check passes
    expect(
      assertConditionalWrite(req, res, '"tag"', { lastModified: exactDate })
    ).toBe(false);
  });

  it("should proceed when required:true and both If-Match and If-Unmodified-Since are present and matching", () => {
    const etag = '"fresh"';
    const pastDate = new Date("2025-01-01T00:00:00Z");
    const futureHeader = new Date("2026-06-01T00:00:00Z").toUTCString();
    const req = mockReq({
      "if-match": etag,
      "if-unmodified-since": futureHeader,
    });
    const res = mockRes();
    expect(
      assertConditionalWrite(req, res, etag, {
        required: true,
        lastModified: pastDate,
      })
    ).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// isNotModified — branch coverage for If-Modified-Since path
// ---------------------------------------------------------------------------

describe("isNotModified (unit)", () => {
  it("should return true when If-Modified-Since header is present and resource has not been modified since", () => {
    const lastModified = new Date("2026-01-01T00:00:00Z");
    const sinceHeader = new Date("2026-06-01T00:00:00Z").toUTCString();
    const req = mockReq({ "if-modified-since": sinceHeader });
    expect(isNotModified(req, '"etag"', lastModified)).toBe(true);
  });

  it("should return false when If-Modified-Since header is present but resource has been modified after", () => {
    const lastModified = new Date("2026-06-20T00:00:00Z");
    const sinceHeader = new Date("2026-01-01T00:00:00Z").toUTCString();
    const req = mockReq({ "if-modified-since": sinceHeader });
    expect(isNotModified(req, '"etag"', lastModified)).toBe(false);
  });

  it("should return true when If-Modified-Since date is exactly equal to lastModified", () => {
    const exactDate = new Date("2026-06-15T12:00:00Z");
    const req = mockReq({ "if-modified-since": exactDate.toUTCString() });
    expect(isNotModified(req, '"etag"', exactDate)).toBe(true);
  });

  it("should return false when If-Modified-Since is present but lastModified is null", () => {
    const req = mockReq({
      "if-modified-since": new Date("2026-01-01").toUTCString(),
    });
    expect(isNotModified(req, '"etag"', null)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// extractLastModified — branch coverage for uncovered paths
// ---------------------------------------------------------------------------

describe("extractLastModified (unit)", () => {
  it("should extract timestamp from a plain object with updated_at", () => {
    const result = extractLastModified({ updated_at: 1700000000 });
    expect(result).toBeInstanceOf(Date);
    expect(result!.getTime()).toBe(1700000000 * 1000);
  });

  it("should extract the max timestamp from a data-wrapped array", () => {
    const result = extractLastModified({
      data: [
        { updated_at: 1700000000 },
        { updated_at: 1700001000 },
      ],
    });
    expect(result).toBeInstanceOf(Date);
    expect(result!.getTime()).toBe(1700001000 * 1000);
  });

  it("should return null for a plain array with no timestamp fields", () => {
    const result = extractLastModified([{ name: "no-ts" }]);
    expect(result).toBeNull();
  });

  it("should skip null and non-object records in an array", () => {
    const result = extractLastModified([null, "string", { timestamp: 1700000000 }]);
    expect(result).toBeInstanceOf(Date);
    expect(result!.getTime()).toBe(1700000000 * 1000);
  });

  it("should handle a data-wrapped single record (not array)", () => {
    const result = extractLastModified({ data: { created_at: 1700000000 } });
    expect(result).toBeInstanceOf(Date);
    expect(result!.getTime()).toBe(1700000000 * 1000);
  });
});

// ---------------------------------------------------------------------------
// Integration tests — POST /api/v1/bids with conditional write
// ---------------------------------------------------------------------------

describe("POST /api/v1/bids — conditional write (integration)", () => {
  let TEST_API_KEY = "qlx_test_conditionalwrite";
  let testInvoice: any;
  let invoiceETag: string;
  let poolConnectSpy: any;

  const TEST_INVOICE_ID = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
  const TEST_DB_DIR = path.resolve(__dirname, "../../../.data");
  const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-conditional-write-${crypto.randomUUID()}.db`);

  beforeAll(async () => {
    if (!fs.existsSync(TEST_DB_DIR)) {
      fs.mkdirSync(TEST_DB_DIR, { recursive: true });
    }
    process.env.DATABASE_PATH = TEST_DB_PATH;
    closeDatabase();
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
      )
    `);
    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_key_audit_log (
        id TEXT PRIMARY KEY,
        event_type TEXT NOT NULL,
        key_id TEXT NOT NULL,
        actor TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        ip_address TEXT,
        endpoint TEXT,
        metadata TEXT
      )
    `);
    conn.exec(`
      CREATE TABLE IF NOT EXISTS invoices (
        id TEXT PRIMARY KEY,
        business TEXT NOT NULL,
        amount TEXT NOT NULL,
        currency TEXT NOT NULL,
        due_date INTEGER NOT NULL,
        status TEXT NOT NULL,
        description TEXT NOT NULL,
        category TEXT NOT NULL,
        tags TEXT NOT NULL,
        metadata TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        contract_version INTEGER NOT NULL,
        event_schema_version INTEGER NOT NULL,
        indexed_at TEXT NOT NULL
      )
    `);

    // Create a test API key
    const saKey = await apiKeyService.createApiKey({
      name: "Test Key",
      scopes: ["read:*", "write:*"],
      created_by: "test-user",
    });
    TEST_API_KEY = saKey.plaintext_key;

    // Seed mock invoice
    invoiceStore.insertInvoice({
      id: TEST_INVOICE_ID,
      business: "biz-1",
      amount: "1000",
      currency: "USD",
      due_date: 1234567890,
      status: "Verified" as any,
      description: "Test description",
      category: "Services" as any,
      tags: ["test"],
      metadata: {
        customer_name: "John",
        customer_address: "123 Main St",
        tax_id: "123",
        line_items: [],
        notes: ""
      },
      created_at: 1234567800,
      updated_at: 1234567800,
      contract_version: 1,
      event_schema_version: 1,
      indexed_at: new Date().toISOString()
    });
  });

  afterAll(() => {
    closeDatabase();
    try {
      if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
      try { fs.unlinkSync(TEST_DB_PATH + "-wal"); } catch {}
      try { fs.unlinkSync(TEST_DB_PATH + "-shm"); } catch {}
    } catch {}
  });

  beforeEach(() => {
    poolConnectSpy = jest.spyOn(pool as any, 'connect').mockResolvedValue({
      query: jest.fn(async (sql: any, params?: any): Promise<any> => {
        const sqlStr = String(sql);
        if (sqlStr.includes('SELECT id, status FROM invoices')) {
          return { rows: [{ id: params?.[0], status: 'Verified' }], rowCount: 1 };
        }
        if (sqlStr.includes('SELECT bid_id FROM bids')) {
          return { rows: [], rowCount: 0 };
        }
        if (sqlStr.includes('INSERT INTO bids')) {
          return {
            rows: [{
              bid_id: 'mock-bid-id',
              invoice_id: params?.[1],
              investor: params?.[2],
              bid_amount: params?.[3],
              expected_return: params?.[4],
              timestamp: params?.[5],
              status: 'Placed',
              expiration_timestamp: params?.[7],
              created_by: params?.[8],
            }],
            rowCount: 1
          };
        }
        return { rows: [], rowCount: 0 };
      }) as any,
      release: jest.fn() as any,
    } as any);

    try {
      testInvoice = invoiceStore.findInvoiceById(TEST_INVOICE_ID);
    } catch {
      testInvoice = undefined;
    }
    if (testInvoice) {
      invoiceETag = computeETag(JSON.stringify(testInvoice));
    }
  });

  afterEach(() => {
    if (poolConnectSpy) {
      poolConnectSpy.mockRestore();
    }
    jest.restoreAllMocks();
  });

  const validBid = {
    invoice_id: TEST_INVOICE_ID,
    bid_amount: "1000000",
    expected_return: "1500000",
    expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
  };

  it("should create bid (201) when If-Match matches the current invoice ETag", async () => {
    if (!testInvoice) return;

    const res = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${TEST_API_KEY}`)
      .set("If-Match", invoiceETag)
      .send(validBid);

    expect(res.status).toBe(201);
    expect(res.body.data.bid_id).toBe("mock-bid-id");
  });

  it("should return 412 when If-Match is a stale etag", async () => {
    if (!testInvoice) return;

    const res = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${TEST_API_KEY}`)
      .set("If-Match", '"stale-etag-value"')
      .send(validBid);

    expect(res.status).toBe(412);
    expect(res.body.error.code).toBe("PRECONDITION_FAILED");
    expect(res.headers["cache-control"]).toBe(CC_NO_STORE);
  });

  it("should succeed (not 412) when If-Match header is absent — backward compatibility", async () => {
    const res = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${TEST_API_KEY}`)
      .send(validBid);

    expect(res.status).toBe(201);
  });

  it("should pass with If-Match: * when the invoice exists", async () => {
    if (!testInvoice) return;

    const res = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${TEST_API_KEY}`)
      .set("If-Match", "*")
      .send(validBid);

    expect(res.status).toBe(201);
  });

  it("should ensure no bid is created on 412 (stale etag)", async () => {
    if (!testInvoice) return;

    const res = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${TEST_API_KEY}`)
      .set("If-Match", '"definitely-wrong-etag"')
      .send(validBid);

    expect(res.status).toBe(412);
    expect(res.body.data).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Unit tests — applyCacheHeaders
// ---------------------------------------------------------------------------

describe("applyCacheHeaders (unit)", () => {
  it("should return false and strip headers when cacheControl is CC_NO_STORE", () => {
    const req = mockReq({ "if-none-match": "123", "if-modified-since": "some-date" });
    const res = mockRes();
    const result = applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body: {} });
    expect(result).toBe(false);
    expect(req.headers["if-none-match"]).toBeUndefined();
    expect(req.headers["if-modified-since"]).toBeUndefined();
    expect(res._headers["cache-control"]).toBe(CC_NO_STORE);
  });

  it("should set ETag and Last-Modified when lastModified is present", () => {
    const req = mockReq();
    const res = mockRes();
    const body = { updated_at: 1700000000 };
    const result = applyCacheHeaders(req, res, { cacheControl: "public, max-age=10", body });
    expect(res._headers["etag"]).toBeDefined();
    expect(res._headers["last-modified"]).toBeDefined();
    expect(result).toBe(false);
  });

  it("should set ETag but not Last-Modified when lastModified is null", () => {
    const req = mockReq();
    const res = mockRes();
    const body = { name: "no-date" };
    const result = applyCacheHeaders(req, res, { cacheControl: "public, max-age=10", body });
    expect(res._headers["etag"]).toBeDefined();
    expect(res._headers["last-modified"]).toBeUndefined();
    expect(result).toBe(false);
  });

  it("should return true when resource is not modified", () => {
    const etag = computeETag(JSON.stringify({ name: "test" }));
    const req = mockReq({ "if-none-match": etag });
    const res = mockRes();
    const result = applyCacheHeaders(req, res, { cacheControl: "public, max-age=10", body: { name: "test" } });
    expect(result).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Additional isNotModified checks for branch coverage
// ---------------------------------------------------------------------------

describe("isNotModified — extra branch coverage", () => {
  it("should return true when If-None-Match contains *", () => {
    const req = mockReq({ "if-none-match": "*" });
    expect(isNotModified(req, '"etag"', null)).toBe(true);
  });

  it("should return true when If-None-Match contains matching etag in a list", () => {
    const req = mockReq({ "if-none-match": '"tag1", "tag2", "tag3"' });
    expect(isNotModified(req, '"tag2"', null)).toBe(true);
  });

  it("should return false when If-None-Match does not contain matching etag", () => {
    const req = mockReq({ "if-none-match": '"tag1", "tag3"' });
    expect(isNotModified(req, '"tag2"', null)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Unit tests — Bids Controller
// ---------------------------------------------------------------------------

describe("Bids Controller (unit)", () => {
  beforeEach(() => {
    jest.restoreAllMocks();
  });

  describe("createBid", () => {
    it("should return 401 if req.apiKey is missing", async () => {
      const req = { apiKey: undefined, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();
      await createBid(req, res, next);
      expect(res._status).toBe(401);
      expect(res._json.error.code).toBe("UNAUTHORIZED");
    });

    it("should call next with error if body validation fails", async () => {
      const req = {
        apiKey: { created_by: "test-user" },
        body: {}, // Invalid body
        headers: {},
      } as any;
      const res = mockRes();
      const next = jest.fn();
      await createBid(req, res, next);
      expect(next).toHaveBeenCalled();
    });

    it("should return 400 with INVALID_BID if bidStore throws a known validation error", async () => {
      const req = {
        apiKey: { created_by: "test-user" },
        body: {
          invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
          bid_amount: "1000",
          expected_return: "1500",
          expiration_timestamp: Math.floor(Date.now() / 1000) + 3600,
        },
        headers: {},
      } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(invoiceStore, 'findInvoiceById').mockReturnValue({
        id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        status: "Verified",
      } as any);

      jest.spyOn(bidStore, 'createBid').mockRejectedValue(new Error("Cannot place bid on invoice with status Pending. Only Verified invoices accept bids."));

      await createBid(req, res, next);

      expect(res._status).toBe(400);
      expect(res._json.error.code).toBe("INVALID_BID");
      expect(res._json.error.message).toBe("Cannot place bid on invoice with status Pending. Only Verified invoices accept bids.");
    });

    it("should call next with error if bidStore throws an unknown error", async () => {
      const req = {
        apiKey: { created_by: "test-user" },
        body: {
          invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
          bid_amount: "1000",
          expected_return: "1500",
          expiration_timestamp: Math.floor(Date.now() / 1000) + 3600,
        },
        headers: {},
      } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(invoiceStore, 'findInvoiceById').mockReturnValue({
        id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        status: "Verified",
      } as any);

      jest.spyOn(bidStore, 'createBid').mockRejectedValue(new Error("Database connection lost"));

      await createBid(req, res, next);

      expect(next).toHaveBeenCalledWith(expect.any(Error));
    });

    it("should return 400 if invoice status is not Verified", async () => {
      const req = {
        apiKey: { created_by: "test-user" },
        body: {
          invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
          bid_amount: "1000",
          expected_return: "1500",
          expiration_timestamp: Math.floor(Date.now() / 1000) + 3600,
        },
        headers: {},
      } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(invoiceStore, 'findInvoiceById').mockReturnValue({
        id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        status: "Pending",
      } as any);

      jest.spyOn(bidStore, 'createBid').mockRejectedValue(new Error("Cannot place bid on invoice with status Pending"));

      await createBid(req, res, next);

      expect(res._status).toBe(400);
      expect(res._json.error.code).toBe("INVALID_BID");
    });

    it("should fallback to MOCK_INVOICES in test env if findInvoiceById throws a no such table error", async () => {
      const hexInvoiceId = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      MOCK_INVOICES.push({
        id: hexInvoiceId,
        status: "Verified",
      });

      const req = {
        apiKey: { created_by: "test-user" },
        body: {
          invoice_id: hexInvoiceId,
          bid_amount: "1000",
          expected_return: "1500",
          expiration_timestamp: Math.floor(Date.now() / 1000) + 3600,
        },
        headers: {},
      } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(invoiceStore, 'findInvoiceById').mockImplementation(() => {
        throw new Error("SqliteError: no such table: invoices");
      });

      jest.spyOn(bidStore, 'createBid').mockResolvedValue({
        bid_id: "mock-bid-id",
      } as any);

      await createBid(req, res, next);

      expect(res._status).toBe(201);
      expect(res._json.data.bid_id).toBe("mock-bid-id");
    });

    it("should throw error if findInvoiceById throws an error other than no such table", async () => {
      const hexInvoiceId = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const req = {
        apiKey: { created_by: "test-user" },
        body: {
          invoice_id: hexInvoiceId,
          bid_amount: "1000",
          expected_return: "1500",
          expiration_timestamp: Math.floor(Date.now() / 1000) + 3600,
        },
        headers: {},
      } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(invoiceStore, 'findInvoiceById').mockImplementation(() => {
        throw new Error("Some other database connection error");
      });

      await createBid(req, res, next);

      expect(next).toHaveBeenCalledWith(expect.any(Error));
    });
  });

  describe("getBids", () => {
    it("should return 400 if invoice_id is missing", async () => {
      const req = { query: {}, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();
      await getBids(req, res, next);
      expect(res._status).toBe(400);
      expect(res._json.error.code).toBe("MISSING_REQUIRED_FIELD");
    });

    it("should return 400 on PaginationError", async () => {
      const req = { query: { invoice_id: "123", limit: "invalid" }, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();
      await getBids(req, res, next);
      expect(res._status).toBe(400);
      expect(res._json.error.code).toBe("INVALID_PAGINATION");
    });

    it("should return 200 and list of bids", async () => {
      const req = { query: { invoice_id: "123" }, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(bidStore, 'getBidsPaginated').mockResolvedValue({
        data: [],
        next_cursor: null,
        has_more: false,
      });

      await getBids(req, res, next);
      expect(res._json).toBeDefined();
      expect(res._json.data).toEqual([]);
    });

    it("should call next with error if getBidsPaginated fails", async () => {
      const req = { query: { invoice_id: "123" }, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(bidStore, 'getBidsPaginated').mockRejectedValue(new Error("Query failed"));

      await getBids(req, res, next);
      expect(next).toHaveBeenCalledWith(expect.any(Error));
    });
  });

  describe("getBestBid", () => {
    it("should return 404 if no best bid found", async () => {
      const req = { params: { invoiceId: "123" }, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(bidStore, 'getBestBid').mockResolvedValue(null);

      await getBestBid(req, res, next);
      expect(res._status).toBe(404);
      expect(res._json.error).toBe("No best bid found for this invoice");
    });

    it("should return 200 with best bid if found", async () => {
      const req = { params: { invoiceId: "123" }, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();

      const mockBid = { bid_id: "bid-1" } as any;
      jest.spyOn(bidStore, 'getBestBid').mockResolvedValue(mockBid);

      await getBestBid(req, res, next);
      expect(res._json.data).toBe(mockBid);
    });

    it("should call next with error if getBestBid fails", async () => {
      const req = { params: { invoiceId: "123" }, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(bidStore, 'getBestBid').mockRejectedValue(new Error("DB error"));

      await getBestBid(req, res, next);
      expect(next).toHaveBeenCalledWith(expect.any(Error));
    });
  });

  describe("getTopBids", () => {
    it("should return 200 with ranked bids", async () => {
      const req = { params: { invoiceId: "123" }, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(bidStore, 'getRankedBids').mockResolvedValue([]);

      await getTopBids(req, res, next);
      expect(res._json.data).toEqual([]);
    });

    it("should call next with error if getRankedBids fails", async () => {
      const req = { params: { invoiceId: "123" }, headers: {} } as any;
      const res = mockRes();
      const next = jest.fn();

      jest.spyOn(bidStore, 'getRankedBids').mockRejectedValue(new Error("DB error"));

      await getTopBids(req, res, next);
      expect(next).toHaveBeenCalledWith(expect.any(Error));
    });
  });
});
