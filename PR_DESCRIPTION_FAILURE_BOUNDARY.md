# Deterministic failure-boundary coverage for `assertBidId`, `clearStatementCache`, and `isPromise`

## Summary

Adds focused, deterministic failure-boundary coverage for three backend helpers that sit on
hot or security-sensitive paths, and makes the existing `tracing` / `database` unit suites
execute again so the new coverage is actually enforced:

| Issue | Entry point | What it guards |
| --- | --- | --- |
| #2666 | `assertBidId` in `backend/src/lib/entityId.ts` | Request-boundary validation of bid identifiers |
| #2661 | `clearStatementCache` in `backend/src/lib/database.ts` | Prepared-statement cache lifecycle & metrics reset |
| #2715 | `isPromise` in `backend/src/lib/tracing.ts` | Sync/async branch selection on every traced call |

No public signatures were removed; the only production changes are additive or fix
already-documented behavior (details below).

## Changes

### Issue #2666 — `assertBidId`
- **`backend/src/lib/entityId.test.ts`** — added a `assertBidId deterministic failure boundaries`
  suite covering:
  - valid `bid_` + 26-char ULID (upper/lower case, surrounding whitespace/tabs/newlines) and the
    all-zero boundary ULID;
  - non-string inputs (`null`, `undefined`, `0`, `false`, `true`, `NaN`, `{}`, `[]`, functions);
  - malformed boundaries (`""`, whitespace, prefix-only, 25/27-char ULIDs, trailing garbage,
    doubled prefix, wrong separator, wrong case, stray `_` in the ULID body);
  - excluded Crockford characters (`I`, `L`, `O`, `U`);
  - wrong-prefix ids (`inv_`, `stl_`, `exp_`, bare ULID);
  - the invoice-only `0x…` hex form, which must **not** be accepted as a bid id;
  - repeated-call determinism, boundary-length (`+1` char) rejection, and caller-string immutability.

### Issue #2661 — `clearStatementCache`
- **`backend/src/lib/__tests__/clearStatementCache.test.ts`** (new) — deterministic tests against a
  real in-memory SQLite database:
  - empties cached statements but leaves the connection healthy (`pingDatabase()`, same instance);
  - resets `hits`, `misses`, and `evicts` counters to zero;
  - is idempotent and never throws on an empty cache;
  - forces the next lookup to miss/re-prepare instead of serving a stale entry;
  - stays stable across repeated clear/prepare cycles and is safe after `closeDatabase()`.
- **`backend/src/lib/__tests__/database.test.ts`** — rewrote the `getPreparedStatement`
  failure-boundary suite to mock the `better-sqlite3` driver instead of trying to spy on the
  module's own internal `getDatabase` binding. The suite previously could not observe the paths it
  claimed to cover; it now genuinely exercises success/caching, `SQLITE_BUSY` retry + exhaustion,
  read-only permission failure (no caching), `SQLITE_SCHEMA` eviction, and generic wrap-up.
- **`backend/src/lib/database.ts`** — fixed the permission-error path so a `DatabasePermissionError`
  is re-thrown verbatim instead of being re-wrapped by the outer catch into a generic
  `DatabasePrepareError`. This matches the function's documented contract ("Permission checks
  surface a `DatabasePermissionError` without caching") and makes the failure diagnosable.

### Issue #2715 — `isPromise`
- **`backend/src/lib/tracing.ts`** — exported `isPromise` (additive) and hardened it with a guard so a
  hostile/broken `then` accessor can never throw. The module's explicit invariant is that tracing
  must never break the calling operation, and `isPromise` runs on every `withSpan` call.
- **`backend/src/lib/tracing.isPromise.test.ts`** (new) — deterministic tests for falsy/primitive
  values, plain objects/arrays/functions without a callable `then`, objects whose `then` is present
  but not callable, real/pending/async promises, prototype-inherited `then`, a throwing `then`
  accessor (returns `false`, never throws), proof that `then` is not invoked, and repeated-probe
  determinism.
- **`backend/src/tests/tracing.test.ts`** — repaired the pre-existing breakage that prevented the
  whole suite from running: an unbalanced tuple type, Jest 29 `mockResults.calls` (removed in Jest
  30, replaced with `mock.calls`), and an invalid `expect(...).not.to(...)` matcher. The suite now
  runs and all 18 tests pass.

## Why this is safe / compatibility

- `assertBidId`, `clearStatementCache`, `getPreparedStatement`, and `withSpan` public signatures are
  unchanged. `isPromise` is newly exported, which is backwards compatible.
- Existing callers of `getPreparedStatement` observe a strictly more specific, documented error on
  permission failures; no caller depended on the previous double-wrapped type.
- No broad refactors, dependency upgrades, or loosened validation. The `isPromise` guard only turns a
  previously-throwing hostile case into the deterministic `false` branch.

## Testing

Focused suites (all green):

```bash
cd backend
npx jest src/lib/entityId.test.ts \
         src/lib/__tests__/clearStatementCache.test.ts \
         src/lib/__tests__/database.test.ts \
         src/lib/tracing.isPromise.test.ts \
         src/tests/tracing.test.ts --no-coverage
```

Evidence: `entityId` 63 passed, `clearStatementCache` 7 passed, `database` 7 passed,
`isPromise` 34 passed, `tracing` 18 passed.

Full backend suite: failures reduced from **72 → 69** tests and **19 → 17** suites, with no new
failures introduced (the two suites touched by this PR now pass; the remaining failures are
pre-existing and unrelated).

`npx tsc --noEmit` reports no errors in any file touched by this PR; the only remaining errors are
pre-existing in `src/services/api-key-service.ts` (`'cypto'` import typo, missing
`./api-key-errors`, `dbKey` typo).

## Failure-mode mapping

- **Valid / boundary inputs** — explicit accept + `+1`/`-1` boundary cases.
- **Invalid inputs** — stable public error shape (`BadRequestError` `INVALID_ENTITY_ID`), never a
  crash.
- **Retry / stale / permission** — `SQLITE_BUSY` retry and exhaustion, `SQLITE_SCHEMA` eviction,
  read-only `DatabasePermissionError` without caching.
- **Concurrency / partial failure** — cache clear is idempotent and cannot leave partial state;
  `isPromise` never throws and has no side effects.
- **Diagnosability** — typed errors (`DatabaseBusyError`, `DatabasePermissionError`,
  `DatabasePrepareError`) surface distinct causes without leaking internals.

## Related Issues

Closes #2666
Closes #2661
Closes #2715
