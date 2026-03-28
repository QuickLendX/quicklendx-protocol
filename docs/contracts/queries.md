# Query Hard Caps and Resilience

> **Module:** `quicklendx-contracts/src/lib.rs` — query endpoints
> **Tests:** `quicklendx-contracts/src/test_queries.rs`, `quicklendx-contracts/src/test_limit.rs`

---

## Overview

All query endpoints in the QuickLendX protocol are designed to handle missing
or non-existent records gracefully — returning `None`, an empty `Vec`, or a
typed `Err` rather than panicking or producing inconsistent results.

Additionally, all paginated endpoints enforce strict **hard caps** on query limits
to prevent resource abuse and ensure predictable performance characteristics.

---

## MAX_QUERY_LIMIT Hard Cap Enforcement

### Security Guarantee

**All paginated endpoints enforce a hard cap of `MAX_QUERY_LIMIT = 100` records per query.**

This limit cannot be bypassed by:
- Passing `limit > MAX_QUERY_LIMIT` (automatically capped)
- Using overflow attacks with large offset values (validated and rejected)
- Combining parameters to exceed resource bounds (comprehensive validation)

### Validation Rules

1. **Limit Capping**: `limit` parameter is automatically capped using `limit.min(MAX_QUERY_LIMIT)`
2. **Overflow Protection**: Offset values that could cause `offset + MAX_QUERY_LIMIT` to overflow are rejected
3. **Empty Results**: Invalid parameters return empty results rather than errors
4. **Zero Limit Handling**: `limit=0` returns empty results (not an error)

### Implementation

```rust
/// Maximum number of records returned by paginated query endpoints.
pub(crate) const MAX_QUERY_LIMIT: u32 = 100;

/// Validates and caps query limit to prevent resource abuse
#[inline]
fn cap_query_limit(limit: u32) -> u32 {
    limit.min(MAX_QUERY_LIMIT)
}

/// Validates query parameters for security and resource protection
fn validate_query_params(offset: u32, limit: u32) -> Result<(), QuickLendXError> {
    // Check for potential overflow in offset + limit calculation
    if offset > u32::MAX - MAX_QUERY_LIMIT {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}
```

---

## Paginated Endpoints with Hard Cap Enforcement

| Endpoint | Hard Cap Applied | Validation |
|---|---|---|
| `get_business_invoices_paged` | ✅ MAX_QUERY_LIMIT | ✅ Overflow protection |
| `get_investor_investments_paged` | ✅ MAX_QUERY_LIMIT | ✅ Overflow protection |
| `get_available_invoices_paged` | ✅ MAX_QUERY_LIMIT | ✅ Overflow protection |
| `get_bid_history_paged` | ✅ MAX_QUERY_LIMIT | ✅ Overflow protection |
| `get_investor_bids_paged` | ✅ MAX_QUERY_LIMIT | ✅ Overflow protection |
| `get_whitelisted_currencies_paged` | ✅ MAX_QUERY_LIMIT | ✅ Overflow protection |

---

## Resilience Guarantees by Endpoint

| Endpoint | Missing record behaviour |
|---|---|
| `get_invoice(id)` | Returns `Err(InvoiceNotFound)` |
| `get_bid(id)` | Returns `None` |
| `get_investment(id)` | Returns `Err(StorageKeyNotFound)` |
| `get_invoice_investment(id)` | Returns `Err(StorageKeyNotFound)` |
| `get_bids_for_invoice(id)` | Returns empty `Vec` |
| `get_best_bid(id)` | Returns `None` |
| `get_ranked_bids(id)` | Returns empty `Vec` |
| `get_bids_by_status(id, status)` | Returns empty `Vec` |
| `get_bids_by_investor(id, investor)` | Returns empty `Vec` |
| `get_all_bids_by_investor(investor)` | Returns empty `Vec` |
| `get_business_invoices(business)` | Returns empty `Vec` |
| `get_investments_by_investor(investor)` | Returns empty `Vec` |
| `get_escrow_details(id)` | Returns `Err(StorageKeyNotFound)` |
| `get_bid_history_paged(id, ...)` | Returns empty `Vec` (capped) |
| `get_investor_bids_paged(investor, ...)` | Returns empty `Vec` (capped) |
| `cleanup_expired_bids(id)` | Returns `0` |

---

## Security Assumptions

- **Hard cap enforcement**: No query endpoint can return more than `MAX_QUERY_LIMIT` records
- **Overflow protection**: All pagination arithmetic uses overflow-safe operations
- **No panics**: No query endpoint panics on missing input — all storage lookups use `Option`
  returns (`get` returning `None`) which are handled before unwrapping
- **Authorization-free**: Query endpoints are read-only and require no authorization — they cannot
  mutate state
- **Information isolation**: Missing records never leak information about other records
- **Resource bounds**: Query execution time and memory usage are bounded by `MAX_QUERY_LIMIT`

---

## Test Coverage

### Comprehensive Hard Cap Tests (`test_limit.rs`)

- **Limit=0 scenarios**: All endpoints return empty results
- **Limit > MAX_QUERY_LIMIT**: All endpoints cap to MAX_QUERY_LIMIT
- **Large offset scenarios**: Offsets beyond data return empty results
- **Overflow protection**: Dangerous offset values are safely handled
- **Pagination consistency**: Multi-page results maintain order and completeness
- **Edge cases**: Exactly MAX_QUERY_LIMIT items, extreme values

### Integration Tests (`test_queries.rs`)

- **Cross-endpoint validation**: Consistent behavior across all paginated endpoints
- **Parameter validation**: Edge cases for offset/limit combinations
- **Data consistency**: No duplicates or missing items across pagination

---

## Running Tests
```bash
cd quicklendx-contracts
cargo test test_queries
cargo test test_limit
```
