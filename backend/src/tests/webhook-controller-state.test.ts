import express from "express";
import request from "supertest";
import webhookRouter from "../routes/v1/webhooks";
import { errorHandler } from "../middleware/error-handler";
import { WebhookSecretStatus } from "../types/webhook";
import { webhookSecretService, WebhookSecretService, WebhookSecretStore } from "../services/webhookSecretService";

let store: WebhookSecretStore;
let service: WebhookSecretService;
let app: express.Express;

beforeEach(() => {
  jest.useFakeTimers({ doNotFake: ["nextTick", "setImmediate"] });
  jest.setSystemTime(new Date("2026-01-01T00:00:00Z"));
  store = new WebhookSecretStore();
  service = new WebhookSecretService(store);
  // Use the real service and an isolated store through the production routes.
  jest.spyOn(webhookSecretService, "registerSubscriber").mockImplementation(service.registerSubscriber.bind(service));
  jest.spyOn(webhookSecretService, "getSubscriberView").mockImplementation(service.getSubscriberView.bind(service));
  jest.spyOn(webhookSecretService, "initiateRotation").mockImplementation(service.initiateRotation.bind(service));
  jest.spyOn(webhookSecretService, "finalizeRotation").mockImplementation(service.finalizeRotation.bind(service));
  jest.spyOn(webhookSecretService, "cancelRotation").mockImplementation(service.cancelRotation.bind(service));
  jest.spyOn(webhookSecretService, "verifySignature").mockImplementation(service.verifySignature.bind(service));
  app = express();
  app.use(express.json());
  app.use("/webhooks", webhookRouter);
  app.use(errorHandler);
});

afterEach(() => {
  jest.restoreAllMocks();
  jest.useRealTimers();
});

const SUBSCRIBERS = "/webhooks/subscribers";
const ROTATE = `${SUBSCRIBERS}/subscriber/rotate`;

