import express from "express";
import supertest from "supertest";
import { rateLimitMiddleware, rateLimiter, strictRateLimitMiddleware, strictRateLimiter, createRateLimitMiddleware } from "../middleware/rate-limit";
import { RateLimiterRes, RateLimiterMemory } from "rate-limiter-flexible";

describe("rate-limit configuration", () => {
  const originalNodeEnv = process.env.NODE_ENV;

  beforeEach(() => {
    // Reset points before each test
    rateLimiter.delete("test-ip");
    rateLimiter.delete("127.0.0.1");
    rateLimiter.delete("unknown");
    strictRateLimiter.delete("127.0.0.1");
  });

  afterEach(() => {
    jest.resetModules();
    process.env.NODE_ENV = originalNodeEnv;
  });

  it("uses test-specific point budget in test env", async () => {
    process.env.NODE_ENV = "test";
    const { rateLimiter: importedLimiter } = await import("../middleware/rate-limit");
    expect((importedLimiter as any)._points).toBe(1000);
  });

  it("uses production/default point budget outside test env", async () => {
    process.env.NODE_ENV = "production";
    const { rateLimiter: importedLimiter } = await import("../middleware/rate-limit");
    expect((importedLimiter as any)._points).toBe(100);
  });
});

describe("rateLimitMiddleware integration", () => {
  let app: express.Express;

  beforeEach(() => {
    app = express();
    // Simulate IP
    app.use((req, res, next) => {
      Object.defineProperty(req, "ip", { value: "127.0.0.1", configurable: true });
      next();
    });
    app.use(rateLimitMiddleware);
    app.get("/test", (req, res) => res.json({ success: true }));
    
    // Reset limiter for the IP
    rateLimiter.delete("127.0.0.1");
  });

  it("adds rate limit headers to successful requests", async () => {
    const response = await supertest(app).get("/test");
    
    expect(response.status).toBe(200);
    expect(response.headers).toHaveProperty("x-ratelimit-limit");
    expect(response.headers).toHaveProperty("x-ratelimit-remaining");
    expect(response.headers).toHaveProperty("x-ratelimit-reset");
    expect(Number(response.headers["x-ratelimit-remaining"])).toBeLessThan(1000);
  });

  it("returns 429 when rate limit is exceeded", async () => {
    // Consume all points
    const limit = (rateLimiter as any)._points;
    
    // Create a proper RateLimiterRes instance
    const rejRes = new RateLimiterRes();
    (rejRes as any).msBeforeNext = 1500;
    (rejRes as any).remainingPoints = 0;
    (rejRes as any).consumedPoints = limit + 1;
    (rejRes as any).isFirstInDuration = false;

    const consumeSpy = jest.spyOn(rateLimiter, "consume");
    consumeSpy.mockRejectedValueOnce(rejRes);

    const response = await supertest(app).get("/test");
    
    expect(response.status).toBe(429);
    expect(response.headers["retry-after"]).toBe("2"); // 1500ms -> 2s
    expect(response.body.error.code).toBe("RATE_LIMIT_EXCEEDED");
    expect(response.body.error.retryAfter).toBe(2);
    
    consumeSpy.mockRestore();
  });

  it("handles requests without an IP", async () => {
    const noIpApp = express();
    noIpApp.use((req, res, next) => {
      Object.defineProperty(req, "ip", { value: undefined });
      next();
    });
    noIpApp.use(rateLimitMiddleware);
    noIpApp.get("/test", (req, res) => res.json({ success: true }));

    const response = await supertest(noIpApp).get("/test");
    expect(response.status).toBe(200);
  });

  it("returns 500 when rate limiter throws an unexpected error", async () => {
    const consumeSpy = jest.spyOn(rateLimiter, "consume");
    consumeSpy.mockRejectedValueOnce(new Error("Unexpected error"));

    const response = await supertest(app).get("/test");
    
    expect(response.status).toBe(500);
    expect(response.body.error.code).toBe("RATE_LIMIT_ERROR");
    
    consumeSpy.mockRestore();
  });

  it("applies strict rate limits to sensitive routes", async () => {
    const strictApp = express();
    strictApp.use(strictRateLimitMiddleware);
    strictApp.get("/sensitive", (req, res) => res.json({ success: true }));

    // Mock strict limiter rejection
    const rejRes = new RateLimiterRes();
    (rejRes as any).msBeforeNext = 300000;
    
    const consumeSpy = jest.spyOn(strictRateLimiter, "consume");
    consumeSpy.mockRejectedValueOnce(rejRes);

    const response = await supertest(strictApp).get("/sensitive");
    expect(response.status).toBe(429);
    expect(response.body.error.code).toBe("STRICT_RATE_LIMIT_EXCEEDED");
    
    consumeSpy.mockRestore();
  });

  it("allows requests through custom limiter when points are available", async () => {
    const customLimiter = new RateLimiterMemory({ points: 10, duration: 60 });
    const customMiddleware = createRateLimitMiddleware(customLimiter);
    const customApp = express();
    customApp.use(customMiddleware);
    customApp.get("/custom", (req, res) => res.json({ success: true }));

    const response = await supertest(customApp).get("/custom");
    expect(response.status).toBe(200);
    expect(response.headers).toHaveProperty("x-ratelimit-limit");
  });

  it("calls next() in custom limiter when an unexpected error occurs", async () => {
    const customLimiter = new RateLimiterMemory({ points: 10, duration: 60 });
    const customMiddleware = createRateLimitMiddleware(customLimiter);
    const customApp = express();
    customApp.use(customMiddleware);
    customApp.get("/custom", (req, res) => res.json({ success: true }));

    const consumeSpy = jest.spyOn(customLimiter, "consume");
    consumeSpy.mockRejectedValueOnce(new Error("Unexpected"));

    const response = await supertest(customApp).get("/custom");
    expect(response.status).toBe(200); // Should fall back to next()
    
    consumeSpy.mockRestore();
  });

  it("uses X-Forwarded-For and unknown fallback in custom limiter", async () => {
    const customLimiter = new RateLimiterMemory({ points: 10, duration: 60 });
    const customMiddleware = createRateLimitMiddleware(customLimiter);
    const app = express();
    app.use((req, res, next) => {
      Object.defineProperty(req, "ip", { value: undefined, configurable: true });
      next();
    });
    app.use(customMiddleware);
    app.get("/test", (req, res) => res.json({ success: true }));

    // Test XFF
    const res1 = await supertest(app).get("/test").set("X-Forwarded-For", "1.1.1.1");
    expect(res1.status).toBe(200);

    // Test fallback to unknown
    const res2 = await supertest(app).get("/test");
    expect(res2.status).toBe(200);
  });
});
