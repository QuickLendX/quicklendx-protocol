# Event Dashboards

> **Audience**: Operators running the QuickLendX backend and indexer who need
> to build or navigate Grafana dashboards for protocol health, event
> throughput, and alert triage.

This document lists the standard operator dashboards, the panel URL patterns
used in each deployment, and the exact Prometheus metric names and PromQL
queries that back each panel. Cross-references to the SQL-level indexer
queries and raw event subscriptions are provided at the bottom.

---

## Prerequisites

| Component | Required |
|-----------|----------|
| Prometheus scraping `/v1/metrics` | Yes |
| Grafana connected to Prometheus | Yes |
| Backend API key for `/v1/metrics` Bearer auth | Yes |
| SQLite replica for indexer queries | Recommended |

See [`quicklendx-backend/docs/observability.md`](../quicklendx-backend/docs/observability.md)
for Prometheus scrape configuration and the full metric reference.

---

## Metric reference

All backend metrics use the `qlx_` prefix. The table below lists every metric
the `/v1/metrics` endpoint exposes.

| Metric | Type | What it measures |
|--------|------|-----------------|
| `qlx_ingest_lag_ledgers` | Gauge | Ledgers the indexer is behind the network tip |
| `qlx_webhook_queue_depth` | Gauge | Webhooks currently queued but not yet delivered |
| `qlx_webhook_overflow_total` | Counter | Cumulative webhook queue overflows since startup |
| `qlx_rpc_circuit_state` | Gauge | RPC circuit breaker: `0` = closed (healthy), `1` = open (tripping), `2` = half-open (recovering) |
| `qlx_invariant_violations_total` | Counter | Cumulative invariant violations detected by the backend |

Scrape the endpoint with a Bearer token:

```bash
curl -s -H "Authorization: Bearer $QLX_API_KEY" \
  http://localhost:3000/v1/metrics
```

Example output for a healthy deployment:

```
# HELP qlx_ingest_lag_ledgers Current ingest lag in ledgers
# TYPE qlx_ingest_lag_ledgers gauge
qlx_ingest_lag_ledgers 2

# HELP qlx_webhook_queue_depth Current webhook queue depth
# TYPE qlx_webhook_queue_depth gauge
qlx_webhook_queue_depth 7

# HELP qlx_webhook_overflow_total Total webhook queue overflows
# TYPE qlx_webhook_overflow_total counter
qlx_webhook_overflow_total 0

# HELP qlx_rpc_circuit_state RPC circuit breaker state (0=closed, 1=open, 2=half-open)
# TYPE qlx_rpc_circuit_state gauge
qlx_rpc_circuit_state 0

# HELP qlx_invariant_violations_total Total invariant violations detected
# TYPE qlx_invariant_violations_total counter
qlx_invariant_violations_total 0
```

---

## Standard dashboards

### 1. Protocol Health Overview

**Panel URL pattern**

```
https://<grafana-host>/d/qlx-health/protocol-health-overview
```

This is the first dashboard operators open when they receive an alert.
It shows the current state of every critical metric on one screen.

| Panel title | PromQL | Visualization |
|-------------|--------|---------------|
| Ingest lag (ledgers) | `qlx_ingest_lag_ledgers` | Stat with colour threshold: green < 20, yellow 20–100, red > 100 |
| Webhook queue depth | `qlx_webhook_queue_depth` | Stat with colour threshold: green < 100, yellow 100–1000, red > 1000 |
| Webhook overflow rate | `rate(qlx_webhook_overflow_total[5m])` | Time-series |
| RPC circuit breaker | `qlx_rpc_circuit_state` | State timeline: 0 = green (Closed), 1 = red (Open), 2 = yellow (Half-Open) |
| Invariant violations (5 min) | `increase(qlx_invariant_violations_total[5m])` | Stat; any non-zero value turns red |

**Suggested alert thresholds** (copy into Grafana alert rules or Prometheus
`rules.yml`):

