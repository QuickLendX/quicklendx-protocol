import { describe, expect, it, beforeEach } from "@jest/globals";
import request from "supertest";
import app from "../src/app";
import { rateLimiter } from "../src/middleware/rate-limit";

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
      expect(Array.isArray(res.body)).toBe(true);
      expect(res.body.length).toBeGreaterThan(0);
      expect(res.body[0]).toHaveProperty("id");
      expect(res.body[0]).toHaveProperty("status");
    });

    it("should filter invoices by business", async () => {
      const business = "GDVLRH4G4...7Y";
      const res = await request(app).get(`/api/v1/invoices?business=${business}`);
      expect(res.status).toBe(200);
      expect(res.body.every((i: any) => i.business === business)).toBe(true);
    });

    it("should filter invoices by status", async () => {
      const status = "Verified";
      const res = await request(app).get(`/api/v1/invoices?status=${status}`);
      expect(res.status).toBe(200);
      expect(res.body.every((i: any) => i.status === status)).toBe(true);
    });

    it("should get invoice by ID", async () => {
      const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await request(app).get(`/api/v1/invoices/${id}`);
      expect(res.status).toBe(200);
      expect(res.body.id).toBe(id);
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
      expect(Array.isArray(res.body)).toBe(true);
      expect(res.body.length).toBeGreaterThan(0);
    });

    it("should filter bids by invoice_id", async () => {
      const invoice_id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
      const res = await request(app).get(`/api/v1/bids?invoice_id=${invoice_id}`);
      expect(res.status).toBe(200);
      expect(res.body.every((b: any) => b.invoice_id === invoice_id)).toBe(true);
    });

    it("should filter bids by investor", async () => {
      const investor = "GA...ABC";
      const res = await request(app).get(`/api/v1/bids?investor=${investor}`);
      expect(res.status).toBe(200);
      expect(res.body.every((b: any) => b.investor === investor)).toBe(true);
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

    it("should handle unknown IP in rate limiter", async () => {
      const res = await request(app)
        .get("/health")
        .set("X-Simulate-No-IP", "true");
      expect(res.status).toBe(200);
    });
  });
});
