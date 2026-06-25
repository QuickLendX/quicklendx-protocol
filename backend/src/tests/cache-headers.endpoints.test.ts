import supertest from "supertest";
import app from "../app";
import { freshnessService } from "../services/freshnessService";
import { MOCK_INVOICES } from "../controllers/v1/invoices";
import { MOCK_SETTLEMENTS } from "../controllers/v1/settlements";
import { CC_SHORT, CC_LONG, CC_NO_STORE } from "../middleware/cache-headers";

jest.mock("../middleware/api-key-auth", () => ({
  apiKeyAuthMiddleware: (req: any, res: any, next: any) => {
    req.apiKey = {
      id: "mock-key-id",
      key_hash: "mock-hash",
      scopes: ["read:invoices", "read:bids"],
      created_by: "mock-requester",
    };
    next();
  },
  optionalApiKeyAuth: (req: any, res: any, next: any) => {
    req.apiKey = {
      id: "mock-key-id",
      key_hash: "mock-hash",
      scopes: ["read:invoices", "read:bids"],
      created_by: "mock-requester",
    };
    next();
  },
  requireScopes: () => (req: any, res: any, next: any) => next(),
}));

describe("Conditional Caching & ETag Integration Tests", () => {
  beforeAll(() => {
    // Set deterministic mock values for freshness headers
    freshnessService.setMockNowMs(1710000000000);
    freshnessService.setMockLastIndexedLedger(100000);
    freshnessService.setMockChainTipLedger(100000);
  });

  afterAll(() => {
    // Reset mock values to avoid side-effects in other tests
    freshnessService.setMockNowMs(null);
    freshnessService.setMockLastIndexedLedger(null);
    freshnessService.setMockChainTipLedger(null);
  });

  describe("CC_SHORT tier endpoints (Invoices & Portfolio)", () => {
    it("should return CC_SHORT, ETag, and Last-Modified headers for invoices list", async () => {
      const res = await supertest(app).get("/api/v1/invoices");
      expect(res.status).toBe(200);
      expect(res.headers["cache-control"]).toBe(CC_SHORT);
      expect(res.headers["etag"]).toBeDefined();
      expect(res.headers["last-modified"]).toBeDefined();
      expect(res.headers["vary"]).toBe("Accept-Encoding");

      // Verify conditional GET - If-None-Match
      const etag = res.headers["etag"];
      const revalidateRes = await supertest(app)
        .get("/api/v1/invoices")
        .set("If-None-Match", etag);
      expect(revalidateRes.status).toBe(304);

      // Verify conditional GET - If-Modified-Since
      const lastModified = res.headers["last-modified"];
      const revalidateMsRes = await supertest(app)
        .get("/api/v1/invoices")
        .set("If-Modified-Since", lastModified);
      expect(revalidateMsRes.status).toBe(304);
    });

    it("should return CC_SHORT, ETag, and Last-Modified headers for a single invoice", async () => {
      const invoiceId = MOCK_INVOICES[0].id;
      const res = await supertest(app).get(`/api/v1/invoices/${invoiceId}`);
      expect(res.status).toBe(200);
      expect(res.headers["cache-control"]).toBe(CC_SHORT);
      expect(res.headers["etag"]).toBeDefined();
      expect(res.headers["last-modified"]).toBeDefined();

      // Verify conditional GET - If-None-Match
      const etag = res.headers["etag"];
      const revalidateRes = await supertest(app)
        .get(`/api/v1/invoices/${invoiceId}`)
        .set("If-None-Match", etag);
      expect(revalidateRes.status).toBe(304);
    });

    it("should return CC_SHORT, ETag, and Last-Modified headers for portfolio", async () => {
      const res = await supertest(app).get("/api/v1/portfolio?investor=GA...ABC");
      expect(res.status).toBe(200);
      expect(res.headers["cache-control"]).toBe(CC_SHORT);
      expect(res.headers["etag"]).toBeDefined();
      expect(res.headers["last-modified"]).toBeDefined();

      // Verify conditional GET - If-None-Match
      const etag = res.headers["etag"];
      const revalidateRes = await supertest(app)
        .get("/api/v1/portfolio?investor=GA...ABC")
        .set("If-None-Match", etag);
      expect(revalidateRes.status).toBe(304);
    });

    it("should return 400 for portfolio when investor is missing", async () => {
      const res = await supertest(app).get("/api/v1/portfolio");
      expect(res.status).toBe(400);
      // Errors should not carry cache headers
      expect(res.headers["etag"]).toBeUndefined();
    });
  });

  describe("CC_LONG tier endpoints (Settlements)", () => {
    it("should return CC_LONG, ETag, and Last-Modified headers for settlements list", async () => {
      const res = await supertest(app).get("/api/v1/settlements");
      expect(res.status).toBe(200);
      expect(res.headers["cache-control"]).toBe(CC_LONG);
      expect(res.headers["etag"]).toBeDefined();
      expect(res.headers["last-modified"]).toBeDefined();

      // Verify conditional GET - If-None-Match
      const etag = res.headers["etag"];
      const revalidateRes = await supertest(app)
        .get("/api/v1/settlements")
        .set("If-None-Match", etag);
      expect(revalidateRes.status).toBe(304);
    });

    it("should return CC_LONG, ETag, and Last-Modified headers for a single settlement", async () => {
      const settlementId = MOCK_SETTLEMENTS[0].id;
      const res = await supertest(app).get(`/api/v1/settlements/${settlementId}`);
      expect(res.status).toBe(200);
      expect(res.headers["cache-control"]).toBe(CC_LONG);
      expect(res.headers["etag"]).toBeDefined();
      expect(res.headers["last-modified"]).toBeDefined();

      // Verify conditional GET - If-None-Match
      const etag = res.headers["etag"];
      const revalidateRes = await supertest(app)
        .get(`/api/v1/settlements/${settlementId}`)
        .set("If-None-Match", etag);
      expect(revalidateRes.status).toBe(304);
    });
  });

  describe("CC_NO_STORE tier endpoints (Bids & Disputes)", () => {
    it("should return CC_NO_STORE and omit ETag/Last-Modified for bids list", async () => {
      const validInvoiceId = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await supertest(app).get(`/api/v1/bids?invoice_id=${validInvoiceId}`);
      expect(res.status).toBe(200);
      expect(res.headers["cache-control"]).toBe(CC_NO_STORE);
      expect(res.headers["etag"]).toBeUndefined();
      expect(res.headers["last-modified"]).toBeUndefined();

      // Should never return 304
      const revalidateRes = await supertest(app)
        .get(`/api/v1/bids?invoice_id=${validInvoiceId}`)
        .set("If-None-Match", "*");
      expect(revalidateRes.status).toBe(200);
    });

    it("should return CC_NO_STORE and omit ETag/Last-Modified for disputes", async () => {
      const invoiceId = MOCK_INVOICES[0].id;
      const res = await supertest(app).get(`/api/v1/invoices/${invoiceId}/disputes`);
      expect(res.status).toBe(200);
      expect(res.headers["cache-control"]).toBe(CC_NO_STORE);
      expect(res.headers["etag"]).toBeUndefined();
      expect(res.headers["last-modified"]).toBeUndefined();

      // Should never return 304
      const revalidateRes = await supertest(app)
        .get(`/api/v1/invoices/${invoiceId}/disputes`)
        .set("If-None-Match", "*");
      expect(revalidateRes.status).toBe(200);
    });
  });

  describe("Edge cases & security properties", () => {
    it("should return 404 for non-existent routes without cache headers", async () => {
      const res = await supertest(app).get("/api/v1/non-existent-route-xyz");
      expect(res.status).toBe(404);
      expect(res.headers["etag"]).toBeUndefined();
    });

    it("should return 200 when If-None-Match does not match the current ETag", async () => {
      const res = await supertest(app)
        .get("/api/v1/invoices")
        .set("If-None-Match", '"outdated-etag"');
      expect(res.status).toBe(200);
    });

    it("should return 200 when If-Modified-Since is a past date", async () => {
      const invoiceDate = new Date(MOCK_INVOICES[0].created_at);
      const pastDate = new Date(invoiceDate.getTime() - 24 * 3600 * 1000).toUTCString();
      const res = await supertest(app)
        .get("/api/v1/invoices")
        .set("If-Modified-Since", pastDate);
      expect(res.status).toBe(200);
    });

    it("should return 200 when If-Modified-Since is invalid", async () => {
      const res = await supertest(app)
        .get("/api/v1/invoices")
        .set("If-Modified-Since", "invalid-date-string");
      expect(res.status).toBe(200);
    });
  });
});
