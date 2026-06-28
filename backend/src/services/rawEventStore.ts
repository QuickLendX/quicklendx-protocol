import path from "path";
import { promises as fs } from "fs";
import {
  RawEvent,
  RawEventStore,
  EventValidator,
} from "../types/replay";
import { getDatabase, getPreparedStatement } from "../lib/database";

/** Typed error when a duplicate (tx_hash, event_index) is detected at the DB layer. */
export class RawEventDuplicateError extends Error {
  readonly code = "RAW_EVENT_DUPLICATE";

  constructor(
    public readonly txHash: string,
    public readonly eventIndex: number,
  ) {
    super(`Duplicate raw event for (${txHash}, ${eventIndex})`);
    this.name = "RawEventDuplicateError";
  }
}

/** Returns true when SQLite reports a UNIQUE constraint violation. */
export function isSqliteUniqueViolation(err: unknown): boolean {
  return (
    err != null &&
    typeof err === "object" &&
    "code" in err &&
    (err as { code: string }).code === "SQLITE_CONSTRAINT_UNIQUE"
  );
}

export interface InsertRawEventResult {
  inserted: boolean;
  txHash: string;
  eventIndex: number;
}

export interface RawEventRow {
  id: string;
  tx_hash: string;
  event_index: number;
  ledger: number;
  type: string;
  payload: string;
  indexed_at: string;
}

function resolveEventIndex(event: Pick<RawEvent, "eventIndex">): number {
  return event.eventIndex ?? 0;
}

/**
 * Insert a raw event with deep-layer idempotency on (tx_hash, event_index).
 * Duplicate deliveries are ignored (DO NOTHING) unless `replaceOnConflict` is set
 * for reorg recovery, in which case the canonical row is replaced in place.
 */
export function insertRawEvent(
  event: RawEvent,
  options: { replaceOnConflict?: boolean } = {},
): InsertRawEventResult {
  const eventIndex = resolveEventIndex(event);
  const payload = JSON.stringify(event.payload ?? {});
  const indexedAt = event.indexedAt ?? new Date().toISOString();

  const sql = options.replaceOnConflict
    ? `
      INSERT INTO raw_events (
        id, tx_hash, event_index, ledger, type, payload, indexed_at
      ) VALUES (?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(tx_hash, event_index) DO UPDATE SET
        id = excluded.id,
        ledger = excluded.ledger,
        type = excluded.type,
        payload = excluded.payload,
        indexed_at = excluded.indexed_at
    `
    : `
      INSERT INTO raw_events (
        id, tx_hash, event_index, ledger, type, payload, indexed_at
      ) VALUES (?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(tx_hash, event_index) DO NOTHING
    `;

  try {
    const result = getPreparedStatement(sql).run(
      event.id,
      event.txHash,
      eventIndex,
      event.ledger,
      event.type,
      payload,
      indexedAt,
    ) as { changes: number };

    return {
      inserted: result.changes > 0,
      txHash: event.txHash,
      eventIndex,
    };
  } catch (err) {
    if (isSqliteUniqueViolation(err)) {
      throw new RawEventDuplicateError(event.txHash, eventIndex);
    }
    throw err;
  }
}

/** List all persisted raw events ordered by ledger for invariant checks. */
export function listRawEventIdempotencyKeys(): Array<{
  tx_hash: string;
  event_index: number;
}> {
  const rows = getPreparedStatement(
    "SELECT tx_hash, event_index FROM raw_events ORDER BY ledger, event_index",
  ).all() as Array<{ tx_hash: string; event_index: number }>;
  return rows;
}

/** Count rows sharing the same (tx_hash, event_index) — should always be 0 or 1. */
export function countRawEventsByKey(
  txHash: string,
  eventIndex: number,
): number {
  const row = getPreparedStatement(
    "SELECT COUNT(*) AS count FROM raw_events WHERE tx_hash = ? AND event_index = ?",
  ).get(txHash, eventIndex) as { count: number };
  return row.count;
}

