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
    WebhookQueueService.instance = undefined as any;
  }

  drain(subscriberId: string): { pending: number; drained: number } {
    return webhookDeliveryRepo.drain(subscriberId);
  }


  enqueue(type: string, payload?: unknown): WebhookEvent {
    const stats = webhookDeliveryRepo.getStats();
    if (stats.pending + stats.processing >= MAX_CAPACITY) {
      webhookDeliveryRepo.incrementOverflow();
      const err = new Error("Webhook queue capacity exceeded");
      (err as any).statusCode = 503;
      throw err;
    }

    const delivery = webhookDeliveryRepo.create({
      eventType: type,
      payload,
    });
    return deliveryToEvent(delivery);
  }

  enqueueWithSubscriber(
    type: string,
    payload: unknown,
    subscriberId: string
  ): WebhookEvent {
    const stats = webhookDeliveryRepo.getStats();
    if (stats.pending + stats.processing >= MAX_CAPACITY) {
      webhookDeliveryRepo.incrementOverflow();
      const err = new Error("Webhook queue capacity exceeded");
      (err as any).statusCode = 503;
      throw err;
    }

    const delivery = webhookDeliveryRepo.create({
      eventType: type,
      payload,
      subscriberId,
    });
    return deliveryToEvent(delivery);
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
