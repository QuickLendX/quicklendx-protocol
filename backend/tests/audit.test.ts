import { describe, expect, it, beforeEach, afterEach, afterAll } from "@jest/globals";
import request from "supertest";
import * as fs from "fs";
import * as path from "path";
import os from "os";
import { AuditEntry } from "../src/types/audit";
import { AuditService } from "../src/services/auditService";
import { createApp } from "../src/index";
import { resetApiKeys } from "../src/middleware/apiKeyAuth";
import { redactSensitiveFields, AuditEntrySchema } from "../src/types/audit";

let auditService: AuditService;
let testApp: ReturnType<typeof createApp>;

const TEST_AUDIT_DIR = path.join(os.tmpdir(), `audit-test-${process.pid}`);
const TEST_KEY = "test-key-001";
const TEST_KEY_2 = "test-key-002";
const TEST_ACTOR = "test-admin";
const TEST_ACTOR_2 = "test-admin-2";

function setEnv() {
  process.env.SKIP_API_KEY_AUTH = "true";
  process.env.TEST_ACTOR = TEST_ACTOR;
  process.env.AUDIT_DIR = TEST_AUDIT_DIR;
}

function clearEnv() {
  delete process.env.SKIP_API_KEY_AUTH;
  delete process.env.TEST_ACTOR;
}

function setupAuditDir() {
  if (!fs.existsSync(TEST_AUDIT_DIR)) {
    fs.mkdirSync(TEST_AUDIT_DIR, { recursive: true });
  }
}

function teardownAuditDir() {
  if (fs.existsSync(TEST_AUDIT_DIR)) {
    for (const file of fs.readdirSync(TEST_AUDIT_DIR)) {
      if (file.startsWith("audit-") && file.endsWith(".jsonl")) {
        fs.unlinkSync(path.join(TEST_AUDIT_DIR, file));
      }
    }
    fs.rmdirSync(TEST_AUDIT_DIR);
  }
}

