# Dashboard indexer queries

Audience: **operators** diagnosing indexer health and answering routine support
questions.

These examples match the schemas in `backend/src/migrations/` and are read-only.
The materialized indexer tables use SQLite; the best-bid snapshots use PostgreSQL.
Run queries against a replica when possible, and always add a time or row limit
before adapting them for a large production database.

## Connect

The SQLite database defaults to `.data/dev.db` relative to the backend process.
`DATABASE_PATH` overrides it.

```bash
cd backend
sqlite3 "${DATABASE_PATH:-.data/dev.db}"
```

In the SQLite shell, make results readable:

```sql
.headers on
.mode column
.timeout 5000
```

The repository does not currently expose a GraphQL indexer endpoint. Use these
SQL queries or the REST endpoints documented in `backend/openapi.yaml`; do not
assume a GraphQL schema exists.

## Is ingestion current?

Show the last persisted cursor and its age. A missing row means no freshness
checkpoint has been persisted.

```sql
SELECT
  cursor,
  timestamp AS indexed_through,
  CAST((julianday('now') - julianday(timestamp)) * 86400 AS INTEGER)
    AS age_seconds
FROM freshness_state
WHERE id = 1;
```

Compare the checkpoint with the newest raw event:

```sql
SELECT
  MAX(ledger) AS newest_ledger,
  MAX(indexed_at) AS newest_event_indexed_at,
  COUNT(*) AS raw_event_count
FROM raw_events;
```

## Are events arriving?

Count events indexed during each of the last 24 hours:

```sql
WITH RECURSIVE hours(bucket) AS (
  SELECT datetime('now', '-23 hours', 'start of hour')
  UNION ALL
  SELECT datetime(bucket, '+1 hour')
  FROM hours
  WHERE bucket < datetime('now', 'start of hour')
)
SELECT
  hours.bucket,
  COUNT(raw_events.id) AS event_count
FROM hours
LEFT JOIN raw_events
  ON raw_events.indexed_at >= hours.bucket
 AND raw_events.indexed_at < datetime(hours.bucket, '+1 hour')
GROUP BY hours.bucket
ORDER BY hours.bucket;
```

Show the busiest event types over the last day:

```sql
SELECT type, COUNT(*) AS event_count
FROM raw_events
WHERE indexed_at >= datetime('now', '-24 hours')
GROUP BY type
ORDER BY event_count DESC, type
LIMIT 20;
```

Inspect the latest events without expanding the JSON payload:

```sql
SELECT ledger, event_index, type, tx_hash, indexed_at
FROM raw_events
ORDER BY ledger DESC, event_index DESC
LIMIT 50;
```

## Is replay idempotent?

This should return no rows. Any result violates the indexer's
`(tx_hash, event_index)` uniqueness invariant.

```sql
SELECT tx_hash, event_index, COUNT(*) AS copies
FROM raw_events
GROUP BY tx_hash, event_index
HAVING COUNT(*) > 1
ORDER BY copies DESC
LIMIT 50;
```

Find gaps between ledgers that contain events. Gaps are leads for investigation,
not proof of data loss: many ledgers legitimately contain no QuickLendX events.

```sql
WITH ledgers AS (
  SELECT DISTINCT ledger FROM raw_events
),
ordered AS (
  SELECT ledger, LAG(ledger) OVER (ORDER BY ledger) AS previous_ledger
  FROM ledgers
)
SELECT
  previous_ledger + 1 AS gap_starts_at,
  ledger - 1 AS gap_ends_at,
  ledger - previous_ledger - 1 AS missing_ledger_count
FROM ordered
WHERE ledger - previous_ledger > 1
ORDER BY gap_starts_at DESC
LIMIT 50;
```

## What is the invoice workload?

Current invoice counts by status:

```sql
SELECT status, COUNT(*) AS invoice_count
FROM invoices
GROUP BY status
ORDER BY invoice_count DESC, status;
```

