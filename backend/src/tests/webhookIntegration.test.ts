/**
 * Integration tests for the webhook secret rotation API.
 *
 * Spins up the full Express app via supertest and simulates the complete
 * rotation lifecycle, including the dual-verify window.
 */

import request from "supertest";
import { createHmac } from "crypto";
import app from "../app";
import { webhookSecretService } from "../services/webhookSecretService";
import { WebhookSecretStatus } from "../types/webhook";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function sign(payload: string | Buffer, secretHex: string): string {
  const hmac = createHmac("sha256", Buffer.from(secretHex, "hex"));
  hmac.update(typeof payload === "string" ? Buffer.from(payload) : payload);
  return `sha256=${hmac.digest("hex")}`;
}

function clearStore(): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (webhookSecretService as any).store._clear();
}

beforeEach(() => {
  clearStore();
});

// ---------------------------------------------------------------------------
// Subscriber registration
// ---------------------------------------------------------------------------

describe("POST /api/v1/webhooks/subscribers", () => {
  it("registers a new subscriber and returns initial_secret once", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "int-sub-1" });

    expect(res.status).toBe(201);
    expect(res.body.subscriber_id).toBe("int-sub-1");
    expect(res.body.status).toBe(WebhookSecretStatus.Active);
    expect(res.body.has_pending_secret).toBe(false);
    expect(res.body.initial_secret).toMatch(/^[0-9a-f]{64}$/);
    // Secrets must not appear in any other field.
    expect(res.body.primary_secret).toBeUndefined();
    expect(res.body.pending_secret).toBeUndefined();
  });

  it("returns 409 for duplicate subscriber_id", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "dup-sub" });

    const res = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "dup-sub" });

    expect(res.status).toBe(409);
    expect(res.body.error.code).toBe("SUBSCRIBER_ALREADY_EXISTS");
  });

  it("returns 400 for missing subscriber_id", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({});

    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("VALIDATION_ERROR");
  });

  it("returns 400 for empty subscriber_id", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "" });

    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("VALIDATION_ERROR");
  });

  it("accepts custom grace_period_seconds", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "grace-sub", grace_period_seconds: 1800 });

    expect(res.status).toBe(201);
    expect(res.body.grace_period_seconds).toBe(1800);
  });

  it("returns 400 for grace_period_seconds below minimum", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "bad-grace", grace_period_seconds: 10 });

    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("VALIDATION_ERROR");
  });
});

// ---------------------------------------------------------------------------
// Get subscriber
// ---------------------------------------------------------------------------

describe("GET /api/v1/webhooks/subscribers/:subscriberId", () => {
  it("returns public view for existing subscriber", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "get-sub" });

    const res = await request(app).get("/api/v1/webhooks/subscribers/get-sub");

    expect(res.status).toBe(200);
    expect(res.body.subscriber_id).toBe("get-sub");
    expect(res.body.primary_secret).toBeUndefined();
    expect(res.body.pending_secret).toBeUndefined();
  });

  it("returns 404 for unknown subscriber", async () => {
    const res = await request(app).get("/api/v1/webhooks/subscribers/nobody");
    expect(res.status).toBe(404);
    expect(res.body.error.code).toBe("SUBSCRIBER_NOT_FOUND");
  });
});

// ---------------------------------------------------------------------------
// Initiate rotation
// ---------------------------------------------------------------------------

