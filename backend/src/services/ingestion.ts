/**
 * Transactional ingestion unit-of-work for the QuickLendX indexer.
 *
 * Guarantees:
 *  - An event batch and its cursor update are committed atomically.
 *  - A failure anywhere in the batch rolls back all derived writes for that batch.
 *  - Re-processing the same batch (same cursor) is a no-op (idempotent).
 *  - Chain reorgs are handled via rollbackTo + re-ingestion.
 */

// lagMonitor is loaded dynamically to avoid circular deps in test env

export interface IndexedEvent {
  ledger: number;
  txHash: string;
  type: string;
  payload: Record<string, unknown>;
}

export interface IngestionStore {
  /** Returns the last successfully committed cursor, or null if nothing indexed yet. */
  getCursor(): Promise<number | null>;
  /** Persist a batch of events and advance the cursor in one atomic operation. */
  commitBatch(events: IndexedEvent[], newCursor: number): Promise<void>;
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
  skipped: boolean; // true when the batch was already indexed (idempotent replay)
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
  batchCursor: number
): Promise<IngestionResult> {
  const currentCursor = await store.getCursor();

  // Idempotency: if we've already processed up to or past this cursor, skip.
  if (currentCursor !== null && batchCursor <= currentCursor) {
    return { committed: false, cursor: currentCursor, eventsProcessed: 0, skipped: true };
  }

  // Validate all events before writing anything — prevents partial state.
  for (const event of events) {
    validateEvent(event);
  }

  // Delegate the atomic commit to the store.
  await store.commitBatch(events, batchCursor);

  // Record metric for monitoring (non-fatal if it fails)
  try {
    (await import('./lagMonitor')).lagMonitor.recordIngestion(batchCursor, events.length);
  } catch {
    // Metrics failure must not break ingestion
  }

  return {
    committed: true,
    cursor: batchCursor,
    eventsProcessed: events.length,
    skipped: false,
  };
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
  fetchBatch: (cursor: number) => Promise<{ rawEvents: IndexedEvent[] }>
): Promise<{ newCursor: number }> {
  // 1. Rollback to the last known good cursor
  await store.rollbackTo(targetCursor);

  // Emit rollback metric (non-fatal if it fails)
  try {
    (await import('./lagMonitor')).lagMonitor.recordRollback(targetCursor);
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

      const result = await ingestBatch(store, batch.rawEvents, nextCursor);
      currentCursor = result.cursor;

      if (result.skipped) {
        // Already at or past this cursor, keep going
        continue;
      }
    } catch (err) {
      throw new Error(
        `Re-ingestion failed at cursor ${nextCursor}: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  return { newCursor: currentCursor };
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

  async commitBatch(events: IndexedEvent[], newCursor: number): Promise<void> {
    // Simulate atomicity: build the new state first, then swap.
    const newEvents = [...this.events, ...events];
    // If anything above threw, we haven't mutated state yet.
    this.events = newEvents;
    this.cursor = newCursor;
  }

  async rollbackTo(cursor: number): Promise<void> {
    if (cursor < 0) {
      throw new Error("Cannot rollback below genesis: cursor must be >= 0");
    }
    this.events = this.events.filter(e => e.ledger <= cursor);
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