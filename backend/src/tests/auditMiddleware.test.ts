import { describe, it, expect, beforeEach, afterEach, jest } from "@jest/globals";
import { Request, Response, NextFunction } from "express";
import { auditMiddleware, registerAuditRoute } from "../middleware/auditMiddleware";
import { auditService } from "../services/auditService";
import { AuthenticatedRequest } from "../middleware/apiKeyAuth";
import EventEmitter from "events";

function createMockReq(overrides: Partial<AuthenticatedRequest> = {}): AuthenticatedRequest {
  return {
    method: "POST",
    path: "/maintenance",
    originalUrl: "/maintenance",
    headers: {
      "user-agent": "test-agent/1.0",
      "x-forwarded-for": "203.0.113.195, 70.41.3.18",
    },
    body: { enabled: true },
    actor: "admin-user-1",
    socket: { remoteAddress: "127.0.0.1" } as any,
    ...overrides,
  } as unknown as AuthenticatedRequest;
}

function createMockRes(): Response & EventEmitter & { _status: number; _body: any; _json: any } {
  const emitter = new EventEmitter();
  const res: any = Object.assign(emitter, {
    statusCode: 200,
    _status: 200,
    _body: null,
    _json: null,
    status(code: number) {
      res.statusCode = code;
      res._status = code;
      return res;
    },
    json(body: any) {
      res._json = body;
      res._body = body;
      res.emit("finish");
      return res;
    },
    send(body: any) {
      res._body = body;
      res.emit("finish");
      return res;
    },
  });
  return res;
}

describe("auditMiddleware (failure boundary & deterministic unit tests)", () => {
  let appendSpy: jest.SpiedFunction<typeof auditService.append>;

  beforeEach(() => {
    appendSpy = jest.spyOn(auditService, "append").mockImplementation((entry: any) => entry);
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("should log audit entry on res.json for matching route", () => {
    const req = createMockReq();
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    expect(next).toHaveBeenCalled();

    res.status(200).json({ success: true });

    expect(appendSpy).toHaveBeenCalledTimes(1);
    const callArg = appendSpy.mock.calls[0][0];
    expect(callArg.actor).toBe("admin-user-1");
    expect(callArg.operation).toBe("MAINTENANCE_MODE");
    expect(callArg.ip).toBe("203.0.113.195");
    expect(callArg.userAgent).toBe("test-agent/1.0");
    expect(callArg.success).toBe(true);
  });

  it("should skip audit logging for non-matching routes", () => {
    const req = createMockReq({ path: "/untracked-route", originalUrl: "/untracked-route" });
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    expect(next).toHaveBeenCalled();

    res.status(200).json({ success: true });
    expect(appendSpy).not.toHaveBeenCalled();
  });

  it("should log audit entry on res.send when res.json is not called", () => {
    const req = createMockReq();
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    res.status(200).send("OK");

    expect(appendSpy).toHaveBeenCalledTimes(1);
    expect(appendSpy.mock.calls[0][0].success).toBe(true);
  });

  it("should log audit entry exactly once even if res.json and finish event fire", () => {
    const req = createMockReq();
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    res.status(200).json({ ok: true });
    res.emit("finish");

    expect(appendSpy).toHaveBeenCalledTimes(1);
  });

  it("should capture error message on 400+ status codes", () => {
    const req = createMockReq();
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    res.status(400).json({
      error: { message: "Invalid parameters provided" },
    });

    expect(appendSpy).toHaveBeenCalledTimes(1);
    const callArg = appendSpy.mock.calls[0][0];
    expect(callArg.success).toBe(false);
    expect(callArg.errorMessage).toBe("Invalid parameters provided");
  });

  it("should handle error thrown inside describeEffect gracefully", () => {
    registerAuditRoute("POST", "/test-throwing-effect", {
      operation: "MAINTENANCE_MODE",
      describeEffect: () => {
        throw new Error("Formatting error");
      },
    });

    const req = createMockReq({ path: "/test-throwing-effect", originalUrl: "/test-throwing-effect" });
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    res.status(200).json({ ok: true });

    expect(appendSpy).toHaveBeenCalledTimes(1);
    expect(appendSpy.mock.calls[0][0].effect).toContain("Effect description failed: Formatting error");
  });

  it("should handle error thrown inside auditService.append gracefully without crashing res.json", () => {
    appendSpy.mockImplementation(() => {
      throw new Error("Disk write error");
    });
    const consoleSpy = jest.spyOn(console, "error").mockImplementation(() => {});

    const req = createMockReq();
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    expect(() => res.status(200).json({ ok: true })).not.toThrow();
    expect(consoleSpy).toHaveBeenCalledWith("[Audit] Failed to write audit entry:", expect.any(Error));
  });

  it("should safely handle null/undefined req.body and fallback to empty object", () => {
    const req = createMockReq({ body: undefined });
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    res.status(200).json({ ok: true });

    expect(appendSpy).toHaveBeenCalledTimes(1);
    expect(appendSpy.mock.calls[0][0].params).toEqual({});
  });

  it("should extract client IP from x-real-ip when x-forwarded-for is missing", () => {
    const req = createMockReq({
      headers: { "x-real-ip": "198.51.100.42" },
    });
    const res = createMockRes();
    const next = jest.fn() as NextFunction;

    auditMiddleware(req, res, next);
    res.status(200).json({ ok: true });

    expect(appendSpy).toHaveBeenCalledTimes(1);
    expect(appendSpy.mock.calls[0][0].ip).toBe("198.51.100.42");
  });
});
