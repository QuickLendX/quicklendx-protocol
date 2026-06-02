import path from "path";
import crypto from "crypto";
import { getDatabase, closeDatabase } from "../lib/database";
import { WebhookQueueService, webhookQueueService } from "../services/webhookQueueService";

// ---------------------------------------------------------------------------
// Test database lifecycle – isolated temp file per run
// ---------------------------------------------------------------------------

const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-webhook-queue-${crypto.randomUUID()}.db`);

beforeAll(() => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();

  const conn = getDatabase();
  conn.exec(`
    CREATE TABLE IF NOT EXISTS webhook_queue (
      id TEXT PRIMARY KEY,
      type TEXT NOT NULL,
      payload TEXT NOT NULL,
      status TEXT NOT NULL CHECK(status IN ('pending','processing','success','failed')),
      enqueued_at TEXT NOT NULL DEFAULT (datetime('now'))
    )
  `);

  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_webhook_queue_status_enqueued
    ON webhook_queue(status, enqueued_at)
  `);

  conn.exec(`
    CREATE TABLE IF NOT EXISTS queue_metadata (
      key TEXT PRIMARY KEY,
      value INTEGER NOT NULL DEFAULT 0
    )
  `);

  conn.exec(`
    INSERT OR IGNORE INTO queue_metadata (key, value) VALUES ('overflow_count', 0)
  `);
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
  conn.exec("DELETE FROM webhook_queue");
  conn.exec("UPDATE queue_metadata SET value = 0 WHERE key = 'overflow_count'");
  WebhookQueueService.resetInstance();
});

describe("Durable Webhook Queue Resilience Tests", () => {
  test("should persist queue items across simulated service restarts", async () => {
    const event = webhookQueueService.enqueue("user.created", { userId: 123 });
    expect(event.id).toBeDefined();
    expect(event.type).toBe("user.created");

    // Simulate restart by resetting instance
    WebhookQueueService.resetInstance();
    const newService = WebhookQueueService.getInstance();

    const stats = newService.getStats();
    expect(stats.size).toBe(1);
    expect(stats.pendingCount).toBe(1);
  });

  test("should enforce strict back-pressure constraints and log overflow metrics", async () => {
    // Fill queue to max capacity
    for (let i = 0; i < 5000; i++) {
      webhookQueueService.enqueue("test.event", { index: i });
    }

    // Next one should throw 503
    let caughtError: any = null;
    try {
      webhookQueueService.enqueue("test.event", { index: 5000 });
    } catch (err) {
      caughtError = err;
    }

    expect(caughtError).toBeDefined();
    expect(caughtError.statusCode).toBe(503);

    const stats = webhookQueueService.getStats();
    expect(stats.overflowCount).toBeGreaterThanOrEqual(1);
  });

  test("should handle marking success/failure", async () => {
    const event = webhookQueueService.enqueue("order.placed", { id: 1 });

    const successResult = webhookQueueService.markSuccess(event.id);
    expect(successResult).toBe(true);

    const stats = webhookQueueService.getStats();
    expect(stats.successCount).toBe(1);
    expect(stats.pendingCount).toBe(0);

    // Mark success again should be idempotent
    const repeatSuccessResult = webhookQueueService.markSuccess(event.id);
    expect(repeatSuccessResult).toBe(false);

    const event2 = webhookQueueService.enqueue("order.cancelled", { id: 2 });
    const failResult = webhookQueueService.markFailed(event2.id);
    expect(failResult).toBe(true);

    const stats2 = webhookQueueService.getStats();
    expect(stats2.failureCount).toBe(1);
  });

  test("should flush pending events", async () => {
    webhookQueueService.enqueue("event.1", {});
    webhookQueueService.enqueue("event.2", {});
    webhookQueueService.enqueue("event.3", {});

    const pending = webhookQueueService.flush();
    expect(pending.length).toBe(3);

    const stats = webhookQueueService.getStats();
    expect(stats.size).toBe(0);
  });
});
