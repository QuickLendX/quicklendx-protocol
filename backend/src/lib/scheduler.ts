import Database from "better-sqlite3";
import fs from "fs";
import path from "path";

export interface ScheduledJob {
  name: string;
  cron: string;
  fn: () => Promise<void>;
  running: boolean;
}

export interface SchedulerOptions {
  db?: Database.Database;
  dbPath?: string;
  workerId?: string;
  pollIntervalMs?: number;
  leaseDurationMs?: number;
  now?: () => number;
}

type CronFields = [Set<number>, Set<number>, Set<number>, Set<number>, Set<number>];

function parseCronField(field: string, min: number, max: number): Set<number> {
  if (!field) throw new Error("Cron fields cannot be empty");
  const values = new Set<number>();
  for (const part of field.split(",")) {
    if (!part) throw new Error(`Invalid cron field: ${field}`);
    const segments = part.split("/");
    if (segments.length > 2) throw new Error(`Invalid cron field: ${field}`);
    const [range, stepText] = segments;
    const step = stepText === undefined ? 1 : Number(stepText);
    if (!Number.isInteger(step) || step < 1) throw new Error(`Invalid cron field: ${field}`);

    let start: number;
    let end: number;
    if (range === "*") {
      start = min;
      end = max;
    } else if (range.includes("-")) {
      const bounds = range.split("-").map(Number);
      if (bounds.length !== 2) throw new Error(`Invalid cron field: ${field}`);
      [start, end] = bounds;
    } else {
      start = Number(range);
      end = stepText === undefined ? start : max;
    }

    if (!Number.isInteger(start) || !Number.isInteger(end) || start < min || end > max || start > end) {
      throw new Error(`Invalid cron field: ${field}`);
    }
    for (let value = start; value <= end; value += step) values.add(value);
  }
  return values;
}

function parseCron(expression: string): CronFields {
  const fields = expression.trim().split(/\s+/);
  if (fields.length !== 5) throw new Error(`Invalid five-field cron expression: ${expression}`);
  return [
    parseCronField(fields[0], 0, 59),
    parseCronField(fields[1], 0, 23),
    parseCronField(fields[2], 1, 31),
    parseCronField(fields[3], 1, 12),
    parseCronField(fields[4], 0, 7),
  ];
}

function matchesDay(date: Date, cron: CronFields): boolean {
  const [, , daysOfMonth, months, daysOfWeek] = cron;
  if (!months.has(date.getUTCMonth() + 1)) return false;
  const dayOfMonthMatches = daysOfMonth.has(date.getUTCDate());
  const dayOfWeek = date.getUTCDay();
  const dayOfWeekMatches = daysOfWeek.has(dayOfWeek) || (dayOfWeek === 0 && daysOfWeek.has(7));
  const domIsWildcard = daysOfMonth.size === 31;
  const dowIsWildcard = daysOfWeek.size === 8;
  if (!domIsWildcard && !dowIsWildcard) return dayOfMonthMatches || dayOfWeekMatches;
  return dayOfMonthMatches && dayOfWeekMatches;
}

function previousScheduledTime(expression: string, now: number): number | null {
  const cron = parseCron(expression);
  const [minutes, hours] = cron;
  const current = new Date(now);
  const today = Date.UTC(current.getUTCFullYear(), current.getUTCMonth(), current.getUTCDate());

  for (let daysBack = 0; daysBack <= 366 * 8; daysBack++) {
    const date = new Date(today - daysBack * 86_400_000);
    if (!matchesDay(date, cron)) continue;
    for (const hour of [...hours].sort((a, b) => b - a)) {
      for (const minute of [...minutes].sort((a, b) => b - a)) {
        const fireAt = Date.UTC(
          date.getUTCFullYear(),
          date.getUTCMonth(),
          date.getUTCDate(),
          hour,
          minute,
        );
        if (fireAt <= now) return fireAt;
      }
    }
  }
  return null;
}

export class Scheduler {
  private readonly jobs = new Map<string, ScheduledJob>();
  private readonly db: Database.Database;
  private readonly workerId: string;
  private readonly pollIntervalMs: number;
  private readonly leaseDurationMs: number;
  private readonly now: () => number;
  private timer: ReturnType<typeof setInterval> | null = null;
  private _started = false;
  private _stopped = false;

