/**
 * Migration v006: Scheduler Leases
 *
 * Creates the scheduler_leases table used by the leader-election
 * scheduler to guarantee at-most-once execution across instances.
 */
import type Database from 'better-sqlite3';

export function up(db: Database.Database): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS scheduler_leases (
      job_name    TEXT PRIMARY KEY,
      lease_until TEXT NOT NULL,
      worker_id   TEXT NOT NULL,
      last_run_at TEXT,
      created_at  TEXT NOT NULL DEFAULT (datetime('now')),
      updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
    );
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_scheduler_leases_lease_until
      ON scheduler_leases(lease_until);
  `);
}

export function down(db: Database.Database): void {
  db.exec(`DROP INDEX IF EXISTS idx_scheduler_leases_lease_until`);
  db.exec(`DROP TABLE IF EXISTS scheduler_leases`);
}
