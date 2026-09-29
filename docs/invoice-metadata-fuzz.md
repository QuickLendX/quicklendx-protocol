# Invoice Metadata Fuzz Harness

`src/test_fuzz_invoice_metadata.rs` — proptest harness for `src/invoice.rs` and `src/verification.rs` bounded-vector invariants.

## Bounded vectors

| Vector | Constant | Location | Rejection error |
|---|---|---|---|
| `Invoice.tags` | `MAX_INVOICE_TAGS = 10` | `invoice.rs` | `TagLimitExceeded` |
| `InvoiceMetadata.line_items` | `MAX_METADATA_LINE_ITEMS = 100` | `verification.rs` | `InvalidDescription` |
| `Invoice.ratings` | `MAX_RATINGS_PER_INVOICE = 100` | `invoice.rs` | `OperationNotAllowed` |

## What the harness tests

### Tag boundary (proptest)

- `fuzz_tags_at_or_below_max_accepts` — sweeps `count` in `0..=MAX_INVOICE_TAGS`. Each `Invoice::new` call with `count` distinct tags must succeed and produce exactly `count` tags.
- `fuzz_tags_above_max_rejects` — sweeps `extra` in `1..=10`. `Invoice::new` with `MAX_INVOICE_TAGS + extra` unique tags must return `TagLimitExceeded`.
- `fuzz_add_tag_boundary` — drives `add_tag` for a sequence of `count` calls; asserts capacity is never exceeded and `TagLimitExceeded` fires exactly at the bound.

### Line-items boundary (proptest)

- `fuzz_line_items_at_or_below_max_accepts` — sweeps `count` in `1..=MAX_METADATA_LINE_ITEMS`. Each `validate_invoice_metadata` call with well-formed items must succeed.
- `fuzz_line_items_above_max_rejects` — sweeps `extra` in `1..=10`. `validate_invoice_metadata` with `MAX_METADATA_LINE_ITEMS + extra` items must return `InvalidDescription`.

### Ratings boundary (proptest)

- `fuzz_ratings_at_or_below_max_accepts` — sweeps `count` in `0..=MAX_RATINGS_PER_INVOICE`. Each `add_rating` for a unique rater must succeed.
- `fuzz_ratings_above_max_rejects` — fills the ratings vector to capacity then asserts the next `add_rating` returns `OperationNotAllowed`.

### Tag→invoice index consistency (proptest)

- `fuzz_tag_index_consistency` — drives a random sequence of add/remove operations over a five-tag pool.

  After each operation the harness checks two invariants:

  1. **No missing entries**: every tag present in `invoice.tags` must have `invoice.id` in the corresponding secondary index.
  2. **No orphan entries**: every tag absent from `invoice.tags` must not have `invoice.id` in its secondary index.

  The orphan risk arises when an `InvoiceStorage::remove_tag_index` call is skipped or mismatched — for example if normalization diverges between `add_tag` and `remove_tag`. The harness catches this class of regression.

## Running

```sh
# Fast (CI default)
cargo test --features fuzz-tests test_fuzz_invoice_metadata

# Thorough
PROPTEST_CASES=20000 cargo test --features fuzz-tests test_fuzz_invoice_metadata

# Single deterministic smoke tests only
cargo test --features fuzz-tests -- tags_exactly_at_max
```

## Design notes

- **No silent truncation**: all three helpers return a typed error rather than silently capping the vector. The harness confirms that the *specific* error variant is stable across runs; changing the variant would break this check and force a deliberate protocol decision.
- **Index orphan risk**: the tag→invoice secondary index is the most likely place for a divergence between in-memory state and persistent storage. The proptest harness exercises arbitrary interleaving of add/remove to surface such divergences early.
- **Line-item total consistency**: `validate_invoice_metadata` requires `Σ line_item.total == invoice_amount`. The test helper sets each item to `(qty=1, price=1, total=1)` and passes `invoice_amount = item_count`, satisfying this invariant while keeping the harness focused on the *count* bound.