describe("Audit Log — append-only, redaction, actor identity, query", () => {
  beforeEach(() => {
    setEnv();
    setupAuditDir();
    resetApiKeys();
    AuditService.resetInstance();
    auditService = AuditService.getInstance();
    auditService.setAuditDir(TEST_AUDIT_DIR);
    testApp = createApp();
  });

  afterEach(() => {
    teardownAuditDir();
    clearEnv();
  });

  afterAll(() => {
    teardownAuditDir();
    clearEnv();
  });

  describe("Redaction", () => {
    it("should redact sensitive fields in redactedParams", () => {
      const input = {
        secret: "super-secret-key",
        apiKey: "ak-live-xxx",
        token: "bearer-token-xxx",
        enabled: true,
        actor: "john-doe",
      };
      const redacted = redactSensitiveFields(input);
      expect(redacted["secret"]).toBe("[REDACTED]");
      expect(redacted["apiKey"]).toBe("[REDACTED]");
      expect(redacted["token"]).toBe("[REDACTED]");
      expect(redacted["enabled"]).toBe(true);
      expect(redacted["actor"]).toBe("john-doe");
    });

    it("should redact nested sensitive fields", () => {
      const input = {
        config: {
          password: "hunter2",
          privateKey: "0xabcdef",
        },
        enabled: true,
      };
      const redacted = redactSensitiveFields(input);
      expect(
        (redacted["config"] as Record<string, unknown>)["password"]
      ).toBe("[REDACTED]");
      expect(
        (redacted["config"] as Record<string, unknown>)["privateKey"]
      ).toBe("[REDACTED]");
    });

    it("should handle mixed-case sensitive field names", () => {
      const input = { SECRET: "value", ApiKey: "value", TOKEN: "value" };
      const redacted = redactSensitiveFields(input);
      expect(redacted["SECRET"]).toBe("[REDACTED]");
      expect(redacted["ApiKey"]).toBe("[REDACTED]");
      expect(redacted["TOKEN"]).toBe("[REDACTED]");
    });
  });

  describe("Append-only enforcement", () => {
    it("should only append entries, never modify or delete", () => {
      const e1 = auditService.append({
        actor: TEST_ACTOR,
        operation: "MAINTENANCE_MODE",
        params: { enabled: true },
        redactedParams: { enabled: true },
        ip: "1.2.3.4",
        userAgent: "test",
        effect: "Maintenance mode set to true",
        success: true,
      });

      const entries = auditService.getEntriesForTest();
      expect(entries).toHaveLength(1);
      expect(entries[0].id).toBe(e1.id);
      expect(entries[0].actor).toBe(TEST_ACTOR);

      const e2 = auditService.append({
        actor: TEST_ACTOR,
        operation: "CONFIG_CHANGE",
        params: { key: "fee_bps", value: 25 },
        redactedParams: { key: "fee_bps", value: 25 },
        ip: "1.2.3.4",
        userAgent: "test",
        effect: 'Config updated: "fee_bps" = 25',
        success: true,
      });

      const allEntries = auditService.getEntriesForTest();
      expect(allEntries).toHaveLength(2);
      expect(allEntries[0].id).toBe(e1.id);
      expect(allEntries[1].id).toBe(e2.id);
    });

    it("should fail when trying to overwrite an entry", () => {
      const filePath = path.join(TEST_AUDIT_DIR, "audit-2026-04-25.jsonl");
      fs.appendFileSync(
        filePath,
        JSON.stringify({
          id: "01ARZ3NDEKTSV4RRFFQ69G5FAV",
          timestamp: "2026-04-25T00:00:00.000Z",
          actor: TEST_ACTOR,
          operation: "MAINTENANCE_MODE",
          params: { enabled: true },
          redactedParams: { enabled: true },
          ip: "1.2.3.4",
          userAgent: "test",
          effect: "Maintenance mode set to true",
          success: true,
        }) + "\n"
      );

      const entries = auditService.getEntriesForTest();
      expect(entries).toHaveLength(1);

      auditService.append({
        actor: TEST_ACTOR,
        operation: "CONFIG_CHANGE",
        params: { key: "test", value: 1 },
        redactedParams: { key: "test", value: 1 },
        ip: "1.2.3.4",
        userAgent: "test",
        effect: "test",
        success: true,
      });

      const linesAfter = fs
        .readFileSync(filePath, "utf8")
        .split("\n")
        .filter(Boolean);
      expect(linesAfter).toHaveLength(2);
    });

    it("should create daily log files", () => {
      const today = new Date().toISOString().slice(0, 10);
      auditService.append({
        actor: TEST_ACTOR,
        operation: "MAINTENANCE_MODE",
        params: { enabled: false },
        redactedParams: { enabled: false },
        ip: "127.0.0.1",
        userAgent: "test",
        effect: "Maintenance mode set to false",
        success: true,
      });

      const logFile = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
      expect(fs.existsSync(logFile)).toBe(true);
      const lines = fs.readFileSync(logFile, "utf8").split("\n").filter(Boolean);
      expect(lines).toHaveLength(1);
    });

    it("should reject entries larger than 10KB", () => {
      const hugeParams: Record<string, string> = {};
      for (let i = 0; i < 5000; i++) {
        hugeParams[`key${i}`] = "x".repeat(100);
      }
      expect(() =>
        auditService.append({
          actor: TEST_ACTOR,
          operation: "CONFIG_CHANGE",
          params: hugeParams,
          redactedParams: hugeParams,
          ip: "1.2.3.4",
          userAgent: "test",
          effect: "huge entry",
          success: true,
        })
      ).toThrow(/exceeds maximum size/);
    });

    it("should produce valid JSONL parseable line-by-line", () => {
      auditService.append({
        actor: TEST_ACTOR,
        operation: "WEBHOOK_SECRET_ROTATE",
        params: { keyId: "webhook-key-1", secret: "super-secret" },
        redactedParams: { keyId: "webhook-key-1", secret: "[REDACTED]" },
        ip: "10.0.0.1",
        userAgent: "curl/7.68.0",
        effect: "Webhook secret rotated for keyId: webhook-key-1",
        success: true,
      });

      const today = new Date().toISOString().slice(0, 10);
      const logFile = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
      const lines = fs.readFileSync(logFile, "utf8").split("\n").filter(Boolean);
      expect(lines).toHaveLength(1);

      const parsed = JSON.parse(lines[0]);
      expect(() => AuditEntrySchema.parse(parsed)).not.toThrow();
      expect(parsed["redactedParams"]["secret"]).toBe("[REDACTED]");
      expect(parsed["params"]["secret"]).toBe("super-secret");
    });
  });

  describe("Actor identity", () => {
    it("should use actor from API key map", async () => {
      process.env.ADMIN_API_KEYS = `${TEST_KEY}:${TEST_ACTOR},${TEST_KEY_2}:${TEST_ACTOR_2}`;
      delete process.env.SKIP_API_KEY_AUTH;
      resetApiKeys();

      await request(testApp)
        .post("/api/v1/admin/maintenance")
        .set("X-API-Key", TEST_KEY)
        .send({ enabled: true });

      const entries = auditService.getEntriesForTest();
      const lastEntry = entries[entries.length - 1];
      expect(lastEntry.actor).toBe(TEST_ACTOR);
    });

    it("should reject requests with missing API key", async () => {
      delete process.env.SKIP_API_KEY_AUTH;
      resetApiKeys();

      const res = await request(testApp)
        .post("/api/v1/admin/maintenance")
        .send({ enabled: true });
      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe("UNAUTHORIZED");
    });

    it("should reject requests with invalid API key", async () => {
      delete process.env.SKIP_API_KEY_AUTH;
      resetApiKeys();
      process.env.ADMIN_API_KEYS = `${TEST_KEY}:${TEST_ACTOR}`;

      const res = await request(testApp)
        .post("/api/v1/admin/maintenance")
        .set("X-API-Key", "invalid-key")
        .send({ enabled: true });
      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe("UNAUTHORIZED");
    });

    it("should record different actors separately", () => {
      auditService.append({
        actor: TEST_ACTOR,
        operation: "MAINTENANCE_MODE",
        params: {},
        redactedParams: {},
        ip: "1.1.1.1",
        userAgent: "test",
        effect: "actor-1 action",
        success: true,
      });
      auditService.append({
        actor: TEST_ACTOR_2,
        operation: "CONFIG_CHANGE",
        params: {},
        redactedParams: {},
        ip: "2.2.2.2",
        userAgent: "test",
        effect: "actor-2 action",
        success: true,
      });

      const all = auditService.getEntriesForTest();
      expect(all.some((e: AuditEntry) => e.actor === TEST_ACTOR)).toBe(true);
      expect(all.some((e: AuditEntry) => e.actor === TEST_ACTOR_2)).toBe(true);
    });
  });

  describe("Query endpoint", () => {
    beforeEach(() => {
      for (let i = 0; i < 5; i++) {
        auditService.append({
          actor: TEST_ACTOR,
          operation: "MAINTENANCE_MODE",
          params: { enabled: i % 2 === 0 },
          redactedParams: { enabled: i % 2 === 0 },
          ip: "1.2.3.4",
          userAgent: "test",
          effect: `Maintenance mode toggle ${i}`,
          success: true,
        });
      }
      auditService.append({
        actor: TEST_ACTOR_2,
        operation: "CONFIG_CHANGE",
        params: { key: "fee", value: 10 },
        redactedParams: { key: "fee", value: 10 },
        ip: "5.6.7.8",
        userAgent: "test",
        effect: "Config change by actor-2",
        success: true,
      });
    });

    it("should return all entries by default", async () => {
      const res = await request(testApp).get("/api/v1/admin/audit");
      expect(res.status).toBe(200);
      expect(res.body.entries).toHaveLength(6);
      expect(res.body.total).toBe(6);
      expect(res.body.limit).toBe(100);
      expect(res.body.offset).toBe(0);
      expect(res.body.hasMore).toBe(false);
    });

    it("should filter by actor", async () => {
      const res = await request(testApp).get(
        `/api/v1/admin/audit?actor=${TEST_ACTOR}`
      );
      expect(res.status).toBe(200);
      expect(
        res.body.entries.every((e: AuditEntry) => e.actor === TEST_ACTOR)
      ).toBe(true);
      expect(res.body.total).toBe(5);
    });

    it("should filter by operation", async () => {
      const res = await request(testApp).get(
        "/api/v1/admin/audit?operation=CONFIG_CHANGE"
      );
      expect(res.status).toBe(200);
      expect(
        res.body.entries.every(
          (e: AuditEntry) => e.operation === "CONFIG_CHANGE"
        )
      ).toBe(true);
      expect(res.body.total).toBe(1);
    });

    it("should paginate with limit and offset", async () => {
      const res = await request(testApp).get(
        "/api/v1/admin/audit?limit=2&offset=0"
      );
      expect(res.status).toBe(200);
      expect(res.body.entries).toHaveLength(2);
      expect(res.body.hasMore).toBe(true);
    });

    it("should validate operation enum on query", async () => {
      const res = await request(testApp).get(
        "/api/v1/admin/audit?operation=INVALID_OP"
      );
      expect(res.status).toBe(400);
    });

    it("should list available operations", async () => {
      const res = await request(testApp).get(
        "/api/v1/admin/audit/operations"
      );
      expect(res.status).toBe(200);
      expect(res.body.operations).toContain("MAINTENANCE_MODE");
      expect(res.body.operations).toContain("WEBHOOK_SECRET_ROTATE");
      expect(res.body.operations).toContain("CONFIG_CHANGE");
      expect(res.body.operations).toContain("BACKFILL_START");
    });

    it("should require auth on audit read endpoint", async () => {
      delete process.env.SKIP_API_KEY_AUTH;
      resetApiKeys();

      const res = await request(testApp).get("/api/v1/admin/audit");
      expect(res.status).toBe(401);
    });

    it("should record IP and user agent in audit entries", async () => {
      await request(testApp)
        .post("/api/v1/admin/maintenance")
        .set("User-Agent", "Jest-Test/1.0")
        .set("X-Forwarded-For", "8.8.8.8")
        .send({ enabled: false });

      const entries = auditService.getEntriesForTest();
      const last = entries[entries.length - 1];
      expect(last.ip).toMatch(/8\.8\.8\.8|::ffff:8\.8\.8\.8|127\.0\.0\.1/);
      expect(last.userAgent).toBe("Jest-Test/1.0");
    });

    it("should record success/failure status", () => {
      auditService.append({
        actor: TEST_ACTOR,
        operation: "BACKFILL_START",
        params: { entity: "invoices" },
        redactedParams: { entity: "invoices" },
        ip: "1.2.3.4",
        userAgent: "test",
        effect: "Backfill started",
        success: false,
        errorMessage: "Ledger out of range",
      });

      const entries = auditService.getEntriesForTest();
      const last = entries[entries.length - 1];
      expect(last.success).toBe(false);
      expect(last.errorMessage).toBe("Ledger out of range");
    });
  });

  describe("Tamper resistance", () => {
    it("should preserve tampered values when file is modified", () => {
      auditService.append({
        actor: TEST_ACTOR,
        operation: "MAINTENANCE_MODE",
        params: { enabled: true },
        redactedParams: { enabled: true },
        ip: "1.2.3.4",
        userAgent: "test",
        effect: "Initial entry",
        success: true,
      });

      const today = new Date().toISOString().slice(0, 10);
      const logFile = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
      const content = fs.readFileSync(logFile, "utf8");
      const originalLine = content.trim().split("\n")[0];
      const parsed = JSON.parse(originalLine);

      parsed["actor"] = "hacker";
      parsed["effect"] = "Tampered entry";
      const tampered = content.replace(originalLine, JSON.stringify(parsed));
      fs.writeFileSync(logFile, tampered);

      const entries = auditService.getEntriesForTest();
      expect(entries[0].actor).toBe("hacker");
      expect(entries[0].id).toBe(parsed["id"]);
      expect(entries[0].timestamp).toBe(parsed["timestamp"]);
    });

    it("should return empty when file is emptied", () => {
      auditService.append({
        actor: TEST_ACTOR,
        operation: "MAINTENANCE_MODE",
        params: { enabled: true },
        redactedParams: { enabled: true },
        ip: "1.2.3.4",
        userAgent: "test",
        effect: "Single entry",
        success: true,
      });

      const today = new Date().toISOString().slice(0, 10);
      const logFile = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
      fs.writeFileSync(logFile, "");

      const entries = auditService.getEntriesForTest();
      expect(entries).toHaveLength(0);
    });
  });
});