describe("POST /api/v1/webhooks/subscribers/:subscriberId/rotate", () => {
  it("initiates rotation and returns new_secret once", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "rot-init" });

    const res = await request(app)
      .post("/api/v1/webhooks/subscribers/rot-init/rotate")
      .send({});

    expect(res.status).toBe(202);
    expect(res.body.status).toBe(WebhookSecretStatus.Rotating);
    expect(res.body.new_secret).toMatch(/^[0-9a-f]{64}$/);
    expect(res.body.pending_created_at).toBeTruthy();
    // new_secret must not be re-exposed in subsequent GET
    const view = await request(app).get("/api/v1/webhooks/subscribers/rot-init");
    expect(view.body.new_secret).toBeUndefined();
  });

  it("returns 409 if rotation already in progress", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "double-rot" });
    await request(app)
      .post("/api/v1/webhooks/subscribers/double-rot/rotate")
      .send({});

    const res = await request(app)
      .post("/api/v1/webhooks/subscribers/double-rot/rotate")
      .send({});

    expect(res.status).toBe(409);
    expect(res.body.error.code).toBe("ROTATION_ALREADY_IN_PROGRESS");
  });

  it("returns 404 for unknown subscriber", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers/ghost/rotate")
      .send({});
    expect(res.status).toBe(404);
  });

  it("accepts custom grace_period_seconds in body", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "rot-grace" });

    const res = await request(app)
      .post("/api/v1/webhooks/subscribers/rot-grace/rotate")
      .send({ grace_period_seconds: 900 });

    expect(res.status).toBe(202);
    expect(res.body.grace_period_seconds).toBe(900);
  });

  it("returns 400 for invalid grace_period_seconds", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "rot-bad-grace" });

    const res = await request(app)
      .post("/api/v1/webhooks/subscribers/rot-bad-grace/rotate")
      .send({ grace_period_seconds: 5 });

    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("VALIDATION_ERROR");
  });
});

// ---------------------------------------------------------------------------
// Finalize rotation
// ---------------------------------------------------------------------------

describe("POST /api/v1/webhooks/subscribers/:subscriberId/rotate/finalize", () => {
  it("finalizes rotation successfully", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "fin-sub" });
    await request(app)
      .post("/api/v1/webhooks/subscribers/fin-sub/rotate")
      .send({});

    const res = await request(app).post(
      "/api/v1/webhooks/subscribers/fin-sub/rotate/finalize"
    );

    expect(res.status).toBe(200);
    expect(res.body.status).toBe(WebhookSecretStatus.Active);
    expect(res.body.message).toContain("finalized");
  });

  it("returns 409 when no rotation is in progress", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "no-rot-fin" });

    const res = await request(app).post(
      "/api/v1/webhooks/subscribers/no-rot-fin/rotate/finalize"
    );

    expect(res.status).toBe(409);
    expect(res.body.error.code).toBe("NO_ROTATION_IN_PROGRESS");
  });

  it("returns 404 for unknown subscriber", async () => {
    const res = await request(app).post(
      "/api/v1/webhooks/subscribers/ghost/rotate/finalize"
    );
    expect(res.status).toBe(404);
  });
});

// ---------------------------------------------------------------------------
// Cancel rotation
// ---------------------------------------------------------------------------

describe("POST /api/v1/webhooks/subscribers/:subscriberId/rotate/cancel", () => {
  it("cancels rotation successfully", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "cancel-sub" });
    await request(app)
      .post("/api/v1/webhooks/subscribers/cancel-sub/rotate")
      .send({});

    const res = await request(app).post(
      "/api/v1/webhooks/subscribers/cancel-sub/rotate/cancel"
    );

    expect(res.status).toBe(200);
    expect(res.body.status).toBe(WebhookSecretStatus.Active);
    expect(res.body.has_pending_secret).toBe(false);
  });

  it("returns 409 when no rotation is in progress", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "no-rot-cancel" });

    const res = await request(app).post(
      "/api/v1/webhooks/subscribers/no-rot-cancel/rotate/cancel"
    );

    expect(res.status).toBe(409);
    expect(res.body.error.code).toBe("NO_ROTATION_IN_PROGRESS");
  });

  it("returns 404 for unknown subscriber", async () => {
    const res = await request(app).post(
      "/api/v1/webhooks/subscribers/ghost/rotate/cancel"
    );
    expect(res.status).toBe(404);
  });
});

// ---------------------------------------------------------------------------
// Ingest endpoint (signature verification)
// ---------------------------------------------------------------------------

