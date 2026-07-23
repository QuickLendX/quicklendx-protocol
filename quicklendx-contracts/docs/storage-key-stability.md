# Storage Key Stability Policy

## Why key stability matters

QuickLendX runs on Stellar Soroban, where all contract state is stored in
ledger entries keyed by an XDR-encoded value derived from the Rust type passed
to `env.storage().*().set(key, value)`.  When `key` changes — even a single
character rename of the inner `symbol_short!("…")` string — the old ledger
entries become **permanently unreachable** from the contract's perspective.
No entry is deleted; the data simply sits at an address the contract no longer
computes, orphaned forever.

There is no garbage collection.  There is no automatic migration.  A rename
without an explicit migration function is silent data loss for every deployed
instance of the contract.

## Canonical key register

All storage keys are captured in `src/test_snapshots/storage_keys.txt`.
The snapshot test in `src/test_storage_key_layout.rs` asserts at `cargo test`
time that every key builder in `src/storage.rs`, `src/bid.rs`, and
`src/investment.rs` still produces the symbol string recorded in the snapshot.

### Storage classes

| Class       | Soroban API                   | Scope                                    |
|-------------|-------------------------------|------------------------------------------|
| Persistent  | `env.storage().persistent()`  | Survives contract upgrades; TTL-extended |
| Instance    | `env.storage().instance()`    | Tied to contract instance lifetime       |
| Temporary   | `env.storage().temporary()`   | Automatically expires; not yet used      |

### Key register summary

| Symbol string  | Class      | Key builder / constant                          |
|----------------|------------|-------------------------------------------------|
| `fees`         | Instance   | `StorageKeys::platform_fees()`                  |
| `inv_count`    | Persistent | `StorageKeys::invoice_count()`                  |
| `bid_count`    | Persistent | `StorageKeys::bid_count()`                      |
| `inv_cnt`      | Persistent | `StorageKeys::investment_count()`               |
| `inv_bus`      | Persistent | `Indexes::invoices_by_business()`               |
| `inv_st`       | Persistent | `Indexes::invoices_by_status()` / `investments_by_status()` |
| `inv_cust`     | Persistent | `Indexes::invoices_by_customer()`               |
| `inv_taxid`    | Persistent | `Indexes::invoices_by_tax_id()`                 |
| `inv_tag`      | Persistent | `Indexes::invoices_by_tag()`                    |
| `inv_cat`      | Persistent | `Indexes::invoices_by_category()`               |
| `bids_inv`     | Persistent | `Indexes::bids_by_invoice()`                    |
| `bids_invr`    | Persistent | `Indexes::bids_by_investor()`                   |
| `bids_stat`    | Persistent | `Indexes::bids_by_status()`                     |
| `invst_inv`    | Persistent | `Indexes::investments_by_invoice()`             |
| `inv_invst`    | Persistent | `Indexes::investments_by_investor()`            |
| `all_bids`     | Persistent | `BidStorage` global bid list (`ALL_BIDS_KEY`)   |
| `bid_ttl`      | Instance   | `BidStorage` TTL config (`BID_TTL_KEY`)         |
| `mx_actbd`     | Instance   | `BidStorage` max-active-bids limit              |
| `inv_map`      | Persistent | `InvestmentStorage` invoice-to-investment map   |
| `invst_cnt`    | Instance   | `InvestmentStorage` ID counter                  |

`DataKey` enum variant names (`Invoice`, `Bid`, `Investment`) are also
stable identifiers; renaming a variant changes the XDR discriminant that
prefixes every record key using that variant.

## Migration checklist

Follow this checklist for every proposed key rename or new key addition.

### Renaming an existing key

1. **Risk assessment** — identify all deployed contract instances that hold
   data under the old key.  Estimate how much state would be orphaned if the
   migration is skipped.

2. **Write a migration function** — implement a one-time admin-callable
   function that:
   - reads all data from the old key(s),
   - writes it to the new key(s),
   - deletes the old key(s) to reclaim storage rent.

3. **Paginate large data sets** — if the index being migrated can contain
   many entries (e.g., the invoice status index), use the
   `rebuild_indexes_page` pattern: accept `offset` and `limit` arguments and
   migrate a bounded number of entries per transaction.

4. **Update the snapshot** — edit `src/test_snapshots/storage_keys.txt` so
   the entry for the affected key shows the new symbol string.

5. **Update this document** — update the key register table above.

6. **Admin review** — because snapshot deletions or renames expose
   data-loss risk, the PR must be reviewed by a project admin or security
   team member before merging.

7. **Verify** — run `cargo test test_storage_key_layout` to confirm all
   snapshot assertions pass with the updated snapshot file.

### Adding a new key

1. Add the `symbol_short!("…")` call in the appropriate key builder.
2. Add a new line to `src/test_snapshots/storage_keys.txt`.
3. Add a corresponding test case in `src/test_storage_key_layout.rs`.
4. Update the key register table in this document.
5. Run `cargo test test_storage_key_layout` to confirm the new test passes.

### Removing a key (deprecation)

1. Ensure no live contract instance holds data under the key (or provide a
   migration to clear/move it first).
2. Remove the line from `src/test_snapshots/storage_keys.txt`.
3. Remove the corresponding test case from `src/test_storage_key_layout.rs`.
4. Requires admin review (same as a rename).

## Enforcement in CI

`cargo test` is required to pass on every PR (`test_storage_key_layout`
module is unconditionally compiled under `#[cfg(test)]`).  A snapshot drift —
caused by renaming a symbol or deleting a snapshot entry — will fail CI
immediately, blocking the merge and prompting a migration review.

## Historical renames

| Date | Old symbol | New symbol | Migration function | Author |
|------|-----------|------------|-------------------|--------|
| —    | —         | —          | —                 | —      |

*(No renames have occurred since this policy was introduced.)*
