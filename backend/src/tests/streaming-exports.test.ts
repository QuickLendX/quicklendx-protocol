import request from "supertest";
import express from "express";
import fs from "fs";
import fsp from "fs/promises";
import path from "path";
import os from "os";
import crypto from "crypto";
import { Readable, Writable } from "stream";
import { finished } from "stream/promises";
import { EventEmitter } from "events";

// Keep filesystem state and environment local to this suite. Only storage and
// audit dependencies are mocked; serialization, signing and downloads are real.
const exportDir = fs.mkdtempSync(path.join(os.tmpdir(), "qlx-streaming-"));
const savedEnv = { ...process.env };
process.env.EXPORT_DIR = exportDir;
process.env.EXPORT_SECRET = "streaming-tests-only-signing-secret-32-chars";
process.env.NODE_ENV = "test";

jest.mock("../services/invoiceStore", () => ({
  invoiceStore: { findInvoices: jest.fn() },
}));
jest.mock("../controllers/v1/invoices", () => ({ MOCK_INVOICES: [] }));
jest.mock("../controllers/v1/bids", () => ({ MOCK_BIDS: [] }));
jest.mock("../controllers/v1/settlements", () => ({ MOCK_SETTLEMENTS: [] }));
jest.mock("../services/auditLogService", () => ({
  auditLogService: { recordAuthorization: jest.fn(), recordAdminAction: jest.fn() },
}));

import { exportService, ExportFormat, ExportErrorCode } from "../services/exportService";
import { requestExport, downloadExport, createDownloadExportHandler, DownloadExportDependencies } from "../controllers/v1/exports";
import { requireUserAuth, getUser } from "../middleware/userAuth";
import { exportConcurrencyService } from "../services/exportConcurrency";
import { invoiceStore } from "../services/invoiceStore";
import { MOCK_BIDS } from "../controllers/v1/bids";
import { MOCK_SETTLEMENTS } from "../controllers/v1/settlements";

const userId = "stream-user";
const invoice = { id: "inv-1", business: userId, amount: "123", currency: "USD", status: "pending", due_date: 1700000000 };

function buildApp() {
  const app = express();
  app.post("/api/v1/exports/generate", requireUserAuth, requestExport);
  app.get("/api/v1/exports/download/:token", downloadExport);
  app.use((err: Error, _req: express.Request, res: express.Response, _next: express.NextFunction) => {
    res.status(500).json({ error: { message: err.message } });
  });
  return app;
}

function signedToken(expiresAt: number, secret = process.env.EXPORT_SECRET!) {
  const payload = JSON.stringify({ userId, format: ExportFormat.JSON, expiresAt });
  const signature = crypto.createHmac("sha256", secret).update(payload).digest("hex");
  return Buffer.from(JSON.stringify({ payload, signature })).toString("base64");
}

