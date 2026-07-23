# Runbooks — Scheduled Jobs & Leader Election

## Overview

The backend uses a lightweight cron-style scheduler
(`src/lib/scheduler.ts`) backed by a SQLite `scheduler_leases` table.
Leader election is performed via `BEGIN IMMEDIATE` transactions so that
only one worker instance executes a given job within each cron window.

## Scheduled Jobs

| Job                        | Cron            | Description                                |
|----------------------------|-----------------|--------------------------------------------|
| `retention-cleanup`        | `0 0 * * *`     | Purge stale records (once daily)           |
| `invariant-check`          | `*/5 * * * *`   | Run domain-invariant checks (every 5 min)  |
| `reconciliation`           | `*/5 * * * *`   | Reconcile on-chain / off-chain state       |

## Leader Election (How It Works)

1. Every `pollIntervalMs` (default 10 s) the scheduler ticks.
2. For each registered job it attempts a `BEGIN IMMEDIATE` transaction
   on the shared SQLite database.
3. Inside the transaction it checks:
   - Is the lease still valid (`lease_until > now`)?  → held by another worker, skip.
   - Has the cron interval elapsed since `last_run_at`? → not due yet, skip.
4. If both checks pass the worker updates the lease row and runs the job.
5. After completion the lease is released immediately (`lease_until = now`).
6. If the worker crashes the lease expires automatically after
   `leaseDurationMs` (default 60 s), allowing another worker to pick up
   the next window.

## At-Most-Once Guarantee

- **Within a cron window** only one instance fires because the lease
  prevents concurrent acquisition.
- **Across windows** the interval check (`now - lastRunAt >= interval`)
  skips jobs that have already run.
- **Crash recovery** is bounded by `leaseDurationMs`.

## Configuration

The scheduler lives in `src/lib/scheduler.ts`.  Key parameters:

| Parameter           | Default  | Description                                   |
|---------------------|----------|-----------------------------------------------|
| `pollIntervalMs`    | 10 000   | How often to check for due jobs (ms)          |
| `leaseDurationMs`   | 60 000   | Lease lifetime before another worker can take over (ms) |

The SQLite database file defaults to `./data/scheduler.db` relative to
`process.cwd()`.  Override via `SchedulerOptions.dbPath`.

## Database Migrations

The `scheduler_leases` table is created automatically by the scheduler
constructor.  The migration file `src/migrations/v006_scheduler_leases.ts`
is provided for environments that run formal schema migrations:

```typescript
import { up } from '../src/migrations/v006_scheduler_leases';
up(yourDatabaseHandle);
```

## Graceful Shutdown

Call `scheduler.stop()` before closing the database.  `stop()` prevents
new ticks and waits for any in-flight job to finish (polling every 50 ms).
After `stop()` resolves, call `scheduler.close()` to release the SQLite
connection.

```typescript
const scheduler = new Scheduler({...});
scheduler.register('my-job', '*/5 * * * *', myFn);
scheduler.start();

// Later — e.g. on SIGTERM
await scheduler.stop();
scheduler.close();
```

## Troubleshooting

| Symptom                         | Likely Cause                         | Fix                                      |
|---------------------------------|--------------------------------------|------------------------------------------|
| Job never runs                  | Lease held by another instance       | Wait for lease expiry, or restart workers |
| Job runs too often              | `intervalMs` estimate too short      | Use more precise cron expression         |
| SQLITE_BUSY errors in logs      | Contention on SQLite file            | Increase `busy_timeout` / move to serverless PG |
| Scheduler not starting in tests | `NODE_ENV=test` guard in `index.ts`  | Pass an explicit `db` handle in tests     |