  constructor(options: SchedulerOptions = {}) {
    this.workerId = options.workerId ?? `worker-${process.pid}`;
    this.pollIntervalMs = options.pollIntervalMs ?? 10_000;
    this.leaseDurationMs = options.leaseDurationMs ?? 60_000;
    this.now = options.now ?? Date.now;

    if (options.db) {
      this.db = options.db;
    } else {
      const dbPath = options.dbPath ?? path.join(process.cwd(), "data", "scheduler.db");
      fs.mkdirSync(path.dirname(dbPath), { recursive: true });
      this.db = new Database(dbPath);
    }

    this.db.pragma("journal_mode = WAL");
    this.db.pragma("busy_timeout = 5000");
    this.ensureTable();
  }

  get started(): boolean {
    return this._started;
  }

  get jobCount(): number {
    return this.jobs.size;
  }

  register(name: string, cron: string, fn: () => Promise<void>): this {
    if (this._started) throw new Error("Cannot register jobs after scheduler has started");
    parseCron(cron);
    this.jobs.set(name, { name, cron, fn, running: false });
    return this;
  }

  start(): void {
    if (this._started) return;
    this._started = true;
    this._stopped = false;
    this.tick();
    this.timer = setInterval(() => this.tick(), this.pollIntervalMs);
    this.timer.unref?.();
  }

  async stop(): Promise<void> {
    this._stopped = true;
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    await Promise.all(
      [...this.jobs.values()]
        .filter((job) => job.running)
        .map(
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

  close(): void {
    if (this.db.open) this.db.close();
  }

  private ensureTable(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS scheduler_leases (
        job_name TEXT PRIMARY KEY,
        lease_until TEXT NOT NULL,
        worker_id TEXT NOT NULL,
        last_run_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);
    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_scheduler_leases_lease_until
        ON scheduler_leases(lease_until)
    `);
  }

  private tick(): void {
    for (const job of this.jobs.values()) {
      if (this._stopped) break;
      if (job.running || !this.tryAcquireLease(job.name, job.cron)) continue;
      job.running = true;
      void this.executeJob(job).finally(() => {
        job.running = false;
      }).catch((error: unknown) => {
        console.error(`[Scheduler] Failed to finish job "${job.name}":`, error);
      });
    }
  }

  tryAcquireLease(jobName: string, cronExpr: string): boolean {
    const now = this.now();
    const previousFire = previousScheduledTime(cronExpr, now);
    const transaction = this.db.transaction(() => {
      const row = this.db
        .prepare("SELECT lease_until, last_run_at FROM scheduler_leases WHERE job_name = ?")
        .get(jobName) as { lease_until: string; last_run_at: string | null } | undefined;

      if (row && Date.parse(row.lease_until) > now) return false;
      const lastRunAt = row?.last_run_at ? Date.parse(row.last_run_at) : null;

      if (lastRunAt === null) {
        const isAtScheduledMinute = previousFire === Math.floor(now / 60_000) * 60_000;
        if (!isAtScheduledMinute) {
          this.db
            .prepare(
              `INSERT INTO scheduler_leases (job_name, lease_until, worker_id, last_run_at)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(job_name) DO UPDATE SET
                 lease_until = excluded.lease_until,
                 worker_id = excluded.worker_id,
                 last_run_at = excluded.last_run_at,
                 updated_at = datetime('now')`,
            )
            .run(jobName, new Date(now).toISOString(), this.workerId, new Date(now).toISOString());
          return false;
        }
      } else if (previousFire === null || previousFire <= lastRunAt) {
        return false;
      }

      const leaseUntil = new Date(now + this.leaseDurationMs).toISOString();
      const scheduledAt = new Date(previousFire ?? now).toISOString();
      this.db
        .prepare(
          `INSERT INTO scheduler_leases (job_name, lease_until, worker_id, last_run_at)
           VALUES (?, ?, ?, ?)
           ON CONFLICT(job_name) DO UPDATE SET
             lease_until = excluded.lease_until,
             worker_id = excluded.worker_id,
             last_run_at = excluded.last_run_at,
             updated_at = datetime('now')`,
        )
        .run(jobName, leaseUntil, this.workerId, scheduledAt);
      return true;
    });
    return transaction.immediate();
  }

  private async executeJob(job: ScheduledJob): Promise<void> {
    try {
      await job.fn();
    } catch (error) {
      console.error(`[Scheduler] Job "${job.name}" failed:`, error);
    } finally {
      const now = new Date(this.now()).toISOString();
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
