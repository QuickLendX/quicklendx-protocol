/**
 * webhook-dead-letters.test.ts
 *
 * Test coverage for webhook dead-letter queue persistence and admin replay endpoints:
 *
 * Unit tests:
 *  - Dead-letter record creation and retrieval in webhookDeliveryRepo
 *  - Pagination in listDeadLetters (limit, offset, total, hasMore)
 *  - deliverWithRetry persists exhausted deliveries on permanent 4xx failure
 *  - deliverWithRetry persists exhausted deliveries on max retry budget exhaustion
 *  - deliverWithRetry persists exhausted deliveries on non-retryable errors
 *  - replayDeadLetter re-enqueues with fresh attempt count (attempt_count = 0)
 *  - Replay re-enqueues once: second attempt fails with ALREADY_REPLAYED
 *
 * Integration tests (HTTP via supertest):
 *  - GET /admin/webhooks/dead-letters requires authentication (401)
 *  - GET /admin/webhooks/dead-letters allows support role (200) and returns paginated entries
 *  - POST /admin/webhooks/dead-letters/:id/replay requires authentication (401)
 *  - POST /admin/webhooks/dead-letters/:id/replay denies support role without write permission (403)
 *  - POST /admin/webhooks/dead-letters/:id/replay allows operations admin (200)
 *  - POST /admin/webhooks/dead-letters/:id/replay is recorded in auditLogService
 *  - Duplicate replay returns 409 ALREADY_REPLAYED
 *  - Replay of unknown ID returns 404 NOT_FOUND
 */

import path from "path";
import fs from "fs";
import crypto from "crypto";
import request from "supertest";
import { getDatabase, closeDatabase } from "../lib/database";
import { apiKeyService } from "../services/api-key-service";
import { auditLogService } from "../services/auditLogService";
import {
  WebhookDeliveryRepo,
  webhookDeliveryRepo,
} from "../services/webhookDeliveryRepo";
import { deliverWithRetry } from "../services/webhook/retryScheduler";
import { WebhookDeliveryError } from "../services/webhook/delivery";
import type { WebhookEgressPolicy } from "../services/webhook/egressPolicy";
import app from "../app";

const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(
  TEST_DB_DIR,
  `test-webhook-dead-letters-${crypto.randomUUID()}.db`
);

const basePolicy: WebhookEgressPolicy = {
  timeoutMs: 3000,
  maxResponseBytes: 1024,
  maxRedirects: 3,
  hostAllowRules: [],
  hostDenyRules: [],
};

const testRetryPolicy = {
  maxAttempts: 3,
  initialDelayMs: 10,
  maxDelayMs: 100,
};

