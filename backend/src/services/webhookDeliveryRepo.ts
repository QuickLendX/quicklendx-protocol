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

export interface WebhookDeadLetter {
  id: string;
  deliveryId: string | null;
  eventType: string;
  payload: unknown;
  targetUrl: string | null;
  subscriberId: string | null;
  attemptCount: number;
  maxAttempts: number;
  lastError: string | null;
  status: "dead_letter" | "replayed";
  replayedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface WebhookDeadLetterRow {
  id: string;
  delivery_id: string | null;
  event_type: string;
  payload: string;
  target_url: string | null;
  subscriber_id: string | null;
  attempt_count: number;
  max_attempts: number;
  last_error: string | null;
  status: "dead_letter" | "replayed";
  replayed_at: string | null;
  created_at: string;
  updated_at: string;
}

function rowToDeadLetter(row: WebhookDeadLetterRow): WebhookDeadLetter {
  let parsedPayload: unknown;
  try {
    parsedPayload = JSON.parse(row.payload);
  } catch {
    parsedPayload = row.payload;
  }
  return {
    id: row.id,
    deliveryId: row.delivery_id,
    eventType: row.event_type,
    payload: parsedPayload,
    targetUrl: row.target_url,
    subscriberId: row.subscriber_id,
    attemptCount: row.attempt_count,
    maxAttempts: row.max_attempts,
    lastError: row.last_error,
    status: row.status,
    replayedAt: row.replayed_at,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  };
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
  ensureDeadLetterSchema(): void {
    const db = getDatabase();
    db.exec(`
      CREATE TABLE IF NOT EXISTS webhook_dead_letters (
        id TEXT PRIMARY KEY,
        delivery_id TEXT,
        event_type TEXT NOT NULL,
        payload TEXT NOT NULL,
        target_url TEXT,
        subscriber_id TEXT,
        attempt_count INTEGER NOT NULL DEFAULT 0,
        max_attempts INTEGER NOT NULL DEFAULT 5,
        last_error TEXT,
        status TEXT NOT NULL DEFAULT 'dead_letter'
          CHECK(status IN ('dead_letter', 'replayed')),
        replayed_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);
    db.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_dead_letters_status
      ON webhook_dead_letters(status)
    `);
    db.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_dead_letters_created_at
      ON webhook_dead_letters(created_at)
    `);
    db.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_dead_letters_delivery_id
      ON webhook_dead_letters(delivery_id)
    `);
  }

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
    this.ensureDeadLetterSchema();
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

    if (newStatus === "dead_letter") {
      try {
        this.recordDeadLetter({
          deliveryId: id,
          eventType: delivery.eventType,
          payload: delivery.payload,
          subscriberId: delivery.subscriberId,
          attemptCount: newAttemptCount,
          maxAttempts: delivery.maxAttempts,
          lastError: error ?? null,
        });
      } catch (err) {
        console.error("Failed to record dead letter in markFailed:", err);
      }
    }

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

  recordDeadLetter(params: {
    id?: string;
    deliveryId?: string | null;
    eventType?: string;
    payload: unknown;
    targetUrl?: string | null;
    subscriberId?: string | null;
    attemptCount?: number;
    maxAttempts?: number;
    lastError?: string | null;
  }): WebhookDeadLetter {
    this.ensureDeadLetterSchema();
    const db = getDatabase();
    const id = params.id || ulid();
    const now = new Date().toISOString();
    const payloadStr =
      typeof params.payload === "string"
        ? params.payload
        : JSON.stringify(params.payload ?? "");

    const row: WebhookDeadLetterRow = {
      id,
      delivery_id: params.deliveryId ?? null,
      event_type: params.eventType || "webhook.delivery",
      payload: payloadStr,
      target_url: params.targetUrl ?? null,
      subscriber_id: params.subscriberId ?? null,
      attempt_count: params.attemptCount ?? 0,
      max_attempts: params.maxAttempts ?? MAX_RETRY_ATTEMPTS,
      last_error: params.lastError ?? null,
      status: "dead_letter",
      replayed_at: null,
      created_at: now,
      updated_at: now,
    };

    db.prepare(
      `INSERT INTO webhook_dead_letters (
        id, delivery_id, event_type, payload, target_url, subscriber_id,
        attempt_count, max_attempts, last_error, status, replayed_at,
        created_at, updated_at
      ) VALUES (
        @id, @delivery_id, @event_type, @payload, @target_url, @subscriber_id,
        @attempt_count, @max_attempts, @last_error, @status, @replayed_at,
        @created_at, @updated_at
      )`
    ).run(row);

    return rowToDeadLetter(row);
  }

  getDeadLetterById(id: string): WebhookDeadLetter | null {
    this.ensureDeadLetterSchema();
    const db = getDatabase();
    const row = db
      .prepare("SELECT * FROM webhook_dead_letters WHERE id = ?")
      .get(id) as WebhookDeadLetterRow | undefined;
    return row ? rowToDeadLetter(row) : null;
  }

  listDeadLetters(params?: {
    limit?: number;
    offset?: number;
  }): {
    entries: WebhookDeadLetter[];
    total: number;
    limit: number;
    offset: number;
    hasMore: boolean;
  } {
    this.ensureDeadLetterSchema();
    const db = getDatabase();
    const rawLimit = params?.limit ?? 20;
    const limit = Math.max(1, Math.min(rawLimit, 100));
    const offset = Math.max(0, params?.offset ?? 0);

    const countRow = db
      .prepare("SELECT COUNT(*) as count FROM webhook_dead_letters")
      .get() as { count: number } | undefined;
    const total = countRow?.count ?? 0;

    const rows = db
      .prepare(
        "SELECT * FROM webhook_dead_letters ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?"
      )
      .all(limit, offset) as WebhookDeadLetterRow[];

    const entries = rows.map(rowToDeadLetter);
    const hasMore = offset + entries.length < total;

    return {
      entries,
      total,
      limit,
      offset,
      hasMore,
    };
  }

  replayDeadLetter(id: string): {
    deadLetter: WebhookDeadLetter;
    newDelivery: WebhookDelivery;
  } | null {
    this.ensureDeadLetterSchema();
    const db = getDatabase();
    const deadLetter = this.getDeadLetterById(id);
    if (!deadLetter) {
      return null;
    }

    if (deadLetter.status === "replayed" || deadLetter.replayedAt !== null) {
      const err = new Error("Dead letter has already been replayed");
      (err as any).code = "ALREADY_REPLAYED";
      throw err;
    }

    const now = new Date().toISOString();

    // Atomic update: only transitions if status is still 'dead_letter'
    const updateResult = db
      .prepare(
        `UPDATE webhook_dead_letters
         SET status = 'replayed', replayed_at = ?, updated_at = ?
         WHERE id = ? AND status = 'dead_letter'`
      )
      .run(now, now, id);

    if (updateResult.changes === 0) {
      const err = new Error("Dead letter has already been replayed");
      (err as any).code = "ALREADY_REPLAYED";
      throw err;
    }

    // Re-enqueue with fresh attempt count (attempt_count = 0)
    const newDelivery = this.create({
      eventType: deadLetter.eventType,
      payload: deadLetter.payload,
      subscriberId: deadLetter.subscriberId ?? undefined,
      maxAttempts: deadLetter.maxAttempts,
    });

    const updatedDeadLetter = this.getDeadLetterById(id)!;
    return {
      deadLetter: updatedDeadLetter,
      newDelivery,
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

  vacuum(): void {
    const db = getDatabase();
    db.exec("PRAGMA incremental_vacuum");
  }
}

export const webhookDeliveryRepo = new WebhookDeliveryRepo();
