# Backend Reliability — Load Shedding

## Overview

The load-shedding middleware (`src/middleware/load-shedding.ts`) protects the
backend from traffic spikes by enforcing two independent limits:

1. **Concurrency cap** — hard ceiling on simultaneous in-flight requests.
2. **Per-request timeout** — maximum wall-clock time any single request may
   occupy a slot.

Both limits respond with **HTTP 503 Service Unavailable** and a
`Retry-After` header so clients can back off and retry automatically.

---

## Configuration

| Constant | Test value | Production value | Description |
|:---|---:|---:|:---|
| `CONCURRENCY_CAP` | `5` | `100` | Max simultaneous in-flight requests |
| `REQUEST_TIMEOUT_MS` | `200` | `10 000` | Per-request timeout in milliseconds |
| `RETRY_AFTER_SECONDS` | `5` | `5` | Value of the `Retry-After` response header |

Values are selected via `process.env.NODE_ENV === "test"` so tests run fast
without changing production behaviour.

---

## Middleware position

```
Request
  │
  ▼ helmet / cors / express.json
  ▼ rateLimitMiddleware        ← rejects abusive IPs before they consume a slot
  ▼ loadSheddingMiddleware     ← concurrency cap + timeout
  ▼ route handlers
```

Rate limiting runs first so that a single abusive IP cannot exhaust the
concurrency cap for legitimate users.

---

## Behaviour

### Concurrency cap

```
if activeRequests >= CONCURRENCY_CAP:
    respond 503 + Retry-After
    return          ← counter NOT incremented
else:
    activeRequests++
    proceed to handler
```

The counter is decremented on the `finish` event (normal completion) **and**
the `close` event (client disconnect / abort).  A boolean guard ensures the
decrement runs exactly once regardless of which event fires first.

### Per-request timeout

A `setTimeout` is armed when a request is admitted.  If the handler has not
sent a response within `REQUEST_TIMEOUT_MS`:

1. `shed(res, "TIMEOUT")` sends 503 + `Retry-After` (no-op if headers already sent).
2. The counter is decremented so the slot is freed for the next request.

The timer is cleared on `finish` / `close` so it never fires after a normal
response.

---

## Response format

Both shed conditions return the same JSON shape:

```json
{
  "error": {
    "message": "Server is under heavy load. Please retry shortly.",
    "code": "CONCURRENCY_CAP",
    "retryAfter": 5
  }
}
```

| Field | Values |
|:---|:---|
| `code` | `"CONCURRENCY_CAP"` or `"TIMEOUT"` |
| `retryAfter` | `RETRY_AFTER_SECONDS` (integer seconds) |

HTTP headers:

```
HTTP/1.1 503 Service Unavailable
Retry-After: 5
Content-Type: application/json
```

---

## Security considerations

| Risk | Mitigation |
|:---|:---|
| DoS via connection exhaustion | Concurrency cap rejects excess requests before any handler work begins |
| Slow-loris / slow-response amplification | Per-request timeout frees the slot and closes the response |
| Cap bypass via abusive IP | Rate limiter runs before load shedding; abusive IPs are blocked upstream |
| Counter leak (slot never freed) | Decrement on both `finish` and `close`; boolean guard prevents double-decrement |
| Double response (handler + timeout race) | `res.headersSent` checked before writing in `shed()` |
| Negative counter | Boolean `decremented` flag makes decrement idempotent |

---

## Testing

Test file: `backend/tests/load-shedding.test.ts`

25 tests across 7 sections:

| Section | Tests | What's validated |
|:---|:---|:---|
| Constants | 3 | Numeric values match documentation |
| Counter helpers | 2 | `getActiveRequests` starts at 0; `resetActiveRequests` works |
| Fast requests | 3 | 200 pass-through; counter returns to 0; no leak across sequential requests |
| Concurrency cap | 6 | Admits exactly cap; 503 on cap+1; `Retry-After` header; `CONCURRENCY_CAP` code; counter not incremented for shed; slot freed after completion |
| Request timeout | 6 | 503 after timeout; `Retry-After` header; `TIMEOUT` code; counter decremented; fast handler wins; just-before-timeout handler wins |
| Counter leak prevention | 2 | Non-negative after reset; idempotent decrement |
| Integration | 2 | Real app health endpoint; middleware wired before routes |

### Running the tests

```bash
cd backend
npm test -- --testPathPattern=load-shedding
# or with coverage
npm run test:coverage -- --testPathPattern=load-shedding
```

---

## Tuning guidance

- **`CONCURRENCY_CAP`**: set to roughly `(available_memory_MB / avg_request_memory_MB)` capped at the number of upstream connections your database / RPC node can handle.
- **`REQUEST_TIMEOUT_MS`**: should be slightly above your p99 upstream latency.  Start at 10 s and tighten once you have latency data.
- **`RETRY_AFTER_SECONDS`**: 5 s is a safe default.  Increase if your recovery time after a spike is longer.
