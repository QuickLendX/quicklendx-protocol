/**
 * Tests for degradedGuard middleware.
 *
 * Covers:
 *  - Unit: middleware factory with mocked lagMonitor
 *  - Integration: full HTTP round-trips via supertest against app
 *  - Endpoint gating: 503 at warn level, 503 at critical level, 201 when healthy
 *  - criticalOnly option: passes at warn, blocks at critical
 *  - Error propagation: lag check failure fails open
 *  - Security: auth headers / status codes not modified
 */

import { Request, Response, NextFunction } from "express";
import request from "supertest";
import app from "../app";
import { statusService } from "../services/statusService";
import { lagMonitor } from "../services/lagMonitor";

// ---------------------------------------------------------------------------
// Reset state before each test
// ---------------------------------------------------------------------------

beforeEach(() => {
  statusService.setMaintenanceMode(false);
  statusService.updateLastIndexedLedger(100000);
  statusService.setMockCurrentLedger(100005); // lag = 5, healthy
  lagMonitor.setThresholds(10, 50);
});

afterEach(() => {
  statusService.setMockCurrentLedger(null);
  jest.restoreAllMocks();
});

// ---------------------------------------------------------------------------
// Unit tests – middleware in isolation
// ---------------------------------------------------------------------------

describe("degradedGuard – unit (mocked lagMonitor)", () => {
  function makeReqResNext(): {
    req: Request;
    res: Response;
    next: NextFunction;
  } {
    const req = {} as Request;
    const json = jest.fn().mockReturnThis();
    const status = jest.fn().mockReturnValue({ json });
    const res = { status, json } as unknown as Response;
    const next = jest.fn() as unknown as NextFunction;
    return { req, res, next };
  }

  it("calls next() when system is healthy", async () => {
    jest.spyOn(lagMonitor, "getLagStatus").mockResolvedValueOnce({
      lag: 5,
      warnThreshold: 10,
      criticalThreshold: 50,
      level: "none",
      isDegraded: false,
      isCritical: false,
      checkedAt: new Date().toISOString(),
    });

    const { degradedGuard } = await import("../middleware/degraded-guard");
    const mw = degradedGuard();
    const { req, res, next } = makeReqResNext();

    await mw(req, res, next);

    expect(next).toHaveBeenCalledWith();
    expect((res.status as jest.Mock)).not.toHaveBeenCalled();
  });

  it("returns 503 DEGRADED_MODE when warn level", async () => {
    jest.spyOn(lagMonitor, "getLagStatus").mockResolvedValueOnce({
      lag: 20,
      warnThreshold: 10,
      criticalThreshold: 50,
      level: "warn",
      isDegraded: true,
      isCritical: false,
      checkedAt: new Date().toISOString(),
    });

    const { degradedGuard } = await import("../middleware/degraded-guard");
    const mw = degradedGuard();
    const { req, res, next } = makeReqResNext();

    await mw(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(503);
    const jsonArg = (res.status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(jsonArg.error.code).toBe("DEGRADED_MODE");
    expect(jsonArg.error.details.lag).toBe(20);
    expect(jsonArg.error.details.level).toBe("warn");
    expect(next).not.toHaveBeenCalled();
  });

  it("returns 503 DEGRADED_MODE_CRITICAL when critical level", async () => {
    jest.spyOn(lagMonitor, "getLagStatus").mockResolvedValueOnce({
      lag: 75,
      warnThreshold: 10,
      criticalThreshold: 50,
      level: "critical",
      isDegraded: true,
      isCritical: true,
      checkedAt: new Date().toISOString(),
    });

    const { degradedGuard } = await import("../middleware/degraded-guard");
    const mw = degradedGuard();
    const { req, res, next } = makeReqResNext();

    await mw(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(503);
    const jsonArg = (res.status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(jsonArg.error.code).toBe("DEGRADED_MODE_CRITICAL");
    expect(next).not.toHaveBeenCalled();
  });

  it("criticalOnly: calls next() at warn level", async () => {
    jest.spyOn(lagMonitor, "getLagStatus").mockResolvedValueOnce({
      lag: 20,
      warnThreshold: 10,
      criticalThreshold: 50,
      level: "warn",
      isDegraded: true,
      isCritical: false,
      checkedAt: new Date().toISOString(),
    });

    const { degradedGuard } = await import("../middleware/degraded-guard");
    const mw = degradedGuard({ criticalOnly: true });
    const { req, res, next } = makeReqResNext();

    await mw(req, res, next);

    expect(next).toHaveBeenCalledWith();
    expect((res.status as jest.Mock)).not.toHaveBeenCalled();
  });

  it("criticalOnly: returns 503 at critical level", async () => {
    jest.spyOn(lagMonitor, "getLagStatus").mockResolvedValueOnce({
      lag: 75,
      warnThreshold: 10,
      criticalThreshold: 50,
      level: "critical",
      isDegraded: true,
      isCritical: true,
      checkedAt: new Date().toISOString(),
    });

    const { degradedGuard } = await import("../middleware/degraded-guard");
    const mw = degradedGuard({ criticalOnly: true });
    const { req, res, next } = makeReqResNext();

    await mw(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(503);
    const jsonArg = (res.status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(jsonArg.error.code).toBe("DEGRADED_MODE_CRITICAL");
  });

  it("fails open and calls next(err) when lag check throws", async () => {
    const boom = new Error("RPC unavailable");
    jest.spyOn(lagMonitor, "getLagStatus").mockRejectedValueOnce(boom);

    const { degradedGuard } = await import("../middleware/degraded-guard");
    const mw = degradedGuard();
    const { req, res, next } = makeReqResNext();

    await mw(req, res, next);

    expect(next).toHaveBeenCalledWith(boom);
    expect((res.status as jest.Mock)).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Integration tests – full HTTP via supertest
// ---------------------------------------------------------------------------

describe("degradedGuard – integration (POST /api/v1/write-action)", () => {
  it("returns 201 when system is healthy (lag < warn threshold)", async () => {
    statusService.setMockCurrentLedger(100005); // lag = 5

    const res = await request(app).post("/api/v1/write-action").send({});

    expect(res.status).toBe(201);
    expect(res.body.success).toBe(true);
  });

  it("returns 503 DEGRADED_MODE when lag is at warn threshold", async () => {
    statusService.setMockCurrentLedger(100010); // lag = 10

    const res = await request(app).post("/api/v1/write-action").send({});

    expect(res.status).toBe(503);
    expect(res.body.error.code).toBe("DEGRADED_MODE");
    expect(res.body.error.details.lag).toBe(10);
    expect(res.body.error.details.level).toBe("warn");
  });

  it("returns 503 DEGRADED_MODE when lag is between warn and critical", async () => {
    statusService.setMockCurrentLedger(100030); // lag = 30

    const res = await request(app).post("/api/v1/write-action").send({});

    expect(res.status).toBe(503);
    expect(res.body.error.code).toBe("DEGRADED_MODE");
  });

  it("returns 503 DEGRADED_MODE_CRITICAL when lag is at critical threshold", async () => {
    statusService.setMockCurrentLedger(100050); // lag = 50

    const res = await request(app).post("/api/v1/write-action").send({});

    expect(res.status).toBe(503);
    expect(res.body.error.code).toBe("DEGRADED_MODE_CRITICAL");
    expect(res.body.error.details.level).toBe("critical");
  });

  it("returns 503 DEGRADED_MODE_CRITICAL when lag exceeds critical threshold", async () => {
    statusService.setMockCurrentLedger(100200); // lag = 200

    const res = await request(app).post("/api/v1/write-action").send({});

    expect(res.status).toBe(503);
    expect(res.body.error.code).toBe("DEGRADED_MODE_CRITICAL");
  });

  it("response body includes threshold details", async () => {
    statusService.setMockCurrentLedger(100020); // lag = 20

    const res = await request(app).post("/api/v1/write-action").send({});

    expect(res.body.error.details).toMatchObject({
      lag: 20,
      warn_threshold: 10,
      critical_threshold: 50,
      level: "warn",
    });
  });
});

describe("degradedGuard – integration (POST /api/v1/critical-action, criticalOnly)", () => {
  it("returns 201 when lag is below warn threshold", async () => {
    statusService.setMockCurrentLedger(100005); // lag = 5

    const res = await request(app).post("/api/v1/critical-action").send({});

    expect(res.status).toBe(201);
  });

  it("returns 201 when lag is at warn level (criticalOnly passes through)", async () => {
    statusService.setMockCurrentLedger(100020); // lag = 20, warn only

    const res = await request(app).post("/api/v1/critical-action").send({});

    expect(res.status).toBe(201);
  });

  it("returns 503 DEGRADED_MODE_CRITICAL when lag is at critical threshold", async () => {
    statusService.setMockCurrentLedger(100050); // lag = 50

    const res = await request(app).post("/api/v1/critical-action").send({});

    expect(res.status).toBe(503);
    expect(res.body.error.code).toBe("DEGRADED_MODE_CRITICAL");
  });
});

// ---------------------------------------------------------------------------
// Security: guard does not interfere with auth headers or status codes
// ---------------------------------------------------------------------------

describe("degradedGuard – security contract", () => {
  it("does not strip or modify response headers set before the guard", async () => {
    // The guard only adds a 503 body — it never touches headers set by
    // upstream auth middleware.  We verify the Content-Type is still JSON.
    statusService.setMockCurrentLedger(100020); // degraded

    const res = await request(app).post("/api/v1/write-action").send({});

    expect(res.status).toBe(503);
    expect(res.headers["content-type"]).toMatch(/application\/json/);
  });

  it("read-only endpoints are not affected by degraded state", async () => {
    statusService.setMockCurrentLedger(100200); // critically degraded

    // GET /api/v1/invoices is a read endpoint — no guard applied
    const res = await request(app).get("/api/v1/invoices");

    expect(res.status).toBe(200);
  });

  it("GET /api/v1/status is not blocked by degraded state", async () => {
    statusService.setMockCurrentLedger(100200); // critically degraded

    const res = await request(app).get("/api/v1/status");

    expect(res.status).toBe(200);
    expect(res.body.level).toBe("critical");
  });
});
