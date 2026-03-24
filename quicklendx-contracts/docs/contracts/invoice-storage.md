# Invoice Storage & Category Index

## Category Index

Invoices are indexed by category under the composite key `("cat_idx", InvoiceCategory)`.

### Invariants

| # | Invariant | Enforced by |
|---|-----------|-------------|
| 1 | No stale entries after `update_invoice_category` | `remove_category_index` called on old category before `add_category_index` on new |
| 2 | No duplicates in any bucket | Deduplication guard in `add_category_index` |
| 3 | `get_invoice_count_by_category` == `get_invoices_by_category.len()` | Both read the same bucket |
| 4 | `get_all_categories` always returns exactly 7 variants | Statically constructed, storage-independent |

### Key Functions

**`add_category_index(env, category, invoice_id)`**
Appends `invoice_id` to the bucket only if not already present.

**`remove_category_index(env, category, invoice_id)`**
Rebuilds the bucket without `invoice_id`. No-op if absent.

**`update_invoice_category` (lib.rs)**
Atomically calls `remove_category_index(old)` → `add_category_index(new)`. Requires `invoice.business` auth.

### Security Assumptions

- Only the business owner (`require_auth`) can change a category — no third-party index manipulation.
- Index operations are atomic within a single invocation; no partial-update window exists.
- The deduplication guard prevents index inflation from repeated calls.

## Test Coverage

Regression tests are in:
- `src/test_invoice.rs` — `test_category_index_*` (6 tests)
- `src/test/test_invoice_categories.rs` — full query suite + `test_category_index_*` (6 tests)

```bash
cargo test category_index
cargo test test_invoice_categories
```
