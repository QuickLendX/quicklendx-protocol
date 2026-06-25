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
  require("fs").mkdirSync(TEST_DB_DIR, { recursive: true });

  const conn = getDatabase();
  conn.exec(`
    CREATE TABLE IF NOT EXISTS webhook_deliveries (
      id TEXT PRIMARY KEY,
      event_type TEXT NOT NULL,
      payload TEXT NOT NULL,
      subscriber_id TEXT,
      status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','processing','success','failed','dead_letter')),
      enqueued_at TEXT NOT NULL DEFAULT (datetime('now')),
      attempt_count INTEGER NOT NULL DEFAULT 0,
      max_attempts INTEGER NOT NULL DEFAULT 5,
      next_retry_at TEXT,
      last_error TEXT,
      last_attempt_at TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )
  `);

  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_status_next_retry
    ON webhook_deliveries(status, next_retry_at)
  `);

  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_created_at
    ON webhook_deliveries(created_at)
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
  conn.exec("DELETE FROM webhook_deliveries");
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
    expect(failResult).not.toBeNull();
    expect(failResult!.status).toBe("failed");
    expect(failResult!.attemptCount).toBe(1);

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
