import { z } from "zod";
import { ulid } from "ulid";

const DEFAULT_MAX_SIZE = 1000;

export const WebhookEventStatusSchema = z.enum([
  "pending",
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
  successCount: z.number().int().min(0),
  failureCount: z.number().int().min(0),
  overflowCount: z.number().int().min(0),
  oldestTimestamp: z.string().datetime().nullable(),
});

export type WebhookQueueStats = z.infer<typeof WebhookQueueStatsSchema>;

class WebhookQueueService {
  private static instance: WebhookQueueService;

  private buffer: WebhookEvent[];
  private maxSize: number;
  private head: number = 0;
  private tail: number = 0;
  private count: number = 0;
  private overflowCount: number = 0;
  private successCount: number = 0;
  private failureCount: number = 0;

  private constructor(maxSize?: number) {
    this.maxSize = maxSize ?? DEFAULT_MAX_SIZE;
    this.buffer = new Array(this.maxSize);
  }

  public static getInstance(maxSize?: number): WebhookQueueService {
    if (!WebhookQueueService.instance) {
      WebhookQueueService.instance = new WebhookQueueService(maxSize);
    }
    return WebhookQueueService.instance;
  }

  public static resetInstance(maxSize?: number): void {
    WebhookQueueService.instance = new WebhookQueueService(maxSize);
  }

  enqueue(type: string, payload?: unknown): WebhookEvent {
    const id = ulid();
    const enqueuedAt = new Date().toISOString();

    if (this.count === this.maxSize) {
      this.tail = (this.tail + 1) % this.maxSize;
      this.overflowCount++;
    } else {
      this.count++;
    }

    const event: WebhookEvent = {
      id,
      type,
      payload,
      enqueuedAt,
      status: "pending",
    };

    this.buffer[this.head] = event;
    this.head = (this.head + 1) % this.maxSize;

    return event;
  }

  markSuccess(id: string): boolean {
    const event = this.findById(id);
    if (!event) return false;
    if (event.status !== "pending") return false;
    event.status = "success";
    this.successCount++;
    return true;
  }

  markFailed(id: string): boolean {
    const event = this.findById(id);
    if (!event) return false;
    if (event.status !== "pending") return false;
    event.status = "failed";
    this.failureCount++;
    return true;
  }

  getStats(): WebhookQueueStats {
    let oldestTimestamp: string | null = null;
    if (this.count > 0) {
      const oldest = this.buffer[this.tail];
      if (oldest) {
        oldestTimestamp = oldest.enqueuedAt;
      }
    }

    return {
      depth: this.count,
      successCount: this.successCount,
      failureCount: this.failureCount,
      overflowCount: this.overflowCount,
      oldestTimestamp,
    };
  }

  private findById(id: string): WebhookEvent | undefined {
    for (let i = 0; i < this.count; i++) {
      const idx = (this.tail + i) % this.maxSize;
      const event = this.buffer[idx];
      if (event && event.id === id) {
        return event;
      }
    }
    return undefined;
  }

  getDepth(): number {
    return this.count;
  }
}

export const webhookQueueService = WebhookQueueService.getInstance();
export { WebhookQueueService };