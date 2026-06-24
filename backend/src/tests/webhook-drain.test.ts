import request from "supertest";
import app from "../app";
import crypto from "crypto";
import path from "path";
import fs from "fs";
import { getDatabase, closeDatabase } from "../lib/database";
import { apiKeyService } from "../services/api-key-service";
import { webhookSecretService } from "../services/webhookSecretService";
import { webhookQueueService } from "../services/webhookQueueService";
import { webhookDeliveryRepo } from "../services/webhookDeliveryRepo";
import { auditService } from "../services/auditService";
import { db } from "../db/database";

describe("Webhook Drain Endpoint", () => {
  let superAdminKey: string;
  let supportKey: string;
  const subscriberId1 = "sub-test-1";
  const subscriberId2 = "sub-test-2";

  const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
  const TEST_DB_PATH = path.join(
    TEST_DB_DIR,
    `test-webhook-drain-${crypto.randomUUID()}.db`
  );
  const TEST_AUDIT_DIR = path.join(
    TEST_DB_DIR,
    `test-audit-${crypto.randomUUID()}`
  );

  beforeAll(async () => {
    process.env.DATABASE_PATH = TEST_DB_PATH;
    process.env.SKIP_API_KEY_AUTH = "true"; // Bypass basic X-API-Key check
    auditService.setAuditDir(TEST_AUDIT_DIR);

    closeDatabase();
    const conn = getDatabase();

    // Create tables for API keys
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
        created_by TEXT NOT NULL
      )
    `);
    conn.exec(`
      CREATE TABLE IF NOT EXISTS api_key_audit_log (
        id TEXT PRIMARY KEY,
        event_type TEXT NOT NULL,
        key_id TEXT NOT NULL,
        actor TEXT NOT NULL,
        timestamp TEXT NOT NULL,
        ip_address TEXT,
        endpoint TEXT,
        metadata TEXT
      )
    `);

    // Ensure webhook deliveries schema is created
    webhookDeliveryRepo.ensureSchema();

    // Register test subscribers
    try {
      webhookSecretService.registerSubscriber(subscriberId1);
    } catch {}
    try {
      webhookSecretService.registerSubscriber(subscriberId2);
    } catch {}
  });

  afterAll(async () => {
    // Yield to the event loop to let pending async audit logging finish
    await new Promise((resolve) => setImmediate(resolve));
    await new Promise((resolve) => setImmediate(resolve));

    closeDatabase();
    try {
      if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
      if (fs.existsSync(TEST_DB_PATH + "-wal")) fs.unlinkSync(TEST_DB_PATH + "-wal");
      if (fs.existsSync(TEST_DB_PATH + "-shm")) fs.unlinkSync(TEST_DB_PATH + "-shm");
    } catch {}
    try {
      auditService.clearAll();
      if (fs.existsSync(TEST_AUDIT_DIR)) {
        fs.rmSync(TEST_AUDIT_DIR, { recursive: true, force: true });
      }
    } catch {}
  });

  beforeEach(async () => {
    db.clear(); // Clear memory db representation if any
    const conn = getDatabase();
    conn.exec("DELETE FROM webhook_deliveries");
    auditService.clearAll();

    // Re-generate keys for each test case
    const sa = await apiKeyService.createApiKey({
      name: "Super Admin Key",
      scopes: ["admin:*"],
      created_by: "test",
    });
    superAdminKey = sa.plaintext_key;

    const sup = await apiKeyService.createApiKey({
      name: "Support Key",
      scopes: ["read:*"],
      created_by: "test",
    });
    supportKey = sup.plaintext_key;
  });

  it("rejects request if missing bearer token (401)", async () => {
    const res = await request(app)
      .post(`/api/v1/admin/webhooks/${subscriberId1}/drain`)
      .set("X-CSRF-Token", "test-csrf-token")
      .set("Content-Type", "application/json")
      .send();

    expect(res.status).toBe(401);
  });

  it("rejects request if caller does not have super_admin role (403)", async () => {
    const res = await request(app)
      .post(`/api/v1/admin/webhooks/${subscriberId1}/drain`)
      .set("Authorization", `Bearer ${supportKey}`)
      .send();

    expect(res.status).toBe(403);
  });

  it("returns 404 if subscriber does not exist", async () => {
    const res = await request(app)
      .post("/api/v1/admin/webhooks/nonexistent-sub/drain")
      .set("Authorization", `Bearer ${superAdminKey}`)
      .send();

    expect(res.status).toBe(404);
  });

  it("marks pending and failed deliveries as dead_letter and is idempotent", async () => {
    // Enqueue some deliveries for subscriber 1
    webhookQueueService.enqueueWithSubscriber("event.1", { data: 1 }, subscriberId1);
    webhookQueueService.enqueueWithSubscriber("event.2", { data: 2 }, subscriberId1);

    // Also enqueue for subscriber 2 to verify isolation
    webhookQueueService.enqueueWithSubscriber("event.3", { data: 3 }, subscriberId2);

    // Verify initial states
    let pendingCount1 = webhookDeliveryRepo
      .getPending()
      .filter((d) => d.subscriberId === subscriberId1).length;
    expect(pendingCount1).toBe(2);

    // 1st Drain
    const res1 = await request(app)
      .post(`/api/v1/admin/webhooks/${subscriberId1}/drain`)
      .set("Authorization", `Bearer ${superAdminKey}`)
      .send();

    expect(res1.status).toBe(200);
    expect(res1.body).toMatchObject({
      pending: 0,
      drained: 2,
      audit_entry_id: expect.any(String),
    });

    // Verify subscriber 1 is drained (0 pending)
    pendingCount1 = webhookDeliveryRepo
      .getPending()
      .filter((d) => d.subscriberId === subscriberId1).length;
    expect(pendingCount1).toBe(0);

    const deadLetters1 = webhookDeliveryRepo
      .getDeadLetters()
      .filter((d) => d.subscriberId === subscriberId1);
    expect(deadLetters1.length).toBe(2);

    // Verify subscriber 2 is unaffected (still has pending)
    const pendingCount2 = webhookDeliveryRepo
      .getPending()
      .filter((d) => d.subscriberId === subscriberId2).length;
    expect(pendingCount2).toBe(1);

    // Audit entry check
    const auditEntries = auditService.getEntriesForTest();
    expect(auditEntries.length).toBe(1);
    expect(auditEntries[0].operation).toBe("WEBHOOK_DRAIN");
    expect(auditEntries[0].id).toBe(res1.body.audit_entry_id);
    expect(auditEntries[0].actor).toContain("api_key:");
    expect(auditEntries[0].success).toBe(true);

    // 2nd Drain (Idempotency check)
    const res2 = await request(app)
      .post(`/api/v1/admin/webhooks/${subscriberId1}/drain`)
      .set("Authorization", `Bearer ${superAdminKey}`)
      .send();

    expect(res2.status).toBe(200);
    expect(res2.body).toMatchObject({
      pending: 0,
      drained: 0,
      audit_entry_id: expect.any(String),
    });

    // Verify subscriber 2 remains unaffected
    const pendingCount2After = webhookDeliveryRepo
      .getPending()
      .filter((d) => d.subscriberId === subscriberId2).length;
    expect(pendingCount2After).toBe(1);
  });
});
