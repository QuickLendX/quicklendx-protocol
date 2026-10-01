/**
 * Access Logging Middleware Tests — Deterministic Failure-Boundary Coverage
 *
 * Issue #2719: Add deterministic failure-boundary coverage for
 * accessLogMiddleware in ./backend/src/middleware/access-log.ts
 *
 * Test strategy:
 *  – Unit tests operate on exported helpers and the in-memory ring-buffer
 *    directly via logAccess / getAccessLogs / clearAccessLogs.
 *  – Middleware integration tests build minimal Express-style mock objects
 *    (no real HTTP server required) so every scenario is synchronous,
 *    deterministic, and side-effect-free.
 *  – Failure-boundary tests prove that misbehaving sub-systems (console,
 *    logAccess, identifySensitiveFields) cannot crash res.json or lose an
 *    already-stored log entry.
 *  – All tests reset shared state via clearAccessLogs() in beforeEach.
 */

import { describe, it, expect, beforeEach, afterEach, jest } from "@jest/globals";
import { Request, Response, NextFunction } from "express";
import {
  logAccess,
  getAccessLogs,
  getRedactedAccessLogs,
  clearAccessLogs,
  getAccessLogStats,
  accessLogMiddleware,
  kycAccessLogMiddleware,
  AccessLogEntry,
} from "../middleware/access-log";
import { hashForLog } from "../services/kycService";
import * as requestContext from "../lib/requestContext";

// ---------------------------------------------------------------------------
// Minimal mock builders — no node-mocks-http dependency
// ---------------------------------------------------------------------------

function buildReq(overrides: Partial<Request> = {}): Request {
  return {
    headers: {},
    query: {},
    body: {},
    params: {},
    ip: undefined,
    ...overrides,
  } as unknown as Request;
}

function buildRes(statusCode = 200): Response & {
  _lastBody: any;
  _jsonCallCount: number;
} {
  const res: any = {
    statusCode,
    _lastBody: undefined,
    _jsonCallCount: 0,
    json(body: any) {
      res._lastBody = body;
      res._jsonCallCount += 1;
      return res;
    },
    bind(ctx: any) {
      // Replicate res.json.bind(res) so the middleware wraps it correctly.
      return res.json.bind(res);
    },
  };
  return res;
}

// Run a middleware and immediately invoke the overridden res.json.
function runMiddleware(
  middleware: ReturnType<typeof accessLogMiddleware>,
  req: Request,
  res: Response,
  responseBody: any = { ok: true }
): void {
  const next = jest.fn() as unknown as NextFunction;
  middleware(req, res, next);
  expect(next).toHaveBeenCalled();
  (res as any).json(responseBody);
}

// ---------------------------------------------------------------------------
// 1. Basic Logging — preserved from original tests
// ---------------------------------------------------------------------------

describe("Basic Logging", () => {
  beforeEach(() => clearAccessLogs());

  it("logs an access event and stores it in the ring-buffer", () => {
    logAccess({
      action: "read",
      resource: "kyc",
      userId: "user_123",
      ipAddress: "192.168.1.1",
      fields: ["tax_id", "customer_name"],
      sensitiveFields: ["tax_id"],
      piiFields: ["customer_name"],
      status: "success",
    });

    const logs = getAccessLogs();
    expect(logs).toHaveLength(1);
    expect(logs[0].action).toBe("read");
    expect(logs[0].resource).toBe("kyc");
  });

  it("stores the resourceId when provided", () => {
    logAccess({
      action: "read",
      resource: "invoice",
      resourceId: "inv_456",
      userId: "user_123",
      fields: [],
      sensitiveFields: [],
      piiFields: [],
      status: "success",
    });

    expect(getAccessLogs()[0].resourceId).toBe("inv_456");
  });

  it("stores failure status and error message", () => {
    logAccess({
      action: "read",
      resource: "kyc",
      userId: "user_123",
      fields: [],
      sensitiveFields: [],
      piiFields: [],
      status: "failure",
      error: "HTTP 403",
    });

    const log = getAccessLogs()[0];
    expect(log.status).toBe("failure");
    expect(log.error).toBe("HTTP 403");
  });

  it("always generates a valid ISO timestamp", () => {
    const before = Date.now();
    logAccess({
      action: "write",
      resource: "kyc",
      fields: [],
      sensitiveFields: [],
      piiFields: [],
      status: "success",
    });
    const after = Date.now();

    const ts = new Date(getAccessLogs()[0].timestamp).getTime();
    expect(ts).toBeGreaterThanOrEqual(before);
    expect(ts).toBeLessThanOrEqual(after);
  });
});

