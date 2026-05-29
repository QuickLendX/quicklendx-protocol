# Operations Guide

## Graceful Shutdown

### Overview

The backend handles `SIGTERM` and `SIGINT` with a bounded, ordered shutdown
sequence that prevents dropped webhook deliveries and SQLite write corruption
during deploys or scale-down events.

### Shutdown sequence

```
Signal received (SIGTERM or SIGINT)
        │
        ▼
1. Set readiness → not-ready
   statusService.setMaintenanceMode(true)
   Load balancers / ingress controllers stop sending new traffic.
        │
        ▼
2. Stop HTTP listener
   server.close()
   Already-open connections finish; no new TCP connections accepted.
        │
        ▼
3. Drain in-flight requests  (bounded by SHUTDOWN_DRAIN_TIMEOUT_MS)
   Polls getActiveRequests() every 50 ms.
   Exits the drain loop when the counter reaches 0 or the timeout elapses.
        │
        ▼
4. Flush webhook queue
   webhookQueueService.flush()
   Returns all events still in "pending" status and logs a warning for each
   one that was not delivered.  The queue is cleared.
        │
        ▼
5. Close SQLite
   closeDatabase()
   Triggers a WAL checkpoint and releases the file lock.
   Prevents half-written pages if the OS kills the process after this point.
        │
        ▼
6. process.exit(0)
```

### Configuration

| Variable                    | Default | Description                                                  |
|-----------------------------|---------|--------------------------------------------------------------|
| `SHUTDOWN_DRAIN_TIMEOUT_MS` | `30000` | Max milliseconds to wait for in-flight requests to complete. |

Set this in your deployment environment (e.g. Kubernetes `env:` stanza):

```yaml
env:
  - name: SHUTDOWN_DRAIN_TIMEOUT_MS
    value: "30000"
```

### Edge cases

#### Second signal during drain

If a second `SIGTERM` or `SIGINT` arrives while the first shutdown is still
draining, the process exits immediately with code `1`.  This indicates a
forced/abnormal exit and allows the orchestrator to distinguish it from a
clean shutdown.

#### Drain timeout exceeded

When active requests do not reach zero before `SHUTDOWN_DRAIN_TIMEOUT_MS`,
the shutdown continues through steps 4-6 with a warning log:

```
[shutdown] Drain timeout (30000ms) exceeded — N request(s) still in-flight
```

In-flight requests are abandoned; any partial writes they hold must be handled
at the application layer (transactions, idempotency keys).

#### Webhook queue flush failure

If `flush()` throws (e.g. internal buffer corruption), the error is caught and
logged.  Shutdown continues and `process.exit(0)` is still reached so the
process does not hang.

#### Database close failure

If `closeDatabase()` throws, the error is logged and shutdown completes with
`process.exit(0)`.  The OS will release the file lock regardless.

### Kubernetes termination lifecycle

Pair this with a `preStop` hook so the pod has time to drain before
`SIGTERM` arrives:

```yaml
lifecycle:
  preStop:
    exec:
      command: ["sleep", "5"]
terminationGracePeriodSeconds: 40   # > SHUTDOWN_DRAIN_TIMEOUT_MS / 1000 + preStop
```

Recommended formula:
```
terminationGracePeriodSeconds = (SHUTDOWN_DRAIN_TIMEOUT_MS / 1000) + preStop_seconds + 5
```

### Healthcheck / readiness probe

The `/api/v1/status` endpoint returns `status: "maintenance"` as soon as step 1
completes.  Configure your readiness probe to treat `maintenance` as not-ready:

```yaml
readinessProbe:
  httpGet:
    path: /api/v1/status
    port: 3001
  # Remove the pod from Service endpoints when the response contains
  # "maintenance" or when the probe times out during shutdown.
```

### Verifying graceful shutdown locally

```bash
# Start the server
npm run dev &
SERVER_PID=$!

# Send SIGTERM and observe the logs
kill -TERM $SERVER_PID

# Expected log output:
# [shutdown] SIGTERM — starting graceful shutdown
# [shutdown] Shutdown complete
```
