# Performance Test Harness

## Overview

In-process latency tests using supertest. No external dependencies, no secrets, deterministic dataset.

Files:
- `src/tests/perf/seed.ts` — generates 100 invoices, 500 bids, 50 settlements with a fixed epoch (not `Date.now()`)
- `src/tests/perf/harness.ts` — runs N requests and returns p50/p95/p99
- `src/tests/perf/perf.test.ts` — regression assertions

## Running

```bash
# perf tests only
npx jest "perf/perf.test.ts" --no-coverage

# full suite
npm test
```

## Targets (p95)

| Endpoint | Target |
|---|---|
| `GET /api/v1/invoices` | < 150ms |
| `GET /api/v1/invoices/:id` | < 150ms |
| `GET /api/v1/bids` | < 150ms |
| `GET /api/v1/settlements` | < 150ms |

Targets include the full in-process HTTP stack overhead (~5–30ms baseline depending on environment). Measured at 200 iterations per endpoint.

## CI Gating

The tests run as part of `npm test` and will fail the suite if any p95 exceeds its target. To run on a schedule, add a cron trigger to `.github/workflows/` pointing at `npm test`.

## Adjusting Targets

If you add real DB/indexer calls, re-baseline by running the suite and reading the `[perf]` log lines, then update `TARGETS` in `perf.test.ts`.

---

# Property-Based Testing for Pagination

## Overview

Pagination encoding/decoding is security-critical: it drives opaque base64url cursors used across all list endpoints (`/invoices`, `/bids`, `/settlements`, `/portfolio`). A regression here silently breaks pagination globally and may leak user data.

We use **property-based testing with `fast-check`** to detect such regressions automatically:

- **Positive domain**: Round-trip correctness (`decode(encode(x)) === x`)
- **Negative domain**: Malformed inputs surface as `INVALID_PAGINATION`
- **Invariant properties**: Limits always clamped to `[1, 100]`
- **Robustness**: No crashes on adversarial inputs

## Files

- `src/utils/pagination.ts` — Core encoding/decoding (BASE64URL cursors)
- `src/utils/pagination.properties.test.ts` — Jest-based property tests
- `run-pagination-tests.ts` — Standalone ts-node runner (no jest setup needed)

## Running Tests

```bash
# Jest runner
npm test pagination.properties.test

# Standalone (fast, ~55ms)
npx ts-node run-pagination-tests.ts
```

## Test Coverage

**Round-trip correctness** (3 tests): `decode(encode(x)) === x` for all valid sort values and arbitrary strings

**Negative domain** (5 tests): Invalid base64url, truncated padding, wrong field types, NaN/Infinity values all return null

**Limit clamping** (5 tests): Limits always in `[1, MAX_LIMIT]`; invalid inputs throw `PaginationError`

**Robustness** (3 tests): No crashes on very long strings (10,000+ chars), unicode, edge cases

**Regression prevention** (5 tests): Known failure modes (NaN sort_val, stale cursors, etc.)

## Example: Property-based Bug Detection

If `encodeCursor` used `toString("base64")` instead of `toString("base64url")`:

```
✗ Round-trip: encode and decode arbitrary CursorPayload
  Property failed after 12 tests
  Counterexample: { id: "a+b/c=", sort_val: 123 }
  Expected: { id: "a+b/c=", sort_val: 123 }
  Received: null
```

Property tests catch encoding bugs **instantly** instead of waiting for user reports.

---

# Per-Route Latency SLO Tracker

## Overview

An in-process, dependency-free histogram that records request durations per
route and exposes p50 / p95 / p99 percentiles over a rolling time window.
Operators no longer need an external APM tool to answer "how fast is route X
right now?"

## Architecture

```
Request arrives
     │
     ▼
request-logger middleware   ← records durationMs on res.finish
     │
     ▼
latencyTracker.record(req.path, durationMs)
     │ normalises path (ULID/UUID/hex → :id)
     ▼
RouteBucket (ring buffer, BUCKET_SIZE=1024 samples per route)
     │
     ▼
GET /api/v1/admin/monitoring/latency
  latencyTracker.getStats(windowMs)
  → filters to samples within window
  → sorts → p50/p95/p99 via nearest-rank
```

