import { describe, expect, it, beforeEach } from "@jest/globals";
import request from "supertest";
import app from "../src/app";
import { rateLimiter } from "../src/middleware/rate-limit";
import { freshnessService } from "../src/services/freshnessService";

// ── Helpers ──────────────────────────────────────────────────────────────────

/** Assert that a response body carries the four freshness fields with correct types. */
function assertFreshnessShape(freshness: any) {
  expect(typeof freshness.lastIndexedLedger).toBe("number");
  expect(typeof freshness.indexLagSeconds).toBe("number");
  expect(typeof freshness.lastUpdatedAt).toBe("string");
  expect(typeof freshness.cursor).toBe("string");
}

/** Assert that the cursor contains only digits and underscores (no raw topology). */
function assertCursorOpaque(cursor: string) {
  expect(/^[0-9]+_[0-9]+$/.test(cursor)).toBe(true);
}

// ── Freshness schema & semantics ─────────────────────────────────────────────

describe("Freshness metadata", () => {
  beforeEach(() => {
    freshnessService.setMockNowMs(null);
    freshnessService.setMockLastIndexedLedger(null);
    freshnessService.setMockChainTipLedger(null);
  });

  describe("Schema stability", () => {
    it("invoice list response has all four freshness fields", async () => {
      const res = await request(app).get("/api/v1/invoices");
      expect(res.status).toBe(200);
      expect(res.body).toHaveProperty("data");
      expect(res.body).toHaveProperty("freshness");
      assertFreshnessShape(res.body.freshness);
    });

    it("invoice by-id response has all four freshness fields", async () => {
      const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await request(app).get(`/api/v1/invoices/${id}`);
      expect(res.status).toBe(200);
      assertFreshnessShape(res.body.freshness);
    });

    it("bids list response has all four freshness fields", async () => {
      const res = await request(app).get("/api/v1/bids");
      expect(res.status).toBe(200);
      assertFreshnessShape(res.body.freshness);
    });

    it("settlements list response has all four freshness fields", async () => {
      const res = await request(app).get("/api/v1/settlements");
      expect(res.status).toBe(200);
      assertFreshnessShape(res.body.freshness);
    });

    it("settlement by-id response has all four freshness fields", async () => {
      const id = "0xsettle123";
      const res = await request(app).get(`/api/v1/settlements/${id}`);
      expect(res.status).toBe(200);
      assertFreshnessShape(res.body.freshness);
    });

    it("disputes list response has all four freshness fields", async () => {
      const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await request(app).get(`/api/v1/invoices/${id}/disputes`);
      expect(res.status).toBe(200);
      assertFreshnessShape(res.body.freshness);
    });
  });

  describe("Zero-lag", () => {
    it("indexLagSeconds is 0 when indexer is at chain tip", async () => {
      freshnessService.setMockLastIndexedLedger(100000);
      freshnessService.setMockChainTipLedger(100000);

      const res = await request(app).get("/api/v1/invoices");
      expect(res.body.freshness.indexLagSeconds).toBe(0);
    });

    it("indexLagSeconds is 0 when indexer is ahead of chain tip", async () => {
      freshnessService.setMockLastIndexedLedger(100005);
      freshnessService.setMockChainTipLedger(100000);

      const res = await request(app).get("/api/v1/bids");
      expect(res.body.freshness.indexLagSeconds).toBe(0);
    });
  });

  describe("Lag simulation", () => {
    it("indexLagSeconds equals lagLedgers * 5 for a 10-ledger gap", async () => {
      freshnessService.setMockLastIndexedLedger(100000);
      freshnessService.setMockChainTipLedger(100010); // 10 ledgers behind

      const res = await request(app).get("/api/v1/invoices");
      expect(res.body.freshness.indexLagSeconds).toBe(50); // 10 * 5
    });

    it("indexLagSeconds equals lagLedgers * 5 for a 24-ledger gap", async () => {
      freshnessService.setMockLastIndexedLedger(100000);
      freshnessService.setMockChainTipLedger(100024); // 24 ledgers behind

      const res = await request(app).get("/api/v1/settlements");
      expect(res.body.freshness.indexLagSeconds).toBe(120); // 24 * 5
    });

    it("lastUpdatedAt is earlier than now when there is lag", async () => {
      const nowMs = 1_700_000_000_000; // fixed epoch ms
      freshnessService.setMockNowMs(nowMs);
      freshnessService.setMockLastIndexedLedger(100000);
      freshnessService.setMockChainTipLedger(100012); // 60 s lag

      const res = await request(app).get("/api/v1/invoices");
      const ts = new Date(res.body.freshness.lastUpdatedAt).getTime();
      expect(ts).toBe(nowMs - 60_000);
    });
  });

  describe("Cursor opaqueness", () => {
    it("cursor contains only digits and underscore — no raw topology", async () => {
      const res = await request(app).get("/api/v1/invoices");
      assertCursorOpaque(res.body.freshness.cursor);
    });

    it("cursor does not contain node hostnames or IPs", async () => {
      const res = await request(app).get("/api/v1/bids");
      const cursor: string = res.body.freshness.cursor;
      // Must not contain letters (which would indicate a hostname or hash)
      expect(/[a-zA-Z]/.test(cursor)).toBe(false);
    });

    it("cursor encodes lastIndexedLedger as first segment", async () => {
      freshnessService.setMockLastIndexedLedger(42000);
      freshnessService.setMockChainTipLedger(42000);

      const res = await request(app).get("/api/v1/invoices");
      const [seq] = res.body.freshness.cursor.split("_");
      expect(Number(seq)).toBe(42000);
    });
  });

  describe("lastUpdatedAt format", () => {
    it("is a valid ISO 8601 UTC string", async () => {
      const res = await request(app).get("/api/v1/invoices");
      const ts = res.body.freshness.lastUpdatedAt as string;
      expect(ts).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
    });
  });
});