describe("AuditService — singleton isolation", () => {
  beforeEach(() => {
    setEnv();
    setupAuditDir();
    AuditService.resetInstance();
    auditService = AuditService.getInstance();
    auditService.setAuditDir(TEST_AUDIT_DIR);
    testApp = createApp();
  });

  afterEach(() => {
    teardownAuditDir();
    clearEnv();
  });

  it("should enforce 10KB entry limit via service", () => {
    const bigParams: Record<string, string> = {};
    for (let i = 0; i < 200; i++) {
      bigParams[`key${i}`] = "v".repeat(500);
    }
    expect(() =>
      auditService.append({
        actor: TEST_ACTOR,
        operation: "CONFIG_CHANGE",
        params: bigParams,
        redactedParams: bigParams,
        ip: "1.2.3.4",
        userAgent: "test",
        effect: "big",
        success: true,
      })
    ).toThrow(/exceeds maximum size/);
  });

  it("should return empty array when no log file exists", () => {
    const entries = auditService.getEntriesForTest();
    expect(entries).toHaveLength(0);
  });

  it("should query with date range", () => {
    auditService.append({
      actor: TEST_ACTOR,
      operation: "MAINTENANCE_MODE",
      params: {},
      redactedParams: {},
      ip: "1.2.3.4",
      userAgent: "test",
      effect: "test",
      success: true,
    });
    const from = new Date(Date.now() - 1000).toISOString();
    const result = auditService.query({ from });
    expect(result.entries.length).toBeGreaterThanOrEqual(1);
  });

  it("should produce sorted entries (newest first)", () => {
    auditService.append({
      actor: TEST_ACTOR,
      operation: "MAINTENANCE_MODE",
      params: { enabled: false },
      redactedParams: { enabled: false },
      ip: "1.2.3.4",
      userAgent: "test",
      effect: "first",
      success: true,
    });
    auditService.append({
      actor: TEST_ACTOR,
      operation: "CONFIG_CHANGE",
      params: { key: "a", value: 1 },
      redactedParams: { key: "a", value: 1 },
      ip: "1.2.3.4",
      userAgent: "test",
      effect: "second",
      success: true,
    });

    const result = auditService.query({ limit: 10, offset: 0 });
    const timestamps = result.entries.map((e: AuditEntry) => e.timestamp);
    for (let i = 1; i < timestamps.length; i++) {
      expect(timestamps[i - 1] >= timestamps[i]).toBe(true);
    }
  });
});