## Files

| File | Purpose |
|------|---------|
| `src/services/latencyTracker.ts` | Core service — ring buffers, percentile maths, normalisation |
| `src/middleware/request-logger.ts` | Calls `latencyTracker.record()` inside `res.on("finish")` |
| `src/routes/v1/monitoring.ts` | Exposes `GET /api/v1/admin/monitoring/latency` |
| `src/tests/latency-tracker.test.ts` | Unit + integration tests |

## Admin Endpoint

```
GET /api/v1/admin/monitoring/latency
Authorization: Bearer <api-key>

Query parameters:
  windowMs  (optional)  Rolling window width in ms.
                        Min: 1000 · Max: 3600000 · Default: 300000 (5 min)
```

### Response

```json
{
  "routes": [
    {
      "route":    "/api/v1/invoices/:id",
      "count":    412,
      "p50":      18.4,
      "p95":      67.1,
      "p99":      142.3,
      "min":      4.2,
      "max":      198.7,
      "windowMs": 300000
    }
  ],
  "windowMs":    300000,
  "totalRoutes": 12,
  "maxRoutes":   200,
  "overflowed":  false,
  "generatedAt": "2026-06-23T10:00:00.000Z"
}
```

All latency values are in **milliseconds** (floating-point).  
`null` means no samples arrived in the current window for that route.

### `overflowed` flag

When more than `maxRoutes` (200) distinct route keys are seen, excess routes
are bucketed under the sentinel key `__overflow__`.  
If `overflowed: true` appears in the response, the `__overflow__` entry in
`routes[]` shows aggregate stats for uncapped routes and indicates that route
cardinality should be reviewed.

## Route Key Normalisation

High-cardinality path segments are collapsed before bucketing so that
`/api/v1/invoices/01HXYZ…` and `/api/v1/invoices/01HABC…` share one bucket:

| Pattern | Replaced with |
|---------|--------------|
| ULID (26-char base32) | `:id` |
| UUID v1–v5 | `:id` |
| Hex string ≥ 8 chars | `:id` |
| Bare integer segment | `:id` |

Normalisation is idempotent — running it twice yields the same key.

## Memory Budget

```
MAX_ROUTES (200) × BUCKET_SIZE (1024 samples) × 16 bytes per slot
  = ~3.3 MB durations + ~3.3 MB timestamps
  = ~6.6 MB worst-case
```

Once `MAX_ROUTES` is reached no new route buckets are allocated, so memory
usage is strictly bounded regardless of traffic patterns.

## SLO Targets

These mirror the CI regression targets and serve as the baseline for
alerting:

| Route pattern | p95 SLO |
|---------------|---------|
| `GET /api/v1/invoices` | < 150 ms |
| `GET /api/v1/invoices/:id` | < 150 ms |
| `GET /api/v1/bids` | < 150 ms |
| `GET /api/v1/settlements` | < 150 ms |
| `GET /api/v1/admin/monitoring/latency` | < 50 ms |

To alert on a breach, poll the endpoint on a schedule and compare `p95`
against these targets. A future iteration can push values directly to a
Prometheus gauge or StatsD sink by wrapping `latencyTracker.getStats()` in
a scrape handler.

## Running the Tests

```bash
# latency tracker tests only
npx jest latency-tracker --no-coverage

# with coverage report
npx jest latency-tracker --coverage
```

## Design Notes

- **No external dependency** — percentiles are computed via the "nearest rank"
  method on a sorted in-memory slice. No HDR-Histogram package required.
- **Ring-buffer per route** — `BUCKET_SIZE = 1024` samples. When full the
  oldest sample is overwritten (O(1) write). Statistics iterate only over the
  live capacity so accuracy improves as the buffer fills.
- **Time-windowed filtering** — `getStats(windowMs)` walks the buffer and
  discards samples older than `now - windowMs`. Stale data ages out naturally
  without a background cleanup task.
- **Thread safety** — Node.js executes JS synchronously; `record()` and
  `getStats()` never yield the event loop so no locking is needed.
- **Non-throwing integration** — the `latencyTracker.record()` call in the
  middleware is wrapped in a silent try/catch so a tracker bug can never
  affect request logging or response delivery.