describe("webhook controller state and recovery", () => {
  it.each([undefined, "route", "router"])("returns a safe 500 for thrown %p instead of skipping the route", async (value) => {
    const log = jest.spyOn(console, "error").mockImplementation(() => {});
    jest.spyOn(webhookSecretService, "getSubscriberView").mockImplementation(() => { throw value; });
    app.get(`${SUBSCRIBERS}/subscriber`, (_req, res) => { res.sendStatus(418); });

    const result = await request(app).get(`${SUBSCRIBERS}/subscriber`).expect(500);

    expect(result.body).toEqual({ error: {
      message: "Unexpected webhook controller failure",
      code: "WEBHOOK_CONTROLLER_ERROR",
    } });
    expect(log).toHaveBeenCalledWith("[Error] WEBHOOK_CONTROLLER_ERROR: Unexpected webhook controller failure", "");
    expect(store._all()).toEqual([]);
  });

  it.each([60, 86400])("accepts grace-period boundary %i and exposes only public state on reads", async (grace) => {
    const registered = await request(app).post(SUBSCRIBERS).send({ subscriber_id: "subscriber", grace_period_seconds: grace });
    expect(registered.status).toBe(201);
    expect(registered.body.initial_secret).toMatch(/^[a-f0-9]{64}$/);

    const read = await request(app).get(`${SUBSCRIBERS}/subscriber`);
    expect(read.status).toBe(200);
    expect(read.body).toEqual(service.getSubscriberView("subscriber"));
    expect(read.body.grace_period_seconds).toBe(grace);
    expect(JSON.stringify(read.body)).not.toContain(registered.body.initial_secret);
  });

  it("accepts an absent rotation body with the documented default grace period", async () => {
    service.registerSubscriber("subscriber");
    const result = await request(app).post(ROTATE).expect(202);
    expect(result.body).toMatchObject({ subscriber_id: "subscriber", status: "rotating", grace_period_seconds: 3600 });
    expect(store.get("subscriber")!.pending_secret).toBe(result.body.new_secret);
  });

  it.each([
    { subscriber_id: "" },
    { subscriber_id: "x".repeat(129) },
    { subscriber_id: "subscriber", grace_period_seconds: 59 },
    { subscriber_id: "subscriber", grace_period_seconds: 86401 },
    { subscriber_id: "subscriber", grace_period_seconds: 60.5 },
    { subscriber_id: "subscriber", algorithm: "unsupported" },
  ])("rejects invalid registration without mutating state: %p", async (body) => {
    const result = await request(app).post(SUBSCRIBERS).send(body);
    expect(result.status).toBe(400);
    expect(result.body.error.code).toBe("VALIDATION_ERROR");
    expect(webhookSecretService.registerSubscriber).not.toHaveBeenCalled();
    expect(store._all()).toEqual([]);
  });

  it("serializes duplicate registration and rotation requests without replacing secrets", async () => {
    const registrations = await Promise.all([1, 2].map(() => request(app).post(SUBSCRIBERS).send({ subscriber_id: "subscriber" })));
    expect(registrations.map((r) => r.status).sort()).toEqual([201, 409]);
    const initialSecret = registrations.find((r) => r.status === 201)!.body.initial_secret;
    expect(registrations.find((r) => r.status === 409)!.body.error.code).toBe("SUBSCRIBER_ALREADY_EXISTS");
    expect(store._all()).toHaveLength(1);
    expect(store.get("subscriber")!.primary_secret).toBe(initialSecret);

    const rotations = await Promise.all([1, 2].map(() => request(app).post(ROTATE).send({ grace_period_seconds: 60 })));
    expect(rotations.map((r) => r.status).sort()).toEqual([202, 409]);
    const pending = rotations.find((r) => r.status === 202)!.body.new_secret;
    const rejected = rotations.find((r) => r.status === 409)!;
    expect(rejected.body.error.code).toBe("ROTATION_ALREADY_IN_PROGRESS");
    expect(JSON.stringify(rejected.body)).not.toContain(pending);
    expect(store.get("subscriber")).toMatchObject({ primary_secret: initialSecret, pending_secret: pending, status: "rotating" });
  });

  it("preserves a pending rotation after invalid input and allows cancel/retry recovery", async () => {
    service.registerSubscriber("subscriber");
    await request(app).post(ROTATE).send({ grace_period_seconds: 60 }).expect(202);
    const before = { ...store.get("subscriber")! };
    await request(app).post(ROTATE).send({ grace_period_seconds: 59 }).expect(400);
    expect(store.get("subscriber")).toEqual(before);

    await request(app).post(`${ROTATE}/cancel`).expect(200);
    const canceled = { ...store.get("subscriber")! };
    const duplicate = await request(app).post(`${ROTATE}/cancel`).expect(409);
    expect(duplicate.body.error.code).toBe("NO_ROTATION_IN_PROGRESS");
    expect(store.get("subscriber")).toEqual(canceled);
    await request(app).post(ROTATE).send({ grace_period_seconds: 60 }).expect(202);
    expect(store.get("subscriber")!.primary_secret).toBe(before.primary_secret);
    expect(store.get("subscriber")!.pending_secret).not.toBe(before.pending_secret);
  });

  it("finalizes once and rejects a duplicate without changing the promoted key", async () => {
    service.registerSubscriber("subscriber");
    const rotation = await request(app).post(ROTATE).send({ grace_period_seconds: 60 }).expect(202);
    const finalized = await request(app).post(`${ROTATE}/finalize`).expect(200);
    expect(finalized.body.status).toBe("active");
    expect(JSON.stringify(finalized.body)).not.toContain(rotation.body.new_secret);
    const before = { ...store.get("subscriber")! };
    expect(before.primary_secret).toBe(rotation.body.new_secret);
    expect(before.pending_secret).toBeNull();
    const duplicate = await request(app).post(`${ROTATE}/finalize`).expect(409);
    expect(duplicate.body.error.code).toBe("NO_ROTATION_IN_PROGRESS");
    expect(store.get("subscriber")).toEqual(before);
  });

  it("rejects inconsistent rotation state without modifying or disclosing secrets", async () => {
    service.registerSubscriber("subscriber");
    store.set({ ...store.get("subscriber")!, status: WebhookSecretStatus.Rotating, pending_secret: null });
    const before = { ...store.get("subscriber")! };
    const result = await request(app).post(`${ROTATE}/finalize`).expect(500);
    expect(result.body.error.code).toBe("ROTATION_STATE_INCONSISTENT");
    expect(store.get("subscriber")).toEqual(before);
    expect(JSON.stringify(result.body)).not.toContain(before.primary_secret);
  });

  it("keeps signature enforcement across the exact grace expiry and rejects stale finalization", async () => {
    const { initial_secret: primary } = service.registerSubscriber("subscriber");
    const rotation = await request(app).post(ROTATE).send({ grace_period_seconds: 60 }).expect(202);
    const payload = "signed-payload";
    const ingest = (signature: string) => request(app).post("/webhooks/ingest/subscriber")
      .set("Content-Type", "text/plain")
      .set("X-Webhook-Subscriber-Id", "subscriber")
      .set("X-Webhook-Signature", signature).send(payload);

    const denied = await ingest("sha256=invalid").expect(401);
    expect(denied.body.error.code).toBe("INVALID_WEBHOOK_SIGNATURE");
    expect(store.get("subscriber")!.status).toBe("rotating");
    jest.setSystemTime(new Date("2026-01-01T00:00:59.999Z"));
    await ingest(service.computeSignature(payload, primary)).expect(200);
    jest.setSystemTime(new Date("2026-01-01T00:01:00Z"));
    await ingest(service.computeSignature(payload, primary)).expect(401);
    const accepted = await ingest(service.computeSignature(payload, rotation.body.new_secret)).expect(200);
    expect(accepted.body).toEqual({ received: true, subscriber_id: "subscriber", matched_secret: "primary" });
    const before = { ...store.get("subscriber")! };
    const stale = await request(app).post(`${ROTATE}/finalize`).expect(409);
    expect(stale.body.error.code).toBe("NO_ROTATION_IN_PROGRESS");
    expect(store.get("subscriber")).toEqual(before);
  });

  it("forwards a pre-commit store failure safely and permits a successful retry", async () => {
    const log = jest.spyOn(console, "error").mockImplementation(() => {});
    jest.spyOn(store, "set").mockImplementationOnce(() => { throw new Error("Store temporarily unavailable"); });
    const rejected = await request(app).post(SUBSCRIBERS).send({ subscriber_id: "subscriber", secret: "must-not-echo" }).expect(500);
    expect(rejected.body.error.code).toBe("INTERNAL_ERROR");
    expect(store._all()).toEqual([]);
    expect(JSON.stringify(rejected.body)).not.toContain("must-not-echo");
    expect(JSON.stringify(log.mock.calls)).not.toContain("must-not-echo");
    await request(app).post(SUBSCRIBERS).send({ subscriber_id: "subscriber" }).expect(201);
    expect(store._all()).toHaveLength(1);
    const before = { ...store.get("subscriber")! };
    jest.spyOn(store, "set").mockImplementationOnce(() => { throw new Error("Store temporarily unavailable"); });
    await request(app).post(ROTATE).send({ grace_period_seconds: 60 }).expect(500);
    expect(store.get("subscriber")).toEqual(before);
    await request(app).post(ROTATE).send({ grace_period_seconds: 60 }).expect(202);
    expect(store.get("subscriber")!.primary_secret).toBe(before.primary_secret);
  });
});
