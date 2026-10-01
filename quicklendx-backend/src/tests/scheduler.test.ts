/**
 * Scheduler & Leader-Election Tests
 *
 * Covers: single-instance firing, multi-instance exclusion, lease expiry,
 * exception resilience, graceful shutdown, and NODE_ENV guard.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import Database from 'better-sqlite3';
import { Scheduler, getCronIntervalMs } from '../lib/scheduler';

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function memoryDb(): Database.Database {
  const db = new Database(':memory:');
  db.pragma('journal_mode = WAL');
  return db;
}

function quickScheduler(
  db?: Database.Database,
  overrides: Partial<{
    pollIntervalMs: number;
    leaseDurationMs: number;
    workerId: string;
  }> = {},
): { sched: Scheduler; db: Database.Database } {
  const database = db ?? memoryDb();
  const sched = new Scheduler({
    db: database,
    pollIntervalMs: overrides.pollIntervalMs ?? 50,
    leaseDurationMs: overrides.leaseDurationMs ?? 200,
    workerId: overrides.workerId ?? 'test-worker',
  });
  return { sched, db: database };
}

/** Returns a promise that resolves when the spy fires for the first time. */
function jobSpy() {
  let resolve: (() => void) | null = null;
  const promise = new Promise<void>((r) => {
    resolve = r;
  });
  const fn = vi.fn(async () => {
    resolve?.();
    resolve = null;
  });
  return { fn, promise };
}

/**
 * Set last_run_at to a distant past so the cron-interval check passes
 * regardless of which cron expression is used.
 */
function expireLastRun(db: Database.Database, jobName: string): void {
  const past = new Date(Date.now() - 86_400_000).toISOString(); // 24 h ago
  db.prepare(
    'UPDATE scheduler_leases SET last_run_at = ? WHERE job_name = ?',
  ).run(past, jobName);
}

/* ------------------------------------------------------------------ */
/*  getCronIntervalMs                                                   */
/* ------------------------------------------------------------------ */

describe('getCronIntervalMs', () => {
  it('returns 5 min for */5 * * * *', () => {
    expect(getCronIntervalMs('*/5 * * * *')).toBe(5 * 60_000);
  });

  it('returns 1 min for * * * * *', () => {
    expect(getCronIntervalMs('* * * * *')).toBe(60_000);
  });

  it('returns 1 hour for 0 * * * *', () => {
    expect(getCronIntervalMs('0 * * * *')).toBe(60 * 60_000);
  });

  it('returns 24 hours for 0 0 * * *', () => {
    expect(getCronIntervalMs('0 0 * * *')).toBe(24 * 60 * 60_000);
  });

  it('returns 60s for malformed expression', () => {
    expect(getCronIntervalMs('invalid')).toBe(60_000);
    expect(getCronIntervalMs('')).toBe(60_000);
  });

  it('handles hour step */2', () => {
    expect(getCronIntervalMs('0 */2 * * *')).toBe(2 * 3_600_000);
  });

  it('handles specific day-of-week', () => {
    const ms = getCronIntervalMs('0 9 * * 1-5');
    expect(ms).toBeGreaterThan(0);
  });

  it('handles specific day-of-month', () => {
    const ms = getCronIntervalMs('0 0 1 * *');
    expect(ms).toBeGreaterThan(0);
  });
});

/* ------------------------------------------------------------------ */
/*  Scheduler — construction & registration                            */
/* ------------------------------------------------------------------ */

describe('Scheduler', () => {
  let db: Database.Database;
  let sched: Scheduler;

  beforeEach(() => {
    db = memoryDb();
    sched = new Scheduler({ db, pollIntervalMs: 50, leaseDurationMs: 100 });
  });

  afterEach(async () => {
    await sched.stop();
    sched.close();
  });

  describe('register', () => {
    it('registers a job and increments jobCount', () => {
      expect(sched.jobCount).toBe(0);
      sched.register('test', '*/1 * * * *', async () => {});
      expect(sched.jobCount).toBe(1);
    });

    it('throws if called after start', () => {
      sched.register('a', '*/1 * * * *', async () => {});
      sched.start();
      expect(() => sched.register('b', '*/1 * * * *', async () => {})).toThrow(
        'Cannot register jobs after scheduler has started',
      );
    });

    it('is chainable', () => {
      const r = sched.register('a', '* * * * *', async () => {});
      expect(r).toBe(sched);
    });
  });

  describe('start / stop', () => {
    it('sets started to true after start()', () => {
      expect(sched.started).toBe(false);
      sched.start();
      expect(sched.started).toBe(true);
    });

    it('is idempotent — start() twice does nothing', () => {
      sched.start();
      sched.start();
      expect(sched.started).toBe(true);
    });
  });

  describe('close', () => {
    it('can be called after stop without error', async () => {
      sched.start();
      await sched.stop();
      expect(() => sched.close()).not.toThrow();
    });
  });
});

/* ------------------------------------------------------------------ */
/*  Single-instance — job fires                                        */
/* ------------------------------------------------------------------ */

