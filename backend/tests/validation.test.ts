import request from "supertest";
import app from "../src/app";
import {
  sanitizeInput,
  formatZodError,
  createValidationMiddleware,
  createBodyValidationMiddleware,
  createParamsValidationMiddleware,
} from "../src/middleware/validation";
import { z } from "zod";

const VALID_STELLAR = "GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B";
const VALID_HEX = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

describe("Validation Middleware", () => {
  describe("sanitizeInput", () => {
    it("should remove XSS characters", () => {
      expect(sanitizeInput("<script>alert('xss')</script>")).toBe("scriptalert(xss)/script");
      expect(sanitizeInput("onload=alert(1)")).toBe("alert(1)");
      expect(sanitizeInput("javascript:alert(1)")).toBe("alert(1)");
      expect(sanitizeInput("onerror=alert(1)")).toBe("alert(1)");
    });

    it("should trim whitespace", () => {
      expect(sanitizeInput("  hello world  ")).toBe("hello world");
    });

    it("should preserve normal input", () => {
      expect(sanitizeInput("0x1234567890abcdef")).toBe("0x1234567890abcdef");
    });

    it("should handle empty string", () => {
      expect(sanitizeInput("")).toBe("");
    });

    it("should remove onclick attribute syntax", () => {
      expect(sanitizeInput("onclick=alert(1)")).toBe("alert(1)");
    });
  });

  describe("formatZodError", () => {
    it("should format Zod errors correctly", () => {
      const schema = z.object({ name: z.string().min(2), email: z.string().email() });
      try {
        schema.parse({ name: "x", email: "invalid" });
      } catch (error) {
        if (error instanceof z.ZodError) {
          const formatted = formatZodError(error);
          expect(formatted.message).toBe("Validation failed");
          expect(formatted.code).toBe("VALIDATION_ERROR");
          expect(formatted.details).toBeInstanceOf(Array);
          expect((formatted.details as Array<unknown>).length).toBeGreaterThan(0);
        }
      }
    });

    it("should format error without details when includeDetails is false", () => {
      const schema = z.object({ name: z.string().min(2) });
      try {
        schema.parse({ name: "x" });
      } catch (error) {
        if (error instanceof z.ZodError) {
          const formatted = formatZodError(error, false);
          expect(formatted.message).toBe("Validation failed");
          expect(formatted.code).toBe("VALIDATION_ERROR");
          expect(formatted.details).toBeUndefined();
        }
      }
    });
  });

  describe("createValidationMiddleware", () => {
    it("should validate body schema", async () => {
      const schema = z.object({ amount: z.string() });
      const middleware = createValidationMiddleware({ body: schema });
      const mockReq = { body: { amount: "100" } } as any;
      const mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() } as any;
      const mockNext = jest.fn();
      middleware(mockReq, mockRes, mockNext);
      expect(mockNext).toHaveBeenCalled();
    });

    it("should reject invalid body schema", async () => {
      const schema = z.object({ amount: z.string() });
      const middleware = createValidationMiddleware({ body: schema });
      const mockReq = { body: { amount: 123 } } as any;
      const mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() } as any;
      const mockNext = jest.fn();
      middleware(mockReq, mockRes, mockNext);
      expect(mockRes.status).toHaveBeenCalledWith(400);
      expect(mockNext).not.toHaveBeenCalled();
    });

    it("should validate params schema", async () => {
      const schema = z.object({ id: z.string() });
      const middleware = createValidationMiddleware({ params: schema });
      const mockReq = { params: { id: "valid" } } as any;
      const mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() } as any;
      const mockNext = jest.fn();
      middleware(mockReq, mockRes, mockNext);
      expect(mockNext).toHaveBeenCalled();
    });
  });

  describe("createBodyValidationMiddleware", () => {
    it("should validate body and call next", () => {
      const schema = z.object({ name: z.string() });
      const middleware = createBodyValidationMiddleware(schema);
      const mockReq = { body: { name: "test" } } as any;
      const mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() } as any;
      const mockNext = jest.fn();
      middleware(mockReq, mockRes, mockNext);
      expect(mockNext).toHaveBeenCalled();
    });

    it("should reject invalid body", () => {
      const schema = z.object({ name: z.string() });
      const middleware = createBodyValidationMiddleware(schema);
      const mockReq = { body: { name: 123 } } as any;
      const mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() } as any;
      const mockNext = jest.fn();
      middleware(mockReq, mockRes, mockNext);
      expect(mockRes.status).toHaveBeenCalledWith(400);
    });
  });

  describe("createParamsValidationMiddleware", () => {
    it("should validate params and call next", () => {
      const schema = z.object({ id: z.string().min(1) });
      const middleware = createParamsValidationMiddleware(schema);
      const mockReq = { params: { id: "valid" } } as any;
      const mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() } as any;
      const mockNext = jest.fn();
      middleware(mockReq, mockRes, mockNext);
      expect(mockNext).toHaveBeenCalled();
    });

    it("should reject invalid params with empty string", () => {
      const schema = z.object({ id: z.string().min(1) });
      const middleware = createParamsValidationMiddleware(schema);
      const mockReq = { params: { id: "" } } as any;
      const mockRes = { status: jest.fn().mockReturnThis(), json: jest.fn() } as any;
      const mockNext = jest.fn();
      middleware(mockReq, mockRes, mockNext);
      expect(mockRes.status).toHaveBeenCalledWith(400);
    });
  });

  describe("Query Validation Middleware", () => {
    it("should validate valid query params", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: VALID_HEX, investor: VALID_STELLAR })
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should reject invalid hex string in query", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: "not-a-hex-string" });
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("VALIDATION_ERROR");
    });

    it("should reject invalid Stellar address in query", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ investor: "INVALID_ADDRESS" });
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("VALIDATION_ERROR");
    });

    it("should sanitize query params before validation", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: "0x<script>alert(1)</script>" });
      expect(res.status).toBe(400);
    });

    it("should handle pagination params", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ page: "2", limit: "50" })
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should coerce pagination types", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ page: "1", limit: "20" })
        .expect(200);
      expect(res.body).toBeInstanceOf(Array);
    });

    it("should reject negative page number", async () => {
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ page: "-1" });
      expect(res.status).toBe(400);
    });
  });

  describe("Param Validation Middleware", () => {
    it("should validate valid invoice id param", async () => {
      const res = await request(app)
        .get("/api/v1/invoices/" + VALID_HEX)
        .expect(200);
      expect(res.body).toBeDefined();
    });

    it("should reject invalid invoice id param with 400", async () => {
      const res = await request(app)
        .get("/api/v1/invoices/invalid-id")
        .expect(400);
      expect(res.body.error.code).toBe("VALIDATION_ERROR");
    });

    it("should sanitize XSS characters from valid-format param", async () => {
      expect(sanitizeInput("0x<script>alert(1)</script>")).toBe("0xscriptalert(1)/script");
    });
  });

  describe("Consistent Error Schema", () => {
    it("should return consistent error format for validation errors", async () => {
      const res = await request(app)
        .get("/api/v1/invoices/invalid-id")
        .expect(400);
      expect(res.body).toHaveProperty("error");
      expect(res.body.error).toHaveProperty("message");
      expect(res.body.error).toHaveProperty("code");
      expect(res.body.error.code).toBe("VALIDATION_ERROR");
    });

    it("should include details in development mode", async () => {
      const originalEnv = process.env.NODE_ENV;
      process.env.NODE_ENV = "development";
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: "invalid" })
        .expect(400);
      expect(res.body.error.details).toBeDefined();
      expect(Array.isArray(res.body.error.details)).toBe(true);
      process.env.NODE_ENV = originalEnv;
    });

    it("should hide details in production mode", async () => {
      const originalEnv = process.env.NODE_ENV;
      process.env.NODE_ENV = "production";
      const res = await request(app)
        .get("/api/v1/bids")
        .query({ invoice_id: "invalid" })
        .expect(400);
      expect(res.body.error.details).toBeUndefined();
      process.env.NODE_ENV = originalEnv;
    });
  });
});
