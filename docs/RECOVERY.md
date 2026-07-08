# Recovery Runbook

> Audience: operators recovering QuickLendX off-chain state after an indexer,
> event consumer, or backend replay process lost a ledger range.

This runbook covers recovery when the contract state is still authoritative but
an off-chain consumer missed Soroban events or derived indexes. It is not an
emergency-withdraw procedure and it does not authorize changing protocol state
unless the checks below show on-chain secondary indexes also need repair.

Use this document with:

- [On-Chain Logs](ON_CHAIN_LOGS.md) for Soroban event surfaces and cursors.
- [Events Schema](events.md) for backend event ingestion payloads.
- [Backfill Job](backfill.md) for resumable backend drift repair.
- [Incident Response Runbook](RUNBOOK_INCIDENT_RESPONSE.md) if value-moving
  behavior or authorization is suspected to be wrong.
- [Storage Layout](STORAGE_LAYOUT.md) for primary records vs secondary indexes.

## Recovery Decision Table

| Finding | First recovery path |
| --- | --- |
| RPC cursor, event consumer, or backend queue missed ledgers, but read-only contract queries agree with expected state | Off-chain event replay and backend backfill. Do not pause the contract. |
| UI/API data is stale and `get_health_status` reports index lag or degraded freshness | Off-chain replay, then freshness verification. Keep contract writes live unless a separate protocol fault exists. |
| Contract primary invoice records are correct but secondary invoice indexes drifted after restore, migration, or a known old bug | Admin-paginated `rebuild_invoice_indexes`. |
| Held escrow reserve entries are missing for a token and emergency-withdraw reserve checks are incomplete | Admin-paginated `repair_held_escrow_reserve` for that token. |
| Funds can move incorrectly, settlement is wrong, or admin authorization appears bypassed | Stop this runbook and follow `RUNBOOK_INCIDENT_RESPONSE.md`. |

## Before You Replay

Create a recovery record before changing any state. Capture:

- network name and contract ID,
- missing ledger range, transaction hashes, or last known good cursor,
- indexer service, queue, API route, or database table affected,
- current backend health and `get_health_status` snapshot,
- whether the issue affects reads only or any value-moving write path,
- operator name and timestamp.

Keep private keys, seed phrases, customer secrets, and PII out of the recovery
record. Store redacted request IDs instead of full customer payloads.

## Step 1: Classify the Gap

1. Identify the last processed cursor or ledger in the failed consumer.
2. Identify the first ledger where downstream state is missing or inconsistent.
3. Check whether the missing range is still within the Soroban RPC event
   retention window.
4. Compare a read-only contract query against the off-chain record for at least
   one affected invoice, bid, escrow, or dispute.

Use contract state as the source of truth. Events are the preferred replay input
for off-chain indexes, but if events are unavailable the canonical contract
records and admin repair entrypoints define the recovery target.

## Step 2: Choose the Event Source

Use the lowest-cost source that covers the lost range.

| Source | Use when | Notes |
| --- | --- | --- |
| Soroban RPC `getEvents` | The missing range is still inside RPC retention | Filter by QuickLendX contract ID and topic when possible. Persist the returned cursor after each page. |
| Transaction metadata from an archival source | The gap is older than RPC retention or needs cross-contract token-transfer correlation | Parse all `ContractEvent` entries from transaction meta, then filter by contract ID and topic. |
| Backend drift backfill | Reconciliation identified concrete drift items | Use the bounded, resumable flow in `docs/backfill.md`; do not restart from the beginning after interruption. |

For event shape, topics, and decoding details, use [ON_CHAIN_LOGS.md](ON_CHAIN_LOGS.md)
and [events.md](events.md).

## Step 3: Replay Events Idempotently

Replay in ascending ledger order.

1. Start from the last confirmed good ledger plus one.
2. Fetch one bounded page of events.
3. Decode and validate each event against the backend event schema.
4. Write derived rows inside one database transaction per batch.
5. Persist the replay cursor only after the batch commit succeeds.
6. Re-run duplicate-safe batches if the process crashes before the cursor moves.

Backend event ingestion treats stable event IDs as idempotency keys. Duplicate
events should return `duplicate` or be ignored by the derived writer; they must
not send duplicate notifications or double-count accounting totals.

