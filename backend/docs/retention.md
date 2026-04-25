# Backend Retention Policies and Cleanup Jobs

## Overview

The retention service (`src/services/retention.ts`) defines TTL-based cleanup
jobs for four data categories.  Every job is safe, observable, and designed to
never delete records that are required for compliance, reconciliation, or
in-flight backfills.

---

## Data categories and TTLs

| Category | Default TTL | Reconciliation window | Compliance hold | Notes |
|:---|---:|---:|:---:|:---|
| `raw_events` | 90 days | 24 h | ✓ | Ledger events from the Soroban indexer |
| `snapshots` | 30 days | 1 h | — | Derived / aggregated table snapshots |
| `webhook_logs` | 14 days | 30 min | — | Webhook delivery attempt records |
| `operational_logs` | 7 days | 15 min | — | Server-side debug / info logs |

---

## Safety guarantees

### 1. Compliance hold (raw_events only)

Any `RawEvent` with `complianceHold: true` is **never deleted**, regardless of
age or TTL.  This flag covers AML/KYC audit trails that must be retained for
up to 7 years under financial regulations.

```
if record.complianceHold → skip, increment skippedHold
```

### 2. Reconciliation window

Records younger than `reconciliationWindowMs` are never deleted even if they
exceed the TTL.  This prevents a cleanup job from racing a backfill or
re-index operation that may still be writing records with past timestamps.

```
if (now - record.createdAt) < reconciliationWindowMs → skip, increment skippedWindow
```

### 3. Dry-run mode

Every job accepts `{ dryRun: true }`.  In dry-run mode the job computes and
returns what *would* be deleted without mutating the store.  Use this to audit
before enabling a new retention policy in production.

### 4. Batch limit

Each run deletes at most `batchSize` records.  This bounds the latency impact
on the running process and allows incremental cleanup across multiple scheduled
runs.  Records beyond the batch limit are left intact and will be picked up on
the next run.

### 5. Observable results

Every job returns a `CleanupResult`:

```typescript
interface CleanupResult {
  category: string;      // which data category was cleaned
  deleted: number;       // records removed (or would be removed in dry-run)
  skippedHold: number;   // records skipped due to compliance hold
  skippedWindow: number; // records skipped due to reconciliation window
  dryRun: boolean;
  deletedIds: string[];  // IDs of removed (or candidate) records
}
```

Log or emit `CleanupResult` to your observability pipeline after each run.

---

## API

### Individual jobs

```typescript
import {
  cleanRawEvents,
  cleanSnapshots,
  cleanWebhookLogs,
  cleanOperationalLogs,
  RetentionStore,
  DEFAULT_CONFIG,
} from "./src/services/retention";

const store = new RetentionStore(); // or inject your DB-backed implementation

// Run with defaults
const result = cleanRawEvents(store);

// Run with custom config and fixed clock (useful for testing)
const result = cleanRawEvents(
  store,
  { ttlMs: 90 * 86400_000, reconciliationWindowMs: 86400_000, batchSize: 1000 },
  { now: Date.now(), dryRun: false }
);
```

### Run all jobs

```typescript
import { runAllCleanupJobs } from "./src/services/retention";

const results = runAllCleanupJobs(store, {}, { dryRun: false });
results.forEach((r) => console.log(r));
```

---

## Scheduling

Wire `runAllCleanupJobs` into a cron job or a scheduled task runner.
A daily run at off-peak hours is recommended:

```typescript
// Example: run every day at 02:00 UTC
import cron from "node-cron";
cron.schedule("0 2 * * *", () => {
  const results = runAllCleanupJobs(store);
  results.forEach((r) =>
    logger.info("retention_cleanup", { ...r })
  );
});
```

---

## Configuration reference

```typescript
interface RetentionConfig {
  ttlMs: number;                  // max age before deletion (ms)
  reconciliationWindowMs: number; // minimum age before deletion (ms)
  batchSize: number;              // max records deleted per run
}
```

Override any category by passing a partial config map to `runAllCleanupJobs`:

```typescript
runAllCleanupJobs(store, {
  raw_events: { ttlMs: 180 * 86400_000, reconciliationWindowMs: 86400_000, batchSize: 500 },
});
```

---

## Security considerations

| Risk | Mitigation |
|:---|:---|
| Deleting compliance-required records | `complianceHold` flag checked before any deletion |
| Racing a backfill / re-index | `reconciliationWindowMs` prevents deletion of recently-created records |
| Bulk accidental deletion | `batchSize` cap limits blast radius per run |
| Silent data loss | `CleanupResult.deletedIds` provides a full audit trail |
| Untested policy changes | `dryRun` mode lets you preview deletions before applying |

---

## Testing

Test file: `backend/tests/retention.test.ts`

~55 tests across 7 sections:

| Section | Tests | What's validated |
|:---|:---|:---|
| DEFAULT_CONFIG | 6 | TTL values, positive window and batchSize |
| cleanRawEvents | 12 | Delete/keep by age, compliance hold, window, batchSize, dry-run, boundary |
| cleanSnapshots | 7 | Delete/keep by age, window, batchSize, dry-run, no hold |
| cleanWebhookLogs | 6 | Delete/keep by age, window, batchSize, dry-run |
| cleanOperationalLogs | 6 | Delete/keep by age, window, batchSize, dry-run |
| runAllCleanupJobs | 5 | All categories, combined clean, dry-run, default config |
| Safety — does not delete required | 8 | Held records, windowed records, mixed batch, batchSize continuity, deletedIds accuracy |
| CleanupResult shape | 3 | All fields present, dryRun default, deletedIds is array |

```bash
cd backend
npm test -- --testPathPattern=retention
npm run test:coverage -- --testPathPattern=retention
```