```yaml
groups:
  - name: qlx-health
    rules:
      - alert: IngestLagHigh
        expr: qlx_ingest_lag_ledgers > 100
        for: 5m
        annotations:
          summary: "Indexer is {{ $value }} ledgers behind the tip"
          runbook: "https://github.com/QuickLendX/quicklendx-protocol/blob/main/docs/RUNBOOK_INCIDENT_RESPONSE.md"

      - alert: WebhookQueueBackpressure
        expr: qlx_webhook_queue_depth > 1000
        for: 2m
        annotations:
          summary: "Webhook queue depth is {{ $value }}"

      - alert: RPCCircuitOpen
        expr: qlx_rpc_circuit_state == 1
        for: 1m
        annotations:
          summary: "RPC circuit breaker is open — backend cannot reach the Soroban node"

      - alert: InvariantViolationDetected
        expr: increase(qlx_invariant_violations_total[5m]) > 0
        for: 1m
        annotations:
          summary: "{{ $value }} invariant violation(s) in the past 5 min — page on-call"
```

---

### 2. Event Throughput

**Panel URL pattern**

```
https://<grafana-host>/d/qlx-events/event-throughput
```

Tracks how many contract events the indexer is ingesting per minute,
broken down by event type.

These panels use the indexer's SQLite database, not Prometheus. Operators
running Grafana with an SQLite data-source plugin (e.g. `frser-sqlite-datasource`)
can paste the queries directly. Without the plugin, run them via the CLI
and build the panels from a CSV/JSON data source refresh.

**Connect to the indexer database**

```bash
cd backend
sqlite3 "${DATABASE_PATH:-.data/dev.db}"
```

```sql
.headers on
.mode column
.timeout 5000
```

**Panel: events per minute (last hour)**

```sql
SELECT
  strftime('%Y-%m-%dT%H:%M:00', indexed_at) AS minute,
  COUNT(*) AS event_count
FROM raw_events
WHERE indexed_at >= datetime('now', '-1 hour')
GROUP BY minute
ORDER BY minute;
```

**Panel: top event types (last 24 h)**

```sql
SELECT type, COUNT(*) AS event_count
FROM raw_events
WHERE indexed_at >= datetime('now', '-24 hours')
GROUP BY type
ORDER BY event_count DESC
LIMIT 20;
```

**Panel: newest indexed ledger and freshness checkpoint**

```sql
SELECT
  (SELECT MAX(ledger) FROM raw_events)         AS newest_indexed_ledger,
  (SELECT cursor FROM freshness_state WHERE id = 1) AS freshness_cursor,
  (SELECT CAST(
      (julianday('now') - julianday(timestamp)) * 86400
      AS INTEGER)
   FROM freshness_state WHERE id = 1)          AS checkpoint_age_seconds;
```

A `checkpoint_age_seconds` above 120 (2 minutes) warrants investigation.
An absent row in `freshness_state` means no checkpoint has been written
since startup.

---

### 3. Dispute and Default Monitor

**Panel URL pattern**

```
https://<grafana-host>/d/qlx-risk/dispute-and-default-monitor
```

Disputes and defaults are high-severity events. Each one requires manual
triage (see [MONITORING.md](MONITORING.md) and
[RUNBOOK_INCIDENT_RESPONSE.md](RUNBOOK_INCIDENT_RESPONSE.md)).

**Panel: open disputes (count)**

```sql
SELECT COUNT(*) AS open_disputes
FROM raw_events
WHERE type = 'DisputeCreated'
  AND id NOT IN (
    SELECT DISTINCT json_extract(payload, '$.invoice_id')
    FROM raw_events
    WHERE type = 'DisputeResolved'
  );
```

> Note: replace `json_extract` column references with the actual column
> names in your deployment's `raw_events` schema. The field names above
> match the `DisputeCreated` event payload documented in
> [`EVENTS_SCHEMA.md`](EVENTS_SCHEMA.md).

**Panel: disputes created (last 7 days, by day)**

