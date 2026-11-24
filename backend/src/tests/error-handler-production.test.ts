/**
 * Production error-handler tests (issue #3216).
 *
 * Verifies:
 *  1. /api/v1/test-errors/* returns 404 when NODE_ENV === "production"
 *     (by mounting the test-error router conditionally in a minimal Express app).
 *  2. The error handler never leaks err.details in production responses.
 *  3. The /development route no longer mutates process.env.NODE_ENV.
 *  4. createErrorHandler({ exposeDetails }) correctly controls detail exposure.
 *  5. redactSensitiveFields regression guard.
 */

import express from "express";
import request from "supertest";
import {
  createErrorHandler,
  redactSensitiveFields,
  AppError,
} from "../middleware/error-handler";

// ---------------------------------------------------------------------------
// Minimal app factory — mirrors the production gate without loading real services
// ---------------------------------------------------------------------------

function buildMinimalApp(nodeEnv: string) {
  const app = express();
  app.use(express.json());

  // Mirror the gate from routes/v1/index.ts
  if (nodeEnv !== "production") {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const testErrorRoutes = require("../routes/v1/test-errors").default;
    app.use("/api/v1/test-errors", testErrorRoutes);
  }

  app.use(createErrorHandler({ exposeDetails: nodeEnv !== "production" }));
  return app;
}

// ---------------------------------------------------------------------------
// 1. Production: /test-errors/* must return 404
// ---------------------------------------------------------------------------

describe("production: /api/v1/test-errors/* is not mounted", () => {
  const app = buildMinimalApp("production");

  it("GET /api/v1/test-errors/500 returns 404", () =>
    request(app).get("/api/v1/test-errors/500").expect(404));

  it("GET /api/v1/test-errors/development returns 404", () =>
    request(app).get("/api/v1/test-errors/development").expect(404));

  it("GET /api/v1/test-errors/no-message returns 404", () =>
    request(app).get("/api/v1/test-errors/no-message").expect(404));

  it("GET /api/v1/test-errors/default-error returns 404", () =>
    request(app).get("/api/v1/test-errors/default-error").expect(404));
});

// ---------------------------------------------------------------------------
// 2. Production error handler: details never exposed
// ---------------------------------------------------------------------------

describe("production errorHandler: does not expose details", () => {
  it("omits err.details from the response body", async () => {
    const app = express();
    app.get("/boom", (_req, _res, next) => {
      const err: AppError = Object.assign(new Error("oops"), {
        status: 500,
        code: "TEST",
        details: { secret: "leaked" },
      });
      next(err);
    });
    app.use(createErrorHandler({ exposeDetails: false }));

    const res = await request(app).get("/boom").expect(500);
    expect(res.body.error.details).toBeUndefined();
    expect(JSON.stringify(res.body)).not.toContain("leaked");
  });
});

// ---------------------------------------------------------------------------
// 3. /development route no longer mutates process.env.NODE_ENV
// ---------------------------------------------------------------------------

describe("non-production: /development route does not mutate process.env.NODE_ENV", () => {
  const app = buildMinimalApp("development");

  it("NODE_ENV is unchanged after the request", async () => {
    const before = process.env.NODE_ENV;
    await request(app).get("/api/v1/test-errors/development");
    expect(process.env.NODE_ENV).toBe(before);
  });

  it("exposes details in development response", async () => {
    const res = await request(app).get("/api/v1/test-errors/development");
    expect(res.status).toBe(500);
    expect(res.body.error.details).toBeDefined();
    expect(res.body.error.details.foo).toBe("bar");
  });
});

// ---------------------------------------------------------------------------
// 4. createErrorHandler injectable exposeDetails option
// ---------------------------------------------------------------------------

describe("createErrorHandler: exposeDetails option", () => {
  function makeApp(exposeDetails: boolean) {
    const app = express();
    app.get("/err", (_req, _res, next) => {
      const err: AppError = Object.assign(new Error("msg"), {
        status: 400,
        code: "FAIL",
        details: { inner: "data" },
      });
      next(err);
    });
    app.use(createErrorHandler({ exposeDetails }));
    return app;
  }

  it("exposes details when exposeDetails: true", async () => {
    const res = await request(makeApp(true)).get("/err").expect(400);
    expect(res.body.error.details).toEqual({ inner: "data" });
  });

  it("hides details when exposeDetails: false", async () => {
    const res = await request(makeApp(false)).get("/err").expect(400);
    expect(res.body.error.details).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 5. redactSensitiveFields (regression guard)
// ---------------------------------------------------------------------------

describe("redactSensitiveFields", () => {
  it("redacts known sensitive keys", () => {
    const result = redactSensitiveFields({ password: "s3cr3t", name: "user" }) as any;
    expect(result.password).toBe("[REDACTED]");
    expect(result.name).toBe("user");
  });

  it("is case-insensitive on key names", () => {
    const result = redactSensitiveFields({ Password: "x", TOKEN: "y" }) as any;
    expect(result.Password).toBe("[REDACTED]");
    expect(result.TOKEN).toBe("[REDACTED]");
  });

  it("handles arrays", () => {
    const result = redactSensitiveFields([{ secret: "x" }, { safe: "y" }]) as any[];
    expect(result[0].secret).toBe("[REDACTED]");
    expect(result[1].safe).toBe("y");
  });

  it("handles null and primitives", () => {
    expect(redactSensitiveFields(null)).toBeNull();
    expect(redactSensitiveFields(42)).toBe(42);
  });
});