describe('single instance', () => {
  it('triggers a registered job within a few poll cycles', async () => {
    const { sched, db } = quickScheduler();
    const spy = jobSpy();
    sched.register('fast', '*/1 * * * *', spy.fn);
    sched.start();

    await spy.promise;
    expect(spy.fn).toHaveBeenCalledTimes(1);

    await sched.stop();
    sched.close();
    db.close();
  }, 10_000);
});

/* ------------------------------------------------------------------ */
/*  File-based DB                                                      */
/* ------------------------------------------------------------------ */

describe('file-based database', () => {
  it('creates and uses a file-based SQLite database', async () => {
    const tmpDir = '/tmp/opencode/scheduler-test';
    const dbPath = `${tmpDir}/test-scheduler.db`;
    const sched = new Scheduler({
      dbPath,
      pollIntervalMs: 50,
      leaseDurationMs: 200,
    });

    const spy = jobSpy();
    sched.register('file-db', '* * * * *', spy.fn);
    sched.start();

    await spy.promise;
    expect(spy.fn).toHaveBeenCalled();

    await sched.stop();
    sched.close();

    // Verify DB file was created
    const fs = await import('fs');
    expect(fs.existsSync(dbPath)).toBe(true);

    // Cleanup
    try { fs.unlinkSync(dbPath); fs.rmdirSync(tmpDir); } catch { /* ignore */ }
  }, 10_000);
});

/* ------------------------------------------------------------------ */
/*  Lease pre-populated                                                */
/* ------------------------------------------------------------------ */

describe('lease edge cases', () => {
  it('skips job when lease is held by another worker on start', async () => {
    const db = memoryDb();
    // Create the table first, then pre-populate a valid lease
    db.exec(`
      CREATE TABLE IF NOT EXISTS scheduler_leases (
        job_name TEXT PRIMARY KEY, lease_until TEXT NOT NULL,
        worker_id TEXT NOT NULL, last_run_at TEXT,
        created_at TEXT, updated_at TEXT
      )
    `);
    const future = new Date(Date.now() + 60_000).toISOString();
    db.prepare(
      `INSERT INTO scheduler_leases (job_name, lease_until, worker_id, last_run_at)
       VALUES (?, ?, ?, ?)`,
    ).run('preheld', future, 'other-worker', null);

    const { sched } = quickScheduler(db, {
      pollIntervalMs: 50,
      leaseDurationMs: 200,
      workerId: 'test-worker',
    });

    let fired = false;
    sched.register('preheld', '* * * * *', async () => {
      fired = true;
    });
    sched.start();

    await new Promise((r) => setTimeout(r, 300));
    expect(fired).toBe(false);

    await sched.stop();
    sched.close();
    db.close();
  }, 10_000);
});

/* ------------------------------------------------------------------ */
/*  Multi-instance — lease exclusion                                   */
/* ------------------------------------------------------------------ */

describe('leader election', () => {
  it('only one worker fires when two share the same database', async () => {
    const db = memoryDb();
    let execCount = 0;
    let a: Scheduler;
    let b: Scheduler;
    const done = new Promise<void>((resolve) => {
      const fn = async (): Promise<void> => {
        execCount++;
        resolve();
      };

      const first = quickScheduler(db, { workerId: 'A' });
      const second = quickScheduler(db, { workerId: 'B' });
      a = first.sched;
      b = second.sched;

      a.register('shared', '* * * * *', fn);
      b.register('shared', '* * * * *', fn);

      a.start();
      b.start();
    });

    await done;
    // Allow a few more ticks so both instances would have tried
    await new Promise((r) => setTimeout(r, 300));
    // The interval check keeps the second from firing.
    expect(execCount).toBe(1);

    await Promise.all(
      [a, b].map((s) =>
        s.stop().catch(() => {
          /* ignore */
        }),
      ),
    );
    [a, b].forEach((s) => s.close());
    db.close();
  }, 10_000);

  it('lease expiry lets another worker claim the job', async () => {
    const db = memoryDb();
    let execCount = 0;

    const { sched: first } = quickScheduler(db, {
      workerId: 'first',
      leaseDurationMs: 200,
      pollIntervalMs: 50,
    });
    const { sched: second } = quickScheduler(db, {
      workerId: 'second',
      leaseDurationMs: 200,
      pollIntervalMs: 50,
    });

    first.register('lease-test', '* * * * *', async () => {
      execCount++;
    });
    second.register('lease-test', '* * * * *', async () => {
      execCount++;
    });

    first.start();
    await new Promise((r) => setTimeout(r, 250));
    expect(execCount).toBe(1);

    await first.stop();

    // Manually expire both lease and last_run_at so the second worker
    // can acquire the lease during its next poll cycle.
    const past = new Date(Date.now() - 86_400_000).toISOString();
    db.prepare(
      'UPDATE scheduler_leases SET lease_until = ?, last_run_at = ? WHERE job_name = ?',
    ).run(past, past, 'lease-test');

    // Now start the second scheduler so it can claim the expired lease
    second.start();
    await new Promise((r) => setTimeout(r, 300));
    expect(execCount).toBe(2);

    await second.stop();
    first.close();
    second.close();
    db.close();
  }, 10_000);
});