export function ensureRawEventsSchema(db: ReturnType<typeof getDatabase>): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS raw_events (
      id TEXT PRIMARY KEY,
      tx_hash TEXT NOT NULL,
      event_index INTEGER NOT NULL,
      ledger INTEGER NOT NULL,
      type TEXT NOT NULL,
      payload TEXT NOT NULL DEFAULT '{}',
      indexed_at TEXT NOT NULL,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )
  `);
  db.exec(`
    CREATE UNIQUE INDEX IF NOT EXISTS raw_events_idempotency
      ON raw_events(tx_hash, event_index)
  `);
}

export class SqliteRawEventStore {
  insertEvent(
    event: RawEvent,
    options?: { replaceOnConflict?: boolean },
  ): InsertRawEventResult {
    return insertRawEvent(event, options);
  }

  insertEvents(
    events: RawEvent[],
    options?: { replaceOnConflict?: boolean },
  ): InsertRawEventResult[] {
    const db = getDatabase();
    const tx = db.transaction((rows: RawEvent[]) => {
      return rows.map((event) => insertRawEvent(event, options));
    });
    return tx(events);
  }

  deleteEventsAboveLedger(ledger: number): number {
    const result = getPreparedStatement(
      "DELETE FROM raw_events WHERE ledger > ?",
    ).run(ledger) as { changes: number };
    return result.changes;
  }

  getEventsByLedgerRange(
    fromLedger: number,
    toLedger: number,
  ): RawEventRow[] {
    return getPreparedStatement(
      `SELECT id, tx_hash, event_index, ledger, type, payload, indexed_at
       FROM raw_events
       WHERE ledger >= ? AND ledger <= ?
       ORDER BY ledger, event_index`,
    ).all(fromLedger, toLedger) as RawEventRow[];
  }
}

export class FileRawEventStore implements RawEventStore {
  private readonly dataDir: string;
  private readonly eventsFile: string;
  private readonly cursorFile: string;
  private readonly maxFileSize: number;
  private readonly validator: EventValidator;

  constructor(
    validator: EventValidator,
    dataDir?: string,
    maxFileSize: number = 100 * 1024 * 1024 // 100MB per file
  ) {
    this.dataDir = dataDir || path.join(process.cwd(), ".data", "raw-events");
    this.eventsFile = path.join(this.dataDir, "events.jsonl");
    this.cursorFile = path.join(this.dataDir, "replay-cursor.json");
    this.maxFileSize = maxFileSize;
    this.validator = validator;
  }

  async storeEvents(events: RawEvent[]): Promise<void> {
    await fs.mkdir(this.dataDir, { recursive: true });

    const sanitizedEvents: RawEvent[] = [];
    const validationErrors: string[] = [];

    for (const event of events) {
      const errors = await this.validator.validateEvent(event);
      if (errors.length > 0) {
        validationErrors.push(`Event ${event.id}: ${errors.join(", ")}`);
        continue;
      }

      const sanitized = await this.validator.sanitizeEvent(event);
      sanitizedEvents.push(sanitized);
    }

    if (validationErrors.length > 0) {
      throw new Error(`Event validation failed: ${validationErrors.join("; ")}`);
    }

    sanitizedEvents.sort((a, b) => a.ledger - b.ledger);

    const lines = sanitizedEvents.map((event) => JSON.stringify(event));
    await fs.appendFile(this.eventsFile, lines.join("\n") + "\n", "utf8");

    await this.rotateFileIfNeeded();
  }

  async hasEventId(eventId: string): Promise<boolean> {
    await fs.mkdir(this.dataDir, { recursive: true });

    try {
      const data = await fs.readFile(this.eventsFile, "utf8");
      const lines = data.trim().split("\n").filter((line) => line.length > 0);

      for (const line of lines) {
        try {
          const event = JSON.parse(line) as RawEvent;
          if (event.id === eventId) {
            return true;
          }
        } catch {
          // Ignore malformed historical rows while scanning for idempotency.
        }
      }

      return false;
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return false;
      }
      throw error;
    }
  }

  async getEventsByLedgerRange(
    fromLedger: number,
    toLedger: number,
    limit?: number
  ): Promise<RawEvent[]> {
    await fs.mkdir(this.dataDir, { recursive: true });

    try {
      const data = await fs.readFile(this.eventsFile, "utf8");
      const lines = data.trim().split("\n").filter((line) => line.length > 0);

      const events: RawEvent[] = [];
      let count = 0;

      for (const line of lines) {
        if (limit && count >= limit) break;

        try {
          const event = JSON.parse(line) as RawEvent;
          if (event.ledger >= fromLedger && event.ledger <= toLedger) {
            events.push(event);
            count++;
          }
        } catch (parseError) {
          console.warn(
            `Skipping malformed event line: ${line.substring(0, 100)}...`
          );
        }
      }

      return events.sort((a, b) => a.ledger - b.ledger);
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return [];
      }
      throw error;
    }
  }

  async getEventCount(fromLedger: number, toLedger: number): Promise<number> {
    const events = await this.getEventsByLedgerRange(fromLedger, toLedger);
    return events.length;
  }

  async getLedgerBounds(): Promise<{ min: number | null; max: number | null }> {
    await fs.mkdir(this.dataDir, { recursive: true });

    try {
      const data = await fs.readFile(this.eventsFile, "utf8");
      const lines = data.trim().split("\n").filter((line) => line.length > 0);

      let min: number | null = null;
      let max: number | null = null;

      for (const line of lines) {
        try {
          const event = JSON.parse(line) as RawEvent;
          if (min === null || event.ledger < min) min = event.ledger;
          if (max === null || event.ledger > max) max = event.ledger;
        } catch (parseError) {
          // Skip malformed lines
        }
      }

      return { min, max };
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return { min: null, max: null };
      }
      throw error;
    }
  }

  async deleteEventsByLedgerRange(
    fromLedger: number,
    toLedger: number
  ): Promise<number> {
    await fs.mkdir(this.dataDir, { recursive: true });

    try {
      const data = await fs.readFile(this.eventsFile, "utf8");
      const lines = data.trim().split("\n").filter((line) => line.length > 0);

      const keptLines: string[] = [];
      let deletedCount = 0;

      for (const line of lines) {
        try {
          const event = JSON.parse(line) as RawEvent;
          if (event.ledger >= fromLedger && event.ledger <= toLedger) {
            deletedCount++;
          } else {
            keptLines.push(line);
          }
        } catch (parseError) {
          keptLines.push(line);
        }
      }

      await fs.writeFile(
        this.eventsFile,
        keptLines.join("\n") + "\n",
        "utf8"
      );

      return deletedCount;
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return 0;
      }
      throw error;
    }
  }

  async getReplayCursor(): Promise<number | null> {
    try {
      const data = await fs.readFile(this.cursorFile, "utf8");
      const cursor = JSON.parse(data);
      return typeof cursor.ledger === "number" ? cursor.ledger : null;
    } catch (error) {
      if ((error as any).code === "ENOENT") {
        return null;
      }
      throw error;
    }
  }

  async setReplayCursor(ledger: number): Promise<void> {
    await fs.mkdir(this.dataDir, { recursive: true });
    await fs.writeFile(
      this.cursorFile,
      JSON.stringify({ ledger, updatedAt: new Date().toISOString() }),
      "utf8"
    );
  }

  async rollbackTo(cursor: number): Promise<void> {
    if (typeof cursor !== "number" || cursor < 0) {
      throw new Error("Invalid cursor for rollback");
    }
    const all = await this.getAllEvents();
    const kept = all.filter((e) => e.ledger <= cursor);
    await this.replaceEvents(kept);
    await this.setReplayCursor(cursor);
  }

  async getAllEvents(): Promise<RawEvent[]> {
    await fs.mkdir(this.dataDir, { recursive: true });

    const files = await this.getEventFiles();
    const events: RawEvent[] = [];

    for (const filePath of files) {
      try {
        const data = await fs.readFile(filePath, "utf8");
        const lines = data.split("\n").filter((line) => line.trim().length > 0);
        for (const line of lines) {
          try {
            events.push(JSON.parse(line) as RawEvent);
          } catch {
            continue;
          }
        }
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
          throw error;
        }
      }
    }

    return events.sort((a, b) => a.ledger - b.ledger);
  }

  async replaceEvents(events: RawEvent[]): Promise<void> {
    await fs.mkdir(this.dataDir, { recursive: true });

    const tempFile = path.join(this.dataDir, `events.${Date.now()}.tmp`);
    const lines = [...events]
      .sort((a, b) => a.ledger - b.ledger)
      .map((event) => JSON.stringify(event));

    await fs.writeFile(
      tempFile,
      lines.length > 0 ? `${lines.join("\n")}\n` : "",
      "utf8"
    );

    const files = await this.getEventFiles();
    await Promise.all(
      files
        .filter((filePath) => filePath !== tempFile)
        .map((filePath) => fs.rm(filePath, { force: true }))
    );
    await fs.rename(tempFile, this.eventsFile);
  }

  async reset(): Promise<void> {
    try {
      await fs.rm(this.dataDir, { recursive: true, force: true });
    } catch {
      // Ignore errors during reset
    }
  }

  private async rotateFileIfNeeded(): Promise<void> {
    try {
      const stats = await fs.stat(this.eventsFile);
      if (stats.size > this.maxFileSize) {
        const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
        const rotatedFile = path.join(
          this.dataDir,
          `events-${timestamp}.jsonl`
        );
        await fs.rename(this.eventsFile, rotatedFile);
      }
    } catch (error) {
      // File might not exist yet
    }
  }

  private async getEventFiles(): Promise<string[]> {
    try {
      const entries = await fs.readdir(this.dataDir);
      return entries
        .filter((entry) => /^events(?:-.*)?\.jsonl$/.test(entry))
        .sort()
        .map((entry) => path.join(this.dataDir, entry));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        return [];
      }
      throw error;
    }
  }
}

// In-memory implementation for testing
export class InMemoryRawEventStore implements RawEventStore {
  private events: RawEvent[] = [];
  private cursor: number | null = null;
  private validator: EventValidator;

  constructor(validator: EventValidator) {
    this.validator = validator;
  }

  async storeEvents(events: RawEvent[]): Promise<void> {
    const sanitizedEvents: RawEvent[] = [];

    for (const event of events) {
      const errors = await this.validator.validateEvent(event);
      if (errors.length > 0) {
        throw new Error(
          `Event validation failed for ${event.id}: ${errors.join(", ")}`
        );
      }

      const sanitized = await this.validator.sanitizeEvent(event);
      const eventIndex = resolveEventIndex(sanitized);
      const duplicate = this.events.some(
        (existing) =>
          existing.txHash === sanitized.txHash &&
          resolveEventIndex(existing) === eventIndex,
      );
      if (!duplicate) {
        sanitizedEvents.push(sanitized);
      }
    }

    sanitizedEvents.sort((a, b) => a.ledger - b.ledger);
    this.events.push(...sanitizedEvents);
  }

  async hasEventId(eventId: string): Promise<boolean> {
    return this.events.some((event) => event.id === eventId);
  }

  async getEventsByLedgerRange(
    fromLedger: number,
    toLedger: number,
    limit?: number
  ): Promise<RawEvent[]> {
    const filtered = this.events
      .filter((e) => e.ledger >= fromLedger && e.ledger <= toLedger)
      .sort((a, b) => a.ledger - b.ledger);

    return limit ? filtered.slice(0, limit) : filtered;
  }

  async getEventCount(fromLedger: number, toLedger: number): Promise<number> {
    return this.events.filter(
      (e) => e.ledger >= fromLedger && e.ledger <= toLedger
    ).length;
  }

  async getLedgerBounds(): Promise<{ min: number | null; max: number | null }> {
    if (this.events.length === 0) {
      return { min: null, max: null };
    }

    const ledgers = this.events.map((e) => e.ledger);
    return { min: Math.min(...ledgers), max: Math.max(...ledgers) };
  }

  async deleteEventsByLedgerRange(
    fromLedger: number,
    toLedger: number
  ): Promise<number> {
    const originalLength = this.events.length;
    this.events = this.events.filter(
      (e) => e.ledger < fromLedger || e.ledger > toLedger
    );
    return originalLength - this.events.length;
  }

  async getReplayCursor(): Promise<number | null> {
    return this.cursor;
  }

  async setReplayCursor(ledger: number): Promise<void> {
    this.cursor = ledger;
  }

  async rollbackTo(cursor: number): Promise<void> {
    if (typeof cursor !== "number" || cursor < 0) {
      throw new Error("Invalid cursor for rollback");
    }
    this.events = this.events.filter((e) => e.ledger <= cursor);
    this.cursor = cursor;
  }

  async getAllEvents(): Promise<RawEvent[]> {
    return [...this.events].sort((a, b) => a.ledger - b.ledger);
  }

  async replaceEvents(events: RawEvent[]): Promise<void> {
    for (const event of events) {
      const eventIndex = resolveEventIndex(event);
      this.events = this.events.filter(
        (existing) =>
          !(
            existing.txHash === event.txHash &&
            resolveEventIndex(existing) === eventIndex
          ),
      );
      this.events.push(event);
    }
    this.events = [...this.events].sort((a, b) => a.ledger - b.ledger);
  }

  reset(): void {
    this.events = [];
    this.cursor = null;
  }

  getEvents(): RawEvent[] {
    return [...this.events];
  }
}
