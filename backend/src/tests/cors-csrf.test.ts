// Virtually mock the 'pg' module to prevent errors on environments where postgres is not installed
jest.mock("pg", () => {
  const mClient = {
    query: jest.fn().mockResolvedValue({ rows: [] }),
    release: jest.fn(),
  };
  const mPool = {
    connect: jest.fn().mockResolvedValue(mClient),
    query: jest.fn().mockResolvedValue({ rows: [] }),
    on: jest.fn(),
    end: jest.fn(),
  };
  return {
    Pool: jest.fn(() => mPool),
  };
}, { virtual: true });

// Set ALLOWED_ORIGINS before importing app to populate config
process.env.ALLOWED_ORIGINS = "https://trusted.app.com,https://another.trusted.com";

import supertest from "supertest";
import app from "../app";
import { statusService } from "../services/statusService";

describe("CORS and CSRF Hardening Integration Tests", () => {
  beforeEach(() => {
    // Reset status service mock ledger to avoid degraded mode (which triggers 503)
    statusService.setMockCurrentLedger(100000);
    statusService.updateLastIndexedLedger(100000);
  });

  afterEach(() => {
    statusService.setMockCurrentLedger(null);
  });

  describe("CORS Invariants", () => {
    it("reflects allowed browser origin in Access-Control-Allow-Origin header", async () => {
      const res = await supertest(app)
        .get("/health")
        .set("Origin", "https://trusted.app.com");

      expect(res.headers["access-control-allow-origin"]).toBe("https://trusted.app.com");
      expect(res.headers["access-control-allow-credentials"]).toBe("true");
    });

    it("does not reflect untrusted browser origin in Access-Control-Allow-Origin header", async () => {
      const res = await supertest(app)
        .get("/health")
        .set("Origin", "https://untrusted.com");

      expect(res.headers["access-control-allow-origin"]).toBeUndefined();
      // Ensure credentials are not reflected to arbitrary origins
      expect(res.headers["access-control-allow-credentials"]).toBeUndefined();
    });

    it("succeeds preflight OPTIONS requests for trusted origin", async () => {
      const res = await supertest(app)
        .options("/api/v1/write-action")
        .set("Origin", "https://trusted.app.com")
        .set("Access-Control-Request-Method", "POST")
        .set("Access-Control-Request-Headers", "Content-Type, X-CSRF-Token");

      expect(res.status).toBe(204);
      expect(res.headers["access-control-allow-origin"]).toBe("https://trusted.app.com");
      expect(res.headers["access-control-allow-headers"]).toContain("X-CSRF-Token");
    });

    it("does not set CORS headers on preflight OPTIONS requests for untrusted origin", async () => {
      const res = await supertest(app)
        .options("/api/v1/write-action")
        .set("Origin", "https://untrusted.com")
        .set("Access-Control-Request-Method", "POST");

      expect(res.headers["access-control-allow-origin"]).toBeUndefined();
    });

    it("uses webhook CORS options (wildcard origin, no credentials) for webhook endpoints", async () => {
      const res = await supertest(app)
        .options("/api/webhooks/callbacks")
        .set("Origin", "https://any-origin.com")
        .set("Access-Control-Request-Method", "POST");

      expect(res.status).toBe(204);
      expect(res.headers["access-control-allow-origin"]).toBe("*");
      expect(res.headers["access-control-allow-credentials"]).toBeUndefined();
    });

    it("handles empty ALLOWED_ORIGINS config gracefully", () => {
      jest.isolateModules(() => {
        const originalEnv = process.env.ALLOWED_ORIGINS;
        delete process.env.ALLOWED_ORIGINS;
        const { allowedBrowserOrigins } = require("../config/cors");
        expect(allowedBrowserOrigins).toEqual([]);
        process.env.ALLOWED_ORIGINS = originalEnv;
      });
    });
  });

  describe("CSRF Protection", () => {
    it("allows GET requests to browser routes without CSRF headers", async () => {
      const res = await supertest(app).get("/health");
      expect(res.status).toBe(200);
    });

    it("rejects browser POST request if X-CSRF-Token header is missing", async () => {
      const res = await supertest(app)
        .post("/api/v1/write-action")
        .set("Origin", "https://trusted.app.com")
        .set("Content-Type", "application/json")
        .send({ data: "test" });

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe("MISSING_CSRF_TOKEN");
      expect(res.body.error.message).toContain("Missing CSRF token");
    });

    it("rejects browser POST request if Origin is disallowed", async () => {
      const res = await supertest(app)
        .post("/api/v1/write-action")
        .set("Origin", "https://untrusted.com")
        .set("X-CSRF-Token", "valid-csrf-token")
        .set("Content-Type", "application/json")
        .send({ data: "test" });

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe("ORIGIN_NOT_ALLOWED");
      expect(res.body.error.message).toContain("origin is not allowed");
    });

    it("rejects browser POST request if content-type is not JSON", async () => {
      const res = await supertest(app)
        .post("/api/v1/write-action")
        .set("Origin", "https://trusted.app.com")
        .set("X-CSRF-Token", "valid-csrf-token")
        .set("Content-Type", "text/plain")
        .send("plain text data");

      expect(res.status).toBe(415);
      expect(res.body.error.code).toBe("INVALID_CONTENT_TYPE");
    });

    it("rejects browser POST request if Content-Type header is missing", async () => {
      const res = await supertest(app)
        .post("/api/v1/write-action")
        .set("Origin", "https://trusted.app.com")
        .set("X-CSRF-Token", "valid-csrf-token")
        .unset("Content-Type")
        .send('{"data":"test"}');

      expect(res.status).toBe(415);
      expect(res.body.error.code).toBe("INVALID_CONTENT_TYPE");
    });

    it("accepts browser POST request when CSRF token, Origin, and JSON content-type are correct", async () => {
      const res = await supertest(app)
        .post("/api/v1/write-action")
        .set("Origin", "https://trusted.app.com")
        .set("X-CSRF-Token", "valid-csrf-token")
        .set("Content-Type", "application/json")
        .send({ data: "test" });

      expect(res.status).toBe(201);
      expect(res.body.success).toBe(true);
    });

    it("accepts browser POST request without Origin (same-origin/client) if CSRF token is present", async () => {
      const res = await supertest(app)
        .post("/api/v1/write-action")
        .set("X-CSRF-Token", "valid-csrf-token")
        .set("Content-Type", "application/json")
        .send({ data: "test" });

      expect(res.status).toBe(201);
      expect(res.body.success).toBe(true);
    });
  });

  describe("CSRF Exemptions (Webhooks and API Keys)", () => {
    it("exempts webhook callbacks route from CSRF checks", async () => {
      // POST to /api/webhooks/callbacks should succeed without CSRF headers
      const res = await supertest(app)
        .post("/api/webhooks/callbacks")
        .set("Origin", "https://some-origin.com")
        .send({});

      expect(res.status).toBe(202);
      expect(res.body.accepted).toBe(true);
    });

    it("exempts webhook ingest route from CSRF checks", async () => {
      // POST to /api/v1/webhooks/ingest/sub_123 should bypass CSRF
      // It will fail at signature validation (400 or 401) rather than CSRF (403/415)
      const res = await supertest(app)
        .post("/api/v1/webhooks/ingest/sub_123")
        .set("Origin", "https://some-origin.com")
        .set("Content-Type", "application/json")
        .send({ event: "dummy" });

      // Missing webhook signature header -> 400 Bad Request
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("MISSING_SUBSCRIBER_HEADER");
    });

    it("exempts request using X-API-Key from CSRF checks", async () => {
      const res = await supertest(app)
        .post("/api/v1/write-action")
        .set("X-API-Key", "qlx_live_somekey")
        .set("Content-Type", "application/json")
        .send({ data: "test" });

      expect(res.status).toBe(201);
      expect(res.body.success).toBe(true);
    });

    it("exempts request using Authorization Bearer API key from CSRF checks", async () => {
      const res = await supertest(app)
        .post("/api/v1/write-action")
        .set("Authorization", "Bearer qlx_live_somekey")
        .set("Content-Type", "application/json")
        .send({ data: "test" });

      expect(res.status).toBe(201);
      expect(res.body.success).toBe(true);
    });
  });
});
