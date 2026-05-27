/**
 * Unit tests for SettlementOrchestrator (src/services/settlementOrchestrator.ts).
 *
 * Coverage targets: >=95% branches, functions, lines, statements.
 *
 * Tests cover all legal/illegal transitions, idempotency, CRUD, error paths,
 * and concurrent safety using an isolated SQLite database.
 */

import path from 'path';
import fs from 'fs';
import crypto from 'crypto';
import { getDatabase, closeDatabase } from '../lib/database';
import { SettlementStatus } from '../types/contract';
import {
  settlementOrchestrator,
  IllegalTransitionError,
  CreateSettlementInput,
} from '../services/settlementOrchestrator';

// ---------------------------------------------------------------------------
// Test database lifecycle – isolated temp file per run
// ---------------------------------------------------------------------------

const TEST_DB_DIR = path.resolve(__dirname, '../../.data');
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-settlements-${crypto.randomUUID()}.db`);

beforeAll(() => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();

  const conn = getDatabase();
  conn.exec(`
    CREATE TABLE IF NOT EXISTS settlements (
      id TEXT PRIMARY KEY,
      invoice_id TEXT NOT NULL,
      amount TEXT NOT NULL,
      payer TEXT NOT NULL,
      recipient TEXT NOT NULL,
      timestamp INTEGER NOT NULL,
      status TEXT NOT NULL CHECK(status IN ('Pending','Processing','Paid','Defaulted')),
      contract_version INTEGER NOT NULL DEFAULT 1,
      event_schema_version INTEGER NOT NULL DEFAULT 1,
      indexed_at TEXT NOT NULL,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      event_id TEXT UNIQUE
    )
  `);
  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_settlements_invoice ON settlements(invoice_id)
  `);
  conn.exec(`
    CREATE INDEX IF NOT EXISTS idx_settlements_status ON settlements(status)
  `);
});

afterAll(() => {
  closeDatabase();
  try {
    if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
    try { fs.unlinkSync(TEST_DB_PATH + '-wal'); } catch { /* ok */ }
    try { fs.unlinkSync(TEST_DB_PATH + '-shm'); } catch { /* ok */ }
  } catch { /* ok */ }
});

beforeEach(() => {
  settlementOrchestrator.clear();
});

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function makeInput(overrides: Partial<CreateSettlementInput> = {}): CreateSettlementInput {
  return {
    invoice_id: `inv_${crypto.randomUUID().slice(0, 8)}`,
    amount: '1000000000',
    payer: 'GA_PAYER',
    recipient: 'GA_RECIP',
    timestamp: Math.floor(Date.now() / 1000),
    event_id: `evt_${crypto.randomUUID().slice(0, 8)}`,
    ...overrides,
  };
}

function seedPending(input?: Partial<CreateSettlementInput>) {
  const inp = makeInput(input);
  return settlementOrchestrator.createPending(inp);
}

// ---------------------------------------------------------------------------
// validateTransition
// ---------------------------------------------------------------------------

