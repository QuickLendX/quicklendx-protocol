# Multi-Currency Whitelist

Admin-managed whitelist of token addresses allowed for invoice currency. Invoice creation and bidding are rejected when the invoice's currency is not whitelisted (when the whitelist is non-empty).

## Entrypoints

| Entrypoint | Visibility | Description |
|------------|------------|--------------|
| `add_currency` | Public (admin) | Add a token address to the whitelist. Idempotent if already present. |
| `remove_currency` | Public (admin) | Remove a token address from the whitelist. |
| `set_currencies` | Public (admin) | Atomically replace entire whitelist with deduplication. |
| `clear_currencies` | Public (admin) | Reset whitelist to empty (allow-all) state. |
| `is_allowed_currency` | Public | Return whether a token is currently whitelisted. |
| `get_whitelisted_currencies` | Public | Return the full list of whitelisted token addresses. |
| `get_whitelisted_currencies_paged` | Public | Return paginated slice of whitelisted addresses. |
| `currency_count` | Public | Return the number of whitelisted currencies. |

## Pagination

### Overview
The `get_whitelisted_currencies_paged(offset, limit)` function provides safe, bounded access to the currency whitelist with comprehensive boundary protection and overflow safety.

### Parameters
- `offset: u32` - Zero-based starting position (0 = first item)
- `limit: u32` - Maximum number of items to return

### Boundary Behavior
- **Empty whitelist**: Returns empty result regardless of offset/limit values
- **Offset >= length**: Returns empty result (no panic or error)
- **Limit = 0**: Returns empty result
- **Offset + limit overflow**: Handled safely using saturating arithmetic
- **Large values**: `u32::MAX` values handled without panic

### Security Features
- **Overflow protection**: Uses `saturating_add()` and `min()` for safe arithmetic
- **No information leakage**: Only returns data within specified bounds
- **Public read access**: No authentication required for pagination queries
- **Consistent ordering**: Results maintain same order as full list across calls
- **No side effects**: Read-only operation with no state modifications

### Performance Characteristics
- **O(1) setup**: Constant time initialization and bounds checking
- **O(min(limit, remaining))**: Linear only in the number of returned items
- **Memory efficient**: Only allocates result vector of actual size needed
- **Storage efficient**: Single read of full list, then efficient slicing

### Examples

```rust
// Get first 10 currencies
let page1 = client.get_whitelisted_currencies_paged(&0u32, &10u32);

// Get next 10 currencies  
let page2 = client.get_whitelisted_currencies_paged(&10u32, &10u32);

// Safe with large values - no panic
let safe = client.get_whitelisted_currencies_paged(&u32::MAX, &u32::MAX); // Returns empty

// Handle empty whitelist gracefully
let empty = client.get_whitelisted_currencies_paged(&0u32, &100u32); // Returns empty if no currencies

// Iterate through all currencies with pagination
let mut offset = 0u32;
let page_size = 20u32;
loop {
    let page = client.get_whitelisted_currencies_paged(&offset, &page_size);
    if page.len() == 0 { break; }
    // Process page...
    offset += page_size;
}
```

### Edge Cases Handled
- Empty whitelist with any offset/limit combination
- Offset beyond whitelist length (returns empty, no error)
- Limit larger than remaining items (returns available items)
- Arithmetic overflow in offset + limit calculations
- Zero limit with valid offset (returns empty)
- Maximum u32 values for both offset and limit parameters
- Single item whitelist with various offset/limit combinations
- Rapid modifications during pagination (consistent results)

## Enforcement

- **Invoice creation** (`store_invoice`, `upload_invoice`): Before creating an invoice, the contract calls `require_allowed_currency(env, &currency)`. If the whitelist is non-empty and the currency is not in it, the call fails with `InvalidCurrency`.
- **Bidding** (`place_bid`): Before accepting a bid, the contract checks the invoice's currency with `require_allowed_currency`. Bids on invoices whose currency is not whitelisted (when the whitelist is set) fail with `InvalidCurrency`.

## Backward Compatibility

When the whitelist is **empty**, all currencies are allowed. This keeps existing deployments and tests working without an initial admin setup. Once at least one currency is added, only whitelisted tokens are accepted for new invoices and bids.

## Admin-Only Operations

Only the contract admin (from `AdminStorage::get_admin`) may call write operations:
- `add_currency` and `remove_currency`: Require admin authentication
- `set_currencies`: Atomic bulk replacement with deduplication
- `clear_currencies`: Reset to allow-all state

The caller must pass the admin address and that address must match the stored admin; `require_auth()` is required for that address. Non-admin callers receive `NotAdmin`.

## Security Considerations

### Authentication Model

Every state-mutating function follows a two-layer auth check:

1. **Storage check** — `AdminStorage::get_admin(env)` retrieves the stored admin address.  If no admin is set the call fails with `NotAdmin`.
2. **Runtime auth** — `admin.require_auth()` forces the Soroban host to verify the transaction was signed by that address.  Both checks must pass; bypassing one is not sufficient.

Functions that enforce this pattern:

| Function | Storage check | `require_auth` |
|---|---|---|
| `add_currency` | ✅ | ✅ |
| `remove_currency` | ✅ | ✅ |
| `set_currencies` | ✅ | ✅ |
| `clear_currencies` | ✅ | ✅ |

