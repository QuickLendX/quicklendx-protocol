# Backend Incident Response Runbooks

This document defines operator procedures for three common backend incidents:

1. Indexer lag or stuck cursor
2. Webhook delivery failure storm
3. Database saturation and backpressure

These runbooks are written to be rollback-safe and avoid unsafe manual database edits.

## Scope and Safety Rules

- Never run direct `UPDATE`/`DELETE` statements against production event or ledger tables during incident response.
- Never skip signature verification or auth checks for webhook processing as a mitigation.
- Prefer reversible configuration changes (feature flags, rate limits, worker concurrency) over destructive actions.
- Capture timestamps, actor, and command output for every action in the incident channel/ticket.

## Shared Triage Checklist

1. Confirm blast radius:
   - Which environments are impacted (`prod`, `staging`)?
   - Which components are degraded (indexer, webhook worker, API latency, DB)?
2. Confirm current deploy:
   - `git rev-parse --short HEAD`
   - `kubectl -n <ns> get deploy -o wide`
3. Confirm health and saturation indicators:
   - Error rate, p95 latency, queue depth, DB CPU, DB connections, deadlocks, webhook retry rate, indexer lag.
4. Freeze risky operations:
   - Pause nonessential backfills/migrations/maintenance jobs until incident stabilizes.
5. Assign an incident commander and scribe.

---

## Runbook 1: Indexer Lag / Stuck Cursor

### Symptoms

- Indexer lag continuously increases.
- Cursor/last processed block does not advance for >= 5 minutes.
- API data appears stale while chain head moves.

### Detection Signals

- `indexer_head_lag_blocks` above threshold.
- `indexer_last_processed_timestamp` stale.
- Repeated parse/write errors in indexer logs.

### Step-by-Step Mitigation

1. Validate indexer process health.

```bash
kubectl -n <ns> get pods -l app=indexer
kubectl -n <ns> logs deploy/indexer --since=10m | tail -n 100
```

Expected output:

- Pods are `Running` and `Ready`.
- Logs show either forward progress or repeated deterministic failure at same block/event.

2. Confirm chain/source connectivity.

```bash
kubectl -n <ns> exec deploy/indexer -- sh -c 'curl -sS <rpc_health_endpoint>'
```

Expected output:

- Healthy RPC response with current chain head.

3. If stuck on deterministic bad event, isolate and skip only through approved replay controls.

- Use existing replay/skip tooling or config (`INDEXER_REPLAY_FROM`, `INDEXER_SKIP_EVENT_IDS`) if supported.
- Record exact skipped identifier and approval in incident log.
- Do not modify database cursor rows manually.

4. Perform controlled restart.

```bash
kubectl -n <ns> rollout restart deploy/indexer
kubectl -n <ns> rollout status deploy/indexer
```

Expected output:

- Successful rollout, indexer resumes advancing cursor.

5. If lag is due to throughput bottleneck, apply reversible scaling.

```bash
kubectl -n <ns> scale deploy/indexer --replicas=<N>
```

Expected output:

- Pod count increases; lag trend starts decreasing.

### Rollback-Safe Actions

- Revert replica count to baseline after stabilization.
- Revert temporary replay/skip flags after backlog drains.
- Keep a record of any skipped event IDs and schedule reconciliation job.

### Recovery Validation

- `indexer_head_lag_blocks` returns to normal SLO.
- Cursor advances continuously for at least 15 minutes.
- Reconciliation confirms no missing finalized records.

---

## Runbook 2: Webhook Delivery Failure Storm

### Symptoms

- Sudden spike in webhook failures/timeouts.
- Retry queue depth grows rapidly.
- Upstream/downstream partner endpoints return 4xx/5xx at high rate.

### Detection Signals

- `webhook_delivery_failures_total` spikes.
- `webhook_retry_queue_depth` exceeds threshold.
- `webhook_delivery_success_rate` drops below SLO.

### Step-by-Step Mitigation

1. Classify failure mode by status code.

```bash
kubectl -n <ns> logs deploy/webhook-worker --since=10m \
  | rg "status=" \
  | tail -n 200
```

Expected output:

