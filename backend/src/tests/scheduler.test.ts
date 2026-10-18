import { mkdtempSync, rmSync } from "fs";
import { tmpdir } from "os";
import path from "path";
import Database from "better-sqlite3";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Scheduler } from "../lib/scheduler";

const schedulers: Scheduler[] = [];
const temporaryDirectories: string[] = [];

afterEach(async () => {
  const current = schedulers.splice(0);
  await Promise.all(current.map((scheduler) => scheduler.stop()));
  current.forEach((scheduler) => scheduler.close());
  temporaryDirectories.splice(0).forEach((directory) => rmSync(directory, { recursive: true, force: true }));
});

function createScheduler(db: Database.Database, workerId: string, now: () => number): Scheduler {
  const scheduler = new Scheduler({ db, workerId, now, leaseDurationMs: 60_000 });
  schedulers.push(scheduler);
  return scheduler;
}

describe("Scheduler cron windows", () => {
  it("honours a daily midnight schedule rather than firing on startup", () => {
    const db = new Database(":memory:");
    const clock = { value: Date.UTC(2026, 0, 1, 13, 37) };
    const scheduler = createScheduler(db, "A", () => clock.value);

    expect(scheduler.tryAcquireLease("daily", "0 0 * * *")).toBe(false);
    clock.value = Date.UTC(2026, 0, 1, 13, 37, 10);
    expect(scheduler.tryAcquireLease("daily", "0 0 * * *")).toBe(false);
    clock.value = Date.UTC(2026, 0, 2, 0, 0);
    expect(scheduler.tryAcquireLease("daily", "0 0 * * *")).toBe(true);
    clock.value += 30_000;
    expect(scheduler.tryAcquireLease("daily", "0 0 * * *")).toBe(false);
    db.close();
  });

  it("runs a */15 schedule at the next wall-clock quarter hour", () => {
    const db = new Database(":memory:");
    const clock = { value: Date.UTC(2026, 0, 1, 10, 7) };
    const scheduler = createScheduler(db, "A", () => clock.value);

    expect(scheduler.tryAcquireLease("quarter-hour", "*/15 * * * *")).toBe(false);
    clock.value = Date.UTC(2026, 0, 1, 10, 15);
    expect(scheduler.tryAcquireLease("quarter-hour", "*/15 * * * *")).toBe(true);
    db.prepare("UPDATE scheduler_leases SET lease_until = ? WHERE job_name = ?")
      .run(new Date(clock.value).toISOString(), "quarter-hour");
    clock.value = Date.UTC(2026, 0, 1, 10, 30);
    expect(scheduler.tryAcquireLease("quarter-hour", "*/15 * * * *")).toBe(true);
    db.close();
  });

  it("allows exactly one of two connections to claim each of 1,000 windows", async () => {
    const directory = mkdtempSync(path.join(tmpdir(), "scheduler-"));
    temporaryDirectories.push(directory);
    const filename = path.join(directory, "leases.sqlite");
    const firstDb = new Database(filename);
    const secondDb = new Database(filename);
    const clock = { value: Date.UTC(2026, 0, 1, 0, 0) };
    const first = createScheduler(firstDb, "A", () => clock.value);
    const second = createScheduler(secondDb, "B", () => clock.value);
    firstDb.prepare(
      `INSERT INTO scheduler_leases (job_name, lease_until, worker_id, last_run_at)
       VALUES (?, ?, ?, ?)`,
    ).run("shared", new Date(clock.value).toISOString(), "seed", new Date(clock.value - 300_000).toISOString());

    let winners = 0;
    for (let iteration = 0; iteration < 1_000; iteration++) {
      clock.value += 300_000;
      const now = clock.value;
      const claimedA = first.tryAcquireLease("shared", "*/5 * * * *");
      const claimedB = second.tryAcquireLease("shared", "*/5 * * * *");
      expect(Number(claimedA) + Number(claimedB)).toBe(1);
      winners += Number(claimedA) + Number(claimedB);
      firstDb.prepare("UPDATE scheduler_leases SET lease_until = ? WHERE job_name = ?")
        .run(new Date(now).toISOString(), "shared");
    }
    expect(winners).toBe(1_000);
  });

  it("records a failed job and releases its lease", async () => {
    const db = new Database(":memory:");
    const now = Date.UTC(2026, 0, 1, 0, 0);
    const scheduler = createScheduler(db, "A", () => now);
    const finished = vi.fn();
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    scheduler.register("throws", "* * * * *", async () => {
      throw new Error("expected failure");
    });
    scheduler.start();

    for (let attempt = 0; attempt < 50; attempt++) {
      const row = db.prepare(
        "SELECT lease_until, last_run_at FROM scheduler_leases WHERE job_name = ?",
      ).get("throws") as { lease_until: string; last_run_at: string | null } | undefined;
      if (row?.last_run_at && row.lease_until === row.last_run_at) {
        finished();
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    const row = db.prepare(
      "SELECT lease_until, last_run_at FROM scheduler_leases WHERE job_name = ?",
    ).get("throws") as { lease_until: string; last_run_at: string | null };
    expect(finished).toHaveBeenCalledOnce();
    expect(row.last_run_at).toBe(new Date(now).toISOString());
    expect(Date.parse(row.lease_until)).toBeLessThanOrEqual(now);
    expect(errorSpy).toHaveBeenCalled();
    errorSpy.mockRestore();
    db.close();
  });
});