> **Security note (PR #524):** `add_currency` and `set_currencies` previously relied on an implicit auth comment rather than an explicit `require_auth()` call.  Both functions now call `admin.require_auth()` unconditionally, closing the gap between the comment and enforced runtime behaviour.

### Write Operations
- Every write requires `admin.require_auth()` + admin storage verification
- No user can modify the whitelist without proper admin credentials
- Use `set_currencies` for bulk updates to avoid partial state inconsistencies
- `set_currencies` deduplicates before writing — passing duplicate addresses does not inflate the list

### Read Operations
- Pagination queries are public and require no authentication
- No DoS risk from pagination due to bounded reads and overflow protection
- Consistent results across multiple pagination calls
- No information leakage beyond intended whitelist data

### Boundary Safety
- **Overflow fix (PR #524):** The original `(offset + limit)` expression was replaced with `offset.saturating_add(limit)` to prevent integer overflow panics when both parameters are large (e.g. `u32::MAX`).
- Large offset/limit values handled gracefully without panics
- Empty whitelist scenarios handled consistently
- Memory usage bounded by actual result size, not input parameters

## Error Conditions

| Error | Cause | Mitigation |
|-------|-------|------------|
| `NotAdmin` | Caller is not the registered admin | Ensure proper admin authentication |
| `InvalidCurrency` | Token not in whitelist (when whitelist is non-empty) | Add currency to whitelist or use allowed currency |

## Testing Coverage

`src/test_currency.rs` provides exhaustive coverage across all validation paths.

### Core flow tests

| Test | What it validates |
|---|---|
| `test_get_whitelisted_currencies_empty_by_default` | Fresh whitelist is empty; `is_allowed_currency` returns false |
| `test_get_whitelisted_currencies_after_add_and_remove` | Add two, verify both present; remove one, verify state |
| `test_is_allowed_currency_true_false_paths` | Allowed / disallowed / removed paths |
| `test_add_remove_currency_admin_only` | Full add-then-remove lifecycle |
| `test_non_admin_cannot_add_currency` | Non-admin `try_add_currency` returns error |
| `test_non_admin_cannot_remove_currency` | Non-admin `try_remove_currency` returns error |
| `test_invoice_with_non_whitelisted_currency_fails_when_whitelist_set` | `store_invoice` rejects non-whitelisted currency |
| `test_invoice_with_whitelisted_currency_succeeds` | `store_invoice` accepts whitelisted currency |
| `test_bid_on_invoice_with_non_whitelisted_currency_fails_when_whitelist_set` | `place_bid` enforces currency check at bid time |
| `test_add_currency_idempotent` | Duplicate `add_currency` does not grow the list |
| `test_remove_currency_when_missing_is_noop` | Second removal of absent currency succeeds (no-op) |
| `test_set_currencies_replaces_whitelist` | Old entries removed; new entries present after `set_currencies` |
| `test_set_currencies_deduplicates` | Duplicate addresses in input list stored once |
| `test_non_admin_cannot_set_currencies` | Non-admin `try_set_currencies` returns error |
| `test_clear_currencies_allows_all` | After clear: count = 0; any token accepted (backward-compat) |
| `test_non_admin_cannot_clear_currencies` | Non-admin `try_clear_currencies` returns error |
| `test_currency_count` | Count increments on add, decrements on remove |
| `test_get_whitelisted_currencies_paged` | Basic pagination: two pages over 3 items; out-of-range returns empty |

### Pagination boundary tests

| Test | Edge cases covered |
|---|---|
| `test_pagination_empty_whitelist_boundaries` | `(0,0)`, `(0,10)`, `(u32::MAX,10)`, `(0,u32::MAX)` on empty list |
| `test_pagination_offset_saturation` | Offset at len, len+1, `u32::MAX`, `u32::MAX-1`; len-1 returns 1 item |
| `test_pagination_limit_saturation` | Limit 0, limit > count, `u32::MAX`, exact, exact-1 |
| `test_pagination_overflow_protection` | `(u32::MAX, u32::MAX)`, large offset + normal limit, `offset + limit > u32::MAX` via `u32::MAX/2` combo |
| `test_pagination_consistency_and_ordering` | 7-item list; three pages reconstruct full list in order; overlapping page aligns correctly |
| `test_pagination_single_item_edge_cases` | `(0,1)` returns item; `(0,10)` returns item; `(1,1)` is empty; `(0,0)` is empty |
| `test_pagination_after_modifications` | Remove 2 of 5; paginated count matches new total; clear resets to empty |
| `test_pagination_security_boundaries` | Public read confirmed; paginated total matches full read; all items match |
| `test_pagination_large_dataset_boundaries` | 50-item set; page-by-page loop retrieves all 50; boundary and past-end offsets empty |
| `test_pagination_concurrent_modification_boundaries` | Remove 2 of 10; re-paginate; paginated count == `currency_count()` |
| `test_pagination_address_handling_boundaries` | Duplicate add stays at 1; 15 unique added; no duplicates in paginated output |
| `test_pagination_storage_efficiency` | Pagination correct at each of 20 growth steps; boundary empty at each step; clear resets |

## Supported Use Cases

### Stablecoin Whitelisting
Admin adds USDC, EURC, and other approved stablecoin addresses to the whitelist. Only these tokens can be used as invoice currency and for placing bids.

### Regulatory Compliance
Restrict invoice creation and bidding to pre-approved token addresses that meet regulatory requirements.

### Risk Management
Limit exposure to specific token types by maintaining a curated whitelist of acceptable currencies.

### Gradual Rollout
Start with empty whitelist (allow-all) for testing, then progressively add approved currencies for production use.