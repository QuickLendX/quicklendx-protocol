# Issue #2456 — Investor exposure pagination: cursor semantics

**QE-2026-08 · Quality/Medium · Area: financial correctness / authorization**

## Summary

`InvestmentQueries::get_investor_investments_paginated` (and the contract
entrypoint built on it, `get_investor_investments_paged`) page over an
investor's investment index using plain `(offset, limit)`. That is correct
and panic-free for a single call, but has no way to detect that the
underlying collection changed **between** two calls a client makes while
paging through results — e.g. a new investment lands for that investor
between the client's page-1 and page-2 requests. When that happens, plain
offset pagination can skip a record (it shifts into a page the client
already fetched) or return a record twice (it shifts into a page the client
hasn't fetched yet). Neither is a panic or an out-of-bounds read; both are
silent, and both are the exact class of bug this issue is about: investor
exposure and available capacity being misstated by a client that trusted a
paged read as complete or consistent.

The codebase already had a purpose-built tool for this
(`crate::pagination::PageCursor` / `require_stable_cursor`, with a generation
tag for snapshot-stability validation) sitting completely unused — nothing
computed or checked a generation anywhere. This change wires it up for the
investor-investments query, the one named as the starting point.

## Design

### Generation counter

`InvestmentStorage` (`src/investment.rs`) gains a per-investor generation
counter, stored under a new key
(`investor_generation_key = (symbol_short!("inv_gen"), investor)`),
independent of the existing investment index key:

- `get_investor_generation(env, investor) -> u64` — `0` if never bumped
  (including an investor with zero investments).
- `bump_investor_generation` (private) — increments by exactly `1`.

`add_to_investor_index` calls the bump **only** when it actually appends a
new id — not on its existing no-op path for a duplicate id. This means the
generation tracks the *size* of the investor's raw investment index, which
is precisely the property that determines whether an offset computed
against an earlier read of that index is still valid.

### Cursor-stable query

`InvestmentQueries::get_investor_investments_paginated_cursored`
(`src/investment_queries.rs`) is a **new, additive** function:

```rust
pub fn get_investor_investments_paginated_cursored(
    env: &Env,
    investor: &Address,
    status_filter: Option<InvestmentStatus>,
    offset: u32,
    limit: u32,
    cursor_generation: Option<u64>,
) -> Result<InvestorInvestmentsPage, QuickLendXError>
```

returning a new type:

```rust
pub struct InvestorInvestmentsPage {
    pub items: Vec<BytesN<32>>,
    pub total_count: u32,
    pub has_more: bool,
    pub generation: u64,
}
```

Protocol: pass `cursor_generation: None` for the first page. Every response
carries `generation` — the investor's generation this page was computed
against. Pass that value back as `cursor_generation` on the next call for
the same investor/filter. If the generation no longer matches (a new
investment was recorded in between), the call returns
`Err(QuickLendXError::UnstableCursor)` instead of a page that might skip or
duplicate records — **fail closed, not silently wrong**. The caller restarts
from `offset: 0` to get a fresh generation and a consistent view.

The new contract entrypoint, `get_investor_investments_paged_cursored`
(`src/lib.rs`), is a thin wrapper with the same signature at the ABI
boundary.

### Ordering, encoding, limits, end-of-stream (acceptance criterion 1)

All four are inherited unchanged from `crate::pagination`, which this
function calls through exactly the same path the existing (uncursored)
function already used:

- **Ordering**: `paginate_slice`/`calculate_safe_bounds` preserve input
  order — no sort, no dedup, no skip within a page (`pagination.rs`
  invariant 3, pre-existing and untouched).
- **Cursor encoding**: `PageCursor`/generation is a plain `u64`, opaque to
  callers beyond round-tripping it — no serialization format to version.
- **Page limits**: `MAX_QUERY_LIMIT = 50` hard cap, enforced by
  `cap_query_limit` inside `calculate_safe_bounds` (pre-existing, invariant
  1) regardless of the requested `limit`.
- **End-of-stream**: `has_more` is computed the same way as the existing
  function (`pagination_metadata`); an out-of-range `offset` (including
  `u32::MAX`) returns an empty page with `has_more: false`, never a panic
  or an error (invariant 2) — cursor validation happens *before* bounds
  computation, so an invalid cursor is reported as `UnstableCursor`, not
  conflated with an empty result.

## Compatibility

**Purely additive.** `get_investor_investments_paginated` and the
`get_investor_investments_paged` contract entrypoint are byte-for-byte
unchanged — same signature, same behavior, same response shape. No existing
caller's request or response changes. `add_to_investor_index`'s public
behavior (what gets stored, when) is unchanged; it does one additional
storage write (the generation bump) on the same code path that already
writes the index itself, only when a new id is actually appended.

No migration is required: the generation key does not exist for any
investor until this code first runs, at which point `get_investor_generation`
transparently returns `0` for existing investors (identical to a investor
who has always had a `0` generation) — the first cursored call any existing
investor's client makes will correctly start from generation `0` and get
`UnstableCursor` on subsequent pages only if that investor's index actually
changes afterward, which is the intended behavior. No rollback action is
needed if this change is reverted: the new generation storage keys are
simply orphaned (still subject to normal TTL extension logic while present,
but unread once the cursored function is gone), not referenced by anything
else.

## Failure behavior / no partial or unauthorized state

- `get_investor_investments_paginated_cursored` is **read-only**. A rejected
  call (bad cursor, i.e. `UnstableCursor`) performs zero storage writes —
  the only state read is `get_investor_generation` and the underlying
  investment index/records, both via existing read-only accessors.
- The single write this change introduces — the generation bump — happens
  inside `add_to_investor_index`, which itself only runs from
  `store_investment`, the investment-creation path already gated by
  whatever authorization that path enforces. This change does not add,
  remove, or alter an authorization check anywhere; it does not touch any
  fund-moving code path (escrow, release, refund, bid acceptance). No new
  entrypoint here can move funds.
- A repeated, identical cursored call (same offset/limit/generation) is
  idempotent by construction — it's a pure read with no counters or
  side-effecting writes on the read path itself (`test_cursored_investments_repeated_identical_calls_are_idempotent`).
- Concurrent inserts are handled by rejection, not by silently interleaving:
  a caller mid-pagination who hits `UnstableCursor` has performed no writes
  and holds no partial page state the contract needs to reconcile — the
  next call from `offset: 0` starts a clean, fully consistent read.

## Operational limitations

- **Status-filtered pagination is not protected by this cursor.** The
  generation only tracks additions to the investor's *raw* investment
  index. An existing investment's status changing (e.g. `Active` →
  `Completed`) between two calls does not bump the generation, but can move
  that record in or out of a `status_filter`ed page. Protecting that case
  would require a generation per `(investor, status)` pair — a materially
  larger change (every status-transition call site across the crate would
  need to bump the relevant generation(s)) that this focused pass
  deliberately does not make. Documented here rather than silently
  under-covering the acceptance criteria: the required "concurrent-insert"
  test case is the one this change protects; a hypothetical
  "concurrent-status-change-under-a-filter" case is not.
- The generation counter is `u64` and only ever increments by `1`; it
  cannot wrap in practice within Soroban's resource-budget lifetime of any
  contract instance.

## Security assumptions

- `get_investor_investments_paged_cursored`, like the existing
  `get_investor_investments_paged`, requires no authorization — it reads
  data that is already individually queryable per-investment, so an
  aggregate/paged read of the same data exposes nothing new (same
  assumption already documented on `InvestorPortfolioSummary`).
- The generation is not a secret and is not used for anything beyond
  detecting whether the *caller's own prior read* is still valid — it is
  not a capability token, and possessing a valid generation value grants no
  additional read or write access beyond what an unauthenticated cursored
  call already has.

## Validation

Commands run in `quicklendx-contracts/`:

```bash
cargo check --tests
cargo test --lib test_cursored_investments
cargo fmt --check
```

See the PR description for the actual output of each command — this file
documents design and invariants; validation results are recorded where the
change was actually run against this repository's toolchain rather than
duplicated here to avoid the two going stale independently of each other.
