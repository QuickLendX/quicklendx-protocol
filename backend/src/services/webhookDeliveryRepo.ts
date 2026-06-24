import { ulid } from "ulid";
import { getDatabase, getPreparedStatement } from "../lib/database";

export const WEBHOOK_DELIVERY_STATUSES = [
  "pending",
  "processing",
  "success",
  "failed",
  "dead_letter",
] as const;

export type WebhookDeliveryStatus = (typeof WEBHOOK_DELIVERY_STATUSES)[number];

export interface WebhookDelivery {
  id: string;
  eventType: string;
  payload: unknown;
  subscriberId: string | null;
  status: WebhookDeliveryStatus;
  enqueuedAt: string;
  attemptCount: number;
  maxAttempts: number;
  nextRetryAt: string | null;
  lastError: string | null;
  lastAttemptAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface WebhookDeliveryRow {
  id: string;
  event_type: string;
  payload: string;
  subscriber_id: string | null;
  status: WebhookDeliveryStatus;
  enqueued_at: string;
  attempt_count: number;
  max_attempts: number;
  next_retry_at: string | null;
  last_error: string | null;
  last_attempt_at: string | null;
  created_at: string;
  updated_at: string;
}

/**
 * Exponential retry delay schedule with jitter (in milliseconds).
 *
 * Attempt index → delay:
 *   0 → 1 min     (60,000 ms)
 *   1 → 5 min     (300,000 ms)
 *   2 → 30 min    (1,800,000 ms)
 *   3 → 2 hr      (7,200,000 ms)
 *   4 → 12 hr     (43,200,000 ms)
 *
 * Jitter adds a random 0–50 % of the base delay to spread retry storms.
 */
const RETRY_DELAYS_MS: readonly number[] = [
  60_000,
  300_000,
  1_800_000,
  7_200_000,
  43_200_000,
];

export const MAX_RETRY_ATTEMPTS = 5;
export const RETENTION_DAYS_DEFAULT = 90;

function rowToDelivery(row: WebhookDeliveryRow): WebhookDelivery {
  return {
    id: row.id,
    eventType: row.event_type,
    payload: JSON.parse(row.payload),
    subscriberId: row.subscriber_id,
    status: row.status,
    enqueuedAt: row.enqueued_at,
    attemptCount: row.attempt_count,
    maxAttempts: row.max_attempts,
    nextRetryAt: row.next_retry_at,
    lastError: row.last_error,
    lastAttemptAt: row.last_attempt_at,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  };
}

export function computeNextRetry(attemptCount: number): string | null {
  if (attemptCount >= MAX_RETRY_ATTEMPTS) return null;
  const baseDelay = RETRY_DELAYS_MS[attemptCount];
  const jitter = Math.round(Math.random() * baseDelay * 0.5);
  return new Date(Date.now() + baseDelay + jitter).toISOString();
}

export class WebhookDeliveryRepo {
  ensureSchema(): void {
    const db = getDatabase();
    db.exec(`
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
    db.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_status_next_retry
      ON webhook_deliveries(status, next_retry_at)
    `);
    db.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_created_at
      ON webhook_deliveries(created_at)
    `);
  }

  create(params: {
    eventType: string;
    payload: unknown;
    subscriberId?: string;
    maxAttempts?: number;
  }): WebhookDelivery {
    const db = getDatabase();
    const id = ulid();
    const now = new Date().toISOString();
    const row: WebhookDeliveryRow = {
      id,
      event_type: params.eventType,
      payload: JSON.stringify(params.payload ?? ""),
      subscriber_id: params.subscriberId ?? null,
      status: "pending",
      enqueued_at: now,
      attempt_count: 0,
      max_attempts: params.maxAttempts ?? MAX_RETRY_ATTEMPTS,
      next_retry_at: null,
      last_error: null,
      last_attempt_at: null,
      created_at: now,
      updated_at: now,
    };

    db.prepare(
      `INSERT INTO webhook_deliveries (id, event_type, payload, subscriber_id, status,
        enqueued_at, attempt_count, max_attempts, next_retry_at, last_error,
        last_attempt_at, created_at, updated_at)
       VALUES (@id, @event_type, @payload, @subscriber_id, @status,
        @enqueued_at, @attempt_count, @max_attempts, @next_retry_at, @last_error,
        @last_attempt_at, @created_at, @updated_at)`
    ).run(row);

    return rowToDelivery(row);
  }

  getById(id: string): WebhookDelivery | null {
    const db = getDatabase();
    const row = db
      .prepare("SELECT * FROM webhook_deliveries WHERE id = ?")
      .get(id) as WebhookDeliveryRow | undefined;
    return row ? rowToDelivery(row) : null;
  }

  getPending(): WebhookDelivery[] {
    const db = getDatabase();
    const now = new Date().toISOString();
    const rows = db
      .prepare(
        `SELECT * FROM webhook_deliveries
         WHERE (status = 'pending' AND (next_retry_at IS NULL OR next_retry_at <= ?))
            OR (status = 'failed' AND next_retry_at <= ?)
         ORDER BY enqueued_at ASC`
      )
      .all(now, now) as WebhookDeliveryRow[];
    return rows.map(rowToDelivery);
  }

  getByStatus(status: WebhookDeliveryStatus): WebhookDelivery[] {
    const db = getDatabase();
    const rows = db
      .prepare("SELECT * FROM webhook_deliveries WHERE status = ? ORDER BY enqueued_at ASC")
      .all(status) as WebhookDeliveryRow[];
    return rows.map(rowToDelivery);
  }

  markProcessing(id: string): boolean {
    const db = getDatabase();
    const now = new Date().toISOString();
    const result = db
      .prepare(
        `UPDATE webhook_deliveries
         SET status = 'processing', updated_at = ?
         WHERE id = ? AND status IN ('pending', 'failed')`
      )
      .run(now, id);
    return result.changes > 0;
  }

  markSuccess(id: string): boolean {
    const db = getDatabase();
    const now = new Date().toISOString();
    const result = db
      .prepare(
        `UPDATE webhook_deliveries
         SET status = 'success', updated_at = ?, last_attempt_at = ?
         WHERE id = ? AND status IN ('pending', 'processing')`
      )
      .run(now, now, id);
    return result.changes > 0;
  }

  markFailed(id: string, error?: string): WebhookDelivery | null {
    const db = getDatabase();
    const now = new Date().toISOString();
    const delivery = this.getById(id);
    if (!delivery) return null;
    if (delivery.status !== "pending" && delivery.status !== "processing" && delivery.status !== "failed") return null;

    const newAttemptCount = delivery.attemptCount + 1;
    const scheduledRetry = computeNextRetry(newAttemptCount);
    const newStatus: WebhookDeliveryStatus =
      newAttemptCount >= delivery.maxAttempts || scheduledRetry === null
        ? "dead_letter"
        : "failed";
    const nextRetry = newStatus === "dead_letter" ? null : scheduledRetry;

    db.prepare(
      `UPDATE webhook_deliveries
       SET status = ?, attempt_count = ?, next_retry_at = ?,
           last_error = ?, last_attempt_at = ?, updated_at = ?
       WHERE id = ?`
    ).run(newStatus, newAttemptCount, nextRetry, error ?? null, now, now, id);

    const updated = this.getById(id);
    if (!updated) return null;

    return {
      ...updated,
      status: newStatus,
      attemptCount: newAttemptCount,
      nextRetryAt: nextRetry,
      lastError: error ?? null,
      lastAttemptAt: now,
      updatedAt: now,
    };
  }

  retryDeadLetter(id: string): boolean {
    const db = getDatabase();
    const now = new Date().toISOString();
    const result = db
      .prepare(
        `UPDATE webhook_deliveries
         SET status = 'pending', next_retry_at = ?, updated_at = ?
         WHERE id = ? AND status = 'dead_letter'`
      )
      .run(now, now, id);
    return result.changes > 0;
  }

  getDeadLetters(): WebhookDelivery[] {
    return this.getByStatus("dead_letter");
  }

  getStats(): {
    total: number;
    pending: number;
    processing: number;
    success: number;
    failed: number;
    deadLetter: number;
    oldestPending: string | null;
  } {
    const db = getDatabase();
    const row = db
      .prepare(
        `SELECT
           COUNT(*) as total,
           COUNT(CASE WHEN status = 'pending' THEN 1 END) as pending,
           COUNT(CASE WHEN status = 'processing' THEN 1 END) as processing,
           COUNT(CASE WHEN status = 'success' THEN 1 END) as success,
           COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed,
           COUNT(CASE WHEN status = 'dead_letter' THEN 1 END) as dead_letter,
           MIN(CASE WHEN status IN ('pending', 'processing') THEN enqueued_at END) as oldest_pending
         FROM webhook_deliveries`
      )
      .get() as {
      total: number;
      pending: number;
      processing: number;
      success: number;
      failed: number;
      dead_letter: number;
      oldest_pending: string | null;
    };

    return {
      total: row.total ?? 0,
      pending: row.pending ?? 0,
      processing: row.processing ?? 0,
      success: row.success ?? 0,
      failed: row.failed ?? 0,
      deadLetter: row.dead_letter ?? 0,
      oldestPending: row.oldest_pending ?? null,
    };
  }

  cleanup(olderThanDays: number = RETENTION_DAYS_DEFAULT): number {
    const db = getDatabase();
    const cutoff = new Date(
      Date.now() - olderThanDays * 24 * 60 * 60 * 1000
    ).toISOString();
    const result = db
      .prepare(
        `DELETE FROM webhook_deliveries
         WHERE status IN ('success', 'dead_letter')
           AND created_at < ?`
      )
      .run(cutoff);
    return result.changes;
  }

  private overflowCount: number = 0;

  incrementOverflow(): number {
    return ++this.overflowCount;
  }

  getOverflowCount(): number {
    return this.overflowCount;
  }

  resetOverflow(): void {
    this.overflowCount = 0;
  }

  drain(subscriberId: string): { pending: number; drained: number } {
    const db = getDatabase();
    const now = new Date().toISOString();
    const result = db
      .prepare(
        `UPDATE webhook_deliveries
         SET status = 'dead_letter', next_retry_at = NULL, updated_at = ?
         WHERE subscriber_id = ? AND status IN ('pending', 'processing', 'failed')`
      )
      .run(now, subscriberId);
    return {
      pending: 0,
      drained: result.changes,
    };
  }

  vacuum(): void {
    const db = getDatabase();
    db.exec("PRAGMA incremental_vacuum");
  }
}

export const webhookDeliveryRepo = new WebhookDeliveryRepo();