- Mostly `429/5xx`: receiver saturation/outage.
- Mostly `401/403`: auth/signature/config issue.
- Mostly network timeout: connectivity/DNS/TLS issue.

2. Reduce pressure safely.

- Decrease worker concurrency and/or outbound QPS limits via config/feature flag.
- Pause noncritical webhook topics if topic-level controls exist.

Example:

```bash
kubectl -n <ns> set env deploy/webhook-worker WEBHOOK_MAX_CONCURRENCY=10
kubectl -n <ns> set env deploy/webhook-worker WEBHOOK_RATE_LIMIT_RPS=50
kubectl -n <ns> rollout restart deploy/webhook-worker
```

Expected output:

- Retry growth slows; DB/API pressure reduces.

3. Protect delivery guarantees.

- Keep durable retry queue enabled.
- Preserve idempotency keys for retries.
- Do not drop queued events unless explicit business approval is documented.

4. Fix root cause path:

- For `401/403`: rotate secrets using secret manager and redeploy.
- For `429/5xx`: coordinate with receiver and apply backoff/jitter.
- For timeout/TLS: verify DNS, cert validity, egress controls.

5. Drain backlog gradually.

- Increase concurrency in steps once success rate recovers.
- Watch duplicate delivery and consumer-side idempotency metrics.

### Rollback-Safe Actions

- Roll back temporary low-concurrency values in controlled increments.
- Revert only configuration changes made during incident (tracked in ticket).
- Keep failed payload samples sanitized and stored for postmortem.

### Recovery Validation

- Success rate back within SLO for 30 minutes.
- Retry queue trends toward zero.
- No abnormal duplicate-processing alerts.

---

## Runbook 3: Database Saturation and Backpressure

### Symptoms

- DB CPU or active connections pinned near limits.
- Query latency and lock wait time spike.
- API/indexer/webhook all degrade simultaneously.

### Detection Signals

- `db_cpu_percent`, `db_connections_active`, `db_lock_wait_ms` high.
- `api_p95_latency_ms` and timeout rate rising.
- Connection pool exhaustion in backend logs.

### Step-by-Step Mitigation

1. Confirm top resource consumers.

```bash
# Example for Postgres-like systems
psql "$DATABASE_URL" -c "select now();"
psql "$DATABASE_URL" -c "select pid, state, wait_event_type, query from pg_stat_activity order by query_start asc limit 20;"
```

Expected output:

- Long-running queries and blocked sessions identified.

2. Apply immediate load shedding (reversible).

- Reduce webhook worker concurrency.
- Slow/stop nonessential indexer backfills.
- Enable API rate limits for heavy endpoints.

3. Stabilize connection usage.

- Lower application pool max temporarily if DB is thrashing.
- Ensure queue consumers use bounded concurrency.

4. Resolve specific blockers.

- Cancel only clearly harmful long-running analytical queries.
- Do not terminate migration or transaction-owner sessions blindly.

Example:

```bash
psql "$DATABASE_URL" -c "select pg_cancel_backend(<pid>);"
```

Expected output:

- Query cancellation succeeds, lock pressure drops.

5. If saturation persists, fail over or scale via approved platform path.

- Vertical/horizontal DB scaling via infrastructure runbook.
- Read-replica offloading where supported.

### Rollback-Safe Actions

- Restore normal worker concurrency in increments.
- Re-enable paused background jobs one at a time.
- Keep emergency rate limits until DB metrics remain stable for at least 30 minutes.

### Recovery Validation

- CPU/connections/lock waits return under alert thresholds.
- API and worker latency/error rates normalize.
- No data-loss indicators from queue lag or missed index reconciliation.

---

## Post-Incident Checklist

1. Create postmortem with timeline, root cause, and corrective actions.
2. Link command history and metric screenshots.
3. File follow-up tasks for missing metrics/alerts and automation gaps.
4. Add regression tests for failure-handling logic if code changes were needed.

## Gaps to Track (if observed)

If any of the following are missing in runtime telemetry, create follow-up issues:

- Indexer cursor freshness metric and lag-by-block gauge.
- Webhook queue depth, success/failure by status class, retry age histogram.
- DB pool saturation metrics from each backend service.

