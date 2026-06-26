# Replay Runbook — Replaying Ingestion from a Specific Ledger

**Audience: operators** performing recovery, backfill, or schema-migration work on a live or
staging QuickLendX backend.

---

## When to use this runbook

| Scenario | What happened | Section |
|---|---|---|
| Chain reorg | The indexer followed a fork; raw events above the fork point are stale | [§ Reorg recovery](#reorg-recovery) |
| Missed ledger range | A network outage left a gap in `raw_events` | [§ Gap backfill](#gap-backfill) |
| Schema migration | A derived-table column was added; old rows need rebuilding | [§ Force rebuild after migration](#force-rebuild-after-migration) |
| Smoke test / dry run | You want to know how many events a range contains before committing | [§ Dry run preview](#dry-run-preview) |
| Stuck or failed run | A previous replay stalled; you need to resume or restart it | [§ Resuming a paused or failed run](#resuming-a-paused-or-failed-run) |

---

## Prerequisites

```bash
# The backend must be running and healthy
curl http://localhost:3001/health   # → {"status":"ok"}

# You need a valid admin API key (ADMIN_API_TOKEN or OPERATIONS_ADMIN_TOKEN)
export ADMIN_TOKEN="<your-admin-api-key>"

# jq is useful for formatting responses
which jq || npm install -g node-jq
```

Every request below uses `Authorization: Bearer $ADMIN_TOKEN`.
All replay operations are appended to the audit log at `backend/.data/backfill-audit-log.jsonl`.

---

## 1. Find your starting ledger

Before triggering a replay you need two ledger numbers: where to start (`fromLedger`) and
where to stop (`toLedger`).

### Read the current ingestion cursor

```bash
curl -s http://localhost:3001/api/v1/admin/status \
  -H "Authorization: Bearer $ADMIN_TOKEN" | jq '.cursor'
```

Or query the SQLite database directly:

```bash
sqlite3 backend/.data/dev.db \
  "SELECT value_number FROM indexer_state WHERE key = 'ingestion_cursor';"
# → 1043200
```

### Find the bounds of stored raw events

```bash
sqlite3 backend/.data/dev.db \
  "SELECT MIN(ledger) AS min_ledger, MAX(ledger) AS max_ledger FROM raw_events;"
# → 1000000|1043200
```

### Identify a safe rollback point after a reorg

Stellar ledger closes are final once they pass the network's consensus threshold (~5 ledgers).
Roll back to at least 10 ledgers before the first suspicious event — this gives you a
confirmed-safe anchor:

```bash
# If the reorg was reported at ledger 1043100, roll back to 1043090
ROLLBACK_TARGET=1043090
```

---

## 2. Dry run preview

Always run a dry run first. It reads stored `raw_events` and reports the event count
for the requested range without mutating any state.

```bash
curl -s -X POST http://localhost:3001/api/admin/backfill \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "startLedger": 1043090,
    "endLedger":   1043200,
    "dryRun":      true
  }' | jq
```

**Expected response:**

```json
{
  "dryRun": true,
  "startLedger": 1043090,
  "endLedger": 1043200,
  "estimatedLedgers": 110,
  "estimatedEvents": 347,
  "wouldReplace": false
}
```

If `estimatedEvents` is `0` it usually means the range isn't in `raw_events` yet — you
need a backfill from the chain node before replaying. See [§ Gap backfill](#gap-backfill).

---

## 3. Reorg recovery

A reorg means events above some ledger are from an orphaned fork. The fix is:

1. Roll back the cursor to the last confirmed-good ledger.
2. Re-ingest the canonical chain from that point forward.

Both steps happen inside `rollbackAndReingest` in `src/services/ingestion.ts`. The
API exposes them as a single resume-from operation.

### Step 1 — Roll back

```bash
curl -s -X POST http://localhost:3001/api/admin/backfill \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "startLedger": 1043090,
    "endLedger":   1043200,
    "dryRun":      false,
    "forceRebuild": false,
    "idempotencyKey": "reorg-recovery-2026-06-25"
  }' | jq '.runId'
# → "rp_1719273600_abc123"
```

The `idempotencyKey` prevents creating a duplicate run if you accidentally POST twice.
Pick a descriptive value (date + reason) and keep it stable for the duration of this incident.

### Step 2 — Monitor progress

```bash
RUN_ID="rp_1719273600_abc123"

# Poll until status is "completed" or "failed"
watch -n 5 "curl -s http://localhost:3001/api/admin/backfill/$RUN_ID \
  -H 'Authorization: Bearer $ADMIN_TOKEN' | jq '{status,processedEvents,cursorLedger}'"
```

**Sample output while running:**

```json
{
  "status": "running",
  "processedEvents": 128,
  "cursorLedger": 1043134
}
```

**Sample output on completion:**

```json
{
  "status": "completed",
  "processedEvents": 347,
  "cursorLedger": 1043200
}
```

### Step 3 — Verify the cursor advanced

```bash
sqlite3 backend/.data/dev.db \
  "SELECT value_number FROM indexer_state WHERE key = 'ingestion_cursor';"
# → 1043200
```

If the cursor didn't reach `endLedger`, check the audit log:

```bash
tail -20 backend/.data/backfill-audit-log.jsonl | jq 'select(.runId == "rp_1719273600_abc123")'
```

---

## 4. Gap backfill

A gap means ledger range `[A, B]` is simply absent from `raw_events` — the indexer was down
or a network partition dropped those blocks.

**Before replaying you must fetch the raw events from the chain node.** Once they are in
`raw_events`, the replay uses the same command as reorg recovery:

```bash
# 1. Ingest the missing range into raw_events first
#    (chain-client command — adjust to your node setup)
./scripts/backfill-from-node.sh --from 1041000 --to 1041500

# 2. Verify the events landed
sqlite3 backend/.data/dev.db \
  "SELECT COUNT(*) FROM raw_events WHERE ledger BETWEEN 1041000 AND 1041500;"
# → 213

# 3. Now replay that range
curl -s -X POST http://localhost:3001/api/admin/backfill \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "startLedger": 1041000,
    "endLedger":   1041500,
    "dryRun":      false,
    "idempotencyKey": "gap-backfill-1041000-1041500"
  }' | jq '.runId'
```

The indexer's two-layer idempotency guarantees mean events already in `raw_events` with
the same `(tx_hash, event_index)` are silently skipped, so re-running this for an
overlapping range is safe.

---

## 5. Force rebuild after migration

When a derived-table schema changes (new column, renamed field, removed index), existing
rows need to be wiped and rebuilt from the stored raw events.

`forceRebuild: true` clears the derived tables before processing so you start from a blank
slate. **Do not use this on a live endpoint without first putting the backend in maintenance
mode.**

```bash
# 1. Enable maintenance mode
curl -s -X POST http://localhost:3001/api/v1/admin/maintenance \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"enabled": true, "reason": "schema migration ledger rebuild"}'

# 2. Start the force-rebuild replay from genesis
curl -s -X POST http://localhost:3001/api/admin/backfill \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "startLedger":  0,
    "endLedger":    1043200,
    "dryRun":       false,
    "forceRebuild": true,
    "batchSize":    200,
    "idempotencyKey": "schema-migration-v2-2026-06-25"
  }' | jq '{runId, status}'

# 3. Monitor (same watch command as above)

# 4. Disable maintenance mode when completed
curl -s -X POST http://localhost:3001/api/v1/admin/maintenance \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"enabled": false, "reason": ""}'
```

### Batch size guidance

| Event volume in range | Recommended `batchSize` |
|---|---|
| < 10 000 events | `500` (default is fine) |
| 10 000 – 100 000 events | `200` |
| > 100 000 events | `100` — watch memory during the run |

The hard ceiling is `REPLAY_MAX_BATCH_SIZE` (default `1000`). Requests above this are
rejected with `400 Bad Request`.

---

## 6. Resuming a paused or failed run

### Pause a running replay

```bash
curl -s -X POST http://localhost:3001/api/admin/backfill/pause \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"runId": "rp_1719273600_abc123"}'
# → {"status": "paused", "cursorLedger": 1043150}
```

### Resume from where it stopped

```bash
curl -s -X POST http://localhost:3001/api/admin/backfill/resume \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"runId": "rp_1719273600_abc123"}'
# → {"status": "running", "cursorLedger": 1043150}
```

Resume works for both `paused` and `failed` status. The run picks up from its last
committed cursor, so no events are re-processed.

### List all runs to find a stalled one

```bash
curl -s http://localhost:3001/api/admin/backfill/runs \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  | jq '.runs[] | select(.status != "completed") | {id, status, cursorLedger, createdAt}'
```

---

## 7. Verifying the result

After any replay, confirm the cursor advanced to the expected ledger and that no events
are missing:

```bash
# Cursor check
sqlite3 backend/.data/dev.db \
  "SELECT value_number FROM indexer_state WHERE key = 'ingestion_cursor';"

# Row count sanity check for the replayed range
sqlite3 backend/.data/dev.db \
  "SELECT COUNT(*) FROM raw_events WHERE ledger BETWEEN 1043090 AND 1043200;"

# Freshness via the API (indexLagSeconds should be near 0 if the cursor is current)
curl -s http://localhost:3001/api/v1/status \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  | jq '{index_lag, last_ledger}'
```

When `indexLagSeconds` is above `120` the API blocks irreversible financial actions
automatically — returning to below `30` is the completion signal for most recoveries.

---

## 8. Environment variables reference

| Variable | Default | Effect |
|---|---|---|
| `BACKFILL_MAX_LEDGER_RANGE` | `5000` | Largest single-request ledger span. Requests wider than this are rejected. |
| `BACKFILL_MAX_CONCURRENCY` | `4` | Maximum parallel ingestion workers per run. |
| `REPLAY_MAX_BATCH_SIZE` | `1000` | Hard ceiling on `batchSize` per request. |
| `REPLAY_MAX_LEDGER_RANGE` | `100000` | Absolute upper bound on the replay range. |
| `BACKFILL_AUDIT_LOG_PATH` | `backend/.data/backfill-audit-log.jsonl` | Path for the JSONL audit trail. |
| `ADMIN_API_TOKEN` | _(required)_ | Bearer token for admin endpoints. Not set → admin endpoints return `503`. |

---

## 9. Troubleshooting

### "estimatedEvents: 0" on a range I know exists

The raw events for that range are not in the database yet. Run the chain-node backfill
script first, then retry the dry run.

### Run is stuck in `running` for > 15 minutes

1. Check server logs for a repeating error at the same cursor ledger.
2. Pause the run, inspect the audit log for the last few entries, then resume once the
   underlying issue (malformed event, full disk, locked DB) is resolved.

### `400 Bad Request` — "range exceeds BACKFILL_MAX_LEDGER_RANGE"

Split the range into smaller chunks and submit them sequentially:

```bash
for START in $(seq 1000000 5000 1043200); do
  END=$((START + 4999))
  curl -s -X POST http://localhost:3001/api/admin/backfill \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"startLedger\": $START, \"endLedger\": $END, \"dryRun\": false}" \
    | jq '{runId, status}'
  sleep 2   # give the server breathing room between chunks
done
```

### State hash mismatch after rebuild

A state hash mismatch means two replay runs over the same range produced different
derived tables — a non-determinism bug. Steps:

1. Note the exact `fromLedger`, `toLedger`, and `batchSize` of both runs.
2. File an incident with those parameters plus the differing state hashes.
3. Do **not** disable maintenance mode until root cause is identified.

---

## Related docs

- [Indexer — Transaction Semantics](indexer.md) — ingestion unit-of-work, cursor behaviour, and reorg recovery internals
- [Deterministic Replay Mode](replay.md) — architecture overview and `ReplayService` API reference
- [Admin Backfill Tooling](backfill.md) — endpoint reference and guardrail configuration
- [Audit Log](audit-log.md) — format and retention of the JSONL audit trail written during every replay
- [Operations Guide](operations.md) — graceful shutdown, maintenance mode, and Kubernetes lifecycle
