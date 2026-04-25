import request from "supertest";
import app from "../src/app";

describe("Request Limits Middleware", () => {
  describe("Body Size Limits", () => {
    it("should reject requests with body over 1MB with 413", async () => {
      const payload = { data: "x".repeat(2 * 1024 * 1024) };
      const res = await request(app).post("/api/v1/bids").send(payload);
      expect(res.status).toBe(413);
    });

    it("should accept requests with body under 1MB", async () => {
      const payload = { invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef" };
      const res = await request(app).post("/api/v1/bids").send(payload);
      expect([400, 404]).toContain(res.status);
    });
  });

  describe("Query Parameter Limits", () => {
    it("should accept query params under 2KB per param", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: "0x" + "a".repeat(100) })
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should reject query params over 2KB per param with 400", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: "x".repeat(3 * 1024) });
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("QUERY_PARAM_LIMIT_EXCEEDED");
    });

    it("should reject total query string over 8KB with 400", async () => {
      const params: Record<string, string> = {};
      for (let i = 0; i < 20; i++) {
        params[`p${i}`] = "x".repeat(500);
      }
      const res = await request(app).get("/api/v1/bids").query(params);
      expect(res.status).toBe(400);
    });
  });

  describe("Header Size Limits", () => {
    it("should accept headers under 16KB per key", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .set("X-Custom-Header", "x".repeat(1000))
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });
  });

  describe("Edge Cases", () => {
    it("should handle empty query gracefully", async () => {
      const res = await request(app).get("/api/v1/bids").expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should handle empty body gracefully", async () => {
      const res = await request(app).get("/api/v1/bids").send({}).expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should handle missing headers gracefully", async () => {
      const res = await request(app).get("/health").expect(200);
      expect(res.body.status).toBe("ok");
    });
  });
});