describe('validateTransition', () => {
  test('Pending -> Processing is legal', () => {
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Pending, SettlementStatus.Processing)).toBe(true);
  });

  test('Processing -> Paid is legal', () => {
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Processing, SettlementStatus.Paid)).toBe(true);
  });

  test('Processing -> Defaulted is legal', () => {
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Processing, SettlementStatus.Defaulted)).toBe(true);
  });

  test('Pending -> Paid is illegal', () => {
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Pending, SettlementStatus.Paid)).toBe(false);
  });

  test('Pending -> Defaulted is illegal', () => {
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Pending, SettlementStatus.Defaulted)).toBe(false);
  });

  test('Paid -> anything is illegal', () => {
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Paid, SettlementStatus.Processing)).toBe(false);
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Paid, SettlementStatus.Defaulted)).toBe(false);
  });

  test('Defaulted -> anything is illegal', () => {
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Defaulted, SettlementStatus.Pending)).toBe(false);
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Defaulted, SettlementStatus.Processing)).toBe(false);
  });

  test('Processing -> Pending is illegal (no going back)', () => {
    expect(settlementOrchestrator.validateTransition(SettlementStatus.Processing, SettlementStatus.Pending)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// createPending
// ---------------------------------------------------------------------------

describe('createPending', () => {
  test('creates a settlement with Pending status', () => {
    const input = makeInput();
    const s = settlementOrchestrator.createPending(input);

    expect(s.id).toBeDefined();
    expect(s.invoice_id).toBe(input.invoice_id);
    expect(s.amount).toBe(input.amount);
    expect(s.payer).toBe(input.payer);
    expect(s.recipient).toBe(input.recipient);
    expect(s.timestamp).toBe(input.timestamp);
    expect(s.status).toBe(SettlementStatus.Pending);
    expect(s.contract_version).toBe(1);
    expect(s.event_schema_version).toBe(1);
    expect(s.indexed_at).toBeDefined();
  });

  test('is idempotent: same event_id returns existing row', () => {
    const input = makeInput();
    const s1 = settlementOrchestrator.createPending(input);
    const s2 = settlementOrchestrator.createPending(input);

    expect(s2.id).toBe(s1.id);
  });

  test('custom contract_version and event_schema_version are respected', () => {
    const input = makeInput({ contract_version: 2, event_schema_version: 3 });
    const s = settlementOrchestrator.createPending(input);

    expect(s.contract_version).toBe(2);
    expect(s.event_schema_version).toBe(3);
  });
});

// ---------------------------------------------------------------------------
// State machine transitions
// ---------------------------------------------------------------------------

describe('state machine transitions', () => {
  test('full lifecycle: Pending -> Processing -> Paid', () => {
    const s = seedPending();
    expect(s.status).toBe(SettlementStatus.Pending);

    const processing = settlementOrchestrator.startProcessing(s.invoice_id, 'evt_process_1');
    expect(processing.status).toBe(SettlementStatus.Processing);

    const paid = settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_paid_1');
    expect(paid.status).toBe(SettlementStatus.Paid);
  });

  test('full lifecycle: Pending -> Processing -> Defaulted', () => {
    const s = seedPending();

    const processing = settlementOrchestrator.startProcessing(s.invoice_id, 'evt_process_2');
    expect(processing.status).toBe(SettlementStatus.Processing);

    const defaulted = settlementOrchestrator.failProcessing(s.invoice_id, 'evt_fail_1');
    expect(defaulted.status).toBe(SettlementStatus.Defaulted);
  });

  test('illegal transition: Pending -> Paid throws IllegalTransitionError', () => {
    const s = seedPending();

    expect(() =>
      settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_bad')
    ).toThrow(IllegalTransitionError);
  });

  test('illegal transition: Pending -> Defaulted throws IllegalTransitionError', () => {
    const s = seedPending();

    expect(() =>
      settlementOrchestrator.failProcessing(s.invoice_id, 'evt_bad')
    ).toThrow(IllegalTransitionError);
  });

  test('illegal transition: Paid -> anything throws', () => {
    const s = seedPending();
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');
    settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_comp');

    expect(() =>
      settlementOrchestrator.failProcessing(s.invoice_id, 'evt_fail')
    ).toThrow(IllegalTransitionError);
  });

  test('illegal transition: Defaulted -> anything throws', () => {
    const s = seedPending();
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');
    settlementOrchestrator.failProcessing(s.invoice_id, 'evt_f');

    expect(() =>
      settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_c')
    ).toThrow(IllegalTransitionError);
  });

  test('transition from non-existent settlement throws', () => {
    expect(() =>
      settlementOrchestrator.startProcessing('nonexistent', 'evt_1')
    ).toThrow('No settlement found');
  });
});

// ---------------------------------------------------------------------------
// Idempotency
// ---------------------------------------------------------------------------

describe('idempotency', () => {
  test('same event_id on startProcessing is no-op', () => {
    const s = seedPending();
    const r1 = settlementOrchestrator.startProcessing(s.invoice_id, 'evt_dup');
    expect(r1.status).toBe(SettlementStatus.Processing);

    const r2 = settlementOrchestrator.startProcessing(s.invoice_id, 'evt_dup');
    expect(r2.id).toBe(r1.id);
    expect(r2.status).toBe(SettlementStatus.Processing);
  });

  test('same event_id on completeProcessing is no-op', () => {
    const s = seedPending();
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');
    const r1 = settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_done');
    expect(r1.status).toBe(SettlementStatus.Paid);

    const r2 = settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_done');
    expect(r2.id).toBe(r1.id);
  });

  test('replayed full lifecycle is safe', () => {
    const s = seedPending();
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');
    settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_c');

    // Replay same transitions — events are stale, return current state
    const r1 = settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');
    expect(r1.status).toBe(SettlementStatus.Paid);

    const r2 = settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_c');
    expect(r2.status).toBe(SettlementStatus.Paid);
  });

  test('transition to current status is no-op', () => {
    const s = seedPending();
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');

    // Call startProcessing again with a new event_id but same invoice
    const r = settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p2');
    expect(r.status).toBe(SettlementStatus.Processing);
  });
});

// ---------------------------------------------------------------------------
// getById and list
// ---------------------------------------------------------------------------

describe('getById and list', () => {
  test('getById returns settlement by ID', () => {
    const s = seedPending();
    const found = settlementOrchestrator.getById(s.id);
    expect(found).not.toBeNull();
    expect(found!.id).toBe(s.id);
  });

  test('getById returns null for non-existent ID', () => {
    expect(settlementOrchestrator.getById('nonexistent')).toBeNull();
  });

  test('list returns all settlements', () => {
    seedPending({ invoice_id: 'inv_1' });
    seedPending({ invoice_id: 'inv_2' });

    const all = settlementOrchestrator.list();
    expect(all).toHaveLength(2);
  });

  test('list filters by invoice_id', () => {
    seedPending({ invoice_id: 'inv_a' });
    seedPending({ invoice_id: 'inv_b' });

    const filtered = settlementOrchestrator.list({ invoice_id: 'inv_a' });
    expect(filtered).toHaveLength(1);
    expect(filtered[0].invoice_id).toBe('inv_a');
  });

  test('list filters by status', () => {
    const s = seedPending({ invoice_id: 'inv_x' });
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');

    const filtered = settlementOrchestrator.list({ status: SettlementStatus.Processing });
    expect(filtered).toHaveLength(1);
    expect(filtered[0].status).toBe(SettlementStatus.Processing);
  });

  test('list filters by both invoice_id and status', () => {
    const s = seedPending({ invoice_id: 'inv_y' });

    const paid = settlementOrchestrator.list({ invoice_id: 'inv_y', status: SettlementStatus.Pending });
    expect(paid).toHaveLength(1);
    expect(paid[0].status).toBe(SettlementStatus.Pending);

    const processing = settlementOrchestrator.list({ invoice_id: 'inv_y', status: SettlementStatus.Processing });
    expect(processing).toHaveLength(0);
  });

  test('list with no matches returns empty array', () => {
    const result = settlementOrchestrator.list({ invoice_id: 'nonexistent' });
    expect(result).toEqual([]);
  });

  test('list returns all settlements', () => {
    seedPending({ invoice_id: 'inv_old', event_id: 'evt_old' });
    seedPending({ invoice_id: 'inv_new', event_id: 'evt_new' });

    const all = settlementOrchestrator.list();
    expect(all).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// getStatus
// ---------------------------------------------------------------------------

describe('getStatus', () => {
  test('returns Pending for newly created settlement', () => {
    const s = seedPending();
    expect(settlementOrchestrator.getStatus(s.invoice_id)).toBe(SettlementStatus.Pending);
  });

  test('returns Processing after startProcessing', () => {
    const s = seedPending();
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');
    expect(settlementOrchestrator.getStatus(s.invoice_id)).toBe(SettlementStatus.Processing);
  });

  test('returns Paid after completeProcessing', () => {
    const s = seedPending();
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');
    settlementOrchestrator.completeProcessing(s.invoice_id, 'evt_c');
    expect(settlementOrchestrator.getStatus(s.invoice_id)).toBe(SettlementStatus.Paid);
  });

  test('returns Defaulted after failProcessing', () => {
    const s = seedPending();
    settlementOrchestrator.startProcessing(s.invoice_id, 'evt_p');
    settlementOrchestrator.failProcessing(s.invoice_id, 'evt_f');
    expect(settlementOrchestrator.getStatus(s.invoice_id)).toBe(SettlementStatus.Defaulted);
  });

  test('returns null for non-existent invoice', () => {
    expect(settlementOrchestrator.getStatus('nonexistent')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// clear
// ---------------------------------------------------------------------------

describe('clear', () => {
  test('removes all settlements', () => {
    seedPending();
    seedPending();
    expect(settlementOrchestrator.list()).toHaveLength(2);

    settlementOrchestrator.clear();
    expect(settlementOrchestrator.list()).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Concurrent rapid writes
// ---------------------------------------------------------------------------

describe('concurrent rapid writes', () => {
  test('multiple sequential transitions on different invoices are safe', () => {
    const count = 20;
    const inputs = Array.from({ length: count }, (_, i) => {
      const inp = makeInput({ invoice_id: `inv_concurrent_${i}`, event_id: `evt_create_${i}` });
      settlementOrchestrator.createPending(inp);
      return inp;
    });

    inputs.forEach((inp, i) => {
      settlementOrchestrator.startProcessing(inp.invoice_id, `evt_proc_${i}`);
      settlementOrchestrator.completeProcessing(inp.invoice_id, `evt_paid_${i}`);
    });

    const all = settlementOrchestrator.list();
    expect(all).toHaveLength(count);
    all.forEach((s) => {
      expect(s.status).toBe(SettlementStatus.Paid);
    });
  });
});
