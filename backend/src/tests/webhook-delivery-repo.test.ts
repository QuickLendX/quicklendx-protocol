import path from "path";
import crypto from "crypto";
import { getDatabase, closeDatabase } from "../lib/database";
import { WebhookDeliveryRepo, computeNextRetry, MAX_RETRY_ATTEMPTS } from "../services/webhookDeliveryRepo";
import { WebhookQueueService, webhookQueueService } from "../services/webhookQueueService";

const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-webhook-delivery-${crypto.randomUUID()}.db`);

let repo: WebhookDeliveryRepo;

beforeAll(() => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();
  repo = new WebhookDeliveryRepo();
  repo.ensureSchema();
});

afterAll(() => {
  closeDatabase();
  try {
    require("fs").unlinkSync(TEST_DB_PATH);
  } catch {
    // ignore
  }
});

beforeEach(() => {
  const conn = getDatabase();
  conn.exec("DELETE FROM webhook_deliveries");
  WebhookQueueService.resetInstance();
});

describe("WebhookDeliveryRepo", () => {
  describe("create and getById", () => {
    it("should create a delivery with defaults", () => {
      const delivery = repo.create({
        eventType: "user.created",
        payload: { userId: 42 },
      });

      expect(delivery.id).toBeDefined();
      expect(delivery.eventType).toBe("user.created");
      expect(delivery.payload).toEqual({ userId: 42 });
      expect(delivery.status).toBe("pending");
      expect(delivery.attemptCount).toBe(0);
      expect(delivery.maxAttempts).toBe(MAX_RETRY_ATTEMPTS);

      const fetched = repo.getById(delivery.id);
      expect(fetched).not.toBeNull();
      expect(fetched!.id).toBe(delivery.id);
      expect(fetched!.payload).toEqual({ userId: 42 });
    });

    it("should create with subscriberId", () => {
      const delivery = repo.create({
        eventType: "invoice.paid",
        payload: { invoiceId: "inv-1" },
        subscriberId: "sub-abc",
      });
      expect(delivery.subscriberId).toBe("sub-abc");

      const fetched = repo.getById(delivery.id);
      expect(fetched!.subscriberId).toBe("sub-abc");
    });

    it("should return null for unknown id", () => {
      expect(repo.getById("nonexistent")).toBeNull();
    });
  });

  describe("status transitions", () => {
    it("should mark as processing", () => {
      const d = repo.create({ eventType: "test", payload: {} });
      expect(repo.markProcessing(d.id)).toBe(true);
      const fetched = repo.getById(d.id);
      expect(fetched!.status).toBe("processing");
    });

    it("should not mark processing on already processed", () => {
      const d = repo.create({ eventType: "test", payload: {} });
      repo.markProcessing(d.id);
      expect(repo.markProcessing(d.id)).toBe(false);
    });

    it("should mark success from pending", () => {
      const d = repo.create({ eventType: "test", payload: {} });
      expect(repo.markSuccess(d.id)).toBe(true);
      expect(repo.getById(d.id)!.status).toBe("success");
    });

    it("should mark success from processing", () => {
      const d = repo.create({ eventType: "test", payload: {} });
      repo.markProcessing(d.id);
      expect(repo.markSuccess(d.id)).toBe(true);
      expect(repo.getById(d.id)!.status).toBe("success");
    });

    it("should be idempotent on markSuccess", () => {
      const d = repo.create({ eventType: "test", payload: {} });
      repo.markSuccess(d.id);
      expect(repo.markSuccess(d.id)).toBe(false);
    });
  });

  describe("markFailed with retry schedule", () => {
    it("should set status to failed and increment attempt count", () => {
      const d = repo.create({ eventType: "test", payload: {} });
      const updated = repo.markFailed(d.id, "Connection refused");

      expect(updated).not.toBeNull();
      expect(updated!.status).toBe("failed");
      expect(updated!.attemptCount).toBe(1);
      expect(updated!.lastError).toBe("Connection refused");
      expect(updated!.nextRetryAt).not.toBeNull();
    });

    it("should promote to dead_letter after max attempts", () => {
      const d = repo.create({ eventType: "test", payload: {}, maxAttempts: 2 });

      const first = repo.markFailed(d.id, "err1");
      expect(first!.status).toBe("failed");
      expect(first!.attemptCount).toBe(1);

      const second = repo.markFailed(d.id, "err2");
      expect(second!.status).toBe("dead_letter");
      expect(second!.attemptCount).toBe(2);
      expect(second!.nextRetryAt).toBeNull();
    });

    it("should return null for nonexistent delivery", () => {
      expect(repo.markFailed("nobody", "err")).toBeNull();
    });

    it("should not mark failed if already success", () => {
      const d = repo.create({ eventType: "test", payload: {} });
      repo.markSuccess(d.id);
      expect(repo.markFailed(d.id, "too late")).toBeNull();
    });
  });

  describe("getPending", () => {
    it("should return pending deliveries without retry schedule", () => {
      repo.create({ eventType: "a", payload: {} });
      repo.create({ eventType: "b", payload: {} });
      expect(repo.getPending().length).toBe(2);
    });

    it("should return pending deliveries with past due retry", () => {
      const d = repo.create({ eventType: "a", payload: {} });
      repo.markFailed(d.id, "retry later");
      expect(repo.getPending().length).toBe(0);

      const db = getDatabase();
      db.prepare(
        "UPDATE webhook_deliveries SET next_retry_at = '2020-01-01T00:00:00.000Z' WHERE id = ?"
      ).run(d.id);

      const pending = repo.getPending();
      expect(pending.length).toBe(1);
      expect(pending[0].id).toBe(d.id);
    });

    it("should not return processing or success deliveries", () => {
      const d1 = repo.create({ eventType: "a", payload: {} });
      const d2 = repo.create({ eventType: "b", payload: {} });
      repo.markSuccess(d2.id);
      repo.markProcessing(d1.id);

      expect(repo.getPending().length).toBe(0);
    });
  });

  describe("dead letter queue", () => {
    it("should retrieve dead letters", () => {
      const d = repo.create({ eventType: "test", payload: {}, maxAttempts: 1 });
      repo.markFailed(d.id, "fatal");

      const deadLetters = repo.getDeadLetters();
      expect(deadLetters.length).toBe(1);
      expect(deadLetters[0].id).toBe(d.id);
    });

    it("should retry dead letter back to pending", () => {
      const d = repo.create({ eventType: "test", payload: {}, maxAttempts: 1 });
      repo.markFailed(d.id, "fatal");

      expect(repo.getDeadLetters().length).toBe(1);
      expect(repo.retryDeadLetter(d.id)).toBe(true);
      expect(repo.getById(d.id)!.status).toBe("pending");
    });

    it("should not retry non-dead-letter", () => {
      const d = repo.create({ eventType: "test", payload: {} });
      expect(repo.retryDeadLetter(d.id)).toBe(false);
    });
  });

  describe("getStats", () => {
    it("should return correct counts", () => {
      expect(repo.getStats().total).toBe(0);

      repo.create({ eventType: "a", payload: {} });
      repo.create({ eventType: "b", payload: {} });
      const d3 = repo.create({ eventType: "c", payload: {} });
      repo.markSuccess(d3.id);

      const stats = repo.getStats();
      expect(stats.total).toBe(3);
      expect(stats.pending).toBe(2);
      expect(stats.success).toBe(1);
    });
  });

  describe("cleanup", () => {
    it("should delete old successful and dead_letter deliveries", () => {
      const db = getDatabase();
      const oldDate = "2020-01-01T00:00:00.000Z";

      const d1 = repo.create({ eventType: "old1", payload: {} });
      const d2 = repo.create({ eventType: "old2", payload: {} });

      db.prepare(
        "UPDATE webhook_deliveries SET status = 'success', created_at = ? WHERE id = ?"
      ).run(oldDate, d1.id);

      db.prepare(
        "UPDATE webhook_deliveries SET status = 'dead_letter', created_at = ? WHERE id = ?"
      ).run(oldDate, d2.id);

      const d3 = repo.create({ eventType: "recent", payload: {} });
      db.prepare("UPDATE webhook_deliveries SET created_at = ? WHERE id = ?").run(
        new Date().toISOString(),
        d3.id
      );

      const deleted = repo.cleanup(1);
      expect(deleted).toBe(2);
    });

    it("should not delete pending or processing deliveries", () => {
      const db = getDatabase();
      const oldDate = "2020-01-01T00:00:00.000Z";

      const d = repo.create({ eventType: "old.pending", payload: {} });
      db.prepare(
        "UPDATE webhook_deliveries SET created_at = ? WHERE id = ?"
      ).run(oldDate, d.id);

      expect(repo.cleanup(1)).toBe(0);
    });
  });

  describe("computeNextRetry", () => {
    it("should return null beyond max attempts", () => {
      expect(computeNextRetry(MAX_RETRY_ATTEMPTS)).toBeNull();
      expect(computeNextRetry(99)).toBeNull();
    });

    it("should return a future date for valid attempts", () => {
      const next = computeNextRetry(0);
      expect(next).not.toBeNull();
      expect(new Date(next!).getTime()).toBeGreaterThan(Date.now());
    });
  });
});

describe("WebhookQueueService integration", () => {
  it("should enqueue and retrieve via service", () => {
    const event = webhookQueueService.enqueue("user.created", { id: 1 });
    expect(event.type).toBe("user.created");
    expect(event.status).toBe("pending");
  });

  it("should persist across restart", () => {
    const event = webhookQueueService.enqueue("persist.test", { x: 1 });
    expect(event.id).toBeDefined();

    WebhookQueueService.resetInstance();
    const svc = WebhookQueueService.getInstance();

    const info = svc.getDeliveryInfo(event.id);
    expect(info).not.toBeNull();
    expect(info!.eventType).toBe("persist.test");
    expect(info!.payload).toEqual({ x: 1 });
  });

  it("should honor retry schedule after restart", () => {
    const event = webhookQueueService.enqueue("retry.test", {});
    const failed = webhookQueueService.markFailed(event.id);
    expect(failed).not.toBeNull();
    expect(failed!.status).toBe("failed");
    expect(failed!.nextRetryAt).not.toBeNull();

    WebhookQueueService.resetInstance();
    const svc = WebhookQueueService.getInstance();

    const info = svc.getDeliveryInfo(event.id);
    expect(info).not.toBeNull();
    expect(info!.attemptCount).toBe(1);
    expect(info!.status).toBe("failed");
    expect(info!.nextRetryAt).not.toBeNull();
  });

  it("should promote to DLQ at max attempts", () => {
    const event = webhookQueueService.enqueueWithSubscriber("dlq.test", {}, "sub-1");
    for (let i = 0; i < 5; i++) {
      webhookQueueService.markFailed(event.id);
    }
    const info = webhookQueueService.getDeliveryInfo(event.id);
    expect(info!.status).toBe("dead_letter");

    const deadLetters = webhookQueueService.getDeadLetters();
    expect(deadLetters.some((dl) => dl.id === event.id)).toBe(true);
  });

  it("should retrieve DLQ entries", () => {
    const e1 = webhookQueueService.enqueue("dlq1", {});
    const e2 = webhookQueueService.enqueue("dlq2", {});
    for (let i = 0; i < 5; i++) {
      webhookQueueService.markFailed(e1.id);
      webhookQueueService.markFailed(e2.id);
    }

    const deadLetters = webhookQueueService.getDeadLetters();
    expect(deadLetters.length).toBe(2);
  });

  it("should cleanup old deliveries", () => {
    const db = getDatabase();
    const d = webhookQueueService.enqueue("cleanup.test", {});
    webhookQueueService.markSuccess(d.id);
    db.prepare(
      "UPDATE webhook_deliveries SET created_at = '2020-01-01T00:00:00.000Z' WHERE id = ?"
    ).run(d.id);

    const deleted = webhookQueueService.cleanupDeliveries(1);
    expect(deleted).toBe(1);
    expect(webhookQueueService.getDeliveryInfo(d.id)).toBeNull();
  });
});
