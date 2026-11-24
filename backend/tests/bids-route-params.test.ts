import request from "supertest";
import express from "express";
import bidsRouter from "../src/routes/v1/bids";

describe("Bids Route Params Validation", () => {
  let app: express.Application;

  beforeAll(() => {
    app = express();
    app.use(express.json());
    app.use("/api/v1/bids", bidsRouter);
  });

  describe("GET /api/v1/bids/:invoiceId/best", () => {
    it("should accept valid hex invoice ID", async () => {
      const response = await request(app).get(
        "/api/v1/bids/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef/best"
      );
      // The controller will likely return 404 for non-existent invoice, but validation should pass
      // We expect it to NOT return 400 validation error
      expect(response.status).not.toBe(400);
    });

    it("should accept plain string invoice ID (for mocks/tests)", async () => {
      const response = await request(app).get("/api/v1/bids/test-invoice-id/best");
      expect(response.status).not.toBe(400);
    });

    it("should reject empty string invoice ID", async () => {
      const response = await request(app).get("/api/v1/bids//best");
      expect(response.status).toBe(400);
      expect(response.body.error.code).toBe("VALIDATION_ERROR");
    });

    it("should reject injection string invoice ID", async () => {
      const response = await request(app).get("/api/v1/bids/<script>alert('xss')</script>/best");
      expect(response.status).toBe(400);
      expect(response.body.error.code).toBe("VALIDATION_ERROR");
    });

    it("should reject SQL injection string", async () => {
      const response = await request(app).get("/api/v1/bids/' OR '1'='1/best");
      expect(response.status).toBe(400);
      expect(response.body.error.code).toBe("VALIDATION_ERROR");
    });
  });

  describe("GET /api/v1/bids/:invoiceId/ranked", () => {
    it("should accept valid hex invoice ID", async () => {
      const response = await request(app).get(
        "/api/v1/bids/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef/ranked"
      );
      expect(response.status).not.toBe(400);
    });

    it("should accept plain string invoice ID (for mocks/tests)", async () => {
      const response = await request(app).get("/api/v1/bids/test-invoice-id/ranked");
      expect(response.status).not.toBe(400);
    });

    it("should reject empty string invoice ID", async () => {
      const response = await request(app).get("/api/v1/bids//ranked");
      expect(response.status).toBe(400);
      expect(response.body.error.code).toBe("VALIDATION_ERROR");
    });

    it("should reject injection string invoice ID", async () => {
      const response = await request(app).get("/api/v1/bids/<script>alert('xss')</script>/ranked");
      expect(response.status).toBe(400);
      expect(response.body.error.code).toBe("VALIDATION_ERROR");
    });

    it("should reject SQL injection string", async () => {
      const response = await request(app).get("/api/v1/bids/' OR '1'='1/ranked");
      expect(response.status).toBe(400);
      expect(response.body.error.code).toBe("VALIDATION_ERROR");
    });
  });
});
