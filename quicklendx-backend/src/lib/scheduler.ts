/**
 * Scheduler — cron-style job framework with SQLite leader-election.
 *
 * register(name, cron, fn)  – declare a recurring job.
 * start()                    – begin the polling loop.
 * stop()                     – graceful shutdown (waits for in-flight jobs).
 *
 * Leader election uses a `scheduler_leases` table and `BEGIN IMMEDIATE`
 * transactions so that only one instance at a time executes a given job
 * within each cron window.
 */

import Database from 'better-sqlite3';
import path from 'path';
import fs from 'fs';

/* ------------------------------------------------------------------ */
/*  Public types                                                      */
/* ------------------------------------------------------------------ */

export interface ScheduledJob {
  name: string;
  /** Standard 5-field cron expression (e.g. 'star/5 * * * *'). */
  cron: string;
  /** The async function to invoke on each scheduled tick. */
  fn: () => Promise<void>;
  running: boolean;
}

export interface SchedulerOptions {
  /** Reuse an existing Database handle (useful for tests with `:memory:`). */
  db?: Database.Database;
  /** Path to the SQLite file (ignored when `db` is provided). */
  dbPath?: string;
  /** Unique identifier for this worker instance. */
  workerId?: string;
  /** How often (ms) to poll for due jobs.  Default 10_000. */
  pollIntervalMs?: number;
  /** How long (ms) a lease lives before another worker may claim it. Default 60_000. */
  leaseDurationMs?: number;
}

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

/**
 * Best-effort extraction of the repeating interval (ms) from a cron
 * expression.  Supports the common patterns used by operational jobs:
 *
 *   star/5 * * * *  → 5 minutes
 *   0 * * * *       → 1 hour
 *   0 0 * * *       → 24 hours
 *
 * Falls back to 1 minute for unusual or unparseable expressions.
 */
export function getCronIntervalMs(cron: string): number {
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5) return 60_000;

  const minField = parts[0];
  const hourField = parts[1];
  const domField = parts[2];
  const monthField = parts[3];
  const dowField = parts[4];

  // — month-level —
  if (monthField !== '*') {
    const months = expandField(monthField, 1, 12);
    if (months.length > 0 && months.length < 12) {
      return Math.round((365.25 / months.length) * 86_400_000);
    }
  }

  // — day-level —
  if (domField !== '*' || dowField !== '*') {
    if (domField !== '*' && dowField === '*') {
      const days = expandField(domField, 1, 31);
      if (days.length > 0 && days.length < 31) return Math.round((31 / days.length) * 86_400_000);
    }
    if (dowField !== '*' && domField === '*') {
      const days = expandField(dowField, 0, 6);
      if (days.length > 0 && days.length < 7) return Math.round((7 / days.length) * 86_400_000);
    }
    return 86_400_000; // daily fallback
  }

  // — hour-level —
  if (hourField !== '*' && minField !== '*') {
    const hours = expandField(hourField, 0, 23);
    if (hours.length > 0 && hours.length < 24) {
      return Math.round((24 / hours.length) * 3_600_000);
    }
  }

  // — /N pattern in minute —
  const minuteStep = parseStepPattern(minField, 1, 59);
  if (minuteStep > 0 && minuteStep < 60) return minuteStep * 60_000;

  // — /N pattern in hour —
  const hourStep = parseStepPattern(hourField, 1, 23);
  if (hourStep > 0 && hourStep < 24) return hourStep * 3_600_000;

  // minute fixed, hour wildcard → hourly
  if (minField !== '*' && hourField === '*') return 3_600_000;

  return 60_000; // safety fallback
}

function parseStepPattern(field: string, _min: number, _max: number): number {
  const m = field.match(/^\*\/(\d+)$/);
  return m ? parseInt(m[1], 10) : 0;
}

function expandField(field: string, lo: number, hi: number): number[] {
  const vals: number[] = [];
  for (const part of field.split(',')) {
    if (part === '*') {
      for (let i = lo; i <= hi; i++) vals.push(i);
    } else if (part.startsWith('*/')) {
      const step = parseInt(part.slice(2), 10);
      if (step > 0) for (let i = lo; i <= hi; i += step) vals.push(i);
    } else if (part.includes('-')) {
      const [a, b] = part.split('-').map(Number);
      for (let i = Math.max(a, lo); i <= Math.min(b, hi); i++) vals.push(i);
    } else {
      const n = parseInt(part, 10);
      if (!isNaN(n) && n >= lo && n <= hi) vals.push(n);
    }
  }
  return [...new Set(vals)].sort((a, b) => a - b);
}

/* ------------------------------------------------------------------ */
/*  Scheduler                                                          */
/* ------------------------------------------------------------------ */

export class Scheduler {
  private jobs = new Map<string, ScheduledJob>();
  private db: Database.Database;
  private workerId: string;
  private pollIntervalMs: number;
  private leaseDurationMs: number;
  private timer: ReturnType<typeof setInterval> | null = null;
  private _started = false;
  private _stopped = false;

