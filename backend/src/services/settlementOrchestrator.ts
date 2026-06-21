import crypto from 'crypto';
import { getDatabase, getPreparedStatement } from '../lib/database';
import { Settlement, SettlementStatus } from '../types/contract';

export class IllegalTransitionError extends Error {
  constructor(from: SettlementStatus, to: SettlementStatus) {
    super(`Illegal transition: ${from} -> ${to}`);
    this.name = 'IllegalTransitionError';
  }
}

export interface CreateSettlementInput {
  invoice_id: string;
  amount: string;
  payer: string;
  recipient: string;
  timestamp: number;
  event_id: string;
  contract_version?: number;
  event_schema_version?: number;
}

const VALID_TRANSITIONS: Record<SettlementStatus, SettlementStatus[]> = {
  [SettlementStatus.Pending]: [SettlementStatus.Processing],
  [SettlementStatus.Processing]: [SettlementStatus.Paid, SettlementStatus.Defaulted],
  [SettlementStatus.Paid]: [],
  [SettlementStatus.Defaulted]: [],
};

const VERSIONED_DEFAULTS = {
  contract_version: 1,
  event_schema_version: 1,
};

function toSettlement(row: any): Settlement {
  return {
    id: row.id,
    invoice_id: row.invoice_id,
    amount: row.amount,
    payer: row.payer,
    recipient: row.recipient,
    timestamp: row.timestamp,
    status: row.status as SettlementStatus,
    contract_version: row.contract_version,
    event_schema_version: row.event_schema_version,
    indexed_at: row.indexed_at,
  };
}

class SettlementOrchestrator {
  private _db: ReturnType<typeof getDatabase> | null = null;

  private getDb() {
    if (!this._db) {
      this._db = getDatabase();
    }
    return this._db;
  }

  /**
   * Validate that a state transition is allowed by the state machine.
   */
  validateTransition(from: SettlementStatus, to: SettlementStatus): boolean {
    return VALID_TRANSITIONS[from]?.includes(to) ?? false;
  }

  /**
   * Create a new settlement in Pending status from an on-chain event.
   * Idempotent: if event_id already exists, returns the existing row.
   */
  createPending(input: CreateSettlementInput): Settlement {
    const db = this.getDb();

    const existing = db.prepare('SELECT * FROM settlements WHERE event_id = ?').get(input.event_id) as any;
    if (existing) {
      return toSettlement(existing);
    }

    const id = crypto.randomUUID();
    const now = new Date().toISOString();
    const cv = input.contract_version ?? VERSIONED_DEFAULTS.contract_version;
    const esv = input.event_schema_version ?? VERSIONED_DEFAULTS.event_schema_version;

    db.prepare(`
      INSERT INTO settlements (id, invoice_id, amount, payer, recipient, timestamp, status, contract_version, event_schema_version, indexed_at, created_at, updated_at, event_id)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      id, input.invoice_id, input.amount, input.payer, input.recipient,
      input.timestamp, SettlementStatus.Pending, cv, esv, now, now, now, input.event_id,
    );

    return this.getById(id)!;
  }

  /**
   * Transition a settlement from Pending → Processing.
   * No-op if already at Processing or beyond.
   */
  startProcessing(invoiceId: string, eventId: string): Settlement {
    return this.transition(invoiceId, eventId, SettlementStatus.Processing);
  }

  /**
   * Transition a settlement from Processing → Paid.
   * No-op if already at Paid or a terminal state.
   */
  completeProcessing(invoiceId: string, eventId: string): Settlement {
    return this.transition(invoiceId, eventId, SettlementStatus.Paid);
  }

  /**
   * Transition a settlement from Processing → Defaulted.
   * No-op if already at Defaulted.
   */
  failProcessing(invoiceId: string, eventId: string): Settlement {
    return this.transition(invoiceId, eventId, SettlementStatus.Defaulted);
  }

  /**
   * Generic transition: finds settlement by invoice_id, validates transition,
   * and persists. Uses the event_id for idempotent replay protection.
   *
   * Idempotency rules:
   *   - If current status already equals target → no-op, return existing.
   *   - If current status is past the target (e.g. Paid → Processing) → stale
   *     event, return existing.
   *   - If event_id was already applied → no-op, return existing.
   */
  private transition(invoiceId: string, eventId: string, to: SettlementStatus): Settlement {
    const db = this.getDb();

    const row = db.prepare(
      'SELECT * FROM settlements WHERE invoice_id = ? ORDER BY updated_at DESC LIMIT 1'
    ).get(invoiceId) as any;

    if (!row) {
      throw new Error(`No settlement found for invoice ${invoiceId}`);
    }

    const current = row.status as SettlementStatus;

    // Already at target → no-op
    if (current === to) {
      return toSettlement(row);
    }

    // Already past target (e.g., Paid when target is Processing) → stale event, no-op
    if (this.validateTransition(to, current)) {
      return toSettlement(row);
    }

    // Same event_id replayed → no-op
    if (row.event_id === eventId) {
      return toSettlement(row);
    }

    // Validate forward transition
    if (!this.validateTransition(current, to)) {
      throw new IllegalTransitionError(current, to);
    }

    const now = new Date().toISOString();
    const updated = db.prepare(`
      UPDATE settlements SET status = ?, updated_at = ?, event_id = ? WHERE id = ?
    `).run(to, now, eventId, row.id);

    if (updated.changes === 0) {
      throw new Error(`Failed to update settlement ${row.id}`);
    }

    return this.getById(row.id)!;
  }

  /**
   * Get a settlement by its primary key.
   */
  getById(id: string): Settlement | null {
    const row = this.getDb().prepare('SELECT * FROM settlements WHERE id = ?').get(id) as any;
    return row ? toSettlement(row) : null;
  }

  /**
   * List settlements with optional filters.
   */
  list(filters?: { invoice_id?: string; status?: SettlementStatus }): Settlement[] {
    const clauses: string[] = [];
    const params: unknown[] = [];

    if (filters?.invoice_id) {
      clauses.push('invoice_id = ?');
      params.push(filters.invoice_id);
    }

    if (filters?.status) {
      clauses.push('status = ?');
      params.push(filters.status);
    }

    let sql = 'SELECT * FROM settlements';
    if (clauses.length > 0) {
      sql += ' WHERE ' + clauses.join(' AND ');
    }
    sql += ' ORDER BY created_at DESC';

    const rows = this.getDb().prepare(sql).all(...params) as any[];
    return rows.map(toSettlement);
  }

  /**
   * Get the current status for an invoice (useful for idempotency checks).
   */
  getStatus(invoiceId: string): SettlementStatus | null {
    const row = this.getDb().prepare(
      'SELECT status FROM settlements WHERE invoice_id = ? ORDER BY updated_at DESC LIMIT 1'
    ).get(invoiceId) as any;
    return row ? (row.status as SettlementStatus) : null;
  }

  /**
   * Clear all settlements (testing utility).
   */
  clear(): void {
    this.getDb().prepare('DELETE FROM settlements').run();
  }
}

export const settlementOrchestrator = new SettlementOrchestrator();