describe("Webhook Dead-Letter Queue & Replay System", () => {
  let adminKey: string; // operations_admin (write:*)
  let supportKey: string; // support (read:*)

  beforeAll(async () => {
    process.env.DATABASE_PATH = TEST_DB_PATH;
    process.env.SKIP_API_KEY_AUTH = "true";
    process.env.TEST_ACTOR = "test-actor";

    fs.mkdirSync(TEST_DB_DIR, { recursive: true });
    closeDatabase();

    const conn = getDatabase();

    // API keys schema for RBAC
    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY,
        key_hash TEXT NOT NULL,
        signing_secret_hash TEXT,
        prev_signing_secret_hash TEXT,
        prefix TEXT NOT NULL,
        name TEXT NOT NULL,
        scopes TEXT NOT NULL,
        created_at TEXT NOT NULL,
        last_used_at TEXT,
        expires_at TEXT,
        prev_secret_expires_at TEXT,
        revoked INTEGER NOT NULL DEFAULT 0,
        created_by TEXT NOT NULL,
        version INTEGER NOT NULL DEFAULT 1,
        revoked_at INTEGER,
        revoked_by TEXT
      )
    `);
    try { conn.exec("ALTER TABLE api_keys ADD COLUMN version INTEGER NOT NULL DEFAULT 1"); } catch {}
    try { conn.exec("ALTER TABLE api_keys ADD COLUMN revoked_at INTEGER"); } catch {}
    try { conn.exec("ALTER TABLE api_keys ADD COLUMN revoked_by TEXT"); } catch {}
    conn.exec(
      "CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix)"
    );
    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_key_audit_log (
        id TEXT PRIMARY KEY,
        event_type TEXT NOT NULL CHECK(event_type IN ('created','used','rotated','revoked')),
        key_id TEXT NOT NULL,
        actor TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        ip_address TEXT,
        endpoint TEXT,
        metadata TEXT,
        FOREIGN KEY (key_id) REFERENCES api_keys(id) ON DELETE CASCADE
      )
    `);

    // Ensure webhook deliveries and dead letters schema
    webhookDeliveryRepo.ensureSchema();

    // Create operations admin key
    const opsKey = await apiKeyService.createApiKey({
      name: "Operations Admin",
      scopes: ["write:*"],
      created_by: "test-setup",
    });
    adminKey = opsKey.plaintext_key;

    // Create support key (read-only)
    const suppKey = await apiKeyService.createApiKey({
      name: "Support Staff",
      scopes: ["read:*"],
      created_by: "test-setup",
    });
    supportKey = suppKey.plaintext_key;
  });

  afterAll(() => {
    closeDatabase();
    try {
      if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
      try { fs.unlinkSync(TEST_DB_PATH + "-wal"); } catch { /* ok */ }
      try { fs.unlinkSync(TEST_DB_PATH + "-shm"); } catch { /* ok */ }
    } catch { /* ok */ }
    delete process.env.SKIP_API_KEY_AUTH;
    delete process.env.TEST_ACTOR;
  });

  beforeEach(() => {
    const conn = getDatabase();
    conn.prepare("DELETE FROM webhook_dead_letters").run();
    conn.prepare("DELETE FROM webhook_deliveries").run();
    auditLogService.clear();
  });

  // ── 1. Repo Methods Unit Tests ──────────────────────────────────────────────

  describe("WebhookDeliveryRepo Dead-Letter Operations", () => {
    it("should record and fetch a dead-letter entry", () => {
      const record = webhookDeliveryRepo.recordDeadLetter({
        deliveryId: "del-001",
        eventType: "invoice.paid",
        payload: { invoiceId: "inv-123", amount: 5000 },
        targetUrl: "https://example.com/webhook",
        subscriberId: "sub-1",
        attemptCount: 5,
        maxAttempts: 5,
        lastError: "Connection timeout",
      });

      expect(record.id).toBeDefined();
      expect(record.deliveryId).toBe("del-001");
      expect(record.eventType).toBe("invoice.paid");
      expect(record.payload).toEqual({ invoiceId: "inv-123", amount: 5000 });
      expect(record.status).toBe("dead_letter");
      expect(record.replayedAt).toBeNull();
      expect(record.attemptCount).toBe(5);

      const fetched = webhookDeliveryRepo.getDeadLetterById(record.id);
      expect(fetched).not.toBeNull();
      expect(fetched!.id).toBe(record.id);
      expect(fetched!.lastError).toBe("Connection timeout");
    });

    it("should list dead-letter entries with pagination", () => {
      for (let i = 1; i <= 25; i++) {
        webhookDeliveryRepo.recordDeadLetter({
          eventType: `event.${i}`,
          payload: { index: i },
          targetUrl: `https://example.com/webhook/${i}`,
          attemptCount: 5,
          lastError: `Error ${i}`,
        });
      }

      const page1 = webhookDeliveryRepo.listDeadLetters({ limit: 10, offset: 0 });
      expect(page1.entries.length).toBe(10);
      expect(page1.total).toBe(25);
      expect(page1.hasMore).toBe(true);

      const page2 = webhookDeliveryRepo.listDeadLetters({ limit: 10, offset: 10 });
      expect(page2.entries.length).toBe(10);
      expect(page2.total).toBe(25);
      expect(page2.hasMore).toBe(true);

      const page3 = webhookDeliveryRepo.listDeadLetters({ limit: 10, offset: 20 });
      expect(page3.entries.length).toBe(5);
      expect(page3.total).toBe(25);
      expect(page3.hasMore).toBe(false);
    });

    it("should replay a dead letter and re-enqueue with fresh attempt count", () => {
      const record = webhookDeliveryRepo.recordDeadLetter({
        eventType: "settlement.completed",
        payload: { settlementId: "set-999" },
        subscriberId: "sub-9",
        attemptCount: 5,
        lastError: "HTTP 500 Internal Error",
      });

      const replayResult = webhookDeliveryRepo.replayDeadLetter(record.id);
      expect(replayResult).not.toBeNull();
      expect(replayResult!.deadLetter.status).toBe("replayed");
      expect(replayResult!.deadLetter.replayedAt).not.toBeNull();

      // Verify the new delivery was re-enqueued with a fresh attempt count (0)
      const newDelivery = replayResult!.newDelivery;
      expect(newDelivery.id).toBeDefined();
      expect(newDelivery.eventType).toBe("settlement.completed");
      expect(newDelivery.payload).toEqual({ settlementId: "set-999" });
      expect(newDelivery.attemptCount).toBe(0);
      expect(newDelivery.status).toBe("pending");

      // Verify second replay attempt is rejected (re-enqueues once only)
      expect(() => {
        webhookDeliveryRepo.replayDeadLetter(record.id);
      }).toThrow(/already been replayed/);
    });
  });

  // ── 2. Retry Scheduler Exhaustion & Dead-Lettering ──────────────────────────

  describe("deliverWithRetry DLQ Persistence on Exhaustion", () => {
    it("persists to webhook_dead_letters on permanent 4xx failure", async () => {
      const mockRequest = jest.fn().mockResolvedValue({
        statusCode: 404,
        headers: {},
        body: Buffer.from("not found"),
      });

      const result = await deliverWithRetry(
        "https://example.com/webhook",
        { event: "payment.failed", id: "del-404" },
        basePolicy,
        testRetryPolicy,
        { requestImpl: mockRequest, createAgent: () => ({} as any) }
      );

      expect(result.success).toBe(false);
      expect(result.deadLettered).toBe(true);
      expect(result.attemptCount).toBe(1);

      // Verify persisted in webhook_dead_letters
      const deadLetters = webhookDeliveryRepo.listDeadLetters();
      expect(deadLetters.total).toBe(1);
      const dl = deadLetters.entries[0];
      expect(dl.targetUrl).toBe("https://example.com/webhook");
      expect(dl.eventType).toBe("payment.failed");
      expect(dl.status).toBe("dead_letter");
      expect(dl.lastError).toContain("404");
    });

    it("persists to webhook_dead_letters when max retry budget is exhausted", async () => {
      const mockRequest = jest.fn().mockResolvedValue({
        statusCode: 503,
        headers: {},
        body: Buffer.from("service unavailable"),
      });

      const result = await deliverWithRetry(
        "https://example.com/webhook-503",
        { event: "order.created" },
        basePolicy,
        testRetryPolicy,
        { requestImpl: mockRequest, createAgent: () => ({} as any) }
      );

      expect(result.success).toBe(false);
      expect(result.deadLettered).toBe(true);
      expect(result.attemptCount).toBe(3);

      const deadLetters = webhookDeliveryRepo.listDeadLetters();
      expect(deadLetters.total).toBe(1);
      const dl = deadLetters.entries[0];
      expect(dl.targetUrl).toBe("https://example.com/webhook-503");
      expect(dl.attemptCount).toBe(3);
      expect(dl.status).toBe("dead_letter");
      expect(dl.lastError).toContain("503");
    });

    it("persists to webhook_dead_letters on non-retryable transport errors", async () => {
      const mockRequest = jest.fn().mockRejectedValue(
        new WebhookDeliveryError("URL_INVALID", "Invalid webhook destination")
      );

      const result = await deliverWithRetry(
        "https://invalid-url.com",
        { event: "user.updated" },
        basePolicy,
        testRetryPolicy,
        { requestImpl: mockRequest, createAgent: () => ({} as any) }
      );

      expect(result.success).toBe(false);
      expect(result.deadLettered).toBe(true);
      expect(result.attemptCount).toBe(1);

      const deadLetters = webhookDeliveryRepo.listDeadLetters();
      expect(deadLetters.total).toBe(1);
      expect(deadLetters.entries[0].lastError).toBe("Invalid webhook destination");
    });
  });

  // ── 3. Admin Endpoints Integration (RBAC, Pagination, Replay) ─────────────

  describe("Admin HTTP Endpoints for Dead-Letter Inspection & Replay", () => {
    it("GET /admin/webhooks/dead-letters requires authentication (401)", async () => {
      const res = await request(app).get("/api/v1/admin/webhooks/dead-letters");
      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe("AUTH_REQUIRED");
    });

    it("GET /admin/webhooks/dead-letters returns paginated list for support and admin", async () => {
      // Seed some dead letters
      for (let i = 1; i <= 5; i++) {
        webhookDeliveryRepo.recordDeadLetter({
          eventType: `test.event.${i}`,
          payload: { index: i },
          targetUrl: `https://example.com/hook-${i}`,
          attemptCount: 5,
          lastError: `Fail ${i}`,
        });
      }

      // Support role can read
      const resSupport = await request(app)
        .get("/api/v1/admin/webhooks/dead-letters?limit=3&offset=0")
        .set("Authorization", `Bearer ${supportKey}`);

      expect(resSupport.status).toBe(200);
      expect(resSupport.body.entries.length).toBe(3);
      expect(resSupport.body.pagination.total).toBe(5);
      expect(resSupport.body.pagination.hasMore).toBe(true);

      // Operations admin can also read
      const resAdmin = await request(app)
        .get("/api/v1/admin/webhooks/dead-letters?limit=3&offset=3")
        .set("Authorization", `Bearer ${adminKey}`);

      expect(resAdmin.status).toBe(200);
      expect(resAdmin.body.entries.length).toBe(2);
      expect(resAdmin.body.pagination.hasMore).toBe(false);

      // Also verify direct /admin path
      const resDirect = await request(app)
        .get("/admin/webhooks/dead-letters")
        .set("Authorization", `Bearer ${supportKey}`);

      expect(resDirect.status).toBe(200);
      expect(resDirect.body.entries.length).toBe(5);
    });

    it("POST /admin/webhooks/dead-letters/:id/replay requires admin permissions", async () => {
      const dl = webhookDeliveryRepo.recordDeadLetter({
        eventType: "invoice.disputed",
        payload: { id: 1 },
      });

      // Anonymous -> 401
      const resAnon = await request(app)
        .post(`/api/v1/admin/webhooks/dead-letters/${dl.id}/replay`)
        .set("Content-Type", "application/json");
      expect(resAnon.status).toBe(401);

      // Support (read-only) -> 403 Forbidden
      const resSupport = await request(app)
        .post(`/api/v1/admin/webhooks/dead-letters/${dl.id}/replay`)
        .set("Authorization", `Bearer ${supportKey}`);

      expect(resSupport.status).toBe(403);
      expect(resSupport.body.error.code).toBe("INSUFFICIENT_ROLE");
    });

    it("POST /admin/webhooks/dead-letters/:id/replay successfully re-enqueues once with audit log", async () => {
      const dl = webhookDeliveryRepo.recordDeadLetter({
        eventType: "bid.accepted",
        payload: { bidId: "bid-777", amount: 1000 },
        subscriberId: "sub-777",
        attemptCount: 5,
        maxAttempts: 5,
        lastError: "Timeout after 5 retries",
      });

      // Replay via operations admin
      const res = await request(app)
        .post(`/api/v1/admin/webhooks/dead-letters/${dl.id}/replay`)
        .set("Authorization", `Bearer ${adminKey}`);

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.dead_letter.status).toBe("replayed");
      expect(res.body.dead_letter.replayedAt).not.toBeNull();

      // Check re-enqueued delivery has fresh attempt count
      expect(res.body.delivery.attemptCount).toBe(0);
      expect(res.body.delivery.status).toBe("pending");
      expect(res.body.delivery.eventType).toBe("bid.accepted");

      // Verify audit log captured the replay action
      const auditEntries = auditLogService.listEntries();
      const replayAudit = auditEntries.find(
        (e) => e.action === "WEBHOOK_DEAD_LETTER_REPLAY"
      );
      expect(replayAudit).toBeDefined();
      expect(replayAudit!.role).toBe("operations_admin");
      expect(replayAudit!.metadata?.deadLetterId).toBe(dl.id);

      // Replay a second time -> 409 Conflict (Replay re-enqueues once)
      const resDuplicate = await request(app)
        .post(`/api/v1/admin/webhooks/dead-letters/${dl.id}/replay`)
        .set("Authorization", `Bearer ${adminKey}`);

      expect(resDuplicate.status).toBe(409);
      expect(resDuplicate.body.error.code).toBe("ALREADY_REPLAYED");

      // Verify delivery queue only has ONE re-enqueued delivery
      const pendingDeliveries = webhookDeliveryRepo.getPending();
      expect(pendingDeliveries.length).toBe(1);
    });

    it("POST /admin/webhooks/dead-letters/:id/replay returns 404 for nonexistent dead letter", async () => {
      const res = await request(app)
        .post("/api/v1/admin/webhooks/dead-letters/nonexistent-id/replay")
        .set("Authorization", `Bearer ${adminKey}`);

      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("NOT_FOUND");
    });
  });
});
