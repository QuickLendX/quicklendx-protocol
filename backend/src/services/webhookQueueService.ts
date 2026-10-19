import { z } from "zod";
import { webhookDeliveryRepo } from "./webhookDeliveryRepo";
import type { WebhookDelivery } from "./webhookDeliveryRepo";

const MAX_CAPACITY = 5000;

export const WebhookEventStatusSchema = z.enum([
  "pending",
  "processing",
  "success",
  "failed",
  "dead_letter",
]);

export type WebhookEventStatus = z.infer<typeof WebhookEventStatusSchema>;

export interface WebhookEvent {
  id: string;
  type: string;
  payload: unknown;
  enqueuedAt: string;
  status: WebhookEventStatus;
}

export interface WebhookDeliveryInfo {
  id: string;
  eventType: string;
  payload: unknown;
  subscriberId: string | null;
  status: WebhookEventStatus;
  enqueuedAt: string;
  attemptCount: number;
  maxAttempts: number;
  nextRetryAt: string | null;
  lastError: string | null;
  lastAttemptAt: string | null;
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

function deliveryToEvent(d: WebhookDelivery): WebhookEvent {
  return {
    id: d.id,
    type: d.eventType,
    payload: d.payload,
    enqueuedAt: d.enqueuedAt,
    status: d.status,
  };
}

class WebhookQueueService {
  private static instance: WebhookQueueService;

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
    try {
      return this.db.transaction(() => {
        // Check current size of pending/processing elements
        const rowCount = this.db
          .prepare("SELECT COUNT(*) as count FROM webhook_queue WHERE status IN ('pending', 'processing')")
          .get().count;

        if (rowCount >= MAX_CAPACITY) {
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
          .run(id, type, JSON.stringify(payload ?? null), "pending", enqueuedAt);

        return event;
      })();
    } catch (err) {
      if ((err as any)?.statusCode === 503) {
        // The transaction above rolls back on overflow, which would also roll
        // back an in-transaction counter update. Persist the overflow metric
        // outside the failed transaction so it is never lost.
        this.db
          .prepare("UPDATE queue_metadata SET value = value + 1 WHERE key = 'overflow_count'")
          .run();
      }
      throw err;
    }
  }

  markSuccess(id: string): boolean {
    return webhookDeliveryRepo.markSuccess(id);
  }

  markFailed(id: string): WebhookDelivery | null {
    return webhookDeliveryRepo.markFailed(id);
  }

  getStats(): WebhookQueueStats {
    const s = webhookDeliveryRepo.getStats();
    return {
      depth: s.pending + s.processing,
      size: s.pending + s.processing,
      capacity: MAX_CAPACITY,
      overflowCount: webhookDeliveryRepo.getOverflowCount(),
      pendingCount: s.pending,
      successCount: s.success,
      failureCount: s.failed,
      oldestTimestamp: s.oldestPending,
    };
  }

  getDepth(): number {
    return this.getStats().size;
  }

  flush(): WebhookEvent[] {
    const pending = webhookDeliveryRepo.getPending();
    for (const delivery of pending) {
      webhookDeliveryRepo.markSuccess(delivery.id);
    }
    return pending.map(deliveryToEvent);
  }

  getPendingDeliveries(): WebhookDelivery[] {
    return webhookDeliveryRepo.getPending();
  }

  getDeadLetters(): WebhookDelivery[] {
    return webhookDeliveryRepo.getDeadLetters();
  }

  retryDeadLetter(id: string): boolean {
    return webhookDeliveryRepo.retryDeadLetter(id);
  }

  cleanupDeliveries(olderThanDays?: number): number {
    return webhookDeliveryRepo.cleanup(olderThanDays);
  }

  vacuumDeliveries(): void {
    webhookDeliveryRepo.vacuum();
  }

  getDeliveryInfo(id: string): WebhookDeliveryInfo | null {
    const delivery = webhookDeliveryRepo.getById(id);
    if (!delivery) return null;
    return {
      id: delivery.id,
      eventType: delivery.eventType,
      payload: delivery.payload,
      subscriberId: delivery.subscriberId,
      status: delivery.status,
      enqueuedAt: delivery.enqueuedAt,
      attemptCount: delivery.attemptCount,
      maxAttempts: delivery.maxAttempts,
      nextRetryAt: delivery.nextRetryAt,
      lastError: delivery.lastError,
      lastAttemptAt: delivery.lastAttemptAt,
    };
  }
}

export const webhookQueueService = WebhookQueueService.getInstance();
export { WebhookQueueService };