// ── FreshnessService unit tests ───────────────────────────────────────────────

describe("FreshnessService", () => {
  const { FreshnessService, buildCursor, parseCursor } = require("../src/services/freshnessService");

  beforeEach(() => {
    freshnessService.setMockNowMs(null);
    freshnessService.setMockLastIndexedLedger(null);
    freshnessService.setMockChainTipLedger(null);
  });

  it("singleton returns the same instance", () => {
    const a = FreshnessService.getInstance();
    const b = FreshnessService.getInstance();
    expect(a).toBe(b);
  });

  it("buildCursor produces <seq>_<offset>", () => {
    expect(buildCursor(1000, 25)).toBe("1000_25");
    expect(buildCursor(0, 0)).toBe("0_0");
  });

  it("parseCursor round-trips correctly", () => {
    expect(parseCursor("1000_25")).toEqual([1000, 25]);
    expect(parseCursor("0_0")).toEqual([0, 0]);
  });

  it("parseCursor returns null for malformed input", () => {
    expect(parseCursor("notacursor")).toBeNull();
    expect(parseCursor("")).toBeNull();
    expect(parseCursor("abc_def")).toBeNull();
    expect(parseCursor("1000_25_extra")).toBeNull();
  });

  it("getFreshness uses offset parameter in cursor", () => {
    freshnessService.setMockLastIndexedLedger(5000);
    freshnessService.setMockChainTipLedger(5000);
    const meta = freshnessService.getFreshness(7);
    expect(meta.cursor).toBe("5000_7");
  });

  it("getFreshness default offset is 0", () => {
    freshnessService.setMockLastIndexedLedger(5000);
    freshnessService.setMockChainTipLedger(5000);
    const meta = freshnessService.getFreshness();
    expect(meta.cursor.endsWith("_0")).toBe(true);
  });
});

// ── Existing API tests (updated for wrapped response shape) ──────────────────

