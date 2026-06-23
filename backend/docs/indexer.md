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

### Batch cursor

Re-submitting a batch with a cursor that has already been committed returns `skipped: true` and makes no writes. This means retries after a crash are safe — the second attempt either commits (if the first truly failed) or is a no-op (if the first succeeded but the caller didn't receive the acknowledgement).

### Event-level `(tx_hash, event_index)`

The persisted raw-event layer enforces a second idempotency invariant independent of the batch cursor:

- Each on-chain event is uniquely identified by `(tx_hash, event_index)`.
- Migration `v011_raw_events_unique` creates the `raw_events` table and the `raw_events_idempotency` unique index on that pair.
- Normal ingestion inserts with `ON CONFLICT(tx_hash, event_index) DO NOTHING`, so a re-delivered event in a different batch is ignored instead of duplicated.
- Reorg recovery re-ingests with `replaceOnConflict: true`, using `ON CONFLICT DO UPDATE` so canonical rows replace orphaned data for the same key.

**Invariant:** at most one row exists per `(tx_hash, event_index)`. The scheduled invariant suite (`checkRawEventIdempotency`) scans for accidental duplicates and alerts if any are found.

SQLite `UNIQUE` violations surface as `RawEventDuplicateError` (code `RAW_EVENT_DUPLICATE`) rather than crashing the process.

## Partial-failure prevention

All events in a batch are validated **before** `commitBatch` is called. If any event is malformed, the entire batch is rejected and the store is not touched. This prevents "half-indexed" states where some events from a batch are persisted but others are not.

## Store contract

`IngestionStore` has three methods:

| Method | Responsibility |
|--------|---------------|
| `getCursor()` | Return the last committed cursor (or `null`). |
| `commitBatch(events, cursor)` | Persist events and advance cursor atomically. Must throw on failure without partial writes. |
| `rollbackTo(cursor)` | Delete events above `cursor` and reset the committed cursor. Used for reorg recovery. |

`InMemoryIngestionStore` and `SqliteIngestionStore` satisfy this contract. Production uses the SQLite implementation backed by `raw_events`.

## Freshness state semantics

- The backend persists current indexer freshness in the SQLite table `freshness_state`.
- On boot, `backend/src/services/freshnessService.ts` loads the saved cursor and timestamp from `freshness_state` and uses them to compute response freshness.
- If the table is empty or missing, the service falls back to the existing in-memory default behavior and returns a conservative freshness estimate.
- Updates to freshness state are debounced by 100ms to avoid repeated database writes during rapid cursor advancement.
- Concurrent freshness updates always keep the latest cursor/timestamp pair, and the final debounced write persists the most recent value.

## Security notes

- Cursor can only advance, never regress. The idempotency check enforces this.
- Validation runs before any write, so invalid payloads cannot corrupt the index.
- The store interface is injected, making it straightforward to swap in a real DB-backed implementation with proper ACID transactions.

