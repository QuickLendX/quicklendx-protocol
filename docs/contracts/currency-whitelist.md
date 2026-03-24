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

### Write Operations
- Every write requires `admin.require_auth()` + admin storage verification
- No user can modify the whitelist without proper admin credentials
- Use `set_currencies` for bulk updates to avoid partial state inconsistencies

### Read Operations
- Pagination queries are public and require no authentication
- No DoS risk from pagination due to bounded reads and overflow protection
- Consistent results across multiple pagination calls
- No information leakage beyond intended whitelist data

### Boundary Safety
- All arithmetic operations use overflow-safe methods
- Large offset/limit values handled gracefully without panics
- Empty whitelist scenarios handled consistently
- Memory usage bounded by actual result size, not input parameters

## Error Conditions

| Error | Cause | Mitigation |
|-------|-------|------------|
| `NotAdmin` | Caller is not the registered admin | Ensure proper admin authentication |
| `InvalidCurrency` | Token not in whitelist (when whitelist is non-empty) | Add currency to whitelist or use allowed currency |

## Testing Coverage

Comprehensive boundary tests ensure robust behavior:

### Empty Whitelist Tests
- Various offset/limit combinations with empty whitelist
- Consistency with `currency_count()` returning 0
- Proper handling of maximum parameter values

### Offset Saturation Tests  
- Offset at exact whitelist length boundary
- Offset beyond whitelist length
- Maximum u32 offset values
- Near-maximum offset with various limits

### Limit Saturation Tests
- Zero limit behavior
- Limit larger than available items
- Maximum u32 limit values
- Limit exactly matching available items

### Overflow Protection Tests
- Offset + limit arithmetic overflow scenarios
- Maximum value combinations for both parameters
- Boundary arithmetic edge cases

### Consistency Tests
- Ordering preservation across pagination calls
- No duplicate items across non-overlapping pages
- Consistent results between full and paginated reads

### Modification Tests
- Pagination behavior after adding/removing currencies
- Boundary changes after whitelist modifications
- Clear operation effects on pagination

### Performance Tests
- Large dataset pagination efficiency
- Memory usage with various page sizes
- Iteration patterns across large whitelists

### Security Tests
- Public read access verification
- No information leakage beyond bounds
- Consistent behavior across multiple calls
- No side effects from read operations

## Supported Use Cases

### Stablecoin Whitelisting
Admin adds USDC, EURC, and other approved stablecoin addresses to the whitelist. Only these tokens can be used as invoice currency and for placing bids.

### Regulatory Compliance
Restrict invoice creation and bidding to pre-approved token addresses that meet regulatory requirements.

### Risk Management
Limit exposure to specific token types by maintaining a curated whitelist of acceptable currencies.

### Gradual Rollout
Start with empty whitelist (allow-all) for testing, then progressively add approved currencies for production use.