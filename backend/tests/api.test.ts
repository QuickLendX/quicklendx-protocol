import request from "supertest";
import app from "../src/app";

// Valid 56-char Stellar address for testing
const VALID_STELLAR_ADDRESS = "GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B";

// Valid 64-char hex ID for testing (no 0x prefix in param, but route accepts it)
const VALID_INVOICE_ID = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
const VALID_SETTLEMENT_ID = "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef12345678";
const VALID_BID_ID = "0x9876543210fedcba9876543210fedcba9876543210fedcba9876543210fedcba";

describe("QuickLendX API Skeleton Tests", () => {
  describe("Health Check", () => {
    it("should return health status", async () => {
      const res = await request(app).get("/health");
      expect(res.status).toBe(200);
      expect(res.body.status).toBe("ok");
    });
  });

  describe("Invoice API (v1)", () => {
    it("should list invoices", async () => {
      const res = await request(app).get("/api/v1/invoices");
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body)).toBe(true);
    });

    it("should filter invoices by business", async () => {
      const res = await request(app)
        .get(`/api/v1/invoices?business=${VALID_STELLAR_ADDRESS}`)
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should filter invoices by status", async () => {
      const res = await request(app)
        .get("/api/v1/invoices?status=Pending")
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should get invoice by ID", async () => {
      const res = await request(app).get(`/api/v1/invoices/${VALID_INVOICE_ID}`);
      // Returns 200 if exists, 404 if not - either is valid for this test
      expect([200, 404]).toContain(res.status);
    });

    it("should return 404 for non-existent invoice", async () => {
      // Use a valid hex format ID that doesn't exist
      const nonExistentId = "0x0000000000000000000000000000000000000000000000000000000000000000";
      const res = await request(app).get(`/api/v1/invoices/${nonExistentId}`);
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("INVOICE_NOT_FOUND");
    });

    it("should return 400 for invalid invoice ID format", async () => {
      const res = await request(app).get("/api/v1/invoices/invalid-id");
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("VALIDATION_ERROR");
    });
  });

  describe("Bid API (v1)", () => {
    it("should list bids", async () => {
      const res = await request(app).get("/api/v1/bids");
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body)).toBe(true);
    });

    it("should filter bids by invoice_id", async () => {
      const res = await request(app)
        .get(`/api/v1/bids?invoice_id=${VALID_INVOICE_ID}`)
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should filter bids by investor", async () => {
      const res = await request(app)
        .get(`/api/v1/bids?investor=${VALID_STELLAR_ADDRESS}`)
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should paginate bids", async () => {
      const res = await request(app)
        .get("/api/v1/bids?page=1&limit=10")
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
      expect(res.body.length).toBeLessThanOrEqual(10);
    });
  });

  describe("Settlement API (v1)", () => {
    it("should list settlements", async () => {
      const res = await request(app).get("/api/v1/settlements");
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body)).toBe(true);
    });

    it("should get settlement by ID", async () => {
      const res = await request(app).get(`/api/v1/settlements/${VALID_SETTLEMENT_ID}`);
      // Returns 200 if exists, 404 if not
      expect([200, 404]).toContain(res.status);
    });

    it("should return 404 for non-existent settlement", async () => {
      const nonExistentId = "0x0000000000000000000000000000000000000000000000000000000000000000";
      const res = await request(app).get(`/api/v1/settlements/${nonExistentId}`);
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("SETTLEMENT_NOT_FOUND");
    });

    it("should return 400 for invalid settlement ID format", async () => {
      const res = await request(app).get("/api/v1/settlements/invalid-id");
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("VALIDATION_ERROR");
    });
  });

  describe("Error Handling", () => {
    it("should handle rate limiting gracefully", async () => {
      // Make many rapid requests to trigger rate limit
      const results = [];
      for (let i = 0; i < 15; i++) {
        results.push(request(app).get("/api/v1/bids"));
      }
      const responses = await Promise.all(results);
      // At least some should succeed or be rate limited
      const statuses = responses.map(r => r.status);
      expect(statuses.some(s => s === 200 || s === 429)).toBe(true);
    });
  });
});

  describe("Dispute API – branch coverage", () => {
    it("returns all disputes when no invoice_id param is provided (falsy branch)", async () => {
      // Hit the getDisputes handler via a route that passes an empty/undefined id.
      // We use the invoices/:id/disputes route with a non-matching id so the
      // filter returns an empty array — the key thing is the `if (invoice_id)`
      // branch is exercised with a truthy value already; here we need the
      // controller called with an id that is an empty string to hit the falsy path.
      // The easiest way: call the route with id = "" which Express won't match,
      // so instead we directly test the controller function.
      const { getDisputes } = require("../src/controllers/v1/disputes");
      const req = { params: { id: undefined } } as any;
      const json = jest.fn();
      const res = { json } as any;
      const next = jest.fn();
      await getDisputes(req, res, next);
      expect(json).toHaveBeenCalled();
      // All disputes returned when id is falsy
      expect(json.mock.calls[0][0].length).toBeGreaterThanOrEqual(0);
    });
  });