Invoices past their due date that are not in a terminal state:

```sql
SELECT id, business, amount, currency, status,
       datetime(due_date, 'unixepoch') AS due_at
FROM invoices
WHERE due_date < unixepoch()
  AND status NOT IN ('Settled', 'Defaulted', 'Cancelled')
ORDER BY due_date
LIMIT 100;
```

Businesses with the most open invoices:

```sql
SELECT business, COUNT(*) AS open_invoice_count
FROM invoices
WHERE status NOT IN ('Settled', 'Defaulted', 'Cancelled')
GROUP BY business
ORDER BY open_invoice_count DESC, business
LIMIT 25;
```

Amounts are stored as integer strings in the smallest token unit. Keep the
currency in the grouping and apply that token's decimal scale in the dashboard.

```sql
SELECT
  currency,
  COUNT(*) AS invoice_count,
  SUM(CAST(amount AS INTEGER)) AS amount_in_smallest_units
FROM invoices
WHERE created_at >= unixepoch('now', '-30 days')
GROUP BY currency
ORDER BY invoice_count DESC, currency;
```

## Which bids need attention?

Placed bids that have expired but have not yet been projected to `Expired`:

```sql
SELECT bid_id, invoice_id, investor, bid_amount,
       datetime(expiration_timestamp, 'unixepoch') AS expired_at
FROM bids
WHERE status = 'Placed'
  AND expiration_timestamp < unixepoch()
ORDER BY expiration_timestamp
LIMIT 100;
```

Bid activity by status:

```sql
SELECT status, COUNT(*) AS bid_count
FROM bids
GROUP BY status
ORDER BY bid_count DESC, status;
```

## Which settlements need attention?

Pending or processing settlements, oldest first:

```sql
SELECT id, invoice_id, amount, payer, recipient, status, indexed_at
FROM settlements
WHERE status IN ('Pending', 'Processing')
ORDER BY timestamp
LIMIT 100;
```

Settlement outcomes indexed during the last seven days:

```sql
SELECT
  date(indexed_at) AS indexed_date,
  status,
  COUNT(*) AS settlement_count
FROM settlements
WHERE indexed_at >= datetime('now', '-7 days')
GROUP BY indexed_date, status
ORDER BY indexed_date DESC, status;
```

## Best-bid snapshots (PostgreSQL)

The snapshot service uses PostgreSQL tables defined in
`backend/src/services/schema.sql`. Connect with the deployment's PostgreSQL
connection string:

```bash
psql "$DATABASE_URL"
```

Snapshot rows that have not been refreshed in five minutes:

```sql
SELECT invoice_id, bid_id, ledger_index, last_updated
FROM best_bids
WHERE last_updated < ((EXTRACT(EPOCH FROM NOW()) * 1000)::bigint - 300000)
ORDER BY last_updated
LIMIT 100;
```

Invoices where the best-bid row and top-bids snapshot disagree on freshness:

```sql
SELECT
  best.invoice_id,
  best.last_updated AS best_bid_updated,
  top.last_updated AS top_bids_updated
FROM best_bids AS best
JOIN top_bids_snapshots AS top USING (invoice_id)
WHERE best.last_updated <> top.last_updated
ORDER BY GREATEST(best.last_updated, top.last_updated) DESC
LIMIT 100;
```

Best-bid rows without a corresponding top-bids snapshot:

```sql
SELECT best.invoice_id, best.bid_id, best.last_updated
FROM best_bids AS best
LEFT JOIN top_bids_snapshots AS top USING (invoice_id)
WHERE top.invoice_id IS NULL
ORDER BY best.last_updated
LIMIT 100;
```

## Related documentation

- [Indexer transaction semantics](../backend/docs/indexer.md)
- [Best-bid snapshot design](../backend/docs/bidding.md)
- [Read-only contract entrypoints](QUERIES.md)
- [Data freshness semantics](data-freshness-semantics.md)
