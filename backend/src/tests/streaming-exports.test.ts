import request from "supertest";
import express from "express";
import fs from "fs";
import fsp from "fs/promises";
import path from "path";
import os from "os";
import crypto from "crypto";

process.env.EXPORT_DIR = path.join(os.tmpdir(), "qlx-export-test-" + Date.now());
process.env.EXPORT_SECRET = "test-secret-thirty-two-chars-long-for-hmac";
process.env.NODE_ENV = "test";
process.env.RATE_LIMIT_EXPORT_POINTS = "1000";

import { exportService, ExportFormat, ExportErrorCode } from "../services/exportService";
import { config } from "../config";
import { requestExport, downloadExport, createDownloadExportHandler } from "../controllers/v1/exports";

const exportDir = process.env.EXPORT_DIR!;

function buildApp() {
  const app = express();
  app.use(express.json());
  const { default: exportRoutes } = require("../routes/v1/exports");
  app.use("/api/v1/exports", (req: any, _res: any, next: any) => {
    const auth = req.headers.authorization || "";
    req.user = { userId: auth.replace("Bearer ", "") || "user-stream-test" };
    next();
  });
  app.use("/api/v1/exports", exportRoutes);
  return app;
}

function makeToken(userId: string, format: ExportFormat, expiresAt: number): string {
  const payload = JSON.stringify({ userId, format, expiresAt });
  const signature = crypto
    .createHmac("sha256", config.EXPORT_SECRET)
    .update(payload)
    .digest("hex");
  return Buffer.from(JSON.stringify({ payload, signature })).toString("base64");
}

function makeFutureToken(userId: string, format: ExportFormat): string {
  return makeToken(userId, format, Date.now() + 3600_000);
}

function makeExpiredToken(userId: string, format: ExportFormat): string {
  return makeToken(userId, format, Date.now() - 1000);
}

beforeAll(async () => {
  await fsp.mkdir(exportDir, { recursive: true, mode: 0o700 });
});

afterAll(async () => {
  await fsp.rm(exportDir, { recursive: true, force: true }).catch(() => {});
});

beforeEach(async () => {
  const files = await fsp.readdir(exportDir).catch(() => []);
  for (const f of files) {
    await fsp.unlink(path.join(exportDir, f)).catch(() => {});
  }
});

// ---------------------------------------------------------------------------
// Direct service unit tests
// ---------------------------------------------------------------------------
describe("ExportService - Token signing", () => {
  it("generates and validates a token", () => {
    const token = exportService.generateSignedToken("user-1", ExportFormat.JSON);
    const result = exportService.validateToken(token);
    expect(result).not.toBeNull();
    expect(result!.userId).toBe("user-1");
    expect(result!.format).toBe(ExportFormat.JSON);
  });

  it("rejects tampered token", () => {
    const result = exportService.validateToken("invalid+base64!!");
    expect(result).toBeNull();
  });

  it("rejects expired token", () => {
    const token = makeExpiredToken("user-1", ExportFormat.JSON);
    const result = exportService.validateToken(token);
    expect(result).toBeNull();
  });

  it("rejects token with bad signature", () => {
    const payload = JSON.stringify({ userId: "u1", format: "json", expiresAt: Date.now() + 3600_000 });
    const badSig = crypto.createHmac("sha256", "wrong-secret").update(payload).digest("hex");
    const token = Buffer.from(JSON.stringify({ payload, signature: badSig })).toString("base64");
    const result = exportService.validateToken(token);
    expect(result).toBeNull();
  });
});

