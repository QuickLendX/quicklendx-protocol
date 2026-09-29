import express from "express";
import supertest from "supertest";
import webhookRoutes from "../routes/v1/webhooks";
import { webhookSecretService } from "../services/webhookSecretService";
import { WebhookSecretStatus } from "../types/webhook";

const app = express();
// Note: In real app, express.raw() is applied to /ingest.
// We mount webhookRoutes which has express.raw({ type: "*/*" }) for /ingest.
app.use("/api/v1/webhooks", webhookRoutes);

describe("Webhook Ed25519 Asymmetric Signing & Algorithm Negotiation", () => {
  beforeEach(() => {
    // Clear in-memory store before each test
    (webhookSecretService as any).store._clear();
  });

  // ---------------------------------------------------------------------------
  // 1. Registration
  // ---------------------------------------------------------------------------
  describe("Registration", () => {
    it("registers a subscriber with ed25519 algorithm", async () => {
      const res = await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({
          subscriber_id: "sub-ed25519",
          algorithm: "ed25519",
          grace_period_seconds: 1800,
        });

      expect(res.status).toBe(201);
      expect(res.body.subscriber_id).toBe("sub-ed25519");
      expect(res.body.algorithm).toBe("ed25519");
      expect(res.body.status).toBe(WebhookSecretStatus.Active);
      expect(res.body.grace_period_seconds).toBe(1800);
      expect(res.body.initial_secret).toContain("-----BEGIN PUBLIC KEY-----");
    });
  });

  // ---------------------------------------------------------------------------
  // 2. JWKS Endpoint
  // ---------------------------------------------------------------------------
  describe("JWKS Endpoint", () => {
    it("publishes Ed25519 public keys in JWKS format", async () => {
      // Register one HMAC subscriber and one Ed25519 subscriber
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-hmac", algorithm: "hmac-sha256" });

      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-ed25519", algorithm: "ed25519" });

      const res = await supertest(app).get("/api/v1/webhooks/jwks");
      expect(res.status).toBe(200);
      expect(res.body.keys).toBeInstanceOf(Array);
      // It should only contain Ed25519 keys
      expect(res.body.keys).toHaveLength(1);
      expect(res.body.keys[0]).toEqual(
        expect.objectContaining({
          kty: "OKP",
          crv: "Ed25519",
          kid: "sub-ed25519",
          use: "sig",
          alg: "EdDSA",
        })
      );
      expect(typeof res.body.keys[0].x).toBe("string");
    });
  });

  // ---------------------------------------------------------------------------
  // 3. Verification & Ingestion
  // ---------------------------------------------------------------------------
  describe("Verification & Ingestion", () => {
    it("verifies Ed25519 signatures successfully", async () => {
      const regRes = await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-verify", algorithm: "ed25519" });

      const subscriberId = "sub-verify";
      const payload = JSON.stringify({ event: "invoice.paid", amount: "100" });

      // Retrieve the private key from the store to sign
      const record = (webhookSecretService as any).store.get(subscriberId);
      const signature = webhookSecretService.signEd25519(payload, record.primary_secret);

      const res = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", signature)
        .set("Content-Type", "application/json")
        .send(payload);

      expect(res.status).toBe(200);
      expect(res.body.received).toBe(true);
      expect(res.body.subscriber_id).toBe(subscriberId);
      expect(res.body.matched_secret).toBe("primary");
    });
  });

  // ---------------------------------------------------------------------------
  // 4. Key Rotation & Grace Window
  // ---------------------------------------------------------------------------
  describe("Key Rotation & Grace Window", () => {
    it("accepts both old primary and new pending keys during the grace window", async () => {
      const regRes = await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-rot", algorithm: "ed25519" });

      const subscriberId = "sub-rot";
      const payload = "test-payload";

      const record1 = (webhookSecretService as any).store.get(subscriberId);
      const oldPrivateKey = record1.primary_secret;

      // Initiate rotation
      const rotRes = await supertest(app)
        .post(`/api/v1/webhooks/subscribers/${subscriberId}/rotate`)
        .send({ grace_period_seconds: 3600 });

      expect(rotRes.status).toBe(202);
      expect(rotRes.body.status).toBe(WebhookSecretStatus.Rotating);
      expect(rotRes.body.new_secret).toContain("-----BEGIN PUBLIC KEY-----");

      // Verify JWKS now has two keys
      const jwksRes = await supertest(app).get("/api/v1/webhooks/jwks");
      expect(jwksRes.body.keys).toHaveLength(2);
      expect(jwksRes.body.keys.map((k: any) => k.kid)).toContain(subscriberId);
      expect(jwksRes.body.keys.map((k: any) => k.kid)).toContain(`${subscriberId}:pending`);

      const record2 = (webhookSecretService as any).store.get(subscriberId);
      const newPrivateKey = record2.pending_secret!;

      // 1. Verify old signature is accepted as primary
      const sigOld = webhookSecretService.signEd25519(payload, oldPrivateKey);
      const resOld = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", sigOld)
        .send(payload);
      expect(resOld.status).toBe(200);
      expect(resOld.body.matched_secret).toBe("primary");

      // 2. Verify new signature is accepted as pending
      const sigNew = webhookSecretService.signEd25519(payload, newPrivateKey);
      const resNew = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", sigNew)
        .send(payload);
      expect(resNew.status).toBe(200);
      expect(resNew.body.matched_secret).toBe("pending");

      // 3. Finalize rotation
      const finRes = await supertest(app)
        .post(`/api/v1/webhooks/subscribers/${subscriberId}/rotate/finalize`);
      expect(finRes.status).toBe(200);

      // Verify old signature is now rejected
      const resOldPostFin = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", sigOld)
        .send(payload);
      expect(resOldPostFin.status).toBe(401);

      // Verify new signature is now accepted as primary
      const resNewPostFin = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", sigNew)
        .send(payload);
      expect(resNewPostFin.status).toBe(200);
      expect(resNewPostFin.body.matched_secret).toBe("primary");
    });

    it("supports cancelling key rotation", async () => {
      const subscriberId = "sub-cancel";
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: subscriberId, algorithm: "ed25519" });

      const record1 = (webhookSecretService as any).store.get(subscriberId);
      const oldPrivateKey = record1.primary_secret;

      // Initiate
      await supertest(app).post(`/api/v1/webhooks/subscribers/${subscriberId}/rotate`);
      const record2 = (webhookSecretService as any).store.get(subscriberId);
      const pendingPrivateKey = record2.pending_secret!;

      // Cancel
      const cancelRes = await supertest(app)
        .post(`/api/v1/webhooks/subscribers/${subscriberId}/rotate/cancel`);
      expect(cancelRes.status).toBe(200);

      // Pending key should be rejected
      const sigPending = webhookSecretService.signEd25519("hello", pendingPrivateKey);
      const resPending = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", sigPending)
        .send("hello");
      expect(resPending.status).toBe(401);

      // Primary key should be accepted
      const sigPrimary = webhookSecretService.signEd25519("hello", oldPrivateKey);
      const resPrimary = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", sigPrimary)
        .send("hello");
      expect(resPrimary.status).toBe(200);
    });

    it("auto-promotes pending secret after grace period elapses (lazy expiry)", async () => {
      const subscriberId = "sub-lazy";
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: subscriberId, algorithm: "ed25519", grace_period_seconds: 60 });

      const record1 = (webhookSecretService as any).store.get(subscriberId);
      const oldPrivateKey = record1.primary_secret;

      await supertest(app).post(`/api/v1/webhooks/subscribers/${subscriberId}/rotate`);
      const record2 = (webhookSecretService as any).store.get(subscriberId);
      const pendingPrivateKey = record2.pending_secret!;

      // Backdate rotation start
      const expired = new Date(Date.now() - 120_000).toISOString();
      (webhookSecretService as any).store.set({ ...record2, pending_created_at: expired });

      // Old signature is now rejected (auto-promoted to primary, old primary discarded)
      const sigOld = webhookSecretService.signEd25519("hello", oldPrivateKey);
      const resOld = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", sigOld)
        .send("hello");
      expect(resOld.status).toBe(401);

      // Pending key is now verified as primary
      const sigNew = webhookSecretService.signEd25519("hello", pendingPrivateKey);
      const resNew = await supertest(app)
        .post(`/api/v1/webhooks/ingest/${subscriberId}`)
        .set("x-webhook-subscriber-id", subscriberId)
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", sigNew)
        .send("hello");
      expect(resNew.status).toBe(200);
      expect(resNew.body.matched_secret).toBe("primary");
    });
  });

  // ---------------------------------------------------------------------------
  // 5. Error Scopes & Security (Enumeration Resistance)
  // ---------------------------------------------------------------------------
  describe("Security and Error Handling", () => {
    it("returns 400 when missing signature or subscriber header", async () => {
      const res1 = await supertest(app)
        .post("/api/v1/webhooks/ingest/any")
        .set("x-webhook-signature", "some-sig")
        .send("data");
      expect(res1.status).toBe(400);
      expect(res1.body.error.code).toBe("MISSING_SUBSCRIBER_HEADER");

      const res2 = await supertest(app)
        .post("/api/v1/webhooks/ingest/any")
        .set("x-webhook-subscriber-id", "some-sub")
        .send("data");
      expect(res2.status).toBe(400);
      expect(res2.body.error.code).toBe("MISSING_SIGNATURE_HEADER");
    });

    it("returns 401 for unknown negotiation algorithm", async () => {
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-algo-test", algorithm: "ed25519" });

      const res = await supertest(app)
        .post("/api/v1/webhooks/ingest/sub-algo-test")
        .set("x-webhook-subscriber-id", "sub-algo-test")
        .set("x-webhook-signature-algorithm", "invalid-algorithm-name")
        .set("x-webhook-signature", "some-sig")
        .send("data");

      expect(res.status).toBe(401);
      expect(res.body.error.message).toBe("Webhook signature verification failed");
    });

    it("returns 401 for mismatched algorithm vs database slot", async () => {
      // 1. Database is HMAC, client requests Ed25519
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-hmac-only", algorithm: "hmac-sha256" });

      const res1 = await supertest(app)
        .post("/api/v1/webhooks/ingest/sub-hmac-only")
        .set("x-webhook-subscriber-id", "sub-hmac-only")
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", "some-sig")
        .send("data");
      expect(res1.status).toBe(401);

      // 2. Database is Ed25519, client requests HMAC
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-ed-only", algorithm: "ed25519" });

      const res2 = await supertest(app)
        .post("/api/v1/webhooks/ingest/sub-ed-only")
        .set("x-webhook-subscriber-id", "sub-ed-only")
        .set("x-webhook-signature-algorithm", "hmac-sha256")
        .set("x-webhook-signature", "sha256=abcdef")
        .send("data");
      expect(res2.status).toBe(401);
    });

    it("returns 401 for malformed base64url signatures", async () => {
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-malformed", algorithm: "ed25519" });

      // Malformed signature length or characters
      const malformedSigs = [
        "not-base64-url!",
        "AAAA",
        "A".repeat(86), // valid characters but wrong key signature mathematical mapping
      ];

      for (const sig of malformedSigs) {
        const res = await supertest(app)
          .post("/api/v1/webhooks/ingest/sub-malformed")
          .set("x-webhook-subscriber-id", "sub-malformed")
          .set("x-webhook-signature-algorithm", "ed25519")
          .set("x-webhook-signature", sig)
          .send("hello");
        expect(res.status).toBe(401);
        expect(res.body.error.message).toBe("Webhook signature verification failed");
      }
    });

    it("returns 401 when subscriber is revoked (deleted)", async () => {
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-revoked", algorithm: "ed25519" });

      // Revoke the subscriber (delete from database/store)
      (webhookSecretService as any).store.delete("sub-revoked");

      const res = await supertest(app)
        .post("/api/v1/webhooks/ingest/sub-revoked")
        .set("x-webhook-subscriber-id", "sub-revoked")
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", "A".repeat(86))
        .send("hello");

      expect(res.status).toBe(401);
      expect(res.body.error.message).toBe("Webhook signature verification failed");
    });

    it("preserves enumeration resistance by returning identical 401 responses for different failure reasons", async () => {
      // 1. Ghost subscriber
      const resGhost = await supertest(app)
        .post("/api/v1/webhooks/ingest/sub-ghost")
        .set("x-webhook-subscriber-id", "sub-ghost")
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", "A".repeat(86))
        .send("hello");

      // 2. Mismatched algorithm
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-mismatch-enum", algorithm: "hmac-sha256" });
      const resMismatch = await supertest(app)
        .post("/api/v1/webhooks/ingest/sub-mismatch-enum")
        .set("x-webhook-subscriber-id", "sub-mismatch-enum")
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", "A".repeat(86))
        .send("hello");

      // 3. Wrong signature
      await supertest(app)
        .post("/api/v1/webhooks/subscribers")
        .send({ subscriber_id: "sub-wrong-sig", algorithm: "ed25519" });
      const resWrongSig = await supertest(app)
        .post("/api/v1/webhooks/ingest/sub-wrong-sig")
        .set("x-webhook-subscriber-id", "sub-wrong-sig")
        .set("x-webhook-signature-algorithm", "ed25519")
        .set("x-webhook-signature", "A".repeat(86))
        .send("hello");

      expect(resGhost.status).toBe(401);
      expect(resMismatch.status).toBe(401);
      expect(resWrongSig.status).toBe(401);

      expect(resGhost.body).toEqual(resMismatch.body);
      expect(resGhost.body).toEqual(resWrongSig.body);
    });
  });
});