Example Soroban RPC filter shape:

```json
{
  "method": "getEvents",
  "params": {
    "startLedger": 1234567,
    "filters": [
      {
        "type": "contract",
        "contractIds": ["C...QUICKLENDX_CONTRACT..."]
      }
    ],
    "limit": 100
  }
}
```

## Step 4: Run Backend Drift Backfill When Needed

If reconciliation produced drift items, use the resumable backend backfill
instead of hand-editing database rows.

```bash
curl -X POST http://localhost:3000/api/v1/reconciliation/run \
  -H "Authorization: Bearer $QLX_OPERATIONS_TOKEN"

curl -X POST http://localhost:3000/api/v1/reconciliation/backfill \
  -H "Authorization: Bearer $QLX_OPERATIONS_TOKEN"

curl http://localhost:3000/api/v1/admin/monitoring/backfill-progress \
  -H "x-api-key: $API_KEY"
```

Continue bounded backfill calls until the progress row reports
`status = completed` and `remaining_count = 0`. A completed run is an
idempotent no-op, so it is safe to verify by calling progress again.

## Step 5: Repair On-Chain Secondary Indexes Only If Proven

Most lost-ledger events require only off-chain replay. Use admin repair
entrypoints only when read-only contract queries prove on-chain secondary
indexes are inconsistent with canonical primary records.

### Rebuild Invoice Indexes

Use `rebuild_invoice_indexes(admin, offset, limit)` when invoice primary
records exist but customer, tax ID, tag, category, or related invoice indexes
drifted after backup restore, partial migration, or a past indexing bug.

Run it in pages:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- rebuild_invoice_indexes \
  --admin <ADMIN_ADDRESS> \
  --offset 0 \
  --limit 100
```

Pass the returned `next_offset` into the next call until it stops advancing.
The operation is designed to be idempotent: repeated full passes should leave
indexes in the same final state.

### Repair Held Escrow Reserve

Use `repair_held_escrow_reserve(admin, currency, offset, limit)` only when a
specific token's held escrow reserve is missing or incomplete.

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network <NETWORK> \
  -- repair_held_escrow_reserve \
  --admin <ADMIN_ADDRESS> \
  --currency <TOKEN_CONTRACT_ID> \
  --offset 0 \
  --limit 100
```

Continue with each returned `next_offset` while it is non-zero. Same-token
escrow create, release, and refund operations may be rejected during a multi-page
repair; schedule the repair during an approved maintenance window when possible.

## Step 6: Verify Completion

Recovery is complete only when all relevant checks agree.

| Check | Evidence |
| --- | --- |
| Event replay consumed the lost range | Stored cursor is at or beyond the target ledger; replay logs show no rejected events. |
| Backend backfill completed | `backfill-progress.status == completed` and `remaining_count == 0`. |
| Freshness recovered | `get_health_status` and backend health show acceptable index lag. |
| Contract query parity holds | Affected invoices, bids, escrows, disputes, and payment histories match canonical contract reads. |
| Index repair completed, if used | Last repair page returned no further offset and sampled queries match primary records. |

For invoice lifecycle checks, compare:

- `get_invoice(invoice_id)`,
- `get_invoices_by_status(status)`,
- `get_business_invoices_paged(...)`,
- dispute getters when `dispute_status != None`,
- audit getters such as `get_invoice_audit_trail(invoice_id)` when an audit
  chain exists for the invoice.

## Do Not Use These As Generic Recovery

- `prune_terminal_invoices`: irreversible retention cleanup, not event replay.
- Emergency withdrawal: last-resort stuck-funds procedure, not index recovery.
- Direct database edits: bypasses idempotency, audit rows, and replay cursors.
- Manual event insertion without stable event IDs: risks duplicate notifications
  and incorrect analytics.

## Closeout

Add the following to the recovery record:

- final cursor or ledger,
- commands run and their outputs,
- number of events replayed and rejected,
- backfill run ID and final progress row,
- any admin repair entrypoints invoked,
- sampled contract reads used for parity checks,
- follow-up PRs or issues for missing monitoring or documentation.
