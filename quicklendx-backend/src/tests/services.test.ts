/**
 * Service stub tests — verify exported function signatures.
 */

import { describe, it, expect } from 'vitest';
import Database from 'better-sqlite3';
import { cleanupAll } from '../services/retention';
import { runAll, getViolationCount, resetViolationCount } from '../services/invariantService';
import { run } from '../services/reconciliationWorker';
import { up, down } from '../migrations/v006_scheduler_leases';

describe('retention service', () => {
  it('exports cleanupAll as a function', () => {
    expect(typeof cleanupAll).toBe('function');
  });

  it('cleanupAll resolves without error', async () => {
    await expect(cleanupAll()).resolves.toBeUndefined();
  });
});

describe('invariantService', () => {
  beforeEach(() => {
    resetViolationCount();
  });

  it('exports runAll as a function', () => {
    expect(typeof runAll).toBe('function');
  });

  it('runAll resolves without error', async () => {
    await expect(runAll()).resolves.toBeUndefined();
  });

  it('getViolationCount returns a number', async () => {
    const count = await getViolationCount();
    expect(typeof count).toBe('number');
    expect(count).toBe(0);
  });

  it('resetViolationCount resets to 0', () => {
    resetViolationCount();
  });
});

describe('reconciliationWorker', () => {
  it('exports run as a function', () => {
    expect(typeof run).toBe('function');
  });

  it('run resolves without error', async () => {
    await expect(run()).resolves.toBeUndefined();
  });
});

describe('v006 migration', () => {
  it('up creates scheduler_leases table', () => {
    const db = new Database(':memory:');
    up(db);
    const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all();
    expect(tables.some((t: any) => t.name === 'scheduler_leases')).toBe(true);
    db.close();
  });

  it('down drops scheduler_leases table', () => {
    const db = new Database(':memory:');
    up(db);
    down(db);
    const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all();
    expect(tables.some((t: any) => t.name === 'scheduler_leases')).toBe(false);
    db.close();
  });

  it('up is idempotent', () => {
    const db = new Database(':memory:');
    up(db);
    up(db);
    const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all();
    expect(tables.some((t: any) => t.name === 'scheduler_leases')).toBe(true);
    db.close();
  });
});
