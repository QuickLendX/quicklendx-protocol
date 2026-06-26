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

import supertest from "supertest";
import app from "../app";
import { statusService } from "../services/statusService";

describe("Event Ingest Limits Middleware Tests", () => {
  beforeEach(() => {
    // Reset status service mock ledger to avoid degraded mode
    statusService.setMockCurrentLedger(100000);
    statusService.updateLastIndexedLedger(100000);
  });

  afterEach(() => {
    statusService.setMockCurrentLedger(null);
  });

  describe("Content-Type Validation", () => {
    it("rejects requests with missing Content-Type header", async () => {
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("Content-Length", "20")
        .send('{"test":"data"}');

      expect(res.status).toBe(415);
      expect(res.body.error.code).toBe("INVALID_CONTENT_TYPE");
    });

    it("rejects requests with non-JSON Content-Type", async () => {
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("Content-Type", "text/plain")
        .set("Content-Length", "20")
        .send("plain text");

      expect(res.status).toBe(415);
      expect(res.body.error.code).toBe("INVALID_CONTENT_TYPE");
    });

    it("accepts requests with application/json Content-Type", async () => {
      // This will fail later validation but should pass middleware
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("Content-Type", "application/json")
        .set("Content-Length", "20")
        .send('{"test":"data"}');

      expect(res.status).not.toBe(415);
    });
  });

  describe("Content-Length Validation", () => {
    it("rejects requests with missing Content-Length header", async () => {
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("Content-Type", "application/json")
        .send('{"test":"data"}');

      expect(res.status).toBe(411);
      expect(res.body.error.code).toBe("CONTENT_LENGTH_REQUIRED");
    });

    it("rejects requests with Content-Length exceeding 256KB", async () => {
      const largeBody = "x".repeat(256 * 1024 + 1);
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("Content-Type", "application/json")
        .set("Content-Length", String(largeBody.length))
        .send(largeBody);

      expect(res.status).toBe(413);
      expect(res.body.error.code).toBe("BODY_LIMIT_EXCEEDED");
    });

    it("accepts requests with Content-Length within limit", async () => {
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("Content-Type", "application/json")
        .set("Content-Length", "20")
        .send('{"test":"data"}');

      expect(res.status).not.toBe(411);
      expect(res.status).not.toBe(413);
    });
  });

  describe("Transfer-Encoding Validation", () => {
    it("rejects requests with chunked Transfer-Encoding without allowlist header", async () => {
      // Note: supertest doesn't easily send chunked encoding, so we'll test via direct middleware call
      const { eventIngestLimitsMiddleware } = require("../middleware/event-ingest-limits");
      const mockReq = {
        headers: {
          "content-type": "application/json",
          "content-length": "20",
          "transfer-encoding": "chunked",
        },
      };
      let mockRes = {
        status: jest.fn().mockReturnThis(),
        json: jest.fn(),
      };
      let nextCalled = false;
      const mockNext = () => { nextCalled = true; };

      eventIngestLimitsMiddleware(mockReq as any, mockRes as any, mockNext);

      expect(mockRes.status).toHaveBeenCalledWith(400);
      expect(mockRes.json).toHaveBeenCalledWith(expect.objectContaining({
        error: expect.objectContaining({
          code: "CHUNKED_ENCODING_NOT_ALLOWED",
        }),
      }));
      expect(nextCalled).toBe(false);
    });

    it("accepts requests with chunked Transfer-Encoding when allowlist header is present", async () => {
      const { eventIngestLimitsMiddleware } = require("../middleware/event-ingest-limits");
      const mockReq = {
        headers: {
          "content-type": "application/json",
          "content-length": "20",
          "transfer-encoding": "chunked",
          "x-allow-chunked-encoding": "true",
        },
      };
      let mockRes = {
        status: jest.fn().mockReturnThis(),
        json: jest.fn(),
      };
      let nextCalled = false;
      const mockNext = () => { nextCalled = true; };

      eventIngestLimitsMiddleware(mockReq as any, mockRes as any, mockNext);

      expect(mockRes.status).not.toHaveBeenCalled();
      expect(nextCalled).toBe(true);
    });
  });
});
