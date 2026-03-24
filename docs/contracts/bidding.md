# Bidding System Documentation

## Overview

The QuickLendX protocol implements a competitive bidding system where verified investors can place bids on verified invoices. The system ensures fair competition, ranking, and bounded storage through various mechanisms including a maximum bids per invoice limit.

## Key Features

### Maximum Bids Per Invoice Limit

To ensure system performance and bounded storage, the protocol enforces a maximum limit on the number of active bids per invoice.

#### Configuration

- **Default Limit**: 50 active bids per invoice
- **Constant**: `MAX_BIDS_PER_INVOICE` in `src/bid.rs`
- **Error**: `MaxBidsPerInvoiceExceeded` (error code 1406)

#### Behavior

1. **Active Bid Counting**: Only bids with `Placed` status count towards the limit
2. **Automatic Cleanup**: Expired bids are automatically removed before counting
3. **Dynamic Limiting**: When bids are withdrawn or accepted, new bids can be placed
4. **Real-time Enforcement**: The limit is checked during `place_bid` execution

#### Implementation Details

```rust
// Check if maximum bids per invoice limit is reached
let active_bid_count = BidStorage::get_active_bid_count(&env, &invoice_id);
if active_bid_count >= bid::MAX_BIDS_PER_INVOICE {
    return Err(QuickLendXError::MaxBidsPerInvoiceExceeded);
}
```

#### Bid Status Impact on Limit

| Status | Counts Towards Limit | Notes |
|--------|---------------------|-------|
| Placed | ✅ Yes | Active competitive bids |
| Accepted | ❌ No | Bid accepted, no longer competing |
| Withdrawn | ❌ No | Investor removed their bid |
| Expired | ❌ No | Automatically cleaned up |

## Bidding Process

### 1. Prerequisites

- **Invoice**: Must be in `Verified` status
- **Investor**: Must have completed KYC and be verified
- **Investment Limit**: Bid amount cannot exceed investor's verified limit

### 2. Bid Placement

```rust
pub fn place_bid(
    env: Env,
    investor: Address,
    invoice_id: BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> Result<BytesN<32>, QuickLendXError>
```

#### Validations

1. **Invoice Status**: Must be `Verified`
2. **Investor Verification**: Must be verified and not pending/rejected
3. **Investment Limit**: Bid amount ≤ investor's verified limit
4. **Bid Amount**: Must be positive and within invoice amount range
5. **Expected Return**: Must exceed bid amount
6. **Max Bids Limit**: Cannot exceed 50 active bids per invoice
7. **Duplicate Prevention**: Same investor cannot have multiple active bids

#### Bid Expiration

- **Default TTL**: 7 days (604,800 seconds)
- **Expiration Timestamp**: Calculated as `current_timestamp + DEFAULT_BID_TTL`
- **Automatic Cleanup**: Expired bids are marked as `Expired` and removed from active counting

### 3. Bid Ranking

Bids are ranked based on the following priority:

1. **Profit Margin**: Higher `expected_return - bid_amount`
2. **Expected Return**: Higher total return (if profit margins equal)
3. **Bid Amount**: Higher bid amount (if returns equal)
4. **Timestamp**: Earlier bid wins (if all else equal)

### 4. Bid Acceptance

- **Authorization**: Only the invoice business owner can accept bids
- **Escrow Creation**: Automatically creates escrow for the accepted bid amount
- **Status Changes**: 
  - Bid status changes to `Accepted`
  - Invoice status changes to `Funded`
  - Investment record is created

## Security Considerations

### Rate Limiting

The max bids per invoice limit serves as a rate limiting mechanism to prevent:

- **Storage Bloat**: Unbounded growth of bid data
- **Performance Issues**: Excessive computation during ranking
- **Spam Prevention**: Malicious actors from overwhelming the system

### Access Control

- **Investor Authentication**: Only verified investors can place bids
- **Business Authorization**: Only business owners can accept bids
- **Admin Functions**: Certain operations require admin privileges