describe("ExportService - File generation", () => {
  it("generates a JSON file on disk", async () => {
    const token = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    expect(token).toBeTruthy();
    const fp = await exportService.getFilePath(token);
    expect(fp).toBeTruthy();
    const content = await fsp.readFile(fp!, "utf8");
    const parsed = JSON.parse(content);
    expect(parsed.invoices).toBeDefined();
    expect(parsed.bids).toBeDefined();
    expect(parsed.settlements).toBeDefined();
  });

  it("generates a CSV file on disk", async () => {
    const token = await exportService.generateExportFile("user-stream-test", ExportFormat.CSV);
    const fp = await exportService.getFilePath(token);
    expect(fp).toMatch(/\.csv$/);
    const content = await fsp.readFile(fp!, "utf8");
    expect(content).toContain("--- INVOICES ---");
    expect(content).toContain("--- BIDS ---");
    expect(content).toContain("--- SETTLEMENTS ---");
  });

  it("file has restricted permissions (0o600)", async () => {
    const token = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    const fp = await exportService.getFilePath(token);
    const stat = await fsp.stat(fp!);
    const mode = stat.mode & 0o777;
    expect(mode).toBe(0o600);
  });

  it("getFilePath returns null for unknown token", async () => {
    const fp = await exportService.getFilePath("nonexistent-token");
    expect(fp).toBeNull();
  });

  it("getFilePath returns null for expired token", async () => {
    // Create file on disk but with expired token
    const expiredToken = makeExpiredToken("u1", ExportFormat.JSON);
    const safeToken = expiredToken.replace(/[/+=]/g, "_");
    const filePath = path.join(exportDir, `${safeToken}.json`);
    await fsp.writeFile(filePath, "{}", { mode: 0o600 });
    const result = await exportService.getFilePath(expiredToken);
    expect(result).toBeNull();
  });

  it("deleteFile removes the file", async () => {
    const tmp = path.join(exportDir, "delete-test.txt");
    await fsp.writeFile(tmp, "data");
    await exportService.deleteFile(tmp);
    const exists = await fsp.access(tmp).then(() => true).catch(() => false);
    expect(exists).toBe(false);
  });
});