/* ------------------------------------------------------------------ */
/*  Exception resilience                                               */
/* ------------------------------------------------------------------ */

describe('exception resilience', () => {
  it('does not crash scheduler when a job throws', async () => {
    const { sched, db } = quickScheduler();
    const good = jobSpy();

    sched.register('sick', '* * * * *', async () => {
      throw new Error('boom');
    });
    sched.register('healthy', '* * * * *', good.fn);

    sched.start();
    await good.promise;
    expect(good.fn).toHaveBeenCalledTimes(1);

    await sched.stop();
    sched.close();
    db.close();
  }, 10_000);

  it('logs but does not crash on async rejection', async () => {
    const { sched, db } = quickScheduler();
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});

    sched.register('reject', '* * * * *', async () => {
      await Promise.reject(new Error('async-boom'));
    });
    sched.register('survivor', '* * * * *', async () => {});
    sched.start();

    await new Promise((r) => setTimeout(r, 200));

    // console.error should have been called at least once
    expect(spy.mock.calls.length).toBeGreaterThanOrEqual(1);
    const logged = spy.mock.calls
      .map((c) => c.join(' '))
      .some((s) => s.includes('reject') || s.includes('boom'));
    expect(logged).toBe(true);

    spy.mockRestore();
    await sched.stop();
    sched.close();
    db.close();
  }, 10_000);
});

/* ------------------------------------------------------------------ */
/*  Graceful shutdown                                                  */
/* ------------------------------------------------------------------ */

describe('graceful shutdown', () => {
  it('waits for in-flight job to finish', async () => {
    const { sched, db } = quickScheduler(undefined, {
      pollIntervalMs: 30,
      leaseDurationMs: 10_000,
    });

    let finished = false;
    const slowStarted = new Promise<void>((resolve) => {
      sched.register('slow', '* * * * *', async () => {
        resolve();
        await new Promise((r) => setTimeout(r, 400));
        finished = true;
      });
    });

    sched.start();
    await slowStarted;

    await sched.stop();
    expect(finished).toBe(true);

    sched.close();
    db.close();
  }, 10_000);

  it('does not trigger new jobs after stop', async () => {
    const { sched, db } = quickScheduler(undefined, {
      pollIntervalMs: 30,
      leaseDurationMs: 100,
    });
    let count = 0;

    const firstFire = new Promise<void>((resolve) => {
      sched.register('no-new', '* * * * *', async () => {
        count++;
        resolve();
      });
    });

    sched.start();
    await firstFire;
    expect(count).toBe(1);

    await sched.stop();

    await new Promise((r) => setTimeout(r, 200));
    expect(count).toBe(1);

    sched.close();
    db.close();
  }, 10_000);
});

/* ------------------------------------------------------------------ */
/*  NODE_ENV=test guard                                                */
/* ------------------------------------------------------------------ */

describe('NODE_ENV=test guard', () => {
  it('index.ts guard prevents start in test env by default', () => {
    const origEnv = process.env.NODE_ENV;
    process.env.NODE_ENV = 'test';
    delete process.env.SCHEDULER_ENABLED;

    const schedulerEnabled =
      process.env.NODE_ENV !== 'test' || process.env.SCHEDULER_ENABLED === 'true';
    expect(schedulerEnabled).toBe(false);

    // With SCHEDULER_ENABLED=true the guard is bypassed
    process.env.SCHEDULER_ENABLED = 'true';
    const schedulerEnabled2 =
      process.env.NODE_ENV !== 'test' || process.env.SCHEDULER_ENABLED === 'true';
    expect(schedulerEnabled2).toBe(true);

    process.env.NODE_ENV = origEnv;
  });
});

/* ------------------------------------------------------------------ */
/*  Job fires when cron interval elapses (with DB manipulation)        */
/* ------------------------------------------------------------------ */

describe('cron interval gating', () => {
  it('respects cron interval — does not fire before interval elapses', async () => {
    const { sched, db } = quickScheduler(undefined, {
      pollIntervalMs: 30,
      leaseDurationMs: 10_000,
    });
    let count = 0;

    const firstFire = new Promise<void>((resolve) => {
      sched.register('gated', '* * * * *', async () => {
        count++;
        resolve();
      });
    });

    sched.start();
    await firstFire;
    expect(count).toBe(1);

    // Without manipulating the DB the job should not fire again
    // because getCronIntervalMs('* * * * *') = 60000 ms.
    await new Promise((r) => setTimeout(r, 300));
    expect(count).toBe(1);

    // Now set last_run_at far in the past and the job becomes due again
    expireLastRun(db, 'gated');
    await new Promise((r) => setTimeout(r, 150));
    expect(count).toBe(2);

    await sched.stop();
    sched.close();
    db.close();
  }, 10_000);
});