// ---------------------------------------------------------------------------
// 2. Log Filtering
// ---------------------------------------------------------------------------

describe("Log Filtering", () => {
  beforeEach(() => {
    clearAccessLogs();
    logAccess({ action: "read",   resource: "kyc",        userId: "user_1", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "write",  resource: "kyc",        userId: "user_1", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "read",   resource: "invoice",    userId: "user_2", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "update", resource: "kyc",        userId: "user_1", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "delete", resource: "settlement", userId: "user_3", fields: [], sensitiveFields: [], piiFields: [], status: "failure" });
  });

  it("filters by userId", () => {
    const logs = getAccessLogs({ userId: "user_1" });
    expect(logs).toHaveLength(3);
    logs.forEach(l => expect(l.userId).toBe("user_1"));
  });

  it("filters by resource", () => {
    const logs = getAccessLogs({ resource: "kyc" });
    expect(logs).toHaveLength(3);
    logs.forEach(l => expect(l.resource).toBe("kyc"));
  });

  it("filters by action", () => {
    const logs = getAccessLogs({ action: "read" });
    expect(logs).toHaveLength(2);
    logs.forEach(l => expect(l.action).toBe("read"));
  });

  it("returns all logs within a wide date range", () => {
    const startDate = new Date(Date.now() - 5_000);
    const endDate   = new Date(Date.now() + 5_000);
    expect(getAccessLogs({ startDate, endDate })).toHaveLength(5);
  });

  it("combines multiple filters", () => {
    const logs = getAccessLogs({ userId: "user_1", resource: "kyc" });
    expect(logs).toHaveLength(3);
  });

  it("returns empty array when no logs match the filter", () => {
    expect(getAccessLogs({ userId: "ghost_user" })).toHaveLength(0);
  });

  it("handles empty filter object (returns all logs)", () => {
    expect(getAccessLogs({})).toHaveLength(5);
  });
});

// ---------------------------------------------------------------------------
// 3. Date-Range Filter — off-by-one boundary tests
// ---------------------------------------------------------------------------

