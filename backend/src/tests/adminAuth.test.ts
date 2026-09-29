import { Request, Response } from "express";
import { getAdminActor, requireAdminAuth } from "../middleware/adminAuth";

describe("adminAuth middleware", () => {
  const next = jest.fn();

  const buildRes = () => {
    const res: Partial<Response> = {};
    res.status = jest.fn().mockReturnValue(res as Response);
    res.json = jest.fn().mockReturnValue(res as Response);
    return res as Response;
  };

  beforeEach(() => {
    next.mockReset();
    delete process.env.ADMIN_API_TOKEN;
  });

  it("returns 503 when admin token is not configured", () => {
    const req = { header: jest.fn() } as unknown as Request;
    const res = buildRes();
    requireAdminAuth(req, res, next);
    expect(res.status).toHaveBeenCalledWith(503);
    expect(next).not.toHaveBeenCalled();
  });

  it("returns 401 when auth header is missing", () => {
    process.env.ADMIN_API_TOKEN = "token";
    const req = { header: jest.fn().mockReturnValue(undefined) } as unknown as Request;
    const res = buildRes();
    requireAdminAuth(req, res, next);
    expect(res.status).toHaveBeenCalledWith(401);
  });

  it("returns 403 for invalid bearer token", () => {
    process.env.ADMIN_API_TOKEN = "token";
    const req = { header: jest.fn().mockReturnValue("Bearer wrong") } as unknown as Request;
    const res = buildRes();
    requireAdminAuth(req, res, next);
    expect(res.status).toHaveBeenCalledWith(403);
  });

  it("calls next for valid bearer token", () => {
    process.env.ADMIN_API_TOKEN = "token";
    const req = { header: jest.fn().mockReturnValue("Bearer token") } as unknown as Request;
    const res = buildRes();
    requireAdminAuth(req, res, next);
    expect(next).toHaveBeenCalled();
  });

  it("resolves actor from header or fallback", () => {
    const withActor = { header: jest.fn().mockReturnValue("ops") } as unknown as Request;
    const withoutActor = { header: jest.fn().mockReturnValue(undefined) } as unknown as Request;
    expect(getAdminActor(withActor)).toBe("ops");
    expect(getAdminActor(withoutActor)).toBe("admin");
  });
});
