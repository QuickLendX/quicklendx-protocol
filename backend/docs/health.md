# Health, Liveness, and Readiness Probes

The backend exposes two distinct kinds of health signal. They answer different
questions and orchestrators (Kubernetes, ECS, Nomad, …) act on them
differently. Conflating them — as a single flat `/health` that always returns
`ok` — causes traffic to be routed to instances that are up but unable to serve.

All probes are mounted at the **root** of the app (not under `/api/v1`) and are
**unauthenticated**, because orchestrators probe them without credentials.

| Endpoint   | Kind      | Cost  | Checks dependencies | Healthy | Unhealthy |
| ---------- | --------- | ----- | ------------------- | ------- | --------- |
| `/health`  | Liveness  | cheap | no                  | `200`   | —         |
| `/livez`   | Liveness  | cheap | no                  | `200`   | —         |
| `/readyz`  | Readiness | real  | yes                 | `200`   | `503`     |

## Liveness — `/health`, `/livez`

> "Is the process up and able to serve an HTTP request at all?"

Liveness is cheap and **dependency-free**. It returns `200` whenever the event
loop can service a request. A failing liveness probe instructs the orchestrator
to **restart** the container, so it must never consult downstream dependencies —
a transient database blip should not trigger a restart loop.

`/health` is retained for backward compatibility; `/livez` is the conventional
alias. They are identical.

```json
{ "status": "ok", "timestamp": "2026-06-02T12:00:00.000Z" }
```

## Readiness — `/readyz`

> "Should this instance receive traffic right now?"

Readiness probes real dependencies. A failing readiness probe pulls the instance
out of the load-balancer rotation **without restarting it**, so it can recover
and rejoin once its dependencies are healthy again.

It returns:

- `200` with `status: "ready"` when the instance can serve traffic.
- `503` with `status: "not_ready"` when a hard dependency is unavailable.
- `503` with `status: "maintenance"` when maintenance mode is enabled.

```json
{
  "status": "ready",
  "database": "ok",
  "ingest": "ok",
  "webhookQueue": "ok",
  "timestamp": "2026-06-02T12:00:00.000Z"
}
```

### Sub-status semantics

Each dependency reports a coarse `SubStatus`, the same pattern used by
`/api/v1/admin/monitoring`:

- `ok` — healthy.
- `degraded` — serving but impaired. **Does not** fail readiness.
- `unavailable` — could not be reached / unusable. **Fails** readiness (`503`).

| Sub-status     | Probe                          | `degraded` when                            | `unavailable` when                          |
| -------------- | ------------------------------ | ------------------------------------------ | ------------------------------------------- |
| `database`     | `pingDatabase()` (`SELECT 1`)  | —                                          | connection cannot open or execute           |
| `ingest`       | `lagMonitor.getLagStatus()`    | lag ≥ warn threshold, < critical threshold | lag ≥ critical threshold, or probe throws   |
| `webhookQueue` | `webhookQueueService.getStats()` | queue is saturated (`size ≥ capacity`)   | the queue's backing store is unreachable    |

Ingest lag thresholds are governed by `LagMonitor` and configurable via
`LAG_WARN_THRESHOLD` / `LAG_CRITICAL_THRESHOLD`. See [reliability.md](./reliability.md).

The instance is **not ready** (`503`) if *any* sub-status is `unavailable`.
`degraded` sub-statuses are surfaced for observability but keep the instance in
rotation: a slightly stale index or a back-pressured queue is still serviceable.

### Maintenance mode

When `statusService.isMaintenanceEnabled()` is true, `/readyz` short-circuits
**before** probing any dependency and returns `503` with `status: "maintenance"`.
The instance is intentionally not serving and should be pulled from rotation.
Liveness is unaffected — the process is healthy, just drained.

## Edge-case behaviour

| Scenario                     | `/health`, `/livez` | `/readyz`                                       |
| ---------------------------- | ------------------- | ----------------------------------------------- |
| All healthy                  | `200 ok`            | `200 ready`                                     |
| Database down                | `200 ok`            | `503 not_ready`, `database: unavailable`        |
| Warn-level lag               | `200 ok`            | `200 ready`, `ingest: degraded`                 |
| Critical lag                 | `200 ok`            | `503 not_ready`, `ingest: unavailable`          |
| Lag probe throws             | `200 ok`            | `503 not_ready`, `ingest: unavailable`          |
| Queue store unreachable      | `200 ok`            | `503 not_ready`, `webhookQueue: unavailable`    |
| Queue saturated              | `200 ok`            | `200 ready`, `webhookQueue: degraded`           |
| Maintenance mode             | `200 ok`            | `503 maintenance`                               |
| Partial failure (one dep)    | `200 ok`            | `503 not_ready` (failing dep `unavailable`, rest `ok`) |

## Security

These probes are unauthenticated, so their responses are deliberately minimal.
They expose only the coarse status enums above and a timestamp. They do **not**
leak:

- internal hostnames or connection strings,
- application or dependency versions,
- absolute ledger numbers or ingest-lag values,
- queue depths or capacities,
- underlying exception messages (dependency errors are caught and collapsed to
  `unavailable`).

Richer, sensitive diagnostics (queue depths, invariant counters, cursor
positions, versions) remain behind API-key auth at
[`/api/v1/admin/monitoring`](./admin-monitoring.md).

## Orchestrator configuration (Kubernetes example)

```yaml
livenessProbe:
  httpGet:
    path: /livez
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 10
readinessProbe:
  httpGet:
    path: /readyz
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 5
```