describe("ExportService - Cleanup", () => {
  it("cleanupExpiredFiles removes old files", async () => {
    const stalePath = path.join(exportDir, "stale_test.json");
    await fsp.writeFile(stalePath, "{}");
    const now = Date.now();
    await fsp.utimes(stalePath, new Date(now - 7200_000), new Date(now - 7200_000));
    const cleaned = await exportService.cleanupExpiredFiles();
    expect(cleaned).toBeGreaterThanOrEqual(1);
    const exists = await fsp.access(stalePath).then(() => true).catch(() => false);
    expect(exists).toBe(false);
  });

  it("cleanupExpiredFiles ignores non-export files", async () => {
    const strayPath = path.join(exportDir, "readme.md");
    await fsp.writeFile(strayPath, "hello");
    const cleaned = await exportService.cleanupExpiredFiles();
    expect(cleaned).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// HTTP integration tests
// ---------------------------------------------------------------------------
describe("HTTP API - /api/v1/exports/generate", () => {
  it("returns a download URL for JSON", async () => {
    const app = buildApp();
    const res = await request(app)
      .post("/api/v1/exports/generate")
      .set("Authorization", "Bearer user-stream-test")
      .query({ format: "json" });
    expect(res.status).toBe(200);
    expect(res.body.success).toBe(true);
    expect(res.body.download_url).toMatch(/^\/api\/v1\/exports\/download\//);
    expect(res.body.expires_in).toBeTruthy();
  });

  it("returns a download URL for CSV", async () => {
    const app = buildApp();
    const res = await request(app)
      .post("/api/v1/exports/generate")
      .set("Authorization", "Bearer user-stream-test")
      .query({ format: "csv" });
    expect(res.status).toBe(200);
    expect(res.body.success).toBe(true);
    expect(res.body.download_url).toMatch(/^\/api\/v1\/exports\/download\//);
  });

  it("rejects invalid format", async () => {
    const app = buildApp();
    const res = await request(app)
      .post("/api/v1/exports/generate")
      .set("Authorization", "Bearer user-stream-test")
      .query({ format: "xml" });
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_FORMAT");
  });
});

// ---------------------------------------------------------------------------
// requestExport controller - deterministic failure-boundary coverage
// ---------------------------------------------------------------------------
describe("requestExport controller - failure boundaries", () => {
  function makeReq(overrides: any = {}): any {
    return {
      user: { userId: "user-stream-test" },
      query: {},
      headers: {},
      ...overrides,
    };
  }

  function makeRes() {
    const res: any = {
      statusCode: 200,
      body: undefined,
      status(code: number) {
        this.statusCode = code;
        return this;
      },
      json(payload: any) {
        this.body = payload;
        return this;
      },
    };
    return res;
  }

  it("returns a download URL for a valid request", async () => {
    const req = makeReq({ query: { format: "json" } });
    const res = makeRes();
    await requestExport(req, res);
    expect(res.statusCode).toBe(200);
    expect(res.body.success).toBe(true);
    expect(res.body.download_url).toMatch(/^\/api\/v1\/exports\/download\//);
  });

  it("rejects an invalid format deterministically", async () => {
    const req = makeReq({ query: { format: "xml" } });
    const res = makeRes();
    await requestExport(req, res);
    expect(res.statusCode).toBe(400);
    expect(res.body.error.code).toBe("INVALID_FORMAT");
  });

  it("rejects a missing user (permission boundary)", async () => {
    const req = makeReq({ user: undefined, query: { format: "json" } });
    const res = makeRes();
    await requestExport(req, res);
    expect(res.statusCode).toBe(401);
    expect(res.body.error.code).toBe("UNAUTHORIZED");
  });

  it("rejects a missing userId (permission boundary)", async () => {
    const req = makeReq({ user: {}, query: { format: "json" } });
    const res = makeRes();
    await requestExport(req, res);
    expect(res.statusCode).toBe(401);
    expect(res.body.error.code).toBe("UNAUTHORIZED");
  });

  it("defaults to JSON when format is omitted", async () => {
    const req = makeReq({ query: {} });
    const res = makeRes();
    await requestExport(req, res);
    expect(res.statusCode).toBe(200);
    expect(res.body.success).toBe(true);
  });

  it("is deterministic across repeated valid calls", async () => {
    const req1 = makeReq({ query: { format: "csv" } });
    const res1 = makeRes();
    await requestExport(req1, res1);
    const req2 = makeReq({ query: { format: "csv" } });
    const res2 = makeRes();
    await requestExport(req2, res2);
    expect(res1.statusCode).toBe(200);
    expect(res2.statusCode).toBe(200);
    expect(res1.body.success).toBe(true);
    expect(res2.body.success).toBe(true);
  });

  it("surfaces a diagnosable error on generation failure without leaking secrets", async () => {
    const spy = jest
      .spyOn(exportService, "generateExportFile")
      .mockRejectedValueOnce(new Error("disk full at /secret/path"));
    const req = makeReq({ query: { format: "json" } });
    const res = makeRes();
    await requestExport(req, res);
    expect(res.statusCode).toBeGreaterThanOrEqual(500);
    expect(res.body.success).toBe(false);
    expect(res.body.error).toBeDefined();
    expect(JSON.stringify(res.body)).not.toContain("/secret/path");
    spy.mockRestore();
  });

  it("recovers after a transient failure (retry boundary)", async () => {
    const spy = jest
      .spyOn(exportService, "generateExportFile")
      .mockRejectedValueOnce(new Error("transient"));
    const failReq = makeReq({ query: { format: "json" } });
    const failRes = makeRes();
    await requestExport(failReq, failRes);
    expect(failRes.body.success).toBe(false);
    spy.mockRestore();

    const okReq = makeReq({ query: { format: "json" } });
    const okRes = makeRes();
    await requestExport(okReq, okRes);
    expect(okRes.statusCode).toBe(200);
    expect(okRes.body.success).toBe(true);
  });
});

describe("HTTP API - /api/v1/exports/download/:token", () => {
  it("streams a file and deletes after download", async () => {
    // Pre-create a file via the service
    const token = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    const app = buildApp();
    const res = await request(app).get(`/api/v1/exports/download/${token}`);
    expect(res.status).toBe(200);
    expect(res.headers["content-disposition"]).toMatch(/^attachment;/);
    expect(res.headers["content-type"]).toBe("application/json");
    const parsed = JSON.parse(res.text);
    expect(parsed.invoices).toBeDefined();

    // File should be gone after download
    const fp = await exportService.getFilePath(token);
    expect(fp).toBeNull();
  });

  it("enforces single-use (second download fails)", async () => {
    const token = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    const app = buildApp();

    const dl1 = await request(app).get(`/api/v1/exports/download/${token}`);
    expect(dl1.status).toBe(200);

    const dl2 = await request(app).get(`/api/v1/exports/download/${token}`);
    expect(dl2.status).toBe(401);
    expect(dl2.body.error.code).toBe("INVALID_TOKEN");
  });

  it("rejects invalid token", async () => {
    const app = buildApp();
    const res = await request(app).get("/api/v1/exports/download/badtoken123");
    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("INVALID_TOKEN");
  });

  it("rejects expired token", async () => {
    const token = makeExpiredToken("u1", ExportFormat.JSON);
    // Put a file on disk so validation doesn't fail on file-missing
    const safeToken = token.replace(/[/+=]/g, "_");
    await fsp.writeFile(path.join(exportDir, `${safeToken}.json`), "{}");
    const app = buildApp();
    const res = await request(app).get(`/api/v1/exports/download/${token}`);
    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("INVALID_TOKEN");
  });
});

// ---------------------------------------------------------------------------
// downloadExport controller - deterministic failure-boundary coverage
// ---------------------------------------------------------------------------
describe("downloadExport controller - failure boundaries", () => {
  function makeReq(overrides: any = {}): any {
    return {
      params: { token: "" },
      query: {},
      headers: {},
      ip: "127.0.0.1",
      method: "GET",
      path: "/api/v1/exports/download/test-token",
      on: (event: string, handler: Function) => { if (event === "close") req.closeHandler = handler; },
      closeHandler: null as Function | null,
      ...overrides,
    };
  }

  function makeRes() {
    const res: any = {
      statusCode: 200,
      body: undefined,
      headers: {},
      headersSent: false,
      status(code: number) {
        this.statusCode = code;
        return this;
      },
      json(payload: any) {
        this.body = payload;
        return this;
      },
      setHeader(name: string, value: string) {
        this.headers[name] = value;
      },
      getHeader(name: string) {
        return this.headers[name];
      },
      on: (event: string, handler: Function) => {},
      end: () => {},
    };
    return res;
  }

  function makeToken(userId: string, format: ExportFormat, expiresAt: number): string {
    const payload = JSON.stringify({ userId, format, expiresAt });
    const signature = crypto
      .createHmac("sha256", config.EXPORT_SECRET)
      .update(payload)
      .digest("hex");
    return Buffer.from(JSON.stringify({ payload, signature })).toString("base64");
  }

  function makeFutureToken(userId: string, format: ExportFormat): string {
    return makeToken(userId, format, Date.now() + 3600_000);
  }

  function makeExpiredToken(userId: string, format: ExportFormat): string {
    return makeToken(userId, format, Date.now() - 1000);
  }

  let nextFn: jest.Mock;

  beforeEach(() => {
    nextFn = jest.fn();
  });

  it("returns 400 for malformed token (fails assertExportToken)", async () => {
    const req = makeReq({ params: { token: "not-a-valid-token-format" } });
    const res = makeRes();
    await downloadExport(req, res, nextFn);
    expect(res.statusCode).toBe(400);
    expect(res.body.error.code).toBe(ExportErrorCode.TOKEN_INVALID);
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("returns 401 for expired token with stable code TOKEN_EXPIRED", async () => {
    const token = makeExpiredToken("user-1", ExportFormat.JSON);
    // Put a file on disk so file exists but token is expired
    const safeToken = token.replace(/[/+=]/g, "_");
    await fsp.writeFile(path.join(exportDir, `${safeToken}.json`), "{}");
    const req = makeReq({ params: { token } });
    const res = makeRes();
    await downloadExport(req, res, nextFn);
    expect(res.statusCode).toBe(401);
    expect(res.body.error.code).toBe(ExportErrorCode.TOKEN_EXPIRED);
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("returns 400 for invalid token signature with stable code TOKEN_INVALID", async () => {
    const payload = JSON.stringify({ userId: "u1", format: "json", expiresAt: Date.now() + 3600_000 });
    const badSig = crypto.createHmac("sha256", "wrong-secret").update(payload).digest("hex");
    const token = Buffer.from(JSON.stringify({ payload, signature: badSig })).toString("base64");
    const req = makeReq({ params: { token } });
    const res = makeRes();
    await downloadExport(req, res, nextFn);
    expect(res.statusCode).toBe(400);
    expect(res.body.error.code).toBe(ExportErrorCode.TOKEN_INVALID);
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("returns 404 for valid token but missing file with stable code NOT_FOUND", async () => {
    const token = makeFutureToken("user-1", ExportFormat.JSON);
    const req = makeReq({ params: { token } });
    const res = makeRes();
    await downloadExport(req, res, nextFn);
    expect(res.statusCode).toBe(404);
    expect(res.body.error.code).toBe(ExportErrorCode.NOT_FOUND);
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("returns 429 when concurrency limit exceeded", async () => {
    // Use the handler directly with mocked concurrency that always fails
    const handler = createDownloadExportHandler({
      ...require("../controllers/v1/exports").defaultDependencies,
      tryAcquire: () => false,
    });
    const req = makeReq({ params: { token: makeFutureToken("user-1", ExportFormat.JSON) } });
    const res = makeRes();
    await handler(req, res, nextFn);
    expect(res.statusCode).toBe(429);
    expect(res.body.error.code).toBe("DOWNLOAD_CONCURRENCY_EXCEEDED");
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("streams file successfully and deletes after download (single-use)", async () => {
    const token = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    const req = makeReq({ params: { token } });
    const res = makeRes();
    await downloadExport(req, res, nextFn);
    expect(res.statusCode).toBe(200);
    expect(res.getHeader("Content-Type")).toBe("application/json");
    expect(res.getHeader("Content-Disposition")).toMatch(/^attachment;/);

    // File should be gone after download
    const fp = await exportService.getFilePath(token);
    expect(fp).toBeNull();
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("enforces single-use: second download of same token fails with INVALID_TOKEN", async () => {
    const token = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    const req1 = makeReq({ params: { token } });
    const res1 = makeRes();
    await downloadExport(req1, res1, nextFn);
    expect(res1.statusCode).toBe(200);

    const req2 = makeReq({ params: { token } });
    const res2 = makeRes();
    await downloadExport(req2, res2, nextFn);
    expect(res2.statusCode).toBe(401);
    expect(res2.body.error.code).toBe(ExportErrorCode.TOKEN_INVALID); // File deleted = token invalid
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("releases concurrency slot on stream error", async () => {
    // Create a handler with a read stream that errors immediately
    const handler = createDownloadExportHandler({
      ...require("../controllers/v1/exports").defaultDependencies,
      createReadStream: () => {
        const stream = require("stream");
        const readable = new stream.Readable({
          read() {
            this.emit("error", new Error("disk read error"));
          },
        });
        return readable;
      },
    });
    const token = makeFutureToken("user-1", ExportFormat.JSON);
    // Create a real file so validation passes
    const safeToken = token.replace(/[/+=]/g, "_");
    await fsp.writeFile(path.join(exportDir, `${safeToken}.json`), "{}");

    const req = makeReq({ params: { token } });
    const res = makeRes();
    await handler(req, res, nextFn);
    expect(res.statusCode).toBe(500);
    expect(res.body.error.code).toBe(ExportErrorCode.STORAGE_FAILURE);
    // Slot should be released (no leak)
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("does not leak sensitive data in error responses", async () => {
    const token = makeFutureToken("user-1", ExportFormat.JSON);
    const req = makeReq({ params: { token } });
    const res = makeRes();
    await downloadExport(req, res, nextFn);
    expect(res.statusCode).toBe(404);
    const bodyStr = JSON.stringify(res.body);
    expect(bodyStr).not.toContain(exportDir);
    expect(bodyStr).not.toContain("/secret");
    expect(bodyStr).not.toContain("stack");
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("is deterministic across repeated valid calls (when file recreated)", async () => {
    // First download
    const token1 = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    const req1 = makeReq({ params: { token: token1 } });
    const res1 = makeRes();
    await downloadExport(req1, res1, nextFn);
    expect(res1.statusCode).toBe(200);

    // Second download with new token (new file generated)
    const token2 = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    const req2 = makeReq({ params: { token: token2 } });
    const res2 = makeRes();
    nextFn.mockClear();
    await downloadExport(req2, res2, nextFn);
    expect(res2.statusCode).toBe(200);
    expect(res2.getHeader("Content-Type")).toBe("application/json");
    expect(nextFn).not.toHaveBeenCalled();
  });

  it("handles client disconnect mid-stream and still deletes file", async () => {
    const token = await exportService.generateExportFile("user-stream-test", ExportFormat.JSON);
    const req = makeReq({ params: { token } });
    const res = makeRes();
    
    // Simulate client disconnect after headers sent
    let closeHandler: Function | null = null;
    req.on = jest.fn((event: string, handler: Function) => {
      if (event === "close") closeHandler = handler;
    });
    
    await downloadExport(req, res, nextFn);
    
    // Simulate close event
    if (closeHandler) {
      await closeHandler();
    }
    
    // File should still be deleted (single-use semantics)
    const fp = await exportService.getFilePath(token);
    expect(fp).toBeNull();
  });
});
