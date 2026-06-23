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

## Soak Test (indexer + webhook pipeline)

A long-running soak validates sustained throughput, memory stability, and queue behavior. The soak test is implemented at `src/tests/perf/soak.test.ts` and can be executed locally or in CI.

Run locally (default 60s):

```bash
SOAK_DURATION_MS=60000 npm test -- soak -- --runInBand
```

Run in CI (default 60 minutes when `CI=true`):

```bash
npm test -- soak -- --runInBand
```

Env vars:
- `SOAK_DURATION_MS` — duration in milliseconds. Defaults to 60s locally, 60m in CI.

SLOs asserted by the soak test:
- Queue depth: never exceed configured capacity (`webhookQueue` capacity).
- RSS growth: < 50 MB / hour (scaled to test duration).
- Indexer lag: returns to zero by test end.

CI workflow snippet (add to `.github/workflows/` as a scheduled job):

```yaml
name: Soak Test

on:
	schedule:
		- cron: '0 */6 * * *' # every 6 hours

jobs:
	soak:
		runs-on: ubuntu-latest
		steps:
			- uses: actions/checkout@v4
			- name: Setup Node.js
				uses: actions/setup-node@v4
				with:
					node-version: 18
			- name: Install
				run: cd backend && npm ci
			- name: Run soak test
				env:
					CI: 'true'
					SOAK_DURATION_MS: '3600000' # 60 minutes
				run: cd backend && npm test -- soak -- --runInBand
```

