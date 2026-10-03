import express, { Request, Response } from "express";
import request from "supertest";
import {
  clearAccessLogs,
  getAccessLogs,
  getAccessLogStats,
  kycAccessLogMiddleware,
  logAccess,
} from "../middleware/access-log";
import { apiKeyAuth, loadApiKeys, resetApiKeys } from "../middleware/apiKeyAuth";
import { runWithContext } from "../lib/requestContext";
import { hashForLog } from "../services/kycService";

type Action = Parameters<typeof kycAccessLogMiddleware>[0];

function harness(statusCode = 200) {
  const req = {
    query: {}, body: {}, params: { id: "record-1" },
    headers: {}, ip: "127.0.0.1",
  } as unknown as Request;
  const res = { statusCode } as Response;
  const writer = jest.fn(function (this: Response, _body: unknown) {
    expect(this).toBe(res);
    return res;
  });
  res.json = writer;
  const next = jest.fn();
  return { req, res, writer, next };
}

describe("kycAccessLogMiddleware failure boundaries (#2720)", () => {
  const originalKeys = process.env.ADMIN_API_KEYS;
  const originalSkip = process.env.SKIP_API_KEY_AUTH;

  beforeEach(() => {
    clearAccessLogs();
    jest.spyOn(console, "log").mockImplementation(() => {});
    jest.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
    jest.useRealTimers();
    clearAccessLogs();
    resetApiKeys();
    if (originalKeys === undefined) delete process.env.ADMIN_API_KEYS;
    else process.env.ADMIN_API_KEYS = originalKeys;
    if (originalSkip === undefined) delete process.env.SKIP_API_KEY_AUTH;
    else process.env.SKIP_API_KEY_AUTH = originalSkip;
  });

  it.each<Action>(["read", "write", "update", "delete"])(
    "records %s without mutating the request or response", (action) => {
      jest.useFakeTimers().setSystemTime(new Date("2026-09-29T12:00:00Z"));
      const { req, res, writer, next } = harness();
      req.body = { tax_id: "secret-tax-id" };
      req.query = { email: "private@example.com" };
      const body = { customer_name: "Private Name", tax_id: "secret-tax-id" };
      kycAccessLogMiddleware(action)(req, res, next);
      expect(next).toHaveBeenCalledTimes(1);
      expect(getAccessLogs()).toEqual([]); // No response attempt while loading.
      expect(res.json(body)).toBe(res);
      expect(writer).toHaveBeenCalledTimes(1);
      expect(writer).toHaveBeenCalledWith(body);
      expect(req.body).toEqual({ tax_id: "secret-tax-id" });
      expect(req.query).toEqual({ email: "private@example.com" });
      expect(body.customer_name).toBe("Private Name");
      expect(getAccessLogs()).toEqual([expect.objectContaining({
        resource: "kyc", action, resourceId: "record-1", status: "success",
        timestamp: "2026-09-29T12:00:00.000Z",
        fields: ["email", "tax_id", "customer_name"],
        sensitiveFields: ["customer_name", "tax_id"],
        piiFields: ["customer_name", "tax_id"],
      })]);
      const surfaces = JSON.stringify([getAccessLogs(), (console.log as jest.Mock).mock.calls]);
      for (const secret of ["secret-tax-id", "Private Name", "private@example.com"]) {
        expect(surfaces).not.toContain(secret);
      }
    },
  );

  it.each([199, 200, 299, 300, 399, 400, 401, 403, 500])(
    "classifies HTTP status %i at the exact success/failure boundary", (status) => {
      const { req, res, next } = harness(status);
      kycAccessLogMiddleware("read")(req, res, next);
      res.json({ error: "private backend detail" });
      const [entry] = getAccessLogs();
      expect(entry.status).toBe(status >= 200 && status < 400 ? "success" : "failure");
      expect(entry.error).toBe(status >= 400 ? `HTTP ${status}` : undefined);
      expect(JSON.stringify(entry)).not.toContain("private backend detail");
    },
  );

  it.each([null, undefined, 17, false, "secret-string", ["secret-array"]])(
    "handles non-record payload %j without changing the writer input", (body) => {
      const { req, res, writer, next } = harness();
      req.body = body;
      kycAccessLogMiddleware("read")(req, res, next);
      res.json(body);
      expect(writer).toHaveBeenCalledWith(body);
      expect(getAccessLogs()[0].fields).toEqual([]);
      expect(getAccessLogs()[0].status).toBe("success");
    },
  );

  it("does not execute KYC getters merely to identify field names", () => {
    const { req, res, next } = harness();
    const getter = jest.fn(() => { throw new Error("private getter value"); });
    req.body = Object.defineProperty({}, "tax_id", { enumerable: true, get: getter });
    kycAccessLogMiddleware("write")(req, res, next);
    res.json({ email: "private@example.com" });
    expect(getter).not.toHaveBeenCalled();
    expect(getAccessLogs()[0].fields).toEqual(["tax_id", "email"]);
  });

  it("records a safe metadata failure and still forwards the response", () => {
    const { req, res, writer, next } = harness();
    req.query = new Proxy({}, { ownKeys: () => { throw new Error("private metadata failure"); } });
    const body = { tax_id: "secret-tax-id" };
    kycAccessLogMiddleware("read")(req, res, next);
    expect(res.json(body)).toBe(res);
    expect(writer).toHaveBeenCalledWith(body);
    expect(getAccessLogs()[0]).toMatchObject({ status: "failure", error: "ACCESS_LOG_METADATA_FAILED" });
    expect(console.error).toHaveBeenCalledWith("[ACCESS_LOG_METADATA_FAILED]");
    expect(JSON.stringify((console.error as jest.Mock).mock.calls)).not.toContain("private metadata failure");
  });

  it("retains the audit record and response when both console sinks throw, then recovers", () => {
    const { req, res, writer, next } = harness();
    (console.log as jest.Mock).mockImplementationOnce(() => { throw new Error("private sink error"); });
    (console.error as jest.Mock).mockImplementationOnce(() => { throw new Error("private fallback error"); });
    kycAccessLogMiddleware("read")(req, res, next);
    expect(() => res.json({ tax_id: "secret-tax-id" })).not.toThrow();
    expect(writer).toHaveBeenCalledTimes(1);
    expect(getAccessLogs()).toHaveLength(1);
    expect(console.error).toHaveBeenCalledWith("[ACCESS_LOG_CONSOLE_FAILED]");
    const recovered = harness();
    kycAccessLogMiddleware("read")(recovered.req, recovered.res, recovered.next);
    recovered.res.json({});
    expect(getAccessLogs()).toHaveLength(2);
    expect(getAccessLogs().every(entry => entry.status === "success")).toBe(true);
  });

  it("preserves the exact response-writer error, audits partial failure, and allows retry", () => {
    const { req, res, writer, next } = harness();
    const failure = new Error("secret serialization detail");
    writer.mockImplementationOnce(() => { throw failure; });
    kycAccessLogMiddleware("update")(req, res, next);
    expect(() => res.json({ tax_id: "secret-tax-id" })).toThrow(failure);
    expect(getAccessLogs()[0]).toMatchObject({ status: "failure", error: "RESPONSE_WRITE_FAILED" });
    expect(console.error).toHaveBeenCalledWith("[RESPONSE_WRITE_FAILED]");
    expect(res.json({ tax_id: "secret-tax-id" })).toBe(res);
    expect(getAccessLogs().map(entry => entry.status)).toEqual(["failure", "success"]);
    expect(writer).toHaveBeenCalledTimes(2);
    expect(JSON.stringify(getAccessLogs())).not.toContain(failure.message);
  });

  it("installs duplicate KYC middleware once while calling each next exactly once", () => {
    const { req, res, writer, next } = harness();
    kycAccessLogMiddleware("read")(req, res, next);
    kycAccessLogMiddleware("read")(req, res, next);
    res.json({ tax_id: "secret-tax-id" });
    expect(next).toHaveBeenCalledTimes(2);
    expect(writer).toHaveBeenCalledTimes(1);
    expect(getAccessLogs()).toHaveLength(1);
  });

  it("retains the newest 10,000 records at the capacity boundary", () => {
    for (let id = 0; id <= 10_000; id += 1) {
      logAccess({ action: "read", resource: "kyc", resourceId: String(id),
        fields: [], sensitiveFields: [], piiFields: [], status: "success" });
    }
    const logs = getAccessLogs();
    expect(logs).toHaveLength(10_000);
    expect(logs[0].resourceId).toBe("1");
    expect(logs[9_999].resourceId).toBe("10000");
    expect(getAccessLogStats().total).toBe(10_000);
  });

  it("does not mask a writer failure if constructing the audit timestamp also fails", () => {
    const { req, res, writer, next } = harness();
    const failure = new Error("private writer failure");
    writer.mockImplementationOnce(() => { throw failure; });
    jest.spyOn(Date.prototype, "toISOString").mockImplementationOnce(() => {
      throw new Error("private clock failure");
    });
    kycAccessLogMiddleware("read")(req, res, next);
    expect(() => res.json({})).toThrow(failure);
    expect(console.error).toHaveBeenCalledWith("[ACCESS_LOG_WRITE_FAILED]");
    expect(getAccessLogs()).toHaveLength(0);
    expect(res.json({})).toBe(res);
    expect(getAccessLogs()[0].status).toBe("success");
    expect(JSON.stringify((console.error as jest.Mock).mock.calls)).not.toContain("private");
  });

  it("preserves downstream next errors and audits the application's error response", () => {
    const { req, res, next } = harness();
    const failure = new Error("private downstream failure");
    next.mockImplementationOnce(() => { throw failure; });
    expect(() => kycAccessLogMiddleware("read")(req, res, next)).toThrow(failure);
    expect(next).toHaveBeenCalledTimes(1);
    expect(getAccessLogs()).toEqual([]);
    res.statusCode = 500;
    res.json({ error: "INTERNAL_ERROR" });
    expect(getAccessLogs()[0]).toMatchObject({ status: "failure", error: "HTTP 500" });
  });

  it.each([[200, 403], [403, 200]])(
    "audits the final writer status when %i becomes %i", (initial, final) => {
      const { req, res, writer, next } = harness(initial);
      writer.mockImplementationOnce(() => { res.statusCode = final; return res; });
      kycAccessLogMiddleware("read")(req, res, next);
      res.json({});
      expect(getAccessLogs()[0]).toMatchObject({
        status: final === 200 ? "success" : "failure",
        error: final === 200 ? undefined : "HTTP 403",
      });
    },
  );

  it.each(["", "READ", "bogus", null, undefined, 1])(
    "rejects invalid runtime action %j before installing a wrapper", (action) => {
      expect(() => kycAccessLogMiddleware(action as Action)).toThrow("Invalid access log action");
      expect(getAccessLogs()).toEqual([]);
    },
  );

  it("ignores malformed header arrays and hashes bearer credentials", () => {
    const first = harness();
    first.req.headers = {
      "x-user-id": ["private-user"], "authorization": ["private-token"],
      "x-forwarded-for": ["private-ip"], "user-agent": ["private-agent"],
    };
    kycAccessLogMiddleware("read")(first.req, first.res, first.next);
    first.res.json({});
    expect(getAccessLogs()[0]).toMatchObject({ userId: undefined, ipAddress: "127.0.0.1", userAgent: undefined });
    const second = harness();
    second.req.headers.authorization = "Bearer private-token";
    kycAccessLogMiddleware("read")(second.req, second.res, second.next);
    second.res.json({});
    expect(getAccessLogs()[1].userId).toBe(hashForLog("Bearer private-token"));
    expect(JSON.stringify([getAccessLogs(), (console.log as jest.Mock).mock.calls])).not.toContain("private-token");
  });

  it("preserves API-key denial and attributes successful access to the authenticated actor", async () => {
    process.env.ADMIN_API_KEYS = "permitted-key:verified-actor";
    delete process.env.SKIP_API_KEY_AUTH;
    loadApiKeys();
    const app = express();
    const handler = jest.fn((_req: Request, res: Response) => res.json({ email: "private@example.com" }));
    app.get("/kyc", kycAccessLogMiddleware("read"), apiKeyAuth, handler);
    await request(app).get("/kyc").set("X-API-Key", "rejected-key").expect(401);
    expect(handler).not.toHaveBeenCalled();
    await request(app).get("/kyc").set("X-API-Key", "permitted-key")
      .set("X-User-Id", "spoofed-private-user").expect(200);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(getAccessLogs().map(entry => entry.status)).toEqual(["failure", "success"]);
    expect(getAccessLogs()[1].userId).toBe("verified-actor");
    const surfaces = JSON.stringify([getAccessLogs(), (console.log as jest.Mock).mock.calls]);
    for (const secret of ["permitted-key", "rejected-key", "spoofed-private-user", "private@example.com"]) {
      expect(surfaces).not.toContain(secret);
    }
  });

  it("isolates concurrent correlations and late responses using deterministic barriers", async () => {
    const first = harness(403);
    const second = harness(200);
    let releaseFirst!: () => void;
    const barrier = new Promise<void>(resolve => { releaseFirst = resolve; });
    const early = runWithContext("request-first", async () => {
      kycAccessLogMiddleware("read")(first.req, first.res, first.next);
      await barrier;
      first.res.json({ email: "first-private@example.com" });
    });
    await runWithContext("request-second", async () => {
      kycAccessLogMiddleware("write")(second.req, second.res, second.next);
      await Promise.resolve();
      second.res.json({ tax_id: "second-private-tax-id" });
    });
    releaseFirst();
    await early;
    expect(getAccessLogs().map(entry => [entry.correlationId, entry.action, entry.status])).toEqual([
      ["request-second", "write", "success"], ["request-first", "read", "failure"],
    ]);
    expect(getAccessLogStats()).toMatchObject({ total: 2, byStatus: { success: 1, failure: 1 } });
  });
});
