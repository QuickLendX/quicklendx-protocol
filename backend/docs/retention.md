# Retention Enforcement

## Overview

`src/services/retention.ts` now runs a scheduled retention worker that enforces
TTL cleanup for:

- `rawEventStore.ts`
- audit log files managed by `auditService.ts`
- snapshot rows managed through `snapshotService.ts`

Each run archives the rows it is about to purge, applies the purge with
rollback protection, and writes a `RETENTION_RUN` audit entry summarizing what
was removed.

## Default retention windows

The worker reads its policy from `src/config.ts`.

| Store | Env var | Default | Reasoning |
| --- | --- | ---: | --- |
| Raw events | `RETENTION_RAW_EVENTS_DAYS` | 30 days | Strictest window because raw event payloads are the most likely to contain KYC-adjacent data. |
| Audit logs | `RETENTION_AUDIT_LOG_DAYS` | 90 days | Keeps operator traceability longer than raw business payloads. |
| Snapshots | `RETENTION_SNAPSHOTS_DAYS` | 14 days | Derived data can be recreated and should stay lean for query performance. |
| Batch size | `RETENTION_BATCH_SIZE` | 500 rows per store per run | Limits blast radius and run latency. |
| Schedule | `RETENTION_INTERVAL_MS` | 24 hours | Default daily enforcement cadence. |
| Archive dir | `RETENTION_ARCHIVE_DIR` | `.data/retention-archives` | Stores purge batches before deletion. |

## Safety rules

The worker refuses to purge data that may still be needed:

- Raw events with `complianceHold: true` are never purged.
- If a replay is open, raw events at or above the earliest active replay cursor
  are retained.
- If reconciliation is running, expired raw events, audit logs, and snapshots
  are protected for that run.
- Purges are capped by `RETENTION_BATCH_SIZE`.
- If archiving or a later store update fails, the worker restores previously
  mutated stores before surfacing the error.

## Audit trail

Every successful live run appends a `RETENTION_RUN` entry through
`src/services/auditService.ts` with:

- run timing
- replay/reconciliation protection state
- cutoff timestamps
- scanned, eligible, protected, archived, and purged counts

If the audit summary cannot be written, the worker rolls back the purge so that
every live deletion remains auditable.

## Running tests

```bash
cd backend
npm test -- retention.test.ts
```

The retention test suite covers:

- nothing to purge
- TTL boundary timestamps
- active replay and reconciliation protection
- archive-then-delete failure rollback
- large batch truncation
- dry-run behavior
