# Backfill Job — Resumability and Progress Tracking

## Overview

The drift-backfill job processes `DriftItem` records produced by `ReconciliationWorker.runReconciliation()` and syncs them back to the indexed store. From issue [#1072], the job is now **resumable after a crash or interruption**, processes items in bounded batches, and persists its cursor so subsequent calls continue from where the previous batch ended rather than restarting from scratch.

---

## Architecture

```
ReconciliationWorker.triggerBoundedBackfill(report)
       │
       └─▶ BackfillService.triggerDriftBackfill(report, batchSize, failFlag)
                 │
                 ├─ Reads/writes  backfill_progress  (SQLite)
                 └─ Appends per-item rows to  backfill_audit (SQLite)
```

### Key Components

| File | Role |
|---|---|
| `src/services/backfillService.ts` | Core engine — `triggerDriftBackfill`, `getDriftProgress` |
| `src/services/reconciliationWorker.ts` | Delegates `triggerBoundedBackfill` → `backfillService` |
| `src/migrations/v006_backfill_progress.ts` | Creates `backfill_progress` table |
| `src/routes/v1/reconciliation.ts` | `POST /api/v1/reconciliation/backfill` (ops-role gated) |
| `src/routes/v1/monitoring.ts` | `GET /api/v1/admin/monitoring/backfill-progress` |

---

## Database Schema

### `backfill_progress`

Created by migration `v006_backfill_progress` (version 8).

```sql
CREATE TABLE backfill_progress (
  id                TEXT PRIMARY KEY,
  audit_id          INTEGER REFERENCES backfill_audit(id) ON DELETE CASCADE,
  run_id            TEXT NOT NULL,           -- drift_<report.timestamp>
  last_processed_id TEXT,                    -- id of last successfully processed DriftItem
  remaining_count   INTEGER NOT NULL,
  total_count       INTEGER NOT NULL,
  status            TEXT NOT NULL CHECK(status IN ('running','paused','completed','failed')),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL
);
```

### Linkage to `backfill_audit`

Each successfully processed or failed drift item inserts a row into `backfill_audit` with:

| Column | Value |
|---|---|
| `run_id` | `drift_<timestamp>` |
| `event_type` | `completed` (success) or `failed` (error) |
| `invoice_id` | the `DriftItem.id` |
| `actor` | `system` |

---

## Resumability

**How it works:**

1. On the first call for a given `DriftReport`, a new `backfill_progress` row is inserted with `last_processed_id = NULL`.
2. Each subsequent call for the **same report timestamp** reads the existing row and finds `last_processed_id`.
3. The slice of `drifts` is advanced past `last_processed_id`, so only un-processed items are touched.
4. After each batch, `last_processed_id` and `remaining_count` are updated atomically.
5. When `remaining_count <= 0`, `status` is set to `completed`.

**Crash recovery scenario:**

```
Report has 100 drifts, batchSize = 10.

Call 1  → processes items 1–10, persists last_processed_id = invoice_10
[crash]
Call 2  → reads last_processed_id = invoice_10, resumes at item 11
         processes items 11–20, persists last_processed_id = invoice_20
...
Call 10 → processes items 91–100, sets status = completed
```

Subsequent calls after `status = completed` are **idempotent no-ops** (return `{successCount:0, failCount:0}`).

---

## API Surfaces

### Trigger bounded drift backfill

```
POST /api/v1/reconciliation/backfill
Authorization: Bearer <operations_admin_token>
```

- Requires `operations_admin` or `super_admin` role (enforced by `requireAdminRoles`).
- Uses the latest in-memory `DriftReport` from `ReconciliationWorker.getLatestReport()`.
- Returns `{successCount, failCount, errors[]}`.
- Returns `400` if no drift report exists (run `POST /api/v1/reconciliation/run` first).

### View progress

```
GET /api/v1/admin/monitoring/backfill-progress
x-api-key: <api-key>
```

Returns the most-recently-updated `backfill_progress` row:

```json
{
  "progress": {
    "id": "prog_1716864000000",
    "run_id": "drift_1716860000",
    "last_processed_id": "invoice_42",
    "remaining_count": 58,
    "total_count": 100,
    "status": "running",
    "created_at": "2026-05-28T03:00:00.000Z",
    "updated_at": "2026-05-28T03:05:12.000Z"
  }
}
```

Returns `null` progress object if no job has run yet.

---

## Security

| Surface | Auth |
|---|---|
| `POST /reconciliation/run` | `operations_admin` / `super_admin` Bearer token |
| `POST /reconciliation/backfill` | `operations_admin` / `super_admin` Bearer token |
| `GET /reconciliation/reports` | Public (read-only, no sensitive data) |
| `GET /admin/monitoring/backfill-progress` | API-key authenticated |

---

## Edge Cases

| Scenario | Behaviour |
|---|---|
| Empty drift report (`drifts: []`) | Returns `{successCount:0, failCount:0}` immediately, progress row inserted with `remaining_count=0`, `status=completed` |
| `failBackfill = true` | All items in batch throw, recorded as `failed` in audit log, counts returned in `errors[]` |
| Crash mid-batch | Next call picks up from `last_processed_id`; partially processed batch items re-entered from next index |
| Already completed run | Returns empty result immediately (idempotent re-run) |
| Concurrent calls for same run | SQLite `INSERT OR IGNORE`-style check prevents duplicate progress rows; both calls operate on same checkpoint |
| No RBAC token configured | `503 RBAC_NOT_CONFIGURED` |

---

## Configuration

| Env Var | Default | Description |
|---|---|---|
| `BACKFILL_MAX_LEDGER_RANGE` | `5000` | Max ledger range per ledger-range backfill run |
| `BACKFILL_MAX_CONCURRENCY` | `4` | Max concurrent workers for ledger-range backfill |
| `DATABASE_PATH` | `.data/dev.db` | SQLite file path used by `getDatabase()` |
| `QLX_OPERATIONS_TOKEN` | _(required for write routes)_ | Bearer token granting `operations_admin` role |
| `QLX_SUPER_ADMIN_TOKEN` | _(optional)_ | Bearer token granting `super_admin` role |

---

## Running Locally

```bash
# 1. Start a reconciliation cycle to generate a drift report
curl -X POST http://localhost:3000/api/v1/reconciliation/run \
     -H "Authorization: Bearer $QLX_OPERATIONS_TOKEN"

# 2. Trigger a bounded backfill pass
curl -X POST http://localhost:3000/api/v1/reconciliation/backfill \
     -H "Authorization: Bearer $QLX_OPERATIONS_TOKEN"

# 3. Poll progress
curl http://localhost:3000/api/v1/admin/monitoring/backfill-progress \
     -H "x-api-key: $API_KEY"
```

---

## Test Coverage

Tests live in `src/tests/backfill.service.test.ts` and cover **24 cases** across two suites:

- **Ledger-range runs** (13 tests): validation errors, pause/resume lifecycle, idempotency, dry-run, stale index entries.
- **Drift backfill / resumable** (11 tests): fresh run, bounded pass, crash-resume mid-batch, idempotent completed re-run, empty report, `failBackfill` flag, audit log entries, single progress row invariant, `getDriftProgress` before/after run.

Run with:

```bash
cd backend
npm test -- --testPathPatterns=backfill.service.test.ts
```
