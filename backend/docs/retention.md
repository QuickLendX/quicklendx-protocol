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
| Cold storage dir | `ARCHIVE_DIR` | `.data/archives` | Gzip-archive storage directory for raw events. |
| Archival enabled | `ARCHIVE_ENABLED` | `true` | Enables or disables cold storage archiving. |

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

## Cold Storage Archival & Recovery

For `raw_events`, outright deletion is avoided by using a cold storage archival tier:
- **Archival Tier**: Expired events are grouped by year-month (based on `indexedAt` timestamp) and appended using gzip streaming to `<ARCHIVE_DIR>/raw-events-YYYY-MM.jsonl.gz`.
- **Integrity**: A SHA-256 checksum is calculated for the archive file and written to `<ARCHIVE_DIR>/raw-events-YYYY-MM.jsonl.gz.sha256`. Checksums are verified before any recovery occurs.
- **Dry-run**: Dry-runs simulate planning but do not write archives or delete live events.

### Restoring Archived Events

You can restore a date range of archived raw events back into the live event store using the recovery CLI:

```bash
# From backend directory
npx ts-node scripts/restore-archived-events.ts --start 2026-04-01 --end 2026-04-30
```

The recovery process:
- Locates gzip files matching the range.
- Verifies their checksums.
- Decompresses and parses event lines.
- Enforces event-level idempotency by skipping events already present in the live store.

## Running tests

```bash
cd backend
npm test -- retention.test.ts
npm test -- archival-restore.test.ts
```

The retention and archival test suites cover:

- nothing to purge
- TTL boundary timestamps
- active replay and reconciliation protection
- archive-then-delete failure rollback
- large batch truncation
- dry-run behavior
- gzip-streaming output format & checksum integrity
- recovery date filtering and idempotency

