/**
 * Tests for statusInjector middleware and schema stability.
 *
 * Covers:
 *  - _system field is present on all object JSON responses
 *  - _system field shape is stable (schema stability)
 *  - Existing response fields are never modified or removed
 *  - Array responses are passed through unchanged (no injection)
 *  - Non-object primitives are passed through unchanged
 *  - _system reflects correct degradation state
 *  - GET /api/v1/status returns correct LagStatus shape
 */

import request from "supertest";
import app from "../app";
import { statusService } from "../services/statusService";
import { lagMonitor } from "../services/lagMonitor";

// ---------------------------------------------------------------------------
// Reset state
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
// _system injection – shape and stability
// ---------------------------------------------------------------------------

describe("statusInjector – _system field injection", () => {
  it("injects _system into object responses", async () => {
    const res = await request(app).get("/api/v1/invoices/nonexistent");

    // 404 response is an object
    expect(res.status).toBe(404);
    expect(res.body).toHaveProperty("_system");
  });

  it("_system has the required fields", async () => {
    const res = await request(app).get("/api/v1/health");

    expect(res.body._system).toMatchObject({
      status: expect.stringMatching(/^(operational|degraded|maintenance)$/),
      degraded: expect.any(Boolean),
      lag: expect.any(Number),
      level: expect.stringMatching(/^(none|warn|critical)$/),
    });
  });

  it("does not modify existing fields in the response", async () => {
    const res = await request(app).get("/api/v1/health");

    // Original fields must still be present and unchanged
    expect(res.body.status).toBe("ok");
    expect(res.body.version).toBe("1.0.0");
    expect(res.body.timestamp).toBeTruthy();
  });

  it("injects _system into error responses", async () => {
    const res = await request(app).get("/api/v1/invoices/nonexistent");

    expect(res.body.error).toBeDefined();
    expect(res.body._system).toBeDefined();
  });

  it("does not inject _system into array responses", async () => {
    const res = await request(app).get("/api/v1/invoices");

    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
    // Arrays should not have _system injected
    expect(res.body._system).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// _system reflects degradation state
// ---------------------------------------------------------------------------

describe("statusInjector – _system reflects system state", () => {
  it("_system.status is operational when healthy", async () => {
    statusService.setMockCurrentLedger(100005); // lag = 5

    const res = await request(app).get("/api/v1/health");

    expect(res.body._system.status).toBe("operational");
    expect(res.body._system.degraded).toBe(false);
    expect(res.body._system.level).toBe("none");
  });

  it("_system.status is degraded when lag >= warn threshold", async () => {
    statusService.setMockCurrentLedger(100020); // lag = 20

    const res = await request(app).get("/api/v1/health");

    expect(res.body._system.status).toBe("degraded");
    expect(res.body._system.degraded).toBe(true);
    expect(res.body._system.level).toBe("warn");
    expect(res.body._system.lag).toBe(20);
  });

  it("_system.status is degraded at critical level", async () => {
    statusService.setMockCurrentLedger(100100); // lag = 100

    const res = await request(app).get("/api/v1/health");

    expect(res.body._system.status).toBe("degraded");
    expect(res.body._system.degraded).toBe(true);
    expect(res.body._system.level).toBe("critical");
  });

  it("_system.status is maintenance when maintenance mode is on", async () => {
    statusService.setMaintenanceMode(true);

    const res = await request(app).get("/api/v1/health");

    expect(res.body._system.status).toBe("maintenance");
  });
});

// ---------------------------------------------------------------------------
// Schema stability – existing clients unaffected
// ---------------------------------------------------------------------------

describe("statusInjector – schema stability", () => {
  it("health endpoint response shape is backward-compatible", async () => {
    const res = await request(app).get("/api/v1/health");

    // These fields must always be present for existing clients
    expect(res.body).toHaveProperty("status");
    expect(res.body).toHaveProperty("version");
    expect(res.body).toHaveProperty("timestamp");
    // _system is additive — existing clients can ignore it
    expect(res.body).toHaveProperty("_system");
  });

  it("invoice list response is an array (no _system injection)", async () => {
    const res = await request(app).get("/api/v1/invoices");

    expect(Array.isArray(res.body)).toBe(true);
    // Clients consuming arrays are unaffected
    expect(res.body[0]).not.toHaveProperty("_system");
  });

  it("invoice object response has _system but original fields intact", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/invoices/${id}`);

    expect(res.status).toBe(200);
    expect(res.body.id).toBe(id);
    expect(res.body.status).toBeDefined(); // original field
    expect(res.body._system).toBeDefined(); // additive field
  });

  it("_system field shape is consistent across different endpoints", async () => {
    const endpoints = [
      "/api/v1/health",
      "/api/v1/status",
      "/api/v1/invoices/nonexistent",
    ];

    for (const endpoint of endpoints) {
      const res = await request(app).get(endpoint);
      const sys = res.body._system;

      expect(sys).toBeDefined();
      expect(typeof sys.status).toBe("string");
      expect(typeof sys.degraded).toBe("boolean");
      expect(typeof sys.lag).toBe("number");
      expect(typeof sys.level).toBe("string");
    }
  });

  it("_system.lag is a non-negative number", async () => {
    const res = await request(app).get("/api/v1/health");
    expect(res.body._system.lag).toBeGreaterThanOrEqual(0);
  });
});

// ---------------------------------------------------------------------------
// GET /api/v1/status – LagStatus endpoint shape
// ---------------------------------------------------------------------------

describe("GET /api/v1/status – LagStatus endpoint", () => {
  it("returns 200 with full LagStatus shape", async () => {
    const res = await request(app).get("/api/v1/status");

    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      lag: expect.any(Number),
      warnThreshold: 10,
      criticalThreshold: 50,
      level: expect.stringMatching(/^(none|warn|critical)$/),
      isDegraded: expect.any(Boolean),
      isCritical: expect.any(Boolean),
      checkedAt: expect.stringMatching(/^\d{4}-\d{2}-\d{2}T/),
    });
  });

  it("returns level=none when healthy", async () => {
    statusService.setMockCurrentLedger(100005);

    const res = await request(app).get("/api/v1/status");

    expect(res.body.level).toBe("none");
    expect(res.body.isDegraded).toBe(false);
    expect(res.body.isCritical).toBe(false);
  });

  it("returns level=warn when lag is between thresholds", async () => {
    statusService.setMockCurrentLedger(100025); // lag = 25

    const res = await request(app).get("/api/v1/status");

    expect(res.body.level).toBe("warn");
    expect(res.body.isDegraded).toBe(true);
    expect(res.body.isCritical).toBe(false);
    expect(res.body.lag).toBe(25);
  });

  it("returns level=critical when lag >= critical threshold", async () => {
    statusService.setMockCurrentLedger(100060); // lag = 60

    const res = await request(app).get("/api/v1/status");

    expect(res.body.level).toBe("critical");
    expect(res.body.isDegraded).toBe(true);
    expect(res.body.isCritical).toBe(true);
  });

  it("also has _system injected (meta on meta)", async () => {
    const res = await request(app).get("/api/v1/status");

    expect(res.body._system).toBeDefined();
  });

  it("handles getLagStatus error gracefully via error handler", async () => {
    jest
      .spyOn(lagMonitor, "getLagStatus")
      .mockRejectedValueOnce(new Error("RPC down"));

    const res = await request(app).get("/api/v1/status");

    expect(res.status).toBe(500);
  });
});

// ---------------------------------------------------------------------------
// statusInjector – unit: middleware directly
// ---------------------------------------------------------------------------

describe("statusInjector – unit", () => {
  it("passes through non-object body unchanged", () => {
    const { statusInjector } = require("../middleware/status-injector");

    const jsonMock = jest.fn().mockReturnThis();
    const res = {
      json: jsonMock,
    } as unknown as import("express").Response;

    const req = {} as import("express").Request;
    const next = jest.fn();

    statusInjector(req, res, next);

    // Call the patched json with a primitive
    (res.json as jest.Mock)("plain string");

    // Should have called original json with the primitive unchanged
    expect(jsonMock).toHaveBeenCalledWith("plain string");
    expect(next).toHaveBeenCalled();
  });

  it("passes through null body unchanged", () => {
    const { statusInjector } = require("../middleware/status-injector");

    const jsonMock = jest.fn().mockReturnThis();
    const res = { json: jsonMock } as unknown as import("express").Response;
    const req = {} as import("express").Request;
    const next = jest.fn();

    statusInjector(req, res, next);
    (res.json as jest.Mock)(null);

    expect(jsonMock).toHaveBeenCalledWith(null);
  });

  it("passes through array body unchanged", () => {
    const { statusInjector } = require("../middleware/status-injector");

    const jsonMock = jest.fn().mockReturnThis();
    const res = { json: jsonMock } as unknown as import("express").Response;
    const req = {} as import("express").Request;
    const next = jest.fn();

    statusInjector(req, res, next);
    (res.json as jest.Mock)([1, 2, 3]);

    expect(jsonMock).toHaveBeenCalledWith([1, 2, 3]);
  });

  it("injects _system into object body", () => {
    const { statusInjector } = require("../middleware/status-injector");

    const jsonMock = jest.fn().mockReturnThis();
    const res = { json: jsonMock } as unknown as import("express").Response;
    const req = {} as import("express").Request;
    const next = jest.fn();

    statusInjector(req, res, next);
    (res.json as jest.Mock)({ foo: "bar" });

    const called = jsonMock.mock.calls[0][0];
    expect(called.foo).toBe("bar");
    expect(called._system).toBeDefined();
    expect(typeof called._system.status).toBe("string");
  });
});

// ---------------------------------------------------------------------------
// getLagStatusSync – branch coverage for maintenance and null mockLedger
// ---------------------------------------------------------------------------

describe("statusInjector – getLagStatusSync branch coverage", () => {
  it("_system.status is maintenance when maintenance mode is active (sync path)", () => {
    statusService.setMaintenanceMode(true);
    statusService.setMockCurrentLedger(100005); // healthy lag

    // Any object response will trigger the sync snapshot
    const { statusInjector } = require("../middleware/status-injector");
    const jsonMock = jest.fn().mockReturnThis();
    const res = { json: jsonMock } as unknown as import("express").Response;
    const req = {} as import("express").Request;
    const next = jest.fn();

    statusInjector(req, res, next);
    (res.json as jest.Mock)({ ping: "pong" });

    const called = jsonMock.mock.calls[0][0];
    expect(called._system.status).toBe("maintenance");
  });

  it("_system uses time-based ledger when mockCurrentLedger is null", () => {
    statusService.setMockCurrentLedger(null);
    statusService.updateLastIndexedLedger(0); // force a large lag via time-based path

    const { statusInjector } = require("../middleware/status-injector");
    const jsonMock = jest.fn().mockReturnThis();
    const res = { json: jsonMock } as unknown as import("express").Response;
    const req = {} as import("express").Request;
    const next = jest.fn();

    statusInjector(req, res, next);
    (res.json as jest.Mock)({ ping: "pong" });

    const called = jsonMock.mock.calls[0][0];
    // lag will be large (time-based), so status will be degraded or critical
    expect(called._system.lag).toBeGreaterThanOrEqual(0);
    expect(typeof called._system.status).toBe("string");
  });
});