describe("Date-Range Filter — off-by-one boundaries", () => {
  beforeEach(() => clearAccessLogs());

  it("includes a log whose timestamp equals startDate (inclusive lower bound)", () => {
    logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    const ts = new Date(getAccessLogs()[0].timestamp);

    // startDate === timestamp → should be included
    const logs = getAccessLogs({ startDate: ts });
    expect(logs).toHaveLength(1);
  });

  it("includes a log whose timestamp equals endDate (inclusive upper bound)", () => {
    logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    const ts = new Date(getAccessLogs()[0].timestamp);

    // endDate === timestamp → should be included
    const logs = getAccessLogs({ endDate: ts });
    expect(logs).toHaveLength(1);
  });

  it("excludes a log 1 ms before startDate", () => {
    logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    const ts = new Date(getAccessLogs()[0].timestamp);
    const startDate = new Date(ts.getTime() + 1); // 1 ms after the log

    expect(getAccessLogs({ startDate })).toHaveLength(0);
  });

  it("excludes a log 1 ms after endDate", () => {
    logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    const ts = new Date(getAccessLogs()[0].timestamp);
    const endDate = new Date(ts.getTime() - 1); // 1 ms before the log

    expect(getAccessLogs({ endDate })).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// 4. MAX_LOGS Ring-Buffer Boundary
// ---------------------------------------------------------------------------

describe("MAX_LOGS ring-buffer boundary", () => {
  beforeEach(() => clearAccessLogs());

  const MAX_LOGS = 10_000;

  it("caps stored entries at MAX_LOGS after exactly MAX_LOGS+1 pushes", () => {
    for (let i = 0; i < MAX_LOGS + 1; i++) {
      logAccess({
        action: "read",
        resource: `res-${i}`,
        fields: [],
        sensitiveFields: [],
        piiFields: [],
        status: "success",
      });
    }

    expect(getAccessLogs()).toHaveLength(MAX_LOGS);
  });

  it("keeps the newest entry and drops the oldest after one overflow", () => {
    for (let i = 0; i < MAX_LOGS + 1; i++) {
      logAccess({
        action: "read",
        resource: `res-${i}`,
        fields: [],
        sensitiveFields: [],
        piiFields: [],
        status: "success",
      });
    }

    const logs = getAccessLogs();
    // First entry (res-0) was shifted out; last entry (res-MAX_LOGS) is present
    expect(logs[0].resource).toBe("res-1");
    expect(logs[logs.length - 1].resource).toBe(`res-${MAX_LOGS}`);
  });

  it("stays at MAX_LOGS after many more pushes (ring-buffer stability)", () => {
    for (let i = 0; i < MAX_LOGS + 500; i++) {
      logAccess({
        action: "read",
        resource: "r",
        fields: [],
        sensitiveFields: [],
        piiFields: [],
        status: "success",
      });
    }

    expect(getAccessLogs()).toHaveLength(MAX_LOGS);
  });
});

// ---------------------------------------------------------------------------
// 5. Log Statistics
// ---------------------------------------------------------------------------

describe("Log Statistics", () => {
  beforeEach(() => clearAccessLogs());

  it("counts total entries correctly", () => {
    for (let i = 0; i < 3; i++) {
      logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    }
    expect(getAccessLogStats().total).toBe(3);
  });

  it("counts by action", () => {
    logAccess({ action: "read",  resource: "kyc",     fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "read",  resource: "invoice",  fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "write", resource: "kyc",     fields: [], sensitiveFields: [], piiFields: [], status: "success" });

    const stats = getAccessLogStats();
    expect(stats.byAction.read).toBe(2);
    expect(stats.byAction.write).toBe(1);
  });

  it("counts by resource", () => {
    logAccess({ action: "read", resource: "kyc",     fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "read", resource: "kyc",     fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "read", resource: "invoice",  fields: [], sensitiveFields: [], piiFields: [], status: "success" });

    const stats = getAccessLogStats();
    expect(stats.byResource.kyc).toBe(2);
    expect(stats.byResource.invoice).toBe(1);
  });

  it("counts by status", () => {
    logAccess({ action: "read", resource: "kyc",     fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "read", resource: "invoice",  fields: [], sensitiveFields: [], piiFields: [], status: "failure" });

    const stats = getAccessLogStats();
    expect(stats.byStatus.success).toBe(1);
    expect(stats.byStatus.failure).toBe(1);
  });

  it("returns zero totals on empty log", () => {
    const stats = getAccessLogStats();
    expect(stats.total).toBe(0);
    expect(Object.keys(stats.byAction)).toHaveLength(0);
    expect(Object.keys(stats.byResource)).toHaveLength(0);
    expect(Object.keys(stats.byStatus)).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// 6. Redacted Logs
// ---------------------------------------------------------------------------

describe("Redacted Logs", () => {
  beforeEach(() => clearAccessLogs());

  it("redacts the ipAddress field (PII) in exported logs", () => {
    logAccess({
      action: "read",
      resource: "kyc",
      userId: "user_123",
      ipAddress: "192.168.1.1",
      fields: ["tax_id", "customer_name"],
      sensitiveFields: ["tax_id"],
      piiFields: ["customer_name"],
      status: "success",
    });

    const redacted = getRedactedAccessLogs();
    expect(redacted).toHaveLength(1);
    // ipAddress is a PII field → first 2 + **** + last 2 characters
    expect(redacted[0].ipAddress).toBe("19****.1");
    // Non-PII fields are preserved
    expect(redacted[0].resource).toBe("kyc");
  });

  it("returns an empty array when no logs exist", () => {
    expect(getRedactedAccessLogs()).toHaveLength(0);
  });

  it("applies filters before redacting", () => {
    logAccess({ action: "read",  resource: "kyc",     userId: "u1", ipAddress: "1.1.1.1", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "write", resource: "invoice",  userId: "u2", ipAddress: "2.2.2.2", fields: [], sensitiveFields: [], piiFields: [], status: "success" });

    const redacted = getRedactedAccessLogs({ resource: "kyc" });
    expect(redacted).toHaveLength(1);
    expect(redacted[0].resource).toBe("kyc");
  });

  it("delegating to redactPii: log entry non-PII fields survive redaction intact", () => {
    logAccess({
      action: "delete",
      resource: "settlement",
      fields: [],
      sensitiveFields: [],
      piiFields: [],
      status: "failure",
      error: "HTTP 404",
    });

    const redacted = getRedactedAccessLogs();
    expect(redacted[0].action).toBe("delete");
    expect(redacted[0].resource).toBe("settlement");
    expect(redacted[0].status).toBe("failure");
    expect(redacted[0].error).toBe("HTTP 404");
  });
});

// ---------------------------------------------------------------------------
// 7. Log Management
// ---------------------------------------------------------------------------

describe("Log Management", () => {
  beforeEach(() => clearAccessLogs());

  it("clearAccessLogs removes all stored entries", () => {
    logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    logAccess({ action: "write", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });

    clearAccessLogs();
    expect(getAccessLogs()).toHaveLength(0);
  });

  it("clearAccessLogs is idempotent on an already-empty store", () => {
    expect(() => clearAccessLogs()).not.toThrow();
    expect(getAccessLogs()).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// 8. console.log Failure Boundary in logAccess
// ---------------------------------------------------------------------------

describe("logAccess — console.log transport failure boundary", () => {
  let consoleSpy: jest.SpiedFunction<typeof console.log>;
  let consoleErrorSpy: jest.SpiedFunction<typeof console.error>;

  beforeEach(() => {
    clearAccessLogs();
    consoleSpy = jest.spyOn(console, "log").mockImplementation(() => {
      throw new Error("Broken logging transport");
    });
    consoleErrorSpy = jest.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("does NOT throw when console.log throws", () => {
    expect(() =>
      logAccess({
        action: "read",
        resource: "kyc",
        fields: [],
        sensitiveFields: [],
        piiFields: [],
        status: "success",
      })
    ).not.toThrow();
  });

  it("still stores the entry in the ring-buffer when console.log throws", () => {
    logAccess({
      action: "read",
      resource: "kyc",
      fields: [],
      sensitiveFields: [],
      piiFields: [],
      status: "success",
    });

    // Entry must be persisted even though the transport failed
    expect(getAccessLogs()).toHaveLength(1);
    expect(getAccessLogs()[0].resource).toBe("kyc");
  });

  it("emits a diagnostic to console.error when console.log throws", () => {
    logAccess({ action: "read", resource: "kyc", fields: [], sensitiveFields: [], piiFields: [], status: "success" });

    // Transport exceptions may contain KYC data; diagnostics use a fixed code.
    expect(consoleErrorSpy).toHaveBeenCalledWith("[ACCESS_LOG_CONSOLE_FAILED]");
  });
});

// ---------------------------------------------------------------------------
// 9. res.json override — logAccess failure boundary
// ---------------------------------------------------------------------------

describe("accessLogMiddleware — res.json override survives logAccess failure", () => {
  let consoleSpy: jest.SpiedFunction<typeof console.log>;
  let consoleErrorSpy: jest.SpiedFunction<typeof console.error>;

  beforeEach(() => {
    clearAccessLogs();
    // Suppress console noise in tests
    consoleSpy = jest.spyOn(console, "log").mockImplementation(() => {});
    consoleErrorSpy = jest.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("res.json does NOT throw when logAccess throws internally", () => {
    // Force logAccess to throw by making console.log explode at the transport
    // level — logAccess itself will swallow it now, but let's also test the
    // outer guard by making identifySensitiveFields throw.  We do that by
    // replacing the console.log stub with a more radical stub that throws before
    // the try/catch in logAccess can save us (simulate a bug in getCorrelationId).
    jest.spyOn(requestContext, "getCorrelationId").mockImplementation(() => {
      throw new Error("Corrupted async context");
    });

    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);

    expect(() => runMiddleware(middleware, req, res)).not.toThrow();
  });

  it("res.json returns the original return value even when logAccess throws", () => {
    jest.spyOn(requestContext, "getCorrelationId").mockImplementation(() => {
      throw new Error("Context failure");
    });

    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);
    const next = jest.fn() as unknown as NextFunction;

    middleware(req, res, next);
    const body = { invoiceId: "inv_abc", amount: 500 };
    const returnValue = (res as any).json(body);

    // originalJson is res.json.bind(res) which returns res itself in our mock
    expect(returnValue).toBe(res);
    expect((res as any)._lastBody).toEqual(body);
  });

  it("still delivers the response body when the entire logging path fails", () => {
    jest.spyOn(requestContext, "getCorrelationId").mockImplementation(() => {
      throw new Error("Catastrophic failure");
    });

    const middleware = accessLogMiddleware("invoice", "write");
    const req = buildReq();
    const res = buildRes(201);
    const next = jest.fn() as unknown as NextFunction;

    middleware(req, res, next);
    (res as any).json({ created: true });

    expect((res as any)._lastBody).toEqual({ created: true });
  });

  it("emits a diagnostic error to console.error when logAccess loop throws", () => {
    jest.spyOn(requestContext, "getCorrelationId").mockImplementation(() => {
      throw new Error("Context corrupted");
    });

    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);

    runMiddleware(middleware, req, res);

    // Never print the caught exception, which may contain request data.
    expect(consoleErrorSpy).toHaveBeenCalledWith("[ACCESS_LOG_WRITE_FAILED]");
  });
});

// ---------------------------------------------------------------------------
// 10. getClientIp header priority chain
// ---------------------------------------------------------------------------

describe("getClientIp header priority chain (via middleware)", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  function runAndGetIp(reqOverrides: Partial<Request>): string | undefined {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq(reqOverrides);
    const res = buildRes(200);
    runMiddleware(middleware, req, res);
    return getAccessLogs()[0]?.ipAddress;
  }

  it("x-forwarded-for takes highest priority", () => {
    const ip = runAndGetIp({
      headers: {
        "x-forwarded-for": "10.0.0.1",
        "x-real-ip": "10.0.0.2",
      },
      ip: "10.0.0.3",
    });
    expect(ip).toBe("10.0.0.1");
  });

  it("x-real-ip used when x-forwarded-for is absent", () => {
    const ip = runAndGetIp({
      headers: { "x-real-ip": "10.0.0.2" },
      ip: "10.0.0.3",
    });
    expect(ip).toBe("10.0.0.2");
  });

  it("req.ip used when both x-* headers are absent", () => {
    const ip = runAndGetIp({
      headers: {},
      ip: "10.0.0.3",
    });
    expect(ip).toBe("10.0.0.3");
  });

  it("falls back to 'unknown' when all IP sources are absent", () => {
    const ip = runAndGetIp({
      headers: {},
      ip: undefined,
    });
    expect(ip).toBe("unknown");
  });
});

// ---------------------------------------------------------------------------
// 11. getUserId fallback chain / hash correctness
// ---------------------------------------------------------------------------

describe("getUserId fallback chain and hash correctness (via middleware)", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  function runAndGetUserId(reqOverrides: Partial<Request>): string | undefined {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq(reqOverrides);
    const res = buildRes(200);
    runMiddleware(middleware, req, res);
    return getAccessLogs()[0]?.userId;
  }

  it("x-user-id header has highest priority", () => {
    const userId = runAndGetUserId({
      headers: {
        "x-user-id": "explicit-user-id",
        "authorization": "Bearer some-token",
      },
    });
    expect(userId).toBe("explicit-user-id");
  });

  it("hashes the authorization header when x-user-id is absent", () => {
    const authToken = "Bearer my-secret-token";
    const expectedHash = hashForLog(authToken);

    const userId = runAndGetUserId({
      headers: { "authorization": authToken },
    });
    expect(userId).toBe(expectedHash);
  });

  it("hashForLog produces a deterministic 16-char hex string", () => {
    const hash1 = hashForLog("Bearer token-abc");
    const hash2 = hashForLog("Bearer token-abc");
    expect(hash1).toBe(hash2);
    expect(hash1).toHaveLength(16);
    expect(/^[0-9a-f]+$/.test(hash1)).toBe(true);
  });

  it("hashForLog produces different hashes for different inputs", () => {
    expect(hashForLog("token-A")).not.toBe(hashForLog("token-B"));
  });

  it("userId is undefined when no auth headers are present", () => {
    const userId = runAndGetUserId({ headers: {} });
    expect(userId).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 12. res.statusCode >= 400 triggers "failure" status
// ---------------------------------------------------------------------------

describe("status field based on res.statusCode", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  const cases: Array<[number, "success" | "failure"]> = [
    [200, "success"],
    [201, "success"],
    [204, "success"],
    [301, "success"],
    [399, "success"],
    [400, "failure"],
    [401, "failure"],
    [403, "failure"],
    [404, "failure"],
    [500, "failure"],
    [503, "failure"],
  ];

  it.each(cases)("statusCode %i → status '%s'", (code, expected) => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(code);
    runMiddleware(middleware, req, res);

    expect(getAccessLogs()[0].status).toBe(expected);
  });

  it("sets error field to 'HTTP <code>' for 4xx/5xx responses", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(404);
    runMiddleware(middleware, req, res);

    expect(getAccessLogs()[0].error).toBe("HTTP 404");
  });

  it("does not set error field for 2xx responses", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);
    runMiddleware(middleware, req, res);

    expect(getAccessLogs()[0].error).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 13. res.json preserves return value
// ---------------------------------------------------------------------------

describe("res.json returns the original return value from originalJson", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("returns the res object from originalJson (as bound mock does)", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);
    const next = jest.fn() as unknown as NextFunction;

    middleware(req, res, next);
    const result = (res as any).json({ value: 42 });

    // Our mock's json() returns `res` itself; the wrapper must pass it through.
    expect(result).toBe(res);
  });

  it("passes the exact body object to originalJson unmodified", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);
    const next = jest.fn() as unknown as NextFunction;

    middleware(req, res, next);
    const body = { data: [1, 2, 3], meta: { page: 1 } };
    (res as any).json(body);

    expect((res as any)._lastBody).toEqual({ data: [1, 2, 3], meta: { page: 1 } });
  });
});

// ---------------------------------------------------------------------------
// 14. resourceId extraction from req.params.id
// ---------------------------------------------------------------------------

describe("resourceId extraction from req.params.id", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("extracts req.params.id as resourceId when it is a string", () => {
    const middleware = accessLogMiddleware("invoice", "read");
    const req = buildReq({ params: { id: "inv_789" } });
    const res = buildRes(200);
    runMiddleware(middleware, req, res);

    expect(getAccessLogs()[0].resourceId).toBe("inv_789");
  });

  it("sets resourceId to undefined when req.params.id is absent", () => {
    const middleware = accessLogMiddleware("invoice", "read");
    const req = buildReq({ params: {} });
    const res = buildRes(200);
    runMiddleware(middleware, req, res);

    expect(getAccessLogs()[0].resourceId).toBeUndefined();
  });

  it("sets resourceId to undefined when req.params.id is a non-string value", () => {
    const middleware = accessLogMiddleware("invoice", "read");
    // Express params are always strings, but guard against unusual typing
    const req = buildReq({ params: { id: 123 as any } });
    const res = buildRes(200);
    runMiddleware(middleware, req, res);

    expect(getAccessLogs()[0].resourceId).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 15. Missing / undefined body in middleware
// ---------------------------------------------------------------------------

describe("missing or undefined req.body in middleware", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("does NOT throw when req.body is undefined", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq({ body: undefined });
    const res = buildRes(200);

    expect(() => runMiddleware(middleware, req, res)).not.toThrow();
  });

  it("does NOT throw when req.body is null", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq({ body: null as any });
    const res = buildRes(200);

    expect(() => runMiddleware(middleware, req, res)).not.toThrow();
  });

  it("logs an entry with empty fields when req.body and req.query are empty", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq({ body: {}, query: {} });
    const res = buildRes(200);
    runMiddleware(middleware, req, res);

    const log = getAccessLogs()[0];
    expect(log.fields).toHaveLength(0);
    expect(log.sensitiveFields).toHaveLength(0);
    expect(log.piiFields).toHaveLength(0);
  });

  it("does NOT throw when the response body is null", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);

    expect(() => runMiddleware(middleware, req, res, null)).not.toThrow();
  });

  it("does NOT throw when the response body is a primitive string", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);

    expect(() => runMiddleware(middleware, req, res, "OK")).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// 16. Concurrent middleware instances don't bleed shared state
// ---------------------------------------------------------------------------

describe("concurrent middleware instances — shared module-level ring-buffer", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("entries from separate middleware instances accumulate in the same ring-buffer", () => {
    const mwKyc     = accessLogMiddleware("kyc",     "read");
    const mwInvoice = accessLogMiddleware("invoice",  "write");

    runMiddleware(mwKyc,     buildReq({ params: { id: "k1" } }), buildRes(200));
    runMiddleware(mwInvoice, buildReq({ params: { id: "i1" } }), buildRes(201));

    const logs = getAccessLogs();
    expect(logs).toHaveLength(2);
    expect(logs.map(l => l.resource)).toEqual(expect.arrayContaining(["kyc", "invoice"]));
  });

  it("fields identified for one request do not leak into another request's log entry", () => {
    const mw = accessLogMiddleware("kyc", "read");

    // First request carries a tax_id in query
    runMiddleware(
      mw,
      buildReq({ query: { tax_id: "123-45-6789" }, params: { id: "req1" } }),
      buildRes(200)
    );

    // Second request has no sensitive fields at all
    runMiddleware(
      mw,
      buildReq({ query: {}, params: { id: "req2" } }),
      buildRes(200)
    );

    const logs = getAccessLogs();
    expect(logs).toHaveLength(2);

    const first  = logs.find(l => l.resourceId === "req1")!;
    const second = logs.find(l => l.resourceId === "req2")!;

    expect(first.fields).toContain("tax_id");
    expect(second.fields).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// 17. Sensitive field detection from request body / query
// ---------------------------------------------------------------------------

describe("sensitiveFields / piiFields detection (via middleware)", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("detects sensitive fields from req.query", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq({ query: { tax_id: "xxx", amount: "100" } });
    const res = buildRes(200);
    runMiddleware(middleware, req, res);

    const log = getAccessLogs()[0];
    expect(log.fields).toContain("tax_id");
    expect(log.fields).not.toContain("amount"); // amount is not sensitive
  });

  it("detects sensitive fields from req.body", () => {
    const middleware = accessLogMiddleware("kyc", "write");
    const req = buildReq({ body: { ssn: "123-45-6789", description: "test" } });
    const res = buildRes(201);
    runMiddleware(middleware, req, res);

    const log = getAccessLogs()[0];
    expect(log.fields).toContain("ssn");
    expect(log.fields).not.toContain("description");
  });

  it("detects PII fields from the response body", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq();
    const res = buildRes(200);
    const responseBody = { email: "test@example.com", invoiceId: "inv_1" };
    runMiddleware(middleware, req, res, responseBody);

    const log = getAccessLogs()[0];
    expect(log.piiFields).toContain("email");
    expect(log.sensitiveFields).toContain("email");
  });

  it("produces empty field arrays for a request with no sensitive data", () => {
    const middleware = accessLogMiddleware("invoice", "read");
    const req = buildReq({ query: { page: "1" }, body: { sort: "asc" } });
    const res = buildRes(200);
    runMiddleware(middleware, req, res, { total: 5 });

    const log = getAccessLogs()[0];
    expect(log.fields).toHaveLength(0);
    expect(log.sensitiveFields).toHaveLength(0);
    expect(log.piiFields).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// 18. kycAccessLogMiddleware convenience wrapper
// ---------------------------------------------------------------------------

describe("kycAccessLogMiddleware convenience wrapper", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("produces a function", () => {
    expect(typeof kycAccessLogMiddleware("read")).toBe("function");
  });

  it("logs resource as 'kyc'", () => {
    const mw = kycAccessLogMiddleware("write");
    runMiddleware(mw, buildReq(), buildRes(201));
    expect(getAccessLogs()[0].resource).toBe("kyc");
  });

  it("logs the requested action", () => {
    for (const action of ["read", "write", "update", "delete"] as const) {
      clearAccessLogs();
      runMiddleware(kycAccessLogMiddleware(action), buildReq(), buildRes(200));
      expect(getAccessLogs()[0].action).toBe(action);
    }
  });
});

// ---------------------------------------------------------------------------
// 19. Security / audit field completeness
// ---------------------------------------------------------------------------

describe("Security — audit field completeness", () => {
  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("records userId, ipAddress, userAgent, and timestamp in every log entry", () => {
    const middleware = accessLogMiddleware("invoice", "read");
    const req = buildReq({
      headers: {
        "x-user-id": "u-999",
        "x-forwarded-for": "203.0.113.5",
        "user-agent": "TestAgent/2.0",
      },
    });
    const res = buildRes(200);
    runMiddleware(middleware, req, res);

    const log = getAccessLogs()[0];
    expect(log.userId).toBe("u-999");
    expect(log.ipAddress).toBe("203.0.113.5");
    expect(log.userAgent).toBe("TestAgent/2.0");
    expect(log.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("records all sensitive fields for audit trail", () => {
    logAccess({
      action: "read",
      resource: "invoice",
      fields: ["tax_id", "customer_name", "amount"],
      sensitiveFields: ["tax_id"],
      piiFields: ["customer_name"],
      status: "success",
    });

    const log = getAccessLogs()[0];
    expect(log.fields).toContain("tax_id");
    expect(log.fields).toContain("customer_name");
    expect(log.fields).toContain("amount");
    expect(log.sensitiveFields).toContain("tax_id");
    expect(log.piiFields).toContain("customer_name");
  });

  it("does not include raw PII values in the log entry (only field names)", () => {
    const middleware = accessLogMiddleware("kyc", "read");
    const req = buildReq({ body: { ssn: "123-45-6789" } });
    const res = buildRes(200);
    runMiddleware(middleware, req, res);

    const raw = JSON.stringify(getAccessLogs()[0]);
    // The SSN value itself must not appear in the log entry
    expect(raw).not.toContain("123-45-6789");
    // But the field name is expected (for audit tracking)
    expect(getAccessLogs()[0].fields).toContain("ssn");
  });
});
