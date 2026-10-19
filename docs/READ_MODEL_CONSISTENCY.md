# Read Model Consistency

Audience: contributors.

This note explains how the QuickLendX write model and the query/view model fit together. The goal is to make it easy to confirm that a mutating entrypoint changes the exact state a later `get_*` query will read back.

## The rule of thumb

- Write entrypoints are the state transitions: `upload_invoice`, `place_bid`, `accept_bid_and_fund`, `verify_invoice`, `settle_invoice`, and similar methods.
- Read entrypoints are the projections of that state: `get_invoice`, `get_bid`, `get_ranked_bids`, `get_invoice_progress`, `get_protocol_limits`, and the rest of the `get_*` methods.
- When a write path updates storage, the query path should be able to read the same object back without re-running the mutation.

If a write updates `InvoiceStatus::Verified` or `InvoiceStatus::Funded`, the read model should reflect the new status through `get_invoice` and any status-indexed queries such as `get_invoices_by_status`.

## Concrete read/write pairings

### 1. Invoice creation and retrieval

`upload_invoice` is the write path that creates a new invoice record and stores it in `InvoiceStorage`.

```rust
pub fn upload_invoice(
    env: Env,
    business: Address,
    amount: i128,
    currency: Address,
    due_date: u64,
    description: String,
    category: InvoiceCategory,
    tags: Vec<String>,
) -> Result<BytesN<32>, QuickLendXError>
```

The mutation performs validation, rejects paused or unauthorized execution, and then persists the invoice object:

```rust
let invoice = Invoice::new(...)?;
InvoiceStorage::store_invoice(&env, &invoice);
emit_invoice_uploaded(&env, &invoice);
```

The corresponding read path is `get_invoice`:

```rust
pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError>
```

The important consistency point is that `get_invoice` reads the stored invoice back by its identifier, so the data returned by the query should match the fields that were just written during `upload_invoice`.

### 2. Bid placement and ranking

`place_bid` writes a new `Bid` record into `BidStorage`, stores the bid ID on the invoice, and records idempotency state for the `(invoice_id, investor, salt)` tuple.

```rust
let bid = Bid {
    bid_id: bid_id.clone(),
    invoice_id: invoice_id.clone(),
    investor: investor.clone(),
    bid_amount,
    expected_return,
    timestamp: current_timestamp,
    status: BidStatus::Placed,
    expiration_timestamp: Bid::default_expiration_with_env(&env, current_timestamp),
};
BidStorage::store_bid(&env, &bid);
BidStorage::add_bid_to_invoice(&env, &invoice_id, &bid_id);
```

The read model then exposes that write in several ways:

- `get_bid(env, bid_id)` returns the stored bid record.
- `get_bids_for_invoice(env, invoice_id)` returns all bid records for the invoice.
- `get_ranked_bids(env, invoice_id)` re-reads the stored bid set and sorts it using the ranking rules.

This split is intentional:

- `get_bids_for_invoice` is the raw record view.
- `get_ranked_bids` is the derived view that applies ranking logic over the persisted records.

So even though the ranking query is computed on demand, the inputs are still the stored bids from the write path.

### 3. Acceptance and funding

`accept_bid_and_fund` is the write path that materially changes the invoice lifecycle.

The funding transition is not just a status update; it also changes invoice fields such as `funded_amount`, `funded_at`, and the attached investor relationship. After the write succeeds, the read model should expose those updated fields through `get_invoice` and through investment-oriented queries such as `get_invoice_investment` or `get_investment`.

A useful reviewer checklist is:

1. A successful write should update the canonical record.
2. A query should return the canonical record, not a stale cache or an alternate projection.
3. Status-indexed queries such as `get_invoices_by_status` should reflect the new status after the state transition.

## Why this matters in practice

The protocol is easier to reason about when the write path and the read path are aligned:

- `store_invoice` / `upload_invoice` writes the invoice object.
- `get_invoice` reads the invoice object back.
- `place_bid` writes the bid object.
- `get_bid` and `get_ranked_bids` read the bid object back.
- `accept_bid_and_fund` updates invoice lifecycle fields.
- `get_invoice`, `get_invoice_progress`, and investment getters reflect the same lifecycle.

A common regression to watch for is a write that updates storage but forgets to keep the indexed read model in sync. That is why the contract keeps both full-object retrieval and filtered/indexed query entrypoints.

## Reviewer idiom

When reviewing a lifecycle change, trace one object through the pipeline:

```text
write entrypoint -> storage write -> status index update -> read entrypoint -> filtered query view
```

For example:

```text
place_bid
  -> BidStorage::store_bid
  -> BidStorage::add_bid_to_invoice
  -> get_bid
  -> get_ranked_bids
```

or:

```text
accept_bid_and_fund
  -> InvoiceStorage::update_invoice
  -> status index transition
  -> get_invoice
  -> get_invoice_progress
```

That is the consistency contract the docs are trying to preserve.

## Related documents

- [docs/QUERIES.md](QUERIES.md) for the catalog of common read-only entrypoints.
- [quicklendx-contracts/README.md](../quicklendx-contracts/README.md) for contract-level API and development guidance.
