/**
 * Transactional ingestion unit-of-work for the QuickLendX indexer.
 *
 * Guarantees:
 *  - An event batch and its cursor update are committed atomically.
 *  - A failure anywhere in the batch rolls back all derived writes for that batch.
 *  - Re-processing the same batch (same cursor) is a no-op (idempotent).
 */

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

  return {
    committed: true,
    cursor: batchCursor,
    eventsProcessed: events.length,
    skipped: false,
  };
}

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

/**
 * In-memory store for testing and local development.
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
