/**
 * Transactional ingestion unit-of-work for the QuickLendX indexer.
 *
 * Guarantees:
 *  - An event batch and its cursor update are committed atomically.
 *  - A failure anywhere in the batch rolls back all derived writes for that batch.
 *  - Re-processing the same batch (same cursor) is a no-op (idempotent).
 *  - Re-delivering the same on-chain event with a different batch cursor is ignored
 *    via the persisted (tx_hash, event_index) idempotency key.
 *  - Chain reorgs are handled via rollbackTo + re-ingestion with upsert replacement.
 */

// lagMonitor is loaded dynamically to avoid circular deps in test env
import { withSpan } from "../lib/tracing";
import { getDatabase } from "../lib/database";
import {
  insertRawEvent,
  SqliteRawEventStore,
  ensureRawEventsSchema,
} from "./rawEventStore";
import type { RawEvent } from "../types/replay";

export interface IndexedEvent {
  ledger: number;
  txHash: string;
  eventIndex: number;
  type: string;
  payload: Record<string, unknown>;
}

export interface IngestionStore {
  /** Returns the last successfully committed cursor, or null if nothing indexed yet. */
  getCursor(): Promise<number | null>;
  /** Persist a batch of events and advance the cursor in one atomic operation. */
  commitBatch(
    events: IndexedEvent[],
    newCursor: number,
    options?: { replaceOnConflict?: boolean },
  ): Promise<{ eventsStored: number; eventsSkipped: number }>;
  /**
   * Atomically delete all events with ledger > cursor.
   * Resets the committed cursor to `cursor`.
   * Idempotent: calling with the same cursor multiple times is safe.
   * Throws if cursor < 0 (below genesis).
   */
  rollbackTo(cursor: number): Promise<void>;
}

export interface IngestionResult {
  committed: boolean;
  cursor: number;
  eventsProcessed: number;
  eventsStored: number;
  eventsSkipped: number;
  skipped: boolean; // true when the batch was already indexed (idempotent replay)
}

function eventKey(event: Pick<IndexedEvent, "txHash" | "eventIndex">): string {
  return `${event.txHash}:${event.eventIndex}`;
}

function indexedEventToRawEvent(event: IndexedEvent): RawEvent {
  return {
    id: `${event.txHash}:${event.eventIndex}`,
    ledger: event.ledger,
    txHash: event.txHash,
    eventIndex: event.eventIndex,
    type: event.type,
    payload: event.payload,
    timestamp: Date.now(),
    complianceHold: false,
    indexedAt: new Date().toISOString(),
  };
}

/**
 * Process one batch of events transactionally.
 *
 * The unit-of-work pattern here is intentionally simple:
 *  1. Check whether this batch has already been committed (idempotency guard).
 *  2. Validate every event in the batch before touching the store.
 *  3. Delegate the atomic write to the store implementation.
 *
 * The store is responsible for the actual atomicity (e.g. a DB transaction).
 * This layer owns the idempotency check and pre-commit validation so that
 * partial writes can never reach the store.
 */
export async function ingestBatch(
  store: IngestionStore,
  events: IndexedEvent[],
  batchCursor: number,
  options?: { replaceOnConflict?: boolean },
): Promise<IngestionResult> {
  return withSpan(
    "ingestion.ingestBatch",
    { batch_cursor: batchCursor, events_count: events.length },
    async () => {
      const currentCursor = await store.getCursor();

      // Idempotency: if we've already processed up to or past this cursor, skip.
      if (
        currentCursor !== null &&
        batchCursor <= currentCursor &&
        !options?.replaceOnConflict
      ) {
        return {
          committed: false,
          cursor: currentCursor,
          eventsProcessed: 0,
          eventsStored: 0,
          eventsSkipped: events.length,
          skipped: true,
        };
      }

      // Validate all events before writing anything — prevents partial state.
      for (const event of events) {
        validateEvent(event);
      }

      // Delegate the atomic commit to the store.
      const { eventsStored, eventsSkipped } = await store.commitBatch(
        events,
        batchCursor,
        options,
      );

      // Record metric for monitoring (non-fatal if it fails)
      try {
        (await import("./lagMonitor")).lagMonitor.recordIngestion(
          batchCursor,
          eventsStored,
        );
      } catch {
        // Metrics failure must not break ingestion
      }

      return {
        committed: true,
        cursor: batchCursor,
        eventsProcessed: events.length,
        eventsStored,
        eventsSkipped,
        skipped: false,
      };
    },
  );
}

/**
 * Rollback to a safe cursor and re-ingest the canonical chain.
 *
 * 1. Calls store.rollbackTo(targetCursor) to delete orphaned data.
 * 2. Emits a rollback metric for monitoring.
 * 3. Fetches batches starting from targetCursor + 1 and re-ingests them
 *    using the standard ingestBatch function.
 *
 * The fetchBatch callback should return { rawEvents } for a given cursor,
 * or an empty array when no more batches are available.
 */
export async function rollbackAndReingest(
  store: IngestionStore,
  targetCursor: number,
  fetchBatch: (cursor: number) => Promise<{ rawEvents: IndexedEvent[] }>,
): Promise<{ newCursor: number }> {
  return withSpan(
    "ingestion.rollbackAndReingest",
    { target_cursor: targetCursor },
    async () => {
      // 1. Rollback to the last known good cursor
      await store.rollbackTo(targetCursor);

      // Emit rollback metric (non-fatal if it fails)
      try {
        (await import("./lagMonitor")).lagMonitor.recordRollback(targetCursor);
      } catch {
        // Metrics failure must not break recovery
      }

      // 2. Re-ingest from targetCursor + 1 forward until caught up
      let currentCursor = targetCursor;
      const maxIterations = 10_000; // safety valve

      for (let i = 0; i < maxIterations; i++) {
        const nextCursor = currentCursor + 1;
        try {
          const batch = await fetchBatch(nextCursor);
          if (!batch.rawEvents || batch.rawEvents.length === 0) {
            // No more batches available — we're caught up
            break;
          }

          const result = await ingestBatch(store, batch.rawEvents, nextCursor, {
            replaceOnConflict: true,
          });
          currentCursor = result.cursor;

          if (result.skipped) {
            // Already at or past this cursor, keep going
            continue;
          }
        } catch (err) {
          throw new Error(
            `Re-ingestion failed at cursor ${nextCursor}: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
      }

      return { newCursor: currentCursor };
    },
  );
}

// ---------------------------------------------------------------------------
// Event validation (internal)
// ---------------------------------------------------------------------------

function validateEvent(event: IndexedEvent): void {
  if (!event.txHash || typeof event.txHash !== "string") {
    throw new Error(`Invalid event: missing txHash`);
  }
  if (typeof event.ledger !== "number" || event.ledger < 0) {
    throw new Error(`Invalid event: ledger must be a non-negative number`);
  }
  if (typeof event.eventIndex !== "number" || event.eventIndex < 0) {
    throw new Error(`Invalid event: eventIndex must be a non-negative number`);
  }
  if (!event.type || typeof event.type !== "string") {
    throw new Error(`Invalid event: missing type`);
  }
}

// ---------------------------------------------------------------------------
// In-memory store for testing and local development
// ---------------------------------------------------------------------------

/**
 * InMemoryIngestionStore
 *
 * commitBatch is atomic in the sense that it either succeeds fully or
 * throws without mutating state (the array swap happens only on success).
 */
export class InMemoryIngestionStore implements IngestionStore {
  private cursor: number | null = null;
  private events: IndexedEvent[] = [];

  async getCursor(): Promise<number | null> {
    return this.cursor;
  }

  async commitBatch(
    events: IndexedEvent[],
    newCursor: number,
    options?: { replaceOnConflict?: boolean },
  ): Promise<{ eventsStored: number; eventsSkipped: number }> {
    let eventsStored = 0;
    let eventsSkipped = 0;
    const nextEvents = [...this.events];

    for (const event of events) {
      const key = eventKey(event);
      const existingIndex = nextEvents.findIndex(
        (stored) => eventKey(stored) === key,
      );

      if (existingIndex >= 0) {
        if (options?.replaceOnConflict) {
          nextEvents[existingIndex] = event;
          eventsStored++;
        } else {
          eventsSkipped++;
        }
        continue;
      }

      nextEvents.push(event);
      eventsStored++;
    }

    this.events = nextEvents;
    this.cursor = newCursor;
    return { eventsStored, eventsSkipped };
  }

  async rollbackTo(cursor: number): Promise<void> {
    if (cursor < 0) {
      throw new Error("Cannot rollback below genesis: cursor must be >= 0");
    }
    this.events = this.events.filter((e) => e.ledger <= cursor);
    this.cursor = cursor;
  }

  /** Test helper — returns all indexed events. */
  getEvents(): IndexedEvent[] {
    return [...this.events];
  }

  /** Test helper — reset to empty state. */
  reset(): void {
    this.cursor = null;
    this.events = [];
  }
}

/**
 * SQLite-backed ingestion store that persists raw events with
 * (tx_hash, event_index) idempotency and stores the batch cursor in indexer_state.
 */
export class SqliteIngestionStore implements IngestionStore {
  private readonly rawEvents = new SqliteRawEventStore();

  constructor() {
    ensureRawEventsSchema(getDatabase());
  }

  async getCursor(): Promise<number | null> {
    const row = getDatabase()
      .prepare(
        "SELECT value_number FROM indexer_state WHERE key = 'ingestion_cursor'",
      )
      .get() as { value_number: number | null } | undefined;
    return row?.value_number ?? null;
  }

  async commitBatch(
    events: IndexedEvent[],
    newCursor: number,
    options?: { replaceOnConflict?: boolean },
  ): Promise<{ eventsStored: number; eventsSkipped: number }> {
    const db = getDatabase();
    let eventsStored = 0;
    let eventsSkipped = 0;

    const commit = db.transaction(() => {
      for (const event of events) {
        const result = insertRawEvent(indexedEventToRawEvent(event), options);
        if (result.inserted) {
          eventsStored++;
        } else {
          eventsSkipped++;
        }
      }

      db.prepare(
        `INSERT INTO indexer_state (key, value_number, updated_at)
         VALUES ('ingestion_cursor', ?, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET
           value_number = excluded.value_number,
           updated_at = excluded.updated_at`,
      ).run(newCursor);
    });

    commit();
    return { eventsStored, eventsSkipped };
  }

  async rollbackTo(cursor: number): Promise<void> {
    if (cursor < 0) {
      throw new Error("Cannot rollback below genesis: cursor must be >= 0");
    }

    const db = getDatabase();
    const rollback = db.transaction(() => {
      this.rawEvents.deleteEventsAboveLedger(cursor);
      db.prepare(
        `INSERT INTO indexer_state (key, value_number, updated_at)
         VALUES ('ingestion_cursor', ?, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET
           value_number = excluded.value_number,
           updated_at = excluded.updated_at`,
      ).run(cursor);
    });
    rollback();
  }
}

export { eventKey };
