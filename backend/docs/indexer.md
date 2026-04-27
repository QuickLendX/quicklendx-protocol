# Indexer — Transaction Semantics

## Overview

The ingestion layer (`src/services/ingestion.ts`) processes on-chain event batches atomically. Either the full batch is committed together with the cursor advance, or nothing is written.

## Unit of work

```
ingestBatch(store, events, batchCursor)
  1. Read current cursor from store
  2. If batchCursor <= currentCursor → skip (idempotent)
  3. Validate every event in the batch
  4. store.commitBatch(events, batchCursor)   ← atomic
```

Steps 1–3 happen in application memory. Only step 4 touches durable state, so a crash before step 4 leaves the store unchanged and the batch can be safely retried.

## Idempotency

Re-submitting a batch with a cursor that has already been committed returns `skipped: true` and makes no writes. This means retries after a crash are safe — the second attempt either commits (if the first truly failed) or is a no-op (if the first succeeded but the caller didn't receive the acknowledgement).

## Partial-failure prevention

All events in a batch are validated **before** `commitBatch` is called. If any event is malformed, the entire batch is rejected and the store is not touched. This prevents "half-indexed" states where some events from a batch are persisted but others are not.

## Store contract

`IngestionStore` has two methods:

| Method | Responsibility |
|--------|---------------|
| `getCursor()` | Return the last committed cursor (or `null`). |
| `commitBatch(events, cursor)` | Persist events and advance cursor atomically. Must throw on failure without partial writes. |

The `InMemoryIngestionStore` implementation satisfies this contract and is used in tests. A production implementation would wrap a database transaction.

## Security notes

- Cursor can only advance, never regress. The idempotency check enforces this.
- Validation runs before any write, so invalid payloads cannot corrupt the index.
- The store interface is injected, making it straightforward to swap in a real DB-backed implementation with proper ACID transactions.