async function waitForDeletion(token: string) {
  // The controller's end listener performs asynchronous best-effort deletion.
  for (let i = 0; i < 100; i++) {
    if (await exportService.getFilePath(token) === null) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("Export file was not deleted after stream completion");
}

function downloadDependencies(overrides: Partial<DownloadExportDependencies> = {}): DownloadExportDependencies {
  return {
    getUser,
    validateTokenStrict: (token) => exportService.validateTokenStrict(token),
    getFilePathStrict: (token) => exportService.getFilePathStrict(token),
    deleteFile: (file) => exportService.deleteFile(file),
    tryAcquire: jest.fn(() => true),
    release: jest.fn(),
    recordAudit: jest.fn(),
    createReadStream: (file) => fs.createReadStream(file),
    ...overrides,
  };
}

function streamResponse(write: (chunk: Buffer, done: (error?: Error | null) => void) => void) {
  const response = new Writable({ highWaterMark: 128, write(chunk, _encoding, done) { write(chunk, done); } }) as any;
  response.headers = {};
  response.setHeader = (name: string, value: string) => { response.headers[name] = value; };
  response.status = jest.fn(() => response);
  response.json = jest.fn(() => response);
  return response;
}

beforeEach(() => {
  exportConcurrencyService.reset();
  exportService.__resetInFlightForTests();
  jest.mocked(invoiceStore.findInvoices).mockImplementation(({ business } = {}) => business === userId ? [{ ...invoice }] as any : []);
  MOCK_BIDS.splice(0, MOCK_BIDS.length, { bid_id: "bid-1", investor: userId, invoice_id: invoice.id, bid_amount: "100", status: "active", timestamp: 1700000000 } as any);
  MOCK_SETTLEMENTS.splice(0, MOCK_SETTLEMENTS.length, { id: "stl-1", payer: userId, recipient: "other", invoice_id: invoice.id, amount: "100", status: "paid", timestamp: 1700000000 } as any);
});

afterEach(async () => {
  jest.restoreAllMocks();
  exportConcurrencyService.reset();
  exportService.__resetInFlightForTests();
  for (const file of await fsp.readdir(exportDir)) await fsp.unlink(path.join(exportDir, file));
});

afterAll(async () => {
  await fsp.rm(exportDir, { recursive: true, force: true });
  for (const key of ["EXPORT_DIR", "EXPORT_SECRET", "NODE_ENV"]) {
    if (savedEnv[key] === undefined) delete process.env[key];
    else process.env[key] = savedEnv[key];
  }
});

describe("signed export generation and download", () => {
  it.each([ExportFormat.JSON, ExportFormat.CSV])("generates and downloads populated %s data with single-use cleanup", async (format) => {
    const app = buildApp();
    const generated = await request(app).post("/api/v1/exports/generate").set("Authorization", `Bearer ${userId}`).query({ format }).expect(200);
    expect(generated.body.success).toBe(true);
    expect(generated.body.expires_in).toBeTruthy();
    const token = generated.body.download_url.split("/download/")[1];
    expect(exportService.validateTokenStrict(token)).toMatchObject({ userId, format });
    const file = await exportService.getFilePathStrict(token);
    const expected = await fsp.readFile(file, "utf8");
    const response = await request(app).get(`/api/v1/exports/download/${encodeURIComponent(token)}`).expect(200);
    expect(response.headers["content-disposition"]).toContain(`.${format}`);
    expect(response.headers["content-type"]).toContain(format === ExportFormat.JSON ? "application/json" : "text/csv");
    expect(response.text).toBe(expected);
    if (format === ExportFormat.JSON) {
      expect(response.body.invoices).toEqual([{ id: invoice.id, amount: "123", currency: "USD", status: "pending", due_date: 1700000000 }]);
      expect(response.body.bids[0].bid_id).toBe("bid-1");
      expect(response.body.settlements[0].id).toBe("stl-1");
      expect(response.body.invoices[0]).not.toHaveProperty("business");
    } else {
      expect(response.text).toContain("--- INVOICES ---\nid,amount,currency,status,due_date\ninv-1,123,USD,pending,2023-11-14T22:13:20.000Z");
      expect(response.text).toContain("--- BIDS ---");
      expect(response.text).toContain("--- SETTLEMENTS ---");
    }
    await waitForDeletion(token);
    const repeated = await request(app).get(`/api/v1/exports/download/${encodeURIComponent(token)}`).expect(404);
    expect(repeated.body.error.code).toBe(ExportErrorCode.NOT_FOUND);
    expect(exportConcurrencyService.getActiveCount(userId)).toBe(0);
  });

  it("defaults to JSON and does not include another user's data", async () => {
    const response = await request(buildApp()).post("/api/v1/exports/generate").set("Authorization", "Bearer different-user").expect(200);
    const token = response.body.download_url.split("/download/")[1];
    const file = await exportService.getFilePathStrict(token);
    expect(JSON.parse(await fsp.readFile(file, "utf8"))).toEqual({ invoices: [], bids: [], settlements: [] });
  });

  it.each([undefined, "Bearer "])("rejects missing authentication (%s)", async (authorization) => {
    const req = request(buildApp()).post("/api/v1/exports/generate");
    if (authorization) req.set("Authorization", authorization);
    const response = await req.expect(401);
    expect(response.body.error.code).toBe("UNAUTHORIZED");
  });

  it("rejects unsupported formats", async () => {
    const response = await request(buildApp()).post("/api/v1/exports/generate").set("Authorization", `Bearer ${userId}`).query({ format: "ndjson" }).expect(400);
    expect(response.body.error.code).toBe("INVALID_FORMAT");
  });

  it("reports generation failure without leaking details, releases its slot, and permits retry", async () => {
    jest.spyOn(console, "error").mockImplementation(() => {});
    jest.spyOn(exportService, "generateExportFile").mockRejectedValueOnce(new Error("disk full at /secret/path"));
    const app = buildApp();
    const response = await request(app).post("/api/v1/exports/generate").set("Authorization", `Bearer ${userId}`).expect(500);
    expect(response.body.error.code).toBe("EXPORT_GENERATION_FAILED");
    expect(response.text).not.toContain("/secret/path");
    expect(exportConcurrencyService.getActiveCount(userId)).toBe(0);
    await request(app).post("/api/v1/exports/generate").set("Authorization", `Bearer ${userId}`).expect(200);
  });

  it.each([
    ["malformed", () => "invalid-token", 400, ExportErrorCode.TOKEN_INVALID],
    ["bad signature", () => signedToken(Date.now() + 60000, "wrong-secret"), 400, ExportErrorCode.TOKEN_INVALID],
    ["expired", () => signedToken(Date.now() - 1000), 401, ExportErrorCode.TOKEN_EXPIRED],
    ["missing file", () => signedToken(Date.now() + 60000), 404, ExportErrorCode.NOT_FOUND],
  ] as const)("rejects %s tokens with stable errors", async (_name, makeToken, status, code) => {
    const response = await request(buildApp()).get(`/api/v1/exports/download/${encodeURIComponent(makeToken())}`).expect(status);
    expect(response.body.error.code).toBe(code);
    expect(response.text).not.toContain(exportDir);
    expect(exportConcurrencyService.getActiveCount(userId)).toBe(0);
  });
});

describe("file lifecycle", () => {
  it("creates private files on platforms supporting POSIX modes", async () => {
    const create = jest.spyOn(fs, "createWriteStream");
    const chmod = jest.spyOn(fsp, "chmod");
    const token = await exportService.generateExportFile(userId, ExportFormat.JSON);
    const file = await exportService.getFilePathStrict(token);
    expect(create).toHaveBeenCalledWith(`${file}.tmp`, { mode: 0o600 });
    expect(chmod).toHaveBeenCalledWith(file, 0o600);
    if (process.platform !== "win32") expect((await fsp.stat(file)).mode & 0o777).toBe(0o600);
  });

  it("removes expired exports while retaining fresh exports and unrelated files", async () => {
    const stale = path.join(exportDir, "stale.json");
    await fsp.writeFile(stale, "{}");
    await fsp.utimes(stale, new Date(0), new Date(0));
    const token = await exportService.generateExportFile(userId, ExportFormat.CSV);
    await fsp.writeFile(path.join(exportDir, "readme.md"), "keep");
    expect(await exportService.cleanupExpiredFiles()).toBe(1);
    expect(await exportService.getFilePath(token)).not.toBeNull();
    expect(await fsp.readFile(path.join(exportDir, "readme.md"), "utf8")).toBe("keep");
    expect(await exportService.getFilePath("invalid")).toBeNull();
  });
});

describe("bounded streaming and backpressure", () => {
  it.each([ExportFormat.JSON, ExportFormat.CSV])("bounds the output buffer while generating large %s exports", async (format) => {
    const rows = Array.from({ length: 10000 }, (_, i) => ({ ...invoice, id: `inv-${i}` }));
    jest.mocked(invoiceStore.findInvoices).mockReturnValue(rows as any);
    const originalCreate = fs.createWriteStream.bind(fs);
    let peakBufferedBytes = 0;
    let backpressureSignals = 0;
    jest.spyOn(fs, "createWriteStream").mockImplementation((file, options) => {
      const stream = originalCreate(file, { ...(options as object), highWaterMark: 128 });
      const originalWrite = stream.write.bind(stream);
      jest.spyOn(stream, "write").mockImplementation((...args: any[]) => {
        const accepted = (originalWrite as any)(...args);
        peakBufferedBytes = Math.max(peakBufferedBytes, stream.writableLength);
        if (!accepted) backpressureSignals++;
        return accepted;
      });
      return stream;
    });
    const token = await exportService.generateExportFile(userId, format);
    const file = await exportService.getFilePathStrict(token);
    expect((await fsp.stat(file)).size).toBeGreaterThan(400000);
    expect(backpressureSignals).toBeGreaterThan(0);
    // A deterministic memory bound: queued output stays within one small
    // stream buffer plus a row, rather than retaining the entire export.
    expect(peakBufferedBytes).toBeLessThan(1024);
    const output = await fsp.readFile(file, "utf8");
    if (format === ExportFormat.JSON) expect(JSON.parse(output).invoices).toHaveLength(rows.length);
    else expect(output.match(/^inv-\d+,/gm)).toHaveLength(rows.length);
    expect(await fsp.readdir(exportDir)).not.toContain(path.basename(file) + ".tmp");
  }, 60000);

  it("pauses a download for a slow consumer and releases its slot exactly once", async () => {
    const token = await exportService.generateExportFile(userId, ExportFormat.JSON);
    const file = await exportService.getFilePathStrict(token);
    const bytes = Buffer.alloc(256 * 1024, "x");
    await fsp.writeFile(file, bytes);
    const source = fs.createReadStream(file, { highWaterMark: 1024 });
    const pause = jest.spyOn(source, "pause");
    const chunks: Buffer[] = [];
    let peakBufferedBytes = 0;
    const response = streamResponse((chunk, done) => {
      chunks.push(Buffer.from(chunk));
      peakBufferedBytes = Math.max(peakBufferedBytes, source.readableLength);
      setImmediate(done);
    });
    const done = finished(response);
    const dependencies = downloadDependencies({ createReadStream: () => source });
    const req = Object.assign(new EventEmitter(), { params: { token }, method: "GET", path: "/download", ip: "127.0.0.1" });
    const next = jest.fn();
    await createDownloadExportHandler(dependencies)(req as any, response, next);
    await done;
    await waitForDeletion(token);
    expect(Buffer.concat(chunks)).toEqual(bytes);
    expect(pause).toHaveBeenCalled();
    expect(peakBufferedBytes).toBeLessThanOrEqual(2048);
    expect(dependencies.release).toHaveBeenCalledTimes(1);
    expect(dependencies.release).toHaveBeenCalledWith(userId);
    expect(next).not.toHaveBeenCalled();
  });
});

describe("download failure boundaries", () => {
  it("rejects concurrency overflow before opening a file", async () => {
    const dependencies = downloadDependencies({ tryAcquire: jest.fn(() => false), createReadStream: jest.fn() });
    const response = streamResponse((_chunk, done) => done());
    const next = jest.fn();
    await createDownloadExportHandler(dependencies)({ params: { token: signedToken(Date.now() + 60000) } } as any, response, next);
    expect(response.status).toHaveBeenCalledWith(429);
    expect(dependencies.createReadStream).not.toHaveBeenCalled();
    expect(dependencies.release).not.toHaveBeenCalled();
    expect(next).not.toHaveBeenCalled();
    response.destroy();
  });

  it.each(["error", "disconnect"])("releases the download slot on %s", async (failure) => {
    jest.spyOn(console, "error").mockImplementation(() => {});
    const token = await exportService.generateExportFile(userId, ExportFormat.JSON);
    const source = new Readable({ read() {} });
    const response = streamResponse((_chunk, done) => done());
    const dependencies = downloadDependencies({ createReadStream: () => source as fs.ReadStream });
    const req = Object.assign(new EventEmitter(), { params: { token } });
    const next = jest.fn();
    try {
      await createDownloadExportHandler(dependencies)(req as any, response, next);
      if (failure === "error") source.emit("error", new Error("read failed at /secret/path"));
      else {
        req.emit("close");
        await waitForDeletion(token);
      }
      expect(dependencies.release).toHaveBeenCalledTimes(1);
      expect(JSON.stringify(response.json.mock.calls)).not.toContain("/secret/path");
      expect(next).not.toHaveBeenCalled();
    } finally {
      source.destroy();
      response.destroy();
    }
  });
});

describe("generation storage failures", () => {
  it("cleans up after a backpressured write fails and allows retry", async () => {
    const failed = new Writable({
      highWaterMark: 1,
      write(_chunk, _encoding, done) { setImmediate(() => done(new Error("disk failure /secret/path"))); },
    });
    jest.spyOn(fs, "createWriteStream").mockReturnValueOnce(failed as fs.WriteStream);
    await expect(exportService.generateExportFile(userId, ExportFormat.JSON)).rejects.toMatchObject({
      code: ExportErrorCode.STORAGE_FAILURE,
      message: "Failed to write export file",
      retryable: true,
    });
    expect(failed.destroyed).toBe(true);
    expect(await fsp.readdir(exportDir)).toEqual([]);
    const token = await exportService.generateExportFile(userId, ExportFormat.JSON);
    expect(await exportService.getFilePath(token)).not.toBeNull();
  });
});