describe("QuickLendX API Skeleton Tests", () => {
  describe("Health Check", () => {
    it("should return 200 OK for /health", async () => {
      const res = await request(app).get("/health");
      expect(res.status).toBe(200);
      expect(res.body).toHaveProperty("status", "ok");
    });

    it("should return 200 OK for /api/v1/health", async () => {
      const res = await request(app).get("/api/v1/health");
      expect(res.status).toBe(200);
      expect(res.body).toHaveProperty("status", "ok");
    });
  });

  describe("Invoice API (v1)", () => {
    it("should list invoices", async () => {
      const res = await request(app).get("/api/v1/invoices");
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body.data)).toBe(true);
      expect(res.body.data.length).toBeGreaterThan(0);
      expect(res.body.data[0]).toHaveProperty("id");
      expect(res.body.data[0]).toHaveProperty("status");
    });

    it("should filter invoices by business", async () => {
      const business = "GDVLRH4G4...7Y";
      const res = await request(app).get(`/api/v1/invoices?business=${business}`);
      expect(res.status).toBe(200);
      expect(res.body.data.every((i: any) => i.business === business)).toBe(true);
    });

    it("should filter invoices by status", async () => {
      const status = "Verified";
      const res = await request(app).get(`/api/v1/invoices?status=${status}`);
      expect(res.status).toBe(200);
      expect(res.body.data.every((i: any) => i.status === status)).toBe(true);
    });

    it("should get invoice by ID", async () => {
      const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await request(app).get(`/api/v1/invoices/${id}`);
      expect(res.status).toBe(200);
      expect(res.body.data.id).toBe(id);
    });

    it("should return 404 for non-existent invoice", async () => {
      const res = await request(app).get("/api/v1/invoices/nonexistent");
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("INVOICE_NOT_FOUND");
    });
  });

  describe("Bid API (v1)", () => {
    it("should list bids", async () => {
      const res = await request(app).get("/api/v1/bids");
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body.data)).toBe(true);
      expect(res.body.data.length).toBeGreaterThan(0);
    });

    it("should filter bids by invoice_id", async () => {
      const invoice_id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await request(app).get(`/api/v1/bids?invoice_id=${invoice_id}`);
      expect(res.status).toBe(200);
      expect(res.body.data.every((b: any) => b.invoice_id === invoice_id)).toBe(true);
    });

    it("should filter bids by investor", async () => {
      const investor = "GA...ABC";
      const res = await request(app).get(`/api/v1/bids?investor=${investor}`);
      expect(res.status).toBe(200);
      expect(res.body.data.every((b: any) => b.investor === investor)).toBe(true);
    });
  });

  describe("Settlement API (v1)", () => {
    it("should list settlements", async () => {
      const res = await request(app).get("/api/v1/settlements");
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body.data)).toBe(true);
      expect(res.body.data.length).toBeGreaterThan(0);
    });

    it("should get settlement by ID", async () => {
      const id = "0xsettle123";
      const res = await request(app).get(`/api/v1/settlements/${id}`);
      expect(res.status).toBe(200);
      expect(res.body.data.id).toBe(id);
    });

    it("should return 404 for non-existent settlement", async () => {
      const res = await request(app).get("/api/v1/settlements/nonexistent");
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("SETTLEMENT_NOT_FOUND");
    });

    it("should filter settlements by invoice_id", async () => {
      const invoice_id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await request(app).get(`/api/v1/settlements?invoice_id=${invoice_id}`);
      expect(res.status).toBe(200);
      expect(res.body.data.every((s: any) => s.invoice_id === invoice_id)).toBe(true);
    });
  });

  describe("Dispute API (v1)", () => {
    it("should list disputes for an invoice", async () => {
      const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await request(app).get(`/api/v1/invoices/${id}/disputes`);
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body.data)).toBe(true);
      expect(res.body.data.length).toBeGreaterThan(0);
      expect(res.body.data[0].invoice_id).toBe(id);
    });
  });

  describe("Error Handling", () => {
    it("should return 404 for unknown routes", async () => {
      const res = await request(app).get("/api/v1/unknown-route");
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("NOT_FOUND");
    });

    it("should handle 500 errors with custom code", async () => {
      const res = await request(app).get("/api/v1/test-errors/500");
      expect(res.status).toBe(500);
      expect(res.body.error.code).toBe("TEST_ERROR");
    });

    it("should handle default errors", async () => {
      const res = await request(app).get("/api/v1/test-errors/default-error");
      expect(res.status).toBe(500);
      expect(res.body.error.code).toBe("INTERNAL_ERROR");
    });

    it("should handle errors without message", async () => {
      const res = await request(app).get("/api/v1/test-errors/no-message");
      expect(res.status).toBe(500);
      expect(res.body.error.message).toBe("Internal Server Error");
    });

    it("should include details in development mode", async () => {
      const res = await request(app).get("/api/v1/test-errors/development");
      expect(res.status).toBe(500);
    });

    it("should handle unknown IP in rate limiter", async () => {
      const res = await request(app)
        .get("/health")
        .set("X-Simulate-No-IP", "true");
      expect(res.status).toBe(200);
    });

    it("should return 429 when rate limit is exceeded", async () => {
      const testIp = "127.0.0.1";
      for (let i = 0; i < 1000; i++) {
        await rateLimiter.consume(testIp);
      }

      const res = await request(app).get("/health").set("X-Forwarded-For", testIp);
      expect(res.status).toBe(429);
      expect(res.body.error.code).toBe("RATE_LIMIT_EXCEEDED");
    });
  });
});