describe("POST /api/v1/webhooks/ingest/:subscriberId", () => {
  it("accepts a valid signature and returns received=true", async () => {
    const regRes = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "ingest-sub" });
    const secret = regRes.body.initial_secret as string;

    const payload = '{"event":"invoice.paid"}';
    const sig = sign(payload, secret);

    const res = await request(app)
      .post("/api/v1/webhooks/ingest/ingest-sub")
      .set("x-webhook-subscriber-id", "ingest-sub")
      .set("x-webhook-signature", sig)
      .set("content-type", "application/json")
      .send(payload);

    expect(res.status).toBe(200);
    expect(res.body.received).toBe(true);
    expect(res.body.subscriber_id).toBe("ingest-sub");
    expect(res.body.matched_secret).toBe("primary");
  });

  it("returns 401 for invalid signature", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "ingest-bad" });

    const res = await request(app)
      .post("/api/v1/webhooks/ingest/ingest-bad")
      .set("x-webhook-subscriber-id", "ingest-bad")
      .set("x-webhook-signature", "sha256=badhash")
      .send("payload");

    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("INVALID_WEBHOOK_SIGNATURE");
  });

  it("returns 400 when subscriber header is missing", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/ingest/ingest-sub")
      .set("x-webhook-signature", "sha256=abc")
      .send("payload");

    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("MISSING_SUBSCRIBER_HEADER");
  });

  it("returns 400 when signature header is missing", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/ingest/ingest-sub")
      .set("x-webhook-subscriber-id", "ingest-sub")
      .send("payload");

    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("MISSING_SIGNATURE_HEADER");
  });

  it("returns 401 for unknown subscriber (no enumeration)", async () => {
    const res = await request(app)
      .post("/api/v1/webhooks/ingest/ghost-sub")
      .set("x-webhook-subscriber-id", "ghost-sub")
      .set("x-webhook-signature", "sha256=abc")
      .send("payload");

    expect(res.status).toBe(401);
    expect(res.body.error.message).toBe("Webhook signature verification failed");
  });
});

// ---------------------------------------------------------------------------
// Full rotation lifecycle – zero-downtime dual-verify window
// ---------------------------------------------------------------------------

