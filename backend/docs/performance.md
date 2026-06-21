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