```sql
SELECT
  date(indexed_at) AS day,
  COUNT(*) AS disputes_created
FROM raw_events
WHERE type = 'DisputeCreated'
  AND indexed_at >= datetime('now', '-7 days')
GROUP BY day
ORDER BY day;
```

**Panel: defaults created (last 30 days)**

```sql
SELECT
  date(indexed_at) AS day,
  COUNT(*) AS defaults
FROM raw_events
WHERE type = 'InvoiceDefaulted'
  AND indexed_at >= datetime('now', '-30 days')
GROUP BY day
ORDER BY day;
```

**Panel: escrow refund spike detection**

Isolated refunds are normal. More than 5 in one hour warrants investigation
([MONITORING.md](MONITORING.md) threshold).

```sql
SELECT
  strftime('%Y-%m-%dT%H:00:00', indexed_at) AS hour,
  COUNT(*) AS refund_count
FROM raw_events
WHERE type = 'EscrowRefunded'
  AND indexed_at >= datetime('now', '-24 hours')
GROUP BY hour
ORDER BY hour;
```

---

### 4. Settlement Pipeline

**Panel URL pattern**

```
https://<grafana-host>/d/qlx-settlement/settlement-pipeline
```

Shows settlements flowing through the indexer, aged pending settlements,
and bid-state counts.

**Panel: settlement status breakdown**

```sql
SELECT status, COUNT(*) AS count
FROM settlements
GROUP BY status
ORDER BY count DESC;
```

**Panel: pending settlements older than 10 minutes**

```sql
SELECT id, invoice_id, amount, payer, recipient, indexed_at
FROM settlements
WHERE status IN ('Pending', 'Processing')
  AND indexed_at < datetime('now', '-10 minutes')
ORDER BY indexed_at
LIMIT 50;
```

**Panel: bid status breakdown**

```sql
SELECT status, COUNT(*) AS bid_count
FROM bids
GROUP BY status
ORDER BY bid_count DESC;
```

**Panel: stale bids (placed but past expiry)**

```sql
SELECT bid_id, invoice_id, investor,
       datetime(expiration_timestamp, 'unixepoch') AS expired_at
FROM bids
WHERE status = 'Placed'
  AND expiration_timestamp < unixepoch()
ORDER BY expiration_timestamp
LIMIT 50;
```

---

## Alert routing summary

The table below maps each alert to its first-response owner and the
relevant runbook section.

| Alert | Owner | First action |
|-------|-------|-------------|
| `IngestLagHigh` | Indexer on-call | Check `freshness_state`, review indexer logs for RPC errors |
| `WebhookQueueBackpressure` | Backend on-call | Check consumer lag; scale webhook workers if needed |
| `RPCCircuitOpen` | Backend on-call | Verify Soroban node reachability; check `qlx_rpc_circuit_state` trend |
| `InvariantViolationDetected` | Security on-call | Page immediately; follow [RUNBOOK_INCIDENT_RESPONSE.md §3](RUNBOOK_INCIDENT_RESPONSE.md) |
| `DisputeCreated` event | Support team | Open dispute ticket, freeze funds if needed |
| `InvoiceDefaulted` event | Support team | Notify investors, start off-chain recovery |

---

## Related documentation

| Document | What it covers |
|----------|---------------|
| [`MONITORING.md`](MONITORING.md) | Per-event alert thresholds for contract events |
| [`DASHBOARD_QUERIES.md`](DASHBOARD_QUERIES.md) | Full SQL reference for indexer health and workload queries |
| [`EVENTS_SCHEMA.md`](EVENTS_SCHEMA.md) | Every contract event topic, payload fields, and subscription examples |
| [`ON_CHAIN_LOGS.md`](ON_CHAIN_LOGS.md) | How to subscribe to on-chain events via Soroban RPC |
| [`RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) | Step-by-step operator playbook for incidents |
| [`quicklendx-backend/docs/observability.md`](../quicklendx-backend/docs/observability.md) | Full Prometheus scrape config, metric definitions, and Grafana data-source setup |
