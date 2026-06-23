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
