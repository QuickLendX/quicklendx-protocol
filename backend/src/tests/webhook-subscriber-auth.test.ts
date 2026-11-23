import express from "express";
import request from "supertest";
import webhookRouter from "../routes/v1/webhooks";
import { errorHandler } from "../middleware/error-handler";
import { setApiKeyVerifier } from "../middleware/api-key-auth";
import { webhookSecretService, WebhookSecretStore, WebhookSecretService } from "../services/webhookSecretService";
import { auditLogService } from "../services/auditLogService";
import { ApiKey } from "../models/api-key";

describe("Webhook Subscriber Lifecycle Authentication & Ownership", () => {
  let app: express.Express;
  let store: WebhookSecretStore;
  let service: WebhookSecretService;

  const keyA: ApiKey = {
    id: "key-a-id",
    name: "Key A",
    prefix: "qlx_key_a",
    scopes: ["webhooks:manage"],
    created_by: "owner-a",
    created_at: new Date().toISOString(),
    last_used_at: null,
    expires_at: null,
    revoked: false,
    signing_secret_hash: null,
    prev_signing_secret_hash: null,
    prev_secret_expires_at: null,
    ownerId: "owner-a",
    permissions: ["webhooks:manage"],
    status: "active",
    version: 1,
    createdAt: Date.now(),
    revokedAt: null,
    revokedBy: null,
  };

  const keyB: ApiKey = {
    id: "key-b-id",
    name: "Key B",
    prefix: "qlx_key_b",
    scopes: ["webhooks:manage"],
    created_by: "owner-b",
    created_at: new Date().toISOString(),
    last_used_at: null,
    expires_at: null,
    revoked: false,
    signing_secret_hash: null,
    prev_signing_secret_hash: null,
    prev_secret_expires_at: null,
    ownerId: "owner-b",
    permissions: ["webhooks:manage"],
    status: "active",
    version: 1,
    createdAt: Date.now(),
    revokedAt: null,
    revokedBy: null,
  };

  const keyNoScope: ApiKey = {
    id: "key-no-scope-id",
    name: "Key Without Scope",
    prefix: "qlx_no_scope",
    scopes: ["read:*"],
    created_by: "owner-no-scope",
    created_at: new Date().toISOString(),
    last_used_at: null,
    expires_at: null,
    revoked: false,
    signing_secret_hash: null,
    prev_signing_secret_hash: null,
    prev_secret_expires_at: null,
    ownerId: "owner-no-scope",
    permissions: ["read:*"],
    status: "active",
    version: 1,
    createdAt: Date.now(),
    revokedAt: null,
    revokedBy: null,
  };

  const keySecAdmin: ApiKey & { role: string } = {
    id: "key-sec-admin-id",
    name: "Security Admin Key",
    prefix: "qlx_sec_admin",
    scopes: ["webhooks:manage"],
    role: "security_admin",
    created_by: "sec-admin",
    created_at: new Date().toISOString(),
    last_used_at: null,
    expires_at: null,
    revoked: false,
    signing_secret_hash: null,
    prev_signing_secret_hash: null,
    prev_secret_expires_at: null,
    ownerId: "sec-admin",
    permissions: ["webhooks:manage"],
    status: "active",
    version: 1,
    createdAt: Date.now(),
    revokedAt: null,
    revokedBy: null,
  };

  beforeEach(() => {
    store = new WebhookSecretStore();
    service = new WebhookSecretService(store);

    jest.spyOn(webhookSecretService, "registerSubscriber").mockImplementation(service.registerSubscriber.bind(service));
    jest.spyOn(webhookSecretService, "getSubscriberView").mockImplementation(service.getSubscriberView.bind(service));
    jest.spyOn(webhookSecretService, "getRecord").mockImplementation(service.getRecord.bind(service));
    jest.spyOn(webhookSecretService, "initiateRotation").mockImplementation(service.initiateRotation.bind(service));
    jest.spyOn(webhookSecretService, "finalizeRotation").mockImplementation(service.finalizeRotation.bind(service));
    jest.spyOn(webhookSecretService, "cancelRotation").mockImplementation(service.cancelRotation.bind(service));
    jest.spyOn(webhookSecretService, "verifySignature").mockImplementation(service.verifySignature.bind(service));

    auditLogService.clear();

    setApiKeyVerifier(async (token: string) => {
      if (token === "qlx_key_a") return keyA;
      if (token === "qlx_key_b") return keyB;
      if (token === "qlx_no_scope") return keyNoScope;
      if (token === "qlx_sec_admin") return keySecAdmin as unknown as ApiKey;
      return null;
    });

    app = express();
    app.use(express.json());
    app.use("/webhooks", webhookRouter);
    app.use(errorHandler);
  });

  afterEach(() => {
    setApiKeyVerifier(null);
    jest.restoreAllMocks();
  });

  // Acceptance Criterion 1
  it("each of the 5 management endpoints without Authorization returns 401, and the service spy is not called", async () => {
    const registerSpy = jest.spyOn(webhookSecretService, "registerSubscriber");
    const getSpy = jest.spyOn(webhookSecretService, "getSubscriberView");
    const rotateSpy = jest.spyOn(webhookSecretService, "initiateRotation");
    const finalizeSpy = jest.spyOn(webhookSecretService, "finalizeRotation");
    const cancelSpy = jest.spyOn(webhookSecretService, "cancelRotation");

    // 1. POST /subscribers
    const res1 = await request(app).post("/webhooks/subscribers").send({ subscriber_id: "acme" });
    expect(res1.status).toBe(401);
    expect(res1.body.error.code).toBe("UNAUTHORIZED");
    expect(registerSpy).not.toHaveBeenCalled();

    // 2. GET /subscribers/:id
    const res2 = await request(app).get("/webhooks/subscribers/acme");
    expect(res2.status).toBe(401);
    expect(res2.body.error.code).toBe("UNAUTHORIZED");
    expect(getSpy).not.toHaveBeenCalled();

    // 3. POST /subscribers/:id/rotate
    const res3 = await request(app).post("/webhooks/subscribers/acme/rotate").send({});
    expect(res3.status).toBe(401);
    expect(res3.body.error.code).toBe("UNAUTHORIZED");
    expect(rotateSpy).not.toHaveBeenCalled();

    // 4. POST /subscribers/:id/rotate/finalize
    const res4 = await request(app).post("/webhooks/subscribers/acme/rotate/finalize").send({});
    expect(res4.status).toBe(401);
    expect(res4.body.error.code).toBe("UNAUTHORIZED");
    expect(finalizeSpy).not.toHaveBeenCalled();

    // 5. POST /subscribers/:id/rotate/cancel
    const res5 = await request(app).post("/webhooks/subscribers/acme/rotate/cancel").send({});
    expect(res5.status).toBe(401);
    expect(res5.body.error.code).toBe("UNAUTHORIZED");
    expect(cancelSpy).not.toHaveBeenCalled();
  });

  // Acceptance Criterion 4
  it("a key without webhooks:manage returns 403 FORBIDDEN", async () => {
    const res = await request(app)
      .post("/webhooks/subscribers")
      .set("Authorization", "Bearer qlx_no_scope")
      .send({ subscriber_id: "acme" });

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("FORBIDDEN");
  });

  // Acceptance Criterion 2
  it("key A registers acme, key B gets 404 SUBSCRIBER_NOT_FOUND for GET, rotate, finalize and cancel on acme, and acme's secret is unchanged", async () => {
    // Key A registers acme
    const regRes = await request(app)
      .post("/webhooks/subscribers")
      .set("Authorization", "Bearer qlx_key_a")
      .send({ subscriber_id: "acme" });

    expect(regRes.status).toBe(201);
    const initialSecret = regRes.body.initial_secret;
    expect(initialSecret).toBeDefined();

    // Key B attempts GET -> 404 SUBSCRIBER_NOT_FOUND
    const getRes = await request(app)
      .get("/webhooks/subscribers/acme")
      .set("Authorization", "Bearer qlx_key_b");
    expect(getRes.status).toBe(404);
    expect(getRes.body.error.code).toBe("SUBSCRIBER_NOT_FOUND");

    // Key B attempts rotate -> 404 SUBSCRIBER_NOT_FOUND
    const rotateRes = await request(app)
      .post("/webhooks/subscribers/acme/rotate")
      .set("Authorization", "Bearer qlx_key_b")
      .send({});
    expect(rotateRes.status).toBe(404);
    expect(rotateRes.body.error.code).toBe("SUBSCRIBER_NOT_FOUND");

    // Key B attempts finalize -> 404 SUBSCRIBER_NOT_FOUND
    const finalizeRes = await request(app)
      .post("/webhooks/subscribers/acme/rotate/finalize")
      .set("Authorization", "Bearer qlx_key_b")
      .send({});
    expect(finalizeRes.status).toBe(404);
    expect(finalizeRes.body.error.code).toBe("SUBSCRIBER_NOT_FOUND");

    // Key B attempts cancel -> 404 SUBSCRIBER_NOT_FOUND
    const cancelRes = await request(app)
      .post("/webhooks/subscribers/acme/rotate/cancel")
      .set("Authorization", "Bearer qlx_key_b")
      .send({});
    expect(cancelRes.status).toBe(404);
    expect(cancelRes.body.error.code).toBe("SUBSCRIBER_NOT_FOUND");

    // Verify acme's secret is unchanged by signing a payload with initialSecret and posting to ingest
    const payload = JSON.stringify({ event: "invoice.paid", amount: 100 });
    const signature = service.computeSignature(payload, initialSecret);

    const ingestRes = await request(app)
      .post("/webhooks/ingest/acme")
      .set("Content-Type", "application/json")
      .set("X-Webhook-Subscriber-Id", "acme")
      .set("X-Webhook-Signature", signature)
      .send(payload);

    expect(ingestRes.status).toBe(200);
    expect(ingestRes.body).toMatchObject({
      received: true,
      subscriber_id: "acme",
      matched_secret: "primary",
    });
  });

  // Acceptance Criterion 3
  it("key A runs the rotate → finalize flow on acme -> 202 then 200; new secret passes ingest and old secret fails after finalize", async () => {
    // Key A registers acme
    const regRes = await request(app)
      .post("/webhooks/subscribers")
      .set("Authorization", "Bearer qlx_key_a")
      .send({ subscriber_id: "acme" });
    expect(regRes.status).toBe(201);
    const oldSecret = regRes.body.initial_secret;

    // Key A rotates
    const rotateRes = await request(app)
      .post("/webhooks/subscribers/acme/rotate")
      .set("Authorization", "Bearer qlx_key_a")
      .send({ grace_period_seconds: 3600 });
    expect(rotateRes.status).toBe(202);
    expect(rotateRes.body.status).toBe("rotating");
    const newSecret = rotateRes.body.new_secret;
    expect(newSecret).toBeDefined();

    // Key A finalizes
    const finalizeRes = await request(app)
      .post("/webhooks/subscribers/acme/rotate/finalize")
      .set("Authorization", "Bearer qlx_key_a")
      .send({});
    expect(finalizeRes.status).toBe(200);

    const payload = JSON.stringify({ hello: "world" });

    // Payload signed with new secret passes ingest
    const newSig = service.computeSignature(payload, newSecret);
    const ingestNew = await request(app)
      .post("/webhooks/ingest/acme")
      .set("Content-Type", "application/json")
      .set("X-Webhook-Subscriber-Id", "acme")
      .set("X-Webhook-Signature", newSig)
      .send(payload);
    expect(ingestNew.status).toBe(200);
    expect(ingestNew.body.received).toBe(true);

    // Payload signed with old secret fails ingest
    const oldSig = service.computeSignature(payload, oldSecret);
    const ingestOld = await request(app)
      .post("/webhooks/ingest/acme")
      .set("Content-Type", "application/json")
      .set("X-Webhook-Subscriber-Id", "acme")
      .set("X-Webhook-Signature", oldSig)
      .send(payload);
    expect(ingestOld.status).toBe(401);
    expect(ingestOld.body.error.code).toBe("INVALID_WEBHOOK_SIGNATURE");
  });

  // Acceptance Criterion 5
  it("a security_admin key rotates acme (owned by A) -> 202, and the admin audit log contains one entry with action: webhook_secret_rotate and subscriber id, with no secret value", async () => {
    // Key A registers acme
    const regRes = await request(app)
      .post("/webhooks/subscribers")
      .set("Authorization", "Bearer qlx_key_a")
      .send({ subscriber_id: "acme" });
    expect(regRes.status).toBe(201);
    const initialSecret = regRes.body.initial_secret;

    auditLogService.clear();

    // Security admin rotates acme
    const rotateRes = await request(app)
      .post("/webhooks/subscribers/acme/rotate")
      .set("Authorization", "Bearer qlx_sec_admin")
      .send({ grace_period_seconds: 1800 });
    expect(rotateRes.status).toBe(202);
    const newSecret = rotateRes.body.new_secret;
    expect(newSecret).toBeDefined();

    // Check admin audit log
    const entries = auditLogService.listEntries();
    const rotateEntries = entries.filter((e) => e.action === "webhook_secret_rotate");
    expect(rotateEntries).toHaveLength(1);

    const auditEntry = rotateEntries[0];
    expect(auditEntry.role).toBe("security_admin");
    expect(auditEntry.outcome).toBe("performed");

    const entryJson = JSON.stringify(auditEntry);
    expect(entryJson).toContain("acme");
    expect(entryJson).not.toContain(initialSecret);
    expect(entryJson).not.toContain(newSecret);
  });
});
