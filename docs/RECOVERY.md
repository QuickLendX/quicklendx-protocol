# Recovery Guide

> Audience: operators and support engineers recovering indexed state after the
> QuickLendX indexer misses, reorders, or cannot safely trust a ledger range.

Use this guide when the canonical Soroban ledger is healthy, but QuickLendX
derived state, raw events, webhook delivery, or dashboards are behind or
inconsistent.

## Recovery goals

1. Stop financial actions from using stale indexed data.
2. Identify the last ledger and cursor known to be correct.
3. Refill any missing raw events before replaying derived state.
4. Replay or backfill only bounded ledger ranges with an idempotency key.
5. Verify the cursor, freshness metadata, event counts, and invariants before
   returning the system to normal operation.

## Recovery triggers

| Signal | Likely cause | First action |
|---|---|---|
| `qlx_ingest_lag_ledgers > 100` for more than five minutes | Indexer is behind the chain tip | Check backend logs and pause risky writes |
| `MonotonicIngester` reports `gap` | Missing ledger or skipped event index | Stop ingestion and fill the raw-event gap |
| `MonotonicIngester` reports `before` | Out-of-order event or rollback/reorg | Find the last confirmed-good ledger |
| Reconciliation reports `MISSING` rows | On-chain invoice is absent from the derived store | Run bounded drift backfill |
| Reconciliation reports `STATUS_MISMATCH` | Derived store is stale or replay missed an event | Replay from the last good ledger |
| Dashboard freshness `indexLagSeconds > 120` | API data is too stale for irreversible actions | Keep stale-data warning active and block risky actions |

## Key recovery state

Track these values in the incident notes before changing state:

| Value | Where to read it | Why it matters |
|---|---|---|
| Chain tip ledger | Soroban RPC or backend status endpoint | Defines how far behind the indexer is |
| Last indexed ledger | API freshness metadata or `indexer_state` | Starting point for lag recovery |
| Last committed event cursor | `ChainCursor { ledger_seq, tx_hash, event_index }` | Prevents duplicate or skipped event processing |
| Stored raw-event bounds | `raw_events` min/max ledger query | Shows whether replay data already exists |
| Latest reconciliation report | `/v1/monitoring/reconciliation` | Identifies missing or mismatched derived rows |
| Current replay/backfill run id | Backfill API response or audit log | Lets operators pause, resume, and audit recovery |

The cursor semantics are documented in
[`docs/backend-webhook-cursor-cache.md`](backend-webhook-cursor-cache.md):
duplicates are skipped, gaps halt ingestion, and a safe resume must load the
last committed cursor from durable storage before processing new events.

## Triage checklist

1. Confirm the backend is healthy.

   ```bash
   curl http://localhost:3001/health
   ```

2. Check freshness and lag.

   ```bash
   curl -s http://localhost:3001/api/v1/status \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     | jq '{index_lag, last_ledger}'
   ```

3. Inspect the current ingestion cursor.

   ```bash
   sqlite3 backend/.data/dev.db \
     "SELECT value_number FROM indexer_state WHERE key = 'ingestion_cursor';"
   ```

4. Inspect raw-event coverage.

   ```bash
   sqlite3 backend/.data/dev.db \
     "SELECT MIN(ledger) AS min_ledger, MAX(ledger) AS max_ledger FROM raw_events;"
   ```

5. Check whether reconciliation already identified drift.

   ```bash
   curl -s http://localhost:3001/v1/monitoring/reconciliation \
     -H "Authorization: Bearer $ADMIN_TOKEN" | jq
   ```

If the lag or mismatch affects lending, investing, bid acceptance, settlement,
or dispute actions, keep maintenance mode or stale-data blocking enabled until
verification passes.

## Recovery paths

### 1. Missing raw-event range

Use this when a ledger range is absent from `raw_events`.

1. Identify the missing inclusive range `[FROM_LEDGER, TO_LEDGER]`.
2. Fetch the range from the chain node or archival source.

   ```bash
   ./scripts/backfill-from-node.sh --from "$FROM_LEDGER" --to "$TO_LEDGER"
   ```

3. Verify the raw events landed.

   ```bash
   sqlite3 backend/.data/dev.db \
     "SELECT COUNT(*) FROM raw_events WHERE ledger BETWEEN $FROM_LEDGER AND $TO_LEDGER;"
   ```

4. Replay the range with a stable idempotency key.

   ```bash
   curl -s -X POST http://localhost:3001/api/admin/backfill \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d "{
       \"startLedger\": $FROM_LEDGER,
       \"endLedger\": $TO_LEDGER,
       \"dryRun\": false,
       \"idempotencyKey\": \"gap-backfill-$FROM_LEDGER-$TO_LEDGER\"
     }" | jq
   ```

### 2. Chain reorg or unsafe fork

Use this when indexed events above a ledger are no longer trusted.

1. Pick a rollback anchor at least 10 ledgers before the first suspicious
   ledger.