  constructor(opts: SchedulerOptions = {}) {
    this.workerId = opts.workerId ?? `worker-${process.pid}`;
    this.pollIntervalMs = opts.pollIntervalMs ?? 10_000;
    this.leaseDurationMs = opts.leaseDurationMs ?? 60_000;

    if (opts.db) {
      this.db = opts.db;
    } else {
      const dbPath = opts.dbPath ?? path.join(process.cwd(), 'data', 'scheduler.db');
      fs.mkdirSync(path.dirname(dbPath), { recursive: true });
      this.db = new Database(dbPath);
    }

    this.db.pragma('journal_mode = WAL');
    this.db.pragma('busy_timeout = 5000');
    this.ensureTable();
  }

  /* ----------------------------------------------------------------- */
  /*  Public API                                                        */
  /* ----------------------------------------------------------------- */

  get started(): boolean {
    return this._started;
  }

  /**
   * Register a recurring job.
   * Must be called **before** `start()`.
   */
  register(name: string, cron: string, fn: () => Promise<void>): this {
    if (this._started) {
      throw new Error('Cannot register jobs after scheduler has started');
    }
    this.jobs.set(name, { name, cron, fn, running: false });
    return this;
  }

  /** Begin the polling loop. */
  start(): void {
    if (this._started) return;
    this._started = true;
    this._stopped = false;

    this.tick();

    this.timer = setInterval(() => this.tick(), this.pollIntervalMs);
    if (this.timer && 'unref' in this.timer) {
      this.timer.unref();
    }
  }

  /**
   * Graceful shutdown – prevents new ticks and waits for any
   * in-flight job to finish.
   */
  async stop(): Promise<void> {
    this._stopped = true;
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }

    const inflight = [...this.jobs.values()].filter((j) => j.running);
    if (inflight.length > 0) {
      await Promise.all(
        inflight.map(
          (job) =>
            new Promise<void>((resolve) => {
              const poll = (): void => {
                if (!job.running) resolve();
                else setTimeout(poll, 50);
              };
              poll();
            }),
        ),
      );
    }
  }

  /** Close the underlying database.  Should be called after `stop()`. */
  close(): void {
    this.db.close();
  }

  /** Exposed for tests – number of registered jobs. */
  get jobCount(): number {
    return this.jobs.size;
  }

  /* ----------------------------------------------------------------- */
  /*  Internal                                                          */
  /* ----------------------------------------------------------------- */

  private ensureTable(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS scheduler_leases (
        job_name    TEXT PRIMARY KEY,
        lease_until TEXT NOT NULL,
        worker_id   TEXT NOT NULL,
        last_run_at TEXT,
        created_at  TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);
  }

  private tick(): void {
    for (const job of this.jobs.values()) {
      if (this._stopped) break;
      if (job.running) continue;

      if (this.tryAcquireLease(job.name, job.cron)) {
        job.running = true;
        this.executeJob(job).finally(() => {
          job.running = false;
        });
      }
    }
  }

  /**
   * Attempt to acquire an exclusive lease for `jobName`.
   * Uses `BEGIN IMMEDIATE` so that at most one worker succeeds.
   *
   * Returns `true` when the caller should run the job.
   */
  private tryAcquireLease(jobName: string, cronExpr: string): boolean {
    try {
      return this.db.transaction(
        () => {
          const now = Date.now();
          const row = this.db
            .prepare(
              'SELECT lease_until, last_run_at FROM scheduler_leases WHERE job_name = ?',
            )
            .get(jobName) as { lease_until: string; last_run_at: string | null } | undefined;

          // — lease still held by another worker —
          if (row) {
            const leaseUntil = new Date(row.lease_until).getTime();
            if (leaseUntil > now) return false;
          }

          // — check cron schedule —
          const lastRunAt = row?.last_run_at
            ? new Date(row.last_run_at).getTime()
            : 0;

          if (lastRunAt > 0) {
            const intervalMs = getCronIntervalMs(cronExpr);
            if (now - lastRunAt < intervalMs) return false;
          }

          // — acquire / refresh lease —
          const leaseUntil = new Date(now + this.leaseDurationMs).toISOString();
          this.db
            .prepare(
              `INSERT INTO scheduler_leases (job_name, lease_until, worker_id, last_run_at, created_at, updated_at)
               VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))
               ON CONFLICT(job_name) DO UPDATE SET
                 lease_until = excluded.lease_until,
                 worker_id   = excluded.worker_id,
                 updated_at  = datetime('now')`,
            )
            .run(jobName, leaseUntil, this.workerId, null);

          return true;
        },
        { behavior: 'immediate' },
      )();
    } catch {
      // Another instance may have won the lock; skip this tick.
      return false;
    }
  }

  private async executeJob(job: ScheduledJob): Promise<void> {
    try {
      await job.fn();
    } catch (err) {
      console.error(`[Scheduler] Job "${job.name}" failed:`, err);
    } finally {
      const now = new Date().toISOString();
      this.db
        .prepare(
          `UPDATE scheduler_leases
              SET last_run_at = ?, lease_until = ?, updated_at = datetime('now')
            WHERE job_name = ?`,
        )
        .run(now, now, job.name);
    }
  }
}
