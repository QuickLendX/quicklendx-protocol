import { getDatabase } from "../lib/database";
import { isSqliteUniqueViolation } from "./rawEventStore";

export type ClaimResult = "claimed" | "duplicate";

export interface IEventIdLedger {
  claim(id: string): Promise<ClaimResult>;
  complete(id: string): Promise<void>;
  release(id: string): Promise<void>;
  has(id: string): Promise<boolean>;
  reset(): Promise<void>;
}

export function ensureProcessedEventsSchema(db: ReturnType<typeof getDatabase>): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS processed_events (
      id TEXT PRIMARY KEY,
      status TEXT NOT NULL,
      claimed_at TEXT NOT NULL,
      completed_at TEXT
    )
  `);
  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_processed_events_status
      ON processed_events(status)
  `);
}

export class EventIdLedger implements IEventIdLedger {
  private static instance: EventIdLedger;
  private db?: ReturnType<typeof getDatabase>;
  private currentDbHandle?: ReturnType<typeof getDatabase>;

  private claimStmt?: any;
  private completeStmt?: any;
  private releaseStmt?: any;
  private hasStmt?: any;
  private statusStmt?: any;

  constructor(db?: ReturnType<typeof getDatabase>) {
    this.db = db;
    this.ensureSchema();
  }

  public static getInstance(): EventIdLedger {
    if (!EventIdLedger.instance) {
      EventIdLedger.instance = new EventIdLedger();
    }
    return EventIdLedger.instance;
  }

  private getDb(): ReturnType<typeof getDatabase> {
    return this.db || getDatabase();
  }

  private ensureSchema(): void {
    const db = this.getDb();
    if (this.currentDbHandle !== db || !db.open) {
      ensureProcessedEventsSchema(db);
      this.initStatements(db);
      this.currentDbHandle = db;
    }
  }

  private initStatements(db: ReturnType<typeof getDatabase>): void {
    this.claimStmt = db.prepare(`
      INSERT INTO processed_events (id, status, claimed_at)
      VALUES (?, 'claimed', ?)
      ON CONFLICT(id) DO NOTHING
    `);
    this.completeStmt = db.prepare(`
      UPDATE processed_events
      SET status = 'completed', completed_at = ?
      WHERE id = ?
    `);
    this.releaseStmt = db.prepare(`
      DELETE FROM processed_events
      WHERE id = ? AND status = 'claimed'
    `);
    this.hasStmt = db.prepare(`
      SELECT 1 FROM processed_events WHERE id = ?
    `);
    this.statusStmt = db.prepare(`
      SELECT status FROM processed_events WHERE id = ?
    `);
  }

  /**
   * Atomically claim an event id before side effects run.
   * Returns "claimed" if this is the first caller to claim the id.
   * Returns "duplicate" if the id is already claimed or completed.
   */
  public async claim(id: string): Promise<ClaimResult> {
    if (!id) {
      throw new Error("Event id is required to claim");
    }
    this.ensureSchema();
    const now = new Date().toISOString();

    try {
      const result = this.claimStmt.run(id, now);
      return result.changes === 1 ? "claimed" : "duplicate";
    } catch (err) {
      if (isSqliteUniqueViolation(err)) {
        return "duplicate";
      }
      throw err;
    }
  }

  /**
   * Mark a claimed event as completed after side effects succeed.
   * Once completed, subsequent claims will observe "duplicate" and
   * re-processing is prevented.
   */
  public async complete(id: string): Promise<void> {
    if (!id) return;
    this.ensureSchema();
    const now = new Date().toISOString();
    this.completeStmt.run(now, id);
  }

  /**
   * Release a claimed event if processing failed, so a subsequent retry
   * can claim and process the event. Completed events are never released.
   */
  public async release(id: string): Promise<void> {
    if (!id) return;
    this.ensureSchema();
    this.releaseStmt.run(id);
  }

  /**
   * Check whether an event id has been claimed or completed.
   */
  public async has(id: string): Promise<boolean> {
    this.ensureSchema();
    const row = this.hasStmt.get(id);
    return !!row;
  }

  /**
   * Get current status for an event ID ("claimed", "completed", or null).
   */
  public async getStatus(id: string): Promise<"claimed" | "completed" | null> {
    this.ensureSchema();
    const row = this.statusStmt.get(id) as { status: string } | undefined;
    if (!row) return null;
    return row.status as "claimed" | "completed";
  }

  /**
   * Reset processed_events table (for test isolation).
   */
  public async reset(): Promise<void> {
    this.schemaEnsured = false;
    this.ensureSchema();
    const db = this.getDb();
    db.exec(`DELETE FROM processed_events`);
  }
}

export const eventIdLedger = EventIdLedger.getInstance();