### Data Integrity

- **Unique Bid IDs**: Generated using timestamp and counter
- **Immutable History**: Bid status changes are tracked
- **Audit Trail**: All bid operations are logged

## Error Handling

### Common Errors

| Error | Code | Cause | Resolution |
|-------|------|-------|------------|
| `MaxBidsPerInvoiceExceeded` | 1406 | More than 50 active bids | Wait for bids to expire/withdraw/accept |
| `BusinessNotVerified` | 1600 | Investor not verified | Complete KYC process |
| `InvalidAmount` | 1200 | Bid amount invalid | Check amount limits |
| `InvalidStatus` | 1401 | Invoice not verified | Verify invoice first |
| `OperationNotAllowed` | 1402 | Duplicate active bid | Withdraw existing bid first |

## Testing

### Comprehensive Test Coverage

The implementation includes extensive tests covering:

- **Limit Enforcement**: Verifying 50-bid maximum
- **Dynamic Behavior**: Testing bid withdrawal/acceptance impacts
- **Expiration Handling**: Verifying expired bids don't count
- **Edge Cases**: Boundary conditions and error scenarios

### Test Example

```rust
#[test]
fn test_max_bids_per_invoice_limit() {
    // Creates 55 verified investors
    // Places 50 bids (should succeed)
    // Attempts 51st bid (should fail with MaxBidsPerInvoiceExceeded)
    // Withdraws bid, places new bid (should succeed)
    // Accepts bid, places another bid (should succeed)
    // Tests expiration cleanup and new bid placement
}
```

## Future Enhancements

### Potential Improvements

1. **Configurable Limits**: Allow protocol admins to adjust the limit
2. **Tiered Limits**: Different limits based on invoice amount
3. **Time-based Limits**: Rate limiting per time window
4. **Priority Queues**: Allow exceeding limit with higher priority bids

### Monitoring

Consider implementing metrics for:

- **Bid Velocity**: Rate of bid placement per invoice
- **Limit Utilization**: Percentage of invoices hitting the limit
- **Expiration Rate**: Frequency of bid expirations
- **Competition Metrics**: Average bids per successful invoice

## Integration Notes

### Frontend Considerations

- Display remaining bid slots to investors
- Show real-time bid count updates
- Provide clear error messages for limit violations
- Implement bid expiration timers

### API Integration

- Handle `MaxBidsPerInvoiceExceeded` errors gracefully
- Implement retry logic for temporary limit conditions
- Monitor bid counts for user experience optimization

## Conclusion

The maximum bids per invoice limit is a critical security and performance feature that ensures the QuickLendX protocol remains scalable and efficient while maintaining fair competition among investors. The implementation provides robust error handling, comprehensive testing, and clear documentation for maintainability.

---

## Bid TTL Configuration (Issue #543)

### Overview

Bid TTL (Time-To-Live) controls how long a placed bid remains valid before it
automatically expires. The TTL is admin-configurable within a safe range to
prevent both zero-expiry bids (which expire immediately) and extreme windows
(which lock investor funds indefinitely).

### Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `DEFAULT_BID_TTL_DAYS` | 7 | Used when no admin override exists |
| `MIN_BID_TTL_DAYS` | 1 | Minimum allowed TTL (prevents zero-expiry) |
| `MAX_BID_TTL_DAYS` | 30 | Maximum allowed TTL (prevents extreme windows) |

### Configuration Functions

#### `set_bid_ttl_days(days: u64) → Result<u64, QuickLendXError>`

Admin-only. Sets the bid TTL in whole days.

- Rejects `0` and any value outside `[1, 30]` with `QuickLendXError::InvalidBidTtl`
- Returns the new TTL on success
- Emits `ttl_upd` event with old value, new value, and admin address
- Does **not** retroactively change existing bid expirations

#### `get_bid_ttl_days() → u64`

Returns the currently active TTL in days. Falls back to `DEFAULT_BID_TTL_DAYS`
(7) when no admin override has been set.

