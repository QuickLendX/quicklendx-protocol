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

import { exportService, ExportFormat } from "../services/exportService";
import { config } from "../config";

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
