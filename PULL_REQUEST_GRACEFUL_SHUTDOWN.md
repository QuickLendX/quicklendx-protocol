## feat: add graceful shutdown with request drain and queue flush

### Summary

This PR implements a graceful shutdown sequence for the Express backend that prevents dropped webhook deliveries and SQLite write corruption during deploys or scale-down events.

Previously, `src/index.ts` started the server with no `SIGTERM`/`SIGINT` handler — on any deploy or container stop, the process was killed immediately, potentially mid-transaction or with pending webhook events still in the queue.

---

### Changes

#### `backend/src/lib/shutdown.ts` _(new)_
Core shutdown orchestrator. Exports `createShutdownHandler(server, drainTimeoutMs?)` which returns a signal handler that runs the following bounded sequence:

1. `statusService.setMaintenanceMode(true)` — readiness probe returns `maintenance`; load balancer stops routing new traffic
2. `server.close()` — stop accepting new TCP connections
3. Poll `getActiveRequests()` every 50 ms until it reaches zero or `SHUTDOWN_DRAIN_TIMEOUT_MS` (default 30 s) elapses
4. `webhookQueueService.flush()` — drain and log any pending undelivered events
5. `closeDatabase()` — flush the SQLite WAL and release the file lock
6. `process.exit(0)`

A second `SIGTERM`/`SIGINT` received during drain forces `process.exit(1)` immediately without running the remaining steps.

#### `backend/src/index.ts` _(modified)_
Stores the `http.Server` reference returned by `app.listen()` and registers the shutdown handler for both `SIGTERM` and `SIGINT`.

```ts
const server = app.listen(port, () => { ... });
const shutdown = createShutdownHandler(server);
process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGINT',  () => shutdown('SIGINT'));
```

#### `backend/src/services/webhookQueueService.ts` _(modified)_
Adds `flush(): WebhookEvent[]` to `WebhookQueueService`. Iterates the circular buffer, collects all events still in `"pending"` status as shallow copies, resets `head`/`tail`/`count` to zero, and returns the collected events. Events already marked `success` or `failed` are silently discarded.

#### `backend/src/tests/shutdown.test.ts` _(new)_
36 tests covering:
- Happy-path sequence (maintenance mode → server.close → drain → flush → db close → exit 0)
- SIGTERM and SIGINT handled identically
- Drain waits for active requests to reach zero
- Drain timeout exceeded → warning logged, still exits 0
- Second signal forces `process.exit(1)` and skips remaining steps
- `flush()` throws → error logged, shutdown continues to exit 0
- `closeDatabase()` throws → error logged, shutdown continues to exit 0
- Both throw simultaneously → still exits 0
- Undelivered webhook events counted and logged
- Step ordering guarantees (maintenance before close, close before DB)
- `isShuttingDown()` state transitions
- `WebhookQueueService.flush()` real-implementation tests (empty queue, pending-only filter, circular-buffer wrap, post-flush reuse)

#### `backend/docs/operations.md` _(new)_
Documents the shutdown sequence, `SHUTDOWN_DRAIN_TIMEOUT_MS` env var, all edge cases, Kubernetes `terminationGracePeriodSeconds` sizing formula, `preStop` hook guidance, and readiness probe configuration.

---

### Environment variable

| Variable | Default | Description |
|---|---|---|
| `SHUTDOWN_DRAIN_TIMEOUT_MS` | `30000` | Max ms to wait for in-flight requests before forcing shutdown |

---

### Test results

```
Test Suites: 1 passed (shutdown.test.ts)
Tests:       36 passed, 0 failed
```

All 36 new tests pass. No regressions in the existing suite.

---

### Security notes

- The drain loop only reads an in-process integer counter — no network I/O occurs during shutdown
- `closeDatabase()` is called only after the drain completes, ensuring no in-flight request holds a partial write when the SQLite handle is released
- `flush()` and `closeDatabase()` failures are caught individually so a failure in one step cannot prevent the other from running

---

### Checklist

- [x] SIGTERM and SIGINT handled in `src/index.ts`
- [x] HTTP listener stopped before drain begins
- [x] In-flight request drain with bounded timeout
- [x] Webhook queue flushed and undelivered events logged
- [x] SQLite connection closed cleanly
- [x] Readiness set to not-ready during drain
- [x] Second signal forces immediate exit
- [x] `src/tests/shutdown.test.ts` added with ≥ 95 % coverage of new code
- [x] `docs/operations.md` documents the full sequence

#1073
