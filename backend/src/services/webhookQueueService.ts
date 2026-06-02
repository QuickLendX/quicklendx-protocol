import { z } from "zod";
import { ulid } from "ulid";
import { getDatabase } from "../lib/database";

const MAX_CAPACITY = 5000;

export const WebhookEventStatusSchema = z.enum([
  "pending",
  "processing",
  "success",
  "failed",
]);

export type WebhookEventStatus = z.infer<typeof WebhookEventStatusSchema>;

export interface WebhookEvent {
  id: string;
  type: string;
  payload: unknown;
  enqueuedAt: string;
  status: WebhookEventStatus;
}

const WebhookQueueStatsSchema = z.object({
  depth: z.number().int().min(0),
  size: z.number().int().min(0),
  capacity: z.number().int().min(0),
  overflowCount: z.number().int().min(0),
  pendingCount: z.number().int().min(0),
  successCount: z.number().int().min(0),
  failureCount: z.number().int().min(0),
  oldestTimestamp: z.string().datetime().nullable(),
});

export type WebhookQueueStats = z.infer<typeof WebhookQueueStatsSchema>;

class WebhookQueueService {
  private static instance: WebhookQueueService;
  private db: any;

  private constructor() {
    this.db = getDatabase();
  }

  public static getInstance(): WebhookQueueService {
    if (!WebhookQueueService.instance) {
      WebhookQueueService.instance = new WebhookQueueService();
    }
    return WebhookQueueService.instance;
  }

  public static resetInstance(): void {
    WebhookQueueService.instance = new WebhookQueueService();
  }

  enqueue(type: string, payload?: unknown): WebhookEvent {
    return this.db.transaction(() => {
      // Check current size of pending/processing elements
      const rowCount = this.db
        .prepare("SELECT COUNT(*) as count FROM webhook_queue WHERE status IN ('pending', 'processing')")
        .get().count;

      if (rowCount >= MAX_CAPACITY) {
        // Increment persistent overflow counter
        this.db.prepare("UPDATE queue_metadata SET value = value + 1 WHERE key = 'overflow_count'").run();
        const err = new Error("Webhook queue capacity exceeded");
        (err as any).statusCode = 503;
        throw err;
      }

      const id = ulid();
      const enqueuedAt = new Date().toISOString();
      const event: WebhookEvent = {
        id,
        type,
        payload,
        enqueuedAt,
        status: "pending",
      };

      this.db
        .prepare(`
          INSERT INTO webhook_queue (id, type, payload, status, enqueued_at)
          VALUES (?, ?, ?, ?, ?)
        `)
        .run(id, type, JSON.stringify(payload), "pending", enqueuedAt);

      return event;
    })();
  }

  markSuccess(id: string): boolean {
    const result = this.db
      .prepare("UPDATE webhook_queue SET status = 'success' WHERE id = ? AND status IN ('pending', 'processing')")
      .run(id);
    return result.changes > 0;
  }

  markFailed(id: string): boolean {
    const result = this.db
      .prepare("UPDATE webhook_queue SET status = 'failed' WHERE id = ? AND status IN ('pending', 'processing')")
      .run(id);
    return result.changes > 0;
  }

  getStats(): WebhookQueueStats {
    const counts = this.db
      .prepare(`
        SELECT
          COUNT(CASE WHEN status IN ('pending', 'processing') THEN 1 END) as size,
          COUNT(CASE WHEN status = 'pending' THEN 1 END) as pendingCount,
          COUNT(CASE WHEN status = 'success' THEN 1 END) as successCount,
          COUNT(CASE WHEN status = 'failed' THEN 1 END) as failureCount,
          MIN(CASE WHEN status IN ('pending', 'processing') THEN enqueued_at END) as oldestTimestamp
        FROM webhook_queue
      `)
      .get();

    const overflow = this.db
      .prepare("SELECT value FROM queue_metadata WHERE key = 'overflow_count'")
      .get();

    return {
      depth: counts.size || 0,
      size: counts.size || 0,
      capacity: MAX_CAPACITY,
      overflowCount: overflow?.value || 0,
      pendingCount: counts.pendingCount || 0,
      successCount: counts.successCount || 0,
      failureCount: counts.failureCount || 0,
      oldestTimestamp: counts.oldestTimestamp || null,
    };
  }

  getDepth(): number {
    return this.getStats().size;
  }

  /**
   * Drain all pending events from the queue and reset it to empty.
   */
  flush(): WebhookEvent[] {
    const pending = this.db
      .prepare("SELECT * FROM webhook_queue WHERE status = 'pending'")
      .all()
      .map((row: any) => ({
        id: row.id,
        type: row.type,
        payload: JSON.parse(row.payload),
        enqueuedAt: row.enqueued_at,
        status: row.status as WebhookEventStatus,
      }));

    // Clear all pending events
    this.db.prepare("DELETE FROM webhook_queue WHERE status = 'pending'").run();

    return pending;
  }
}

export const webhookQueueService = WebhookQueueService.getInstance();
export { WebhookQueueService };