#### `get_bid_ttl_config() → BidTtlConfig`

Returns a full configuration snapshot:

```rust
pub struct BidTtlConfig {
    pub current_days: u64,  // active TTL (admin-set or default)
    pub min_days: u64,      // compile-time minimum (1)
    pub max_days: u64,      // compile-time maximum (30)
    pub default_days: u64,  // compile-time default (7)
    pub is_custom: bool,    // true when admin has set an override
}
```

#### `reset_bid_ttl_to_default() → Result<u64, QuickLendXError>`

Admin-only. Removes the stored override so `get_bid_ttl_days` returns
`DEFAULT_BID_TTL_DAYS` and `is_custom` becomes `false`. Idempotent — safe to
call when already at default.

### Expiration Arithmetic

When a bid is placed, its expiration timestamp is computed as:

```
expiration_timestamp = current_ledger_timestamp + (ttl_days × 86_400)
```

Arithmetic uses `saturating_mul` and `saturating_add` to prevent overflow on
extreme inputs.

### Security Assumptions

1. **Zero TTL is impossible** — `MIN_BID_TTL_DAYS = 1` ensures every bid has
   at least a 24-hour window, preventing bids that expire in the same block.
2. **Extreme TTL is impossible** — `MAX_BID_TTL_DAYS = 30` caps the maximum
   lock period, preventing investor funds from being tied up indefinitely.
3. **Existing bids are immutable** — a TTL update only affects bids placed
   after the update; existing bid expirations are never retroactively changed.
4. **Admin-only mutation** — `set_bid_ttl_days` and `reset_bid_ttl_to_default`
   require admin authentication via `AdminStorage::require_admin`.
5. **Deterministic default** — when no override exists, the fallback is always
   `DEFAULT_BID_TTL_DAYS` (compile-time constant), making behaviour
   predictable across contract upgrades.

### Test Coverage (test_bid_ttl.rs)

| Test | Scenario |
|------|----------|
| `test_default_ttl_is_seven_days` | Fresh contract returns 7-day default |
| `test_get_bid_ttl_config_defaults` | Config snapshot correct on fresh contract |
| `test_bid_uses_default_ttl_expiration` | Bid expiry = now + 7 days by default |
| `test_zero_ttl_rejected` | 0 days → `InvalidBidTtl` |
| `test_below_minimum_ttl_rejected` | Sub-minimum → `InvalidBidTtl` |
| `test_above_maximum_ttl_rejected` | 31 days → `InvalidBidTtl` |
| `test_extreme_large_ttl_rejected` | `u64::MAX` → `InvalidBidTtl` |
| `test_minimum_boundary_accepted` | 1 day accepted |
| `test_maximum_boundary_accepted` | 30 days accepted |
| `test_all_valid_ttl_values_accepted` | Every value in [1, 30] accepted |
| `test_config_is_custom_after_set` | `is_custom = true` after admin set |
| `test_get_bid_ttl_days_reflects_update` | `get_bid_ttl_days` returns new value |
| `test_bid_uses_updated_ttl` | Bid after update uses new TTL |
| `test_existing_bid_expiration_unchanged_after_ttl_update` | Existing bids unaffected |
| `test_bid_expiration_with_minimum_ttl` | Expiry = now + 1 day at min TTL |
| `test_bid_expiration_with_maximum_ttl` | Expiry = now + 30 days at max TTL |
| `test_bid_not_expired_before_ttl_boundary` | Bid still Placed 1s before expiry |
| `test_bid_expired_after_ttl_boundary` | Bid Expired 1s after TTL boundary |
| `test_reset_ttl_to_default` | Reset restores default, clears `is_custom` |
| `test_bid_uses_default_after_reset` | Bids use default TTL after reset |
| `test_reset_when_already_default_is_idempotent` | Reset at default is safe |
| `test_multiple_sequential_ttl_updates` | Sequential updates each take effect |
| `test_set_reset_set_cycle` | Set → reset → set cycle works correctly |