describe("Full rotation lifecycle – zero-downtime dual-verify window", () => {
  it("simulates a client rolling from old key to new key without downtime", async () => {
    // 1. Register subscriber.
    const regRes = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "lifecycle-sub" });
    expect(regRes.status).toBe(201);
    const oldSecret = regRes.body.initial_secret as string;

    // 2. Verify old secret works before rotation.
    const payload = '{"event":"bid.placed"}';
    const oldSig = sign(payload, oldSecret);

    const preRotRes = await request(app)
      .post("/api/v1/webhooks/ingest/lifecycle-sub")
      .set("x-webhook-subscriber-id", "lifecycle-sub")
      .set("x-webhook-signature", oldSig)
      .set("content-type", "application/json")
      .send(payload);
    expect(preRotRes.status).toBe(200);
    expect(preRotRes.body.matched_secret).toBe("primary");

    // 3. Initiate rotation – receive new secret.
    const rotRes = await request(app)
      .post("/api/v1/webhooks/subscribers/lifecycle-sub/rotate")
      .send({ grace_period_seconds: 3600 });
    expect(rotRes.status).toBe(202);
    const newSecret = rotRes.body.new_secret as string;

    // 4. During grace window: OLD key still accepted.
    const midOldRes = await request(app)
      .post("/api/v1/webhooks/ingest/lifecycle-sub")
      .set("x-webhook-subscriber-id", "lifecycle-sub")
      .set("x-webhook-signature", oldSig)
      .set("content-type", "application/json")
      .send(payload);
    expect(midOldRes.status).toBe(200);
    expect(midOldRes.body.matched_secret).toBe("primary");

    // 5. During grace window: NEW key also accepted.
    const newSig = sign(payload, newSecret);
    const midNewRes = await request(app)
      .post("/api/v1/webhooks/ingest/lifecycle-sub")
      .set("x-webhook-subscriber-id", "lifecycle-sub")
      .set("x-webhook-signature", newSig)
      .set("content-type", "application/json")
      .send(payload);
    expect(midNewRes.status).toBe(200);
    expect(midNewRes.body.matched_secret).toBe("pending");

    // 6. Finalize rotation.
    const finRes = await request(app).post(
      "/api/v1/webhooks/subscribers/lifecycle-sub/rotate/finalize"
    );
    expect(finRes.status).toBe(200);
    expect(finRes.body.status).toBe(WebhookSecretStatus.Active);

    // 7. After finalization: OLD key rejected.
    const postOldRes = await request(app)
      .post("/api/v1/webhooks/ingest/lifecycle-sub")
      .set("x-webhook-subscriber-id", "lifecycle-sub")
      .set("x-webhook-signature", oldSig)
      .set("content-type", "application/json")
      .send(payload);
    expect(postOldRes.status).toBe(401);

    // 8. After finalization: NEW key accepted as primary.
    const postNewRes = await request(app)
      .post("/api/v1/webhooks/ingest/lifecycle-sub")
      .set("x-webhook-subscriber-id", "lifecycle-sub")
      .set("x-webhook-signature", newSig)
      .set("content-type", "application/json")
      .send(payload);
    expect(postNewRes.status).toBe(200);
    expect(postNewRes.body.matched_secret).toBe("primary");
  });

  it("simulates a client cancelling rotation and staying on old key", async () => {
    const regRes = await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "cancel-lifecycle" });
    const oldSecret = regRes.body.initial_secret as string;

    await request(app)
      .post("/api/v1/webhooks/subscribers/cancel-lifecycle/rotate")
      .send({});

    await request(app).post(
      "/api/v1/webhooks/subscribers/cancel-lifecycle/rotate/cancel"
    );

    const payload = "test-payload";
    const sig = sign(payload, oldSecret);

    const res = await request(app)
      .post("/api/v1/webhooks/ingest/cancel-lifecycle")
      .set("x-webhook-subscriber-id", "cancel-lifecycle")
      .set("x-webhook-signature", sig)
      .set("content-type", "text/plain")
      .send(payload);

    expect(res.status).toBe(200);
    expect(res.body.matched_secret).toBe("primary");
  });
});

// ---------------------------------------------------------------------------
// Controller edge cases – null body in initiateRotation
// ---------------------------------------------------------------------------

describe("POST /api/v1/webhooks/subscribers/:subscriberId/rotate – null body", () => {
  it("uses defaults when body is omitted entirely", async () => {
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "null-body-rot" });

    // Send request with no body at all (content-type not set)
    const res = await request(app)
      .post("/api/v1/webhooks/subscribers/null-body-rot/rotate");

    expect(res.status).toBe(202);
    expect(res.body.grace_period_seconds).toBe(3600);
  });
});

// ---------------------------------------------------------------------------
// Controller handleError – non-WebhookSecretError forwarded to global handler
// ---------------------------------------------------------------------------

describe("Webhook controller – unexpected error forwarding", () => {
  it("forwards non-WebhookSecretError from getSubscriber to global error handler", async () => {
    // Register a subscriber so the route is reached, then mock the service
    // to throw a plain Error (not a WebhookSecretError).
    await request(app)
      .post("/api/v1/webhooks/subscribers")
      .send({ subscriber_id: "err-forward-sub" });

    // Temporarily replace the service method on the singleton.
    const original = webhookSecretService.getSubscriberView.bind(webhookSecretService);
    jest
      .spyOn(webhookSecretService, "getSubscriberView")
      .mockImplementationOnce(() => {
        throw new Error("unexpected db error");
      });

    const res = await request(app).get(
      "/api/v1/webhooks/subscribers/err-forward-sub"
    );

    // The global error handler returns 500 for unhandled errors.
    expect(res.status).toBe(500);

    jest.restoreAllMocks();
  });
});
