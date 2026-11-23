# Resource bound on `get_invoices_by_tags` (issue #2509)

## Problem

`get_invoices_by_tags` is a public, unauthenticated, view-only entrypoint
that looks up invoices matching every tag in a caller-supplied
`tags: Vec<String>` (AND logic). Before this change it accepted a `tags`
vector of any length. Internally
(`InvoiceStorage::get_invoices_by_tags`), for every entry in `first_ids`
(the match set for `tags[0]`), the function re-ran `get_invoices_by_tag`
— a full per-tag index scan — once for each remaining tag:

```
cost ≈ |first_ids| × (tags.len() - 1) × (index scan cost per tag)
```

`tags.len()` was entirely caller-controlled with no cap. Any address
could call this entrypoint directly with an arbitrarily long `tags`
vector to multiply the cost of a single call, with no authorization
required and no fee paid beyond the call's own resource consumption —
the exact "unbounded input... exhausting service, ledger... resources"
scenario this issue exists to close off.

## Design and invariant

No invoice can ever be stored with more than
`verification::MAX_INVOICE_TAG_COUNT` (10) tags — that cap is already
enforced at write time by `verification::validate_invoice_tags` (used by
`Invoice::new`) and by `invoice::MAX_INVOICE_TAGS` (used by
`Invoice::add_tag`). It follows that a query naming more than 10 tags
can **never** match any real invoice: at most 10 of the supplied tags
could possibly correspond to an invoice's actual tag set, so anything
past that is provably wasted work.

The fix enforces this as a hard precondition, checked before any index
work begins:

```rust
if tags.len() > crate::verification::MAX_INVOICE_TAG_COUNT {
    return Err(QuickLendXError::TagLimitExceeded);
}
```

This reuses the exact error already returned when a caller tries to
attach too many tags to an invoice (`invoice.rs`, `verification.rs`),
so callers get one consistent, actionable error for "too many tags"
everywhere in the contract's public API — not a second, redundant error
variant for the same underlying condition.

## Failure behavior

- `tags.len() <= MAX_INVOICE_TAG_COUNT`: unchanged behavior — the query
  runs exactly as before and returns the matching invoice IDs (or an
  empty vector if none match).
- `tags.len() > MAX_INVOICE_TAG_COUNT`: the call now returns
  `Err(QuickLendXError::TagLimitExceeded)` immediately, before touching
  storage. No index read, no partial result, no state mutation of any
  kind (this was already a read-only path, so "no partial state" was
  automatic; the bound now also makes the *rejection* itself immediate
  and cheap rather than proportional to the oversized input).

## Compatibility impact

**This is a response-shape change**, made explicit per this issue's
acceptance criteria: the public entrypoint

```rust
pub fn get_invoices_by_tags(env: Env, tags: Vec<String>) -> Vec<BytesN<32>>
```

becomes

```rust
pub fn get_invoices_by_tags(env: Env, tags: Vec<String>) -> Result<Vec<BytesN<32>>, QuickLendXError>
```

This mirrors an existing, established pattern in this same contract for
other `get_*` queries that can fail validation (`get_invoice`,
`get_platform_fee_config`, `get_revenue_split_config`,
`get_fee_analytics`), so it is not a novel calling convention.

For the Soroban-generated client, this change is backward compatible
for every well-behaved existing caller: the auto-generated `Client`
method (used as `client.get_invoices_by_tags(&tags)`) already unwraps a
`Result<T, E>`-returning contract function on the caller's behalf,
panicking only if the contract actually returns `Err`. Any caller
passing 10 or fewer tags — which is every caller today, since no
invoice can have more — sees no behavioral difference at all. A caller
that inspects the result explicitly (`try_get_invoices_by_tags`) gains
the ability to detect and handle the new rejection instead of it
surfacing as a panic.

No storage layout changed. No migration is required — this is a
stateless validation added to a read-only query.

## Rollback

Reverting this change is a pure code revert (drop the length check,
restore the old `Vec<BytesN<32>>` return type and its two call sites in
`storage.rs` / `lib.rs`). There is no persisted state to roll back or
reconcile, since the change touches neither storage layout nor any
write path.

## Operational limitations

- The cap (`MAX_INVOICE_TAG_COUNT` = 10) is fixed at compile time via
  the shared constant; it is not independently configurable for this
  query without also changing what the write-side cap allows (which is
  intentional — the two must stay in lock-step for the "can never
  match" argument above to hold).
- This bound addresses the caller-controlled `tags.len()` amplification
  vector specifically. It does not change the per-tag index scan's own
  cost, which is proportional to how many invoices legitimately share a
  tag — that growth is organic (tied to real protocol usage over time,
  not a single call's input), the same category of bound every other
  `get_invoices_by_tag`-style query in this contract already accepts.

## Security assumptions

- `get_invoices_by_tags` remains intentionally unauthenticated (it is a
  read-only query over already-public invoice metadata); the fix adds a
  resource bound, not an access-control change.
- The bound is enforced identically regardless of caller identity —
  there is no allowlist or elevated-caller exemption, since the
  underlying invariant (no invoice can have more than 10 tags) holds
  for every invoice regardless of who is querying.

## Tests

`quicklendx-contracts/src/test_max_invoice_tags_boundary.rs`, section 4
(`get_invoices_by_tags query boundary`), exercises the bound through the
actual contract client (`QuickLendXContractClient`) — the real
integration boundary — mirroring the existing below/at/over/far-over
pattern already used for the write-side cap in the same file:

- accepts `MAX_INVOICE_TAG_COUNT - 1` tags (empty match set, no error)
- accepts exactly `MAX_INVOICE_TAG_COUNT` tags (inclusive boundary)
- rejects `MAX_INVOICE_TAG_COUNT + 1` tags with `TagLimitExceeded`
- rejects `MAX_INVOICE_TAG_COUNT * 10` tags with `TagLimitExceeded`
  (proves the check is a real bound, not an off-by-one that only
  catches the first overflow)
