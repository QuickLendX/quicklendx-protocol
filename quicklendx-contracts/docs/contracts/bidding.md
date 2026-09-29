# Bidding Documentation

## Overview

This document describes the bidding mechanism in the QuickLendX protocol, focusing on bid validation rules and protocol minimum enforcement as implemented in issue #719.

## Bid Validation

### Bid Structure

A bid in QuickLendX contains the following key components:

```rust
pub struct Bid {
    pub bid_id: BytesN<32>,           // Unique identifier for the bid
    pub invoice_id: BytesN<32>,        // Associated invoice
    pub investor: Address,               // Investor address
    pub bid_amount: i128,              // Amount being bid
    pub expected_return: i128,           // Expected return for investor
    pub timestamp: u64,                // Bid creation time
    pub expiration_timestamp: u64,       // Bid expiration time
    pub status: BidStatus,              // Current bid status
}
```

### Bid Status Lifecycle

```rust
pub enum BidStatus {
    Placed,     // Bid is active and can be accepted
    Withdrawn,  // Bid was withdrawn by investor
    Accepted,    // Bid was accepted and investment created
    Expired,     // Bid expired without acceptance
}
```

## Protocol Minimum Enforcement

### Minimum Bid Calculation

The protocol enforces a minimum bid amount using both absolute and percentage-based constraints:

```rust
let percent_min = invoice.amount
    .saturating_mul(limits.min_bid_bps as i128)
    .saturating_div(10_000);

let effective_min_bid = if percent_min > limits.min_bid_amount {
    percent_min                    // Use percentage minimum if higher
} else {
    limits.min_bid_amount           // Use absolute minimum otherwise
};
```

### Protocol Limits

The protocol maintains configurable minimum bid parameters:

```rust
pub struct ProtocolLimits {
    pub min_bid_amount: i128,    // Absolute minimum bid amount
    pub min_bid_bps: u32,       // Minimum bid as percentage (basis points)
    // ... other limits
}
```

**Default Values:**
- `min_bid_amount`: 10 (smallest unit)
- `min_bid_bps`: 100 (1% of invoice amount)

### Validation Rules

#### 1. Amount Validation
- **Non-zero**: Bid amount must be > 0
- **Minimum Enforcement**: Must meet or exceed `effective_min_bid`
- **Invoice Cap**: Cannot exceed total invoice amount

#### 2. Invoice Status Validation
- **Verified Only**: Invoice must be in `Verified` status
- **Not Expired**: Invoice due date must be in the future

#### 3. Ownership Validation
- **No Self-Bidding**: Business cannot bid on own invoices
- **Authorization**: Only verified investors can place bids

#### 4. Investor Capacity
- **Investment Limits**: Total active bids cannot exceed investor's verified limit
- **Risk Assessment**: Bid amount considered against investor's risk profile

#### 5. Bid Protection
- **Single Active Bid**: One investor cannot have multiple active bids on same invoice
- **Expiration Handling**: Expired bids are automatically cleaned up

## Bid Placement Flow

### 1. Pre-Validation
```rust
validate_bid(env, invoice, bid_amount, expected_return, investor)?;
```

### 2. Bid Creation
```rust
let bid_id = BidStorage::generate_unique_bid_id(env);
let bid = Bid {
    bid_id,
    invoice_id,
    investor,
    bid_amount,
    expected_return,
    timestamp: env.ledger().timestamp(),
    expiration_timestamp: env.ledger().timestamp() + bid_ttl_seconds,
    status: BidStatus::Placed,
};
```

### 3. Storage
```rust
BidStorage::store_bid(env, &bid);
BidStorage::add_bid_to_invoice_index(env, &invoice_id, &bid_id);
BidStorage::add_bid_to_investor_index(env, &investor, &bid_id);
```

## Bid Selection and Ranking

### Best Bid Selection
The protocol selects the best bid based on:

1. **Profit Priority**: Higher expected return (profit = expected_return - bid_amount)
2. **Return Amount**: Higher expected return if profit equal
3. **Bid Amount**: Higher bid amount if profit and return equal
4. **Timestamp**: Newer bids preferred (deterministic tiebreaker)
5. **Bid ID**: Final stable tiebreaker

### Ranking Algorithm
```rust
pub fn compare_bids(bid1: &Bid, bid2: &Bid) -> Ordering {
    let profit1 = bid1.expected_return.saturating_sub(bid1.bid_amount);
    let profit2 = bid2.expected_return.saturating_sub(bid2.bid_amount);
    
    // Compare by profit, then return, then amount, then timestamp, then bid_id
    // This ensures reproducible ranking across all validators
}
```

## Security Considerations

### Reentrancy Protection
- All bid modifications require proper authorization
- State transitions are atomic
- External calls minimized during validation

### Access Control
- **Investor Authorization**: `investor.require_auth()` for bid placement
- **Business Authorization**: `business.require_auth()` for invoice operations
- **Admin Authorization**: `AdminStorage::require_admin()` for protocol changes

### Input Validation
- **Amount Bounds**: Prevents overflow and underflow
- **Timestamp Validation**: Ensures logical time progression
- **Address Validation**: Prevents invalid address usage

## Gas Optimization

### Efficient Storage
- **Indexed Storage**: Fast lookup by invoice, investor, and status
- **Batch Operations**: Multiple bids processed in single transaction
- **Cleanup Routines**: Automatic removal of expired bids

### Minimal Computations
- **Saturating Arithmetic**: Prevents overflow without expensive checks
- **Lazy Evaluation**: Calculations deferred until needed
- **Constants**: Pre-computed values where possible

## Testing Coverage

### Unit Tests
- **Validation Logic**: All validation rules tested
- **Edge Cases**: Boundary conditions and error scenarios
- **Protocol Limits**: Custom limit configurations tested
- **Integration Tests**: End-to-end bid placement flows

### Test Coverage Requirements
- **95% Coverage**: All bid validation paths tested
- **Error Paths**: All error conditions validated
- **Success Paths**: All valid bid scenarios covered
- **Edge Cases**: Boundary values and special conditions

## Event Emission

### Bid Events
```rust
// Events emitted during bid lifecycle
emit_bid_placed(env, &bid_id, &invoice_id, &investor, bid_amount);
emit_bid_accepted(env, &bid_id, &invoice_id, &investor, bid_amount);
emit_bid_withdrawn(env, &bid_id, &invoice_id, &investor, bid_amount);
emit_bid_expired(env, &bid_id, &invoice_id, &investor, bid_amount);
```

### Audit Trail
- All bid state transitions are logged
- Timestamps recorded for all operations
- Authorization verified for all state changes

## Configuration

### Bid TTL (Time-To-Live)
- **Default**: 7 days from placement
- **Configuration**: Set by admin via `set_bid_ttl`
- **Cleanup**: Automatic expiration and status updates

### Maximum Active Bids
- **Default**: 10 per investor
- **Purpose**: Prevents spam and manages risk
- **Enforcement**: Checked during bid placement

## Migration Notes

### Backward Compatibility
- Existing bids remain valid under previous rules
- New protocol limits apply to future bids
- Storage format unchanged for existing data

### Upgrade Path
- Protocol limits can be updated by admin
- Bid validation logic can be enhanced without breaking changes
- New bid statuses can be added via enum extension

## Best Practices

### For Investors
- **Due Diligence**: Verify invoice details before bidding
- **Risk Management**: Don't exceed investment capacity
- **Timing**: Place bids well before expiration
- **Monitoring**: Track active bids and their status

### For Businesses
- **Verification**: Ensure invoice is verified before expecting bids
- **Terms**: Clear payment terms and due dates
- **Communication**: Respond to appropriate bids promptly

### For Protocol Developers
- **Validation**: Centralize bid validation logic
- **Testing**: Comprehensive test coverage for all scenarios
- **Documentation**: Clear NatSpec comments for all functions
- **Security**: Regular security audits of bid logic
