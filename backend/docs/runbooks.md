# Scheduler runbook

The backend includes an unregistered cron scheduler in
[`src/lib/scheduler.ts`](../src/lib/scheduler.ts). It uses the
`scheduler_leases` SQLite table and an immediate transaction to coordinate
workers that share a database. Cron expressions use five fields and are
evaluated against UTC wall-clock time.

The scheduler is intentionally not wired into backend startup, and no jobs are
registered. Registering retention, invariant, or reconciliation jobs is a
separate operational decision.

## Migration

Apply migration
[`src/migrations/v013_scheduler_leases.ts`](../src/migrations/v013_scheduler_leases.ts)
through the normal backend migration runner. The scheduler also creates the
table idempotently when constructed.

## Usage and shutdown

Create a scheduler with a shared SQLite database and a unique worker ID,
register jobs before starting, and stop it before closing the database:

```ts
const scheduler = new Scheduler({ db, workerId: "worker-a" });
scheduler.register("example", "*/5 * * * *", async () => {
  // Perform the scheduled work.
});
scheduler.start();

await scheduler.stop();
scheduler.close();
```

`pollIntervalMs` defaults to 10 seconds and `leaseDurationMs` to 60 seconds.
The first poll establishes a schedule baseline unless it occurs exactly on a
matching cron minute. Later executions happen only after a scheduled fire time
newer than the stored run time. A completed or failed job records its run time
and releases its lease; a crashed worker's lease expires after its duration.

## SQLite backup and restore

Create a consistent online backup of the configured SQLite database and verify
it with `PRAGMA integrity_check` and row counts from key tables:

```sh
cd backend
npm run db:backup
```

Backups are written to `backups/` next to `DATABASE_PATH`. The newest seven
verified backups are retained by default. Set `BACKUP_RETENTION` to a positive
integer to change that limit. The command exits nonzero if backup creation,
integrity checking, or row-count verification fails. Backup files contain
service state and must be protected like the database itself.

To restore, first stop every process using the database. Preserve the existing
database files (including any `-wal` and `-shm` files), then copy the chosen
verified backup to the path configured by `DATABASE_PATH`. Do not replace a
live database or restore only the main file while SQLite is running. Start the
service and verify its health before removing the preserved files.
