/**
 * Chain Cursor Model & Monotonic Ingestion (#876)
 *
 * Defines the canonical cursor representation for Stellar/Soroban event
 * ingestion and enforces:
 * - Strict monotonic ordering (ledger_seq, event_index)
 * - Gap detection between the last committed cursor and the incoming one
 * - Safe resume from the last persisted cursor after restarts
 * - Idempotent ingestion: duplicate events (same cursor) are silently skipped
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ChainCursor {
  /** Ledger sequence number (monotonically increasing) */
  ledger_seq: number;
  /** Transaction hash (hex string, 64 chars) */
  tx_hash: string;
  /** Index of this event within the transaction */
  event_index: number;
}

export type CursorCompareResult = "before" | "equal" | "after" | "gap";

// ---------------------------------------------------------------------------
// Cursor comparison & gap detection
// ---------------------------------------------------------------------------

/**
 * Compare two cursors.
 *
 * Returns:
 * - `"before"` – `next` is strictly before `current`  → reject (rewind)
 * - `"equal"`  – same position                          → skip (duplicate)
 * - `"after"`  – next sequential event                  → accept
 * - `"gap"`    – one or more events are missing         → pause & alert
 *
 * Gap detection rules:
 * - A ledger jump of > 1 is always a gap.
 * - Within the same ledger, an event_index jump of > 1 is a gap.
 * - When the ledger advances by exactly 1, the first event_index (0) is
 *   valid; any other starting index is a gap.
 */
export function compareCursors(
  current: ChainCursor,
  next: ChainCursor
): CursorCompareResult {
  if (
    next.ledger_seq < current.ledger_seq ||
    (next.ledger_seq === current.ledger_seq &&
      next.event_index < current.event_index)
  ) {
    return "before";
  }

  if (
    next.ledger_seq === current.ledger_seq &&
    next.event_index === current.event_index
  ) {
    return "equal"; // duplicate
  }

  const ledgerDelta = next.ledger_seq - current.ledger_seq;

  if (ledgerDelta === 0) {
    // Same ledger: event_index must be exactly current + 1
    return next.event_index === current.event_index + 1 ? "after" : "gap";
  }

  if (ledgerDelta === 1) {
    // Advancing one ledger: first event must start at index 0
    return next.event_index === 0 ? "after" : "gap";
  }

  // Ledger jumped by more than 1 → gap
  return "gap";
}

/**
 * Serialize a cursor to a stable string suitable for use as a storage key or
 * log label.
 */
export function serializeCursor(cursor: ChainCursor): string {
  return `${cursor.ledger_seq}:${cursor.tx_hash}:${cursor.event_index}`;
}

/**
 * Parse a serialized cursor string back to a ChainCursor.
 * @throws {Error} if the format is invalid
 */
export function parseCursor(raw: string): ChainCursor {
  const parts = raw.split(":");
  if (parts.length !== 3) {
    throw new Error(`Invalid cursor format: "${raw}"`);
  }
  const [seq, txHash, idx] = parts;
  const ledger_seq = parseInt(seq, 10);
  const event_index = parseInt(idx, 10);
  if (isNaN(ledger_seq) || isNaN(event_index) || !txHash) {
    throw new Error(`Invalid cursor values: "${raw}"`);
  }
  return { ledger_seq, tx_hash: txHash, event_index };
}

// ---------------------------------------------------------------------------
// CursorStore – durable state (in-memory stub; replace with Redis/DB)
// ---------------------------------------------------------------------------

/**
 * Minimal interface for persisting the last committed cursor.
 * Swap for a real persistent implementation in production.
 */
export interface CursorStore {
  getLastCursor(): Promise<ChainCursor | null>;
  setLastCursor(cursor: ChainCursor): Promise<void>;
}

export class InMemoryCursorStore implements CursorStore {
  private last: ChainCursor | null = null;

  async getLastCursor(): Promise<ChainCursor | null> {
    return this.last;
  }

  async setLastCursor(cursor: ChainCursor): Promise<void> {
    this.last = cursor;
  }
}

// ---------------------------------------------------------------------------
// MonotonicIngester
// ---------------------------------------------------------------------------

export type IngestResult =
  | { status: "accepted"; cursor: ChainCursor }
  | { status: "duplicate"; cursor: ChainCursor }
  | { status: "gap"; current: ChainCursor; incoming: ChainCursor }
  | { status: "rewind"; current: ChainCursor; incoming: ChainCursor };

export interface IngesterOptions {
  /** Called when an event is accepted for processing */
  onAccept: (cursor: ChainCursor) => Promise<void> | void;
  /** Called when a gap is detected; the ingester will halt until resolved */
  onGap?: (current: ChainCursor, incoming: ChainCursor) => void;
  /** Called when a duplicate event is skipped */
  onDuplicate?: (cursor: ChainCursor) => void;
}

/**
 * Stateful ingester that enforces monotonic cursor progression.
 *
 * Usage:
 * ```ts
 * const store = new InMemoryCursorStore();
 * const ingester = new MonotonicIngester(store, {
 *   onAccept: async (cursor) => { /* process event *\/ }
 * });
 * await ingester.resume();
 *
 * for (const event of stream) {
 *   await ingester.ingest(event.cursor, event);
 * }
 * ```
 */
export class MonotonicIngester {
  private halted = false;
  private lastCursor: ChainCursor | null = null;

  constructor(
    private readonly store: CursorStore,
    private readonly opts: IngesterOptions
  ) {}

  /**
   * Load the last committed cursor from the store.
   * Must be called once before starting ingestion.
   */
  async resume(): Promise<ChainCursor | null> {
    this.lastCursor = await this.store.getLastCursor();
    this.halted = false;
    return this.lastCursor;
  }

  /**
   * Attempt to ingest a single event at the given cursor.
   *
   * - If the cursor is the next expected one → call `onAccept` and commit.
   * - If duplicate → call `onDuplicate` and skip.
   * - If gap or rewind → call `onGap` and halt the ingester.
   */
  async ingest<T>(cursor: ChainCursor, event: T): Promise<IngestResult> {
    if (this.halted) {
      throw new Error(
        "Ingester is halted due to a detected gap. Call resume() after the gap is resolved."
      );
    }

    // First event (no prior cursor) – always accepted
    if (this.lastCursor === null) {
      await this._accept(cursor, event);
      return { status: "accepted", cursor };
    }

    const comparison = compareCursors(this.lastCursor, cursor);

    switch (comparison) {
      case "after": {
        await this._accept(cursor, event);
        return { status: "accepted", cursor };
      }

      case "equal": {
        this.opts.onDuplicate?.(cursor);
        return { status: "duplicate", cursor };
      }

      case "gap": {
        const current = this.lastCursor;
        this.halted = true;
        this.opts.onGap?.(current, cursor);
        return { status: "gap", current, incoming: cursor };
      }

      case "before": {
        const current = this.lastCursor;
        this.halted = true;
        this.opts.onGap?.(current, cursor);
        return { status: "rewind", current, incoming: cursor };
      }
    }
  }

  /** Whether the ingester is currently halted due to a gap. */
  get isHalted(): boolean {
    return this.halted;
  }

  /** The last successfully committed cursor. */
  get lastCommittedCursor(): ChainCursor | null {
    return this.lastCursor;
  }

  // Internal: commit accepted event
  private async _accept<T>(cursor: ChainCursor, _event: T): Promise<void> {
    await this.opts.onAccept(cursor);
    await this.store.setLastCursor(cursor);
    this.lastCursor = cursor;
  }
}