2. Dry-run the replay range.

   ```bash
   curl -s -X POST http://localhost:3001/api/admin/backfill \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d "{
       \"startLedger\": $ROLLBACK_TARGET,
       \"endLedger\": $CURRENT_CURSOR,
       \"dryRun\": true
     }" | jq
   ```

3. If the dry run reports the expected events, replay with an incident-specific
   idempotency key.

   ```bash
   curl -s -X POST http://localhost:3001/api/admin/backfill \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d "{
       \"startLedger\": $ROLLBACK_TARGET,
       \"endLedger\": $CURRENT_CURSOR,
       \"dryRun\": false,
       \"idempotencyKey\": \"reorg-recovery-$(date +%Y%m%d)\"
     }" | jq '.runId'
   ```

See [`backend/docs/REPLAY_RUNBOOK.md`](../backend/docs/REPLAY_RUNBOOK.md) for
pause, resume, and force-rebuild commands.

### 3. Derived-table drift only

Use this when raw events are complete, but reconciliation reports missing or
mismatched derived rows.

1. Run reconciliation if no current report exists.

   ```bash
   curl -X POST http://localhost:3000/api/v1/reconciliation/run \
     -H "Authorization: Bearer $QLX_OPERATIONS_TOKEN"
   ```

2. Trigger a bounded drift backfill.

   ```bash
   curl -X POST http://localhost:3000/api/v1/reconciliation/backfill \
     -H "Authorization: Bearer $QLX_OPERATIONS_TOKEN"
   ```

3. Poll progress.

   ```bash
   curl http://localhost:3000/api/v1/admin/monitoring/backfill-progress \
     -H "x-api-key: $API_KEY" | jq
   ```

The drift backfill progress model is documented in
[`docs/backfill.md`](backfill.md). A completed run is idempotent and should
return no additional work on repeat calls.

### 4. Schema migration rebuild

Use this when a new derived-table field or decoder change requires rebuilding
from stored raw events.

1. Enable maintenance mode before clearing derived state.
2. Dry-run the full or bounded replay range.
3. Run `/api/admin/backfill` with `forceRebuild: true`.
4. Keep maintenance mode enabled until cursor, state, and freshness checks pass.

Follow the force-rebuild section in
[`backend/docs/REPLAY_RUNBOOK.md`](../backend/docs/REPLAY_RUNBOOK.md). Do not
run a live force rebuild without an approved rollback plan.

## Verification checklist

Recovery is complete only when all of these pass:

| Check | Command or source | Expected result |
|---|---|---|
| Cursor advanced | `indexer_state` query | Equals the target `endLedger` |
| Raw events present | `raw_events` count over range | Non-zero for the recovered range |
| Replay/backfill run | Backfill run status endpoint | `completed` |
| Reconciliation | `/v1/monitoring/reconciliation` | No unresolved `MISSING` or `STATUS_MISMATCH` items |
| Freshness | API status or response envelope | `indexLagSeconds <= 30` for normal operation |
| Metrics | `/v1/metrics` | `qlx_ingest_lag_ledgers` returns to normal and invariant violations do not increase |
| Event schema | `docs/EVENTS_SCHEMA.md` topic list | No unknown event types were required for replay |

If any verification step fails, keep stale-data blocking active and continue
from the relevant recovery path above rather than manually editing derived
tables.

## Operational guardrails

- Always run a dry run before mutating replay or backfill state.
- Use a stable, descriptive `idempotencyKey` for every incident.
- Split large ranges so they respect `BACKFILL_MAX_LEDGER_RANGE` and
  `REPLAY_MAX_LEDGER_RANGE`.
- Never skip over a `MonotonicIngester` gap. Fill the raw events first, then
  replay.
- Do not serve silently stale financial data. The freshness envelope requires
  UI warnings, and irreversible actions should remain blocked when lag exceeds
  the documented threshold.
- Do not change event topic names during recovery. Topic strings are part of
  the public schema in `docs/EVENTS_SCHEMA.md`.
- Record the final cursor, replay run id, idempotency key, operator, and
  verification output in the incident notes.

## Related documentation

- [`docs/backend-webhook-cursor-cache.md`](backend-webhook-cursor-cache.md) -
  chain cursor ordering, gap detection, duplicate handling, and safe resume.
- [`docs/backfill.md`](backfill.md) - resumable drift backfill progress and
  monitoring endpoints.
- [`backend/docs/REPLAY_RUNBOOK.md`](../backend/docs/REPLAY_RUNBOOK.md) -
  replay, reorg recovery, gap backfill, and schema rebuild commands.
- [`docs/data-freshness-semantics.md`](data-freshness-semantics.md) - API
  freshness envelope and stale-data thresholds.
- [`docs/EVENTS_SCHEMA.md`](EVENTS_SCHEMA.md) - canonical contract topics and
  payload schemas.
- [`quicklendx-backend/docs/observability.md`](../quicklendx-backend/docs/observability.md) -
  ingest lag, webhook queue, RPC circuit, and invariant metrics.
