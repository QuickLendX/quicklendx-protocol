# Bidding System - Expired Bid Cleanup & Index Safety

## Overview

The QuickLendX bidding system implements a multi-stage cleanup architecture to manage bid lifecycle efficiently while preventing storage bloat and DoS attacks. This document details the expired bid cleanup mechanism, index safety guarantees, and idempotency properties.

## Architecture

### Three-Level Cleanup Strategy

```
┌─────────────────────────────────────────────────────────────┐
│ Bid Lifecycle: Placement → Expiration → Cleanup → Removal   │
├─────────────────────────────────────────────────────────────┤
│ Level 1: Invoice Index (per-invoice bid list)               │
│   └─ cleanup_expired_bids() → refresh_expired_bids()         │
│   └─ Prunes expired bids; preserves terminals                │
│                                                              │
│ Level 2: Investor Index (per-investor bid list)             │
│   └─ refresh_investor_bids()                                 │
│   └─ Maintains active bid count for rate limiting            │
│                                                              │
│ Level 3: Bid Records (status transitions)                   │
│   └─ Placed → Expired (on expiration time)                   │
│   └─ Placed → Accepted/Withdrawn/Cancelled (terminal)       │
└─────────────────────────────────────────────────────────────┘
```

### Key Invariants

#### Invariant 1: Terminal Bids Never Pruned
- **Definition**: Bids in `Accepted`, `Withdrawn`, or `Cancelled` status are NEVER modified or removed by cleanup
- **Rationale**: Terminal statuses represent finalized business logic outcomes that must remain auditable
- **Enforcement**: `refresh_expired_bids()` explicitly checks `is_terminal` before considering pruning
- **Impact**: Enables safe audit trails; historical bid records remain accessible

#### Invariant 2: Active Placed Bids Preserved Until Expiration
- **Definition**: Bids in `Placed` status with timestamp ≤ expiration are kept in the invoice index
- **Rationale**: Prevents loss of valid, active bidding opportunities
- **Enforcement**: `bid.is_expired(current_timestamp)` returns false for non-expired bids
- **Impact**: Ensures bidders can always see and interact with their open bids

#### Invariant 3: Expired Bids Are Pruned or Marked
- **Definition**: Bids in `Placed` status with timestamp > expiration are transitioned to `Expired` and removed from indexes
- **Rationale**: Prevents unbounded index growth; keeps iteration O(N) where N is active bids
- **Enforcement**: Loop checks `bid.is_expired()` and marks status transition + emits event
- **Impact**: Storage remains bounded; cleanup scales predictably

#### Invariant 4: Idempotency
- **Definition**: Calling cleanup multiple times on the same ledger state produces identical results
- **Rationale**: Safe to call cleanup opportunistically without coordinating with other processes
- **Enforcement**: 
  - First call transitions `Placed` → `Expired` and removes from index
  - Second call finds 0 expired bids (all already pruned)
  - No state changes on subsequent calls
- **Impact**: Cleanup can be triggered by any on-chain operation without risk

#### Invariant 5: Bounded, Deterministic Cleanup
- **Definition**: Cleanup cost is O(N) where N ≤ MAX_BIDS_PER_INVOICE; result is deterministic
- **Rationale**: Prevents DoS via unbounded state traversal
- **Enforcement**: Single loop through bid_ids with no recursive calls
- **Impact**: Gas cost is predictable; max cost is fixed per invoice

## Cleanup Functions

### Public API

```rust
/// Trigger cleanup of expired bids for a specific invoice.
///
/// Returns: Count of bids cleaned (transitioned or removed)
/// Idempotent: Multiple calls safe; second call returns 0
pub fn cleanup_expired_bids(env: &Env, invoice_id: &BytesN<32>) -> u32
```

**Usage Example**:
```rust
// Triggered during accept_bid flow to clean up before counting active bids
let cleaned_count = BidStorage::cleanup_expired_bids(&env, &invoice_id);

// Safe to call again later (idempotent)
let cleaned_again = BidStorage::cleanup_expired_bids(&env, &invoice_id);
assert_eq!(cleaned_again, 0); // Nothing left to clean
```

### Internal Functions

#### `refresh_expired_bids(env, invoice_id) → u32`
**Purpose**: Scan invoice's bid list, transition expired Placed bids to Expired, prune from index

**Algorithm**:
```
1. Get current ledger timestamp
2. Retrieve all bid IDs for invoice
3. Initialize empty "active" bid list
4. For each bid:
   a. Load bid record
   b. If terminal (Accepted/Withdrawn/Cancelled): keep in index
   c. Else if Placed and not expired: keep in index
   d. Else if Placed and expired: mark Expired, emit event, do NOT keep in index
   e. Else if already Expired or orphaned: do NOT keep in index
5. Replace invoice bid list with "active"
6. Return count of removed bids
```

**Properties**:
- Time: O(N) where N = bids on invoice
- Space: O(N) for temporary vector
- Storage writes: 1 (index list update, if changed)
- Idempotent: Second call finds 0 to remove

#### `refresh_investor_bids(env, investor) → u32`
**Purpose**: Scan investor's global bid list, prune expired bids from index

**Usage**: Called internally when checking investor bid limits

**Difference from `refresh_expired_bids`**:
- Operates on all bids across all invoices (investor scope)
- May return higher cleanup count due to broader scope
- Used for rate limiting; ensures active bid count is accurate

## Index Structure

### Invoice Bid Index
```
Key: (symbol_short!("bids"), invoice_id)
Value: Vec<BytesN<32>>  // Bid IDs

Invariant: After cleanup, contains only:
  - Placed bids that are NOT expired
  - Terminal bids (Accepted, Withdrawn, Cancelled)
  
Does NOT contain:
  - Expired bids (pruned)
  - Orphaned IDs (pruned)
```

### Investor Bid Index
```
Key: (symbol_short!("bid_inv"), investor)
Value: Vec<BytesN<32>>  // All bid IDs

Invariant: After refresh_investor_bids, contains only:
  - Placed bids that are NOT expired
  - Terminal bids (Accepted, Withdrawn, Cancelled)
  
Does NOT contain:
  - Expired bids (pruned)
```

### Bid Status States
```
Placed → (Expired | Accepted | Withdrawn | Cancelled)
  ↓
  Only Placed can expire
  Terminal states are permanent
```

## Security Analysis

### DoS Prevention

**Attack Vector**: Unbounded bid accumulation on single invoice

**Defense**:
1. **Bounded Index Size**: `MAX_BIDS_PER_INVOICE = 50` caps invoice bid list
2. **Cleanup Efficiency**: O(N) algorithm; max N = 50
3. **Deterministic Cost**: No conditional allocations or recursive calls
4. **Auto-Pruning**: Expired bids removed immediately upon cleanup

**Example**: Even with 50 bids on an invoice, cleanup performs at most 50 comparisons and 1 storage write.

### Index Corruption Prevention

**Mechanism**: Terminal bids are protected by explicit status checks

```rust
let is_terminal = bid.status == BidStatus::Accepted
    || bid.status == BidStatus::Withdrawn
    || bid.status == BidStatus::Cancelled;

if is_terminal {
    active.push_back(bid_id);  // Preserve in index
} else if bid.status == BidStatus::Placed && bid.is_expired(...) {
    // Transition and remove
    bid.status = BidStatus::Expired;
    // ...
} else if bid.status == BidStatus::Placed {
    // Keep active Placed bid
    active.push_back(bid_id);
}
```

**Guarantee**: A bid in `Accepted` status cannot be accidentally removed because the condition `is_terminal` triggers before the expiration check.

### Atomicity & Consistency

**Properties**:
- Cleanup is a single transaction; partial states not observable
- Index updates are atomic; no torn reads possible
- Status transitions are idempotent; re-running has no side effects

## Test Coverage

The test suite (`test_expired_bids_cleanup.rs`) validates:

### 1. Cleanup Only Prunes Expired Bids (3 tests)
- ✅ Active bids preserved
- ✅ Expired bids pruned and transitioned
- ✅ Already-expired bids pruned without re-transition

### 2. Index Integrity & Terminal Preservation (2 tests)
- ✅ Accepted bids never pruned
- ✅ Withdrawn bids never pruned
- ✅ Cancelled bids never pruned
- ✅ Mixed status handling correct

### 3. Idempotency (3 tests)
- ✅ Multiple cleanups on expired bids
- ✅ Idempotency with mixed bid ages
- ✅ Terminal bids always remain

### 4. Edge Cases (3 tests)
- ✅ Empty invoice (no bids)
- ✅ All bids expired
- ✅ No bids expired

### 5. DoS Prevention (2 tests)
- ✅ Linear scaling O(N)
- ✅ Accurate cleanup count reporting

### 6. Investor Index (1 test)
- ✅ Investor index pruned of expired bids

### 7. Integration (1 test)
- ✅ Multiple invoices, investors, comprehensive scenario

**Total: 15 comprehensive tests**

## Example: Full Cleanup Lifecycle

```
Scenario: Invoice with 5 bids, 3 expire, 1 accepted, 1 still active
─────────────────────────────────────────────────────────────

Initial State (Time T0):
  Invoice bid_ids = [bid_A, bid_B, bid_C, bid_D, bid_E]
  bid_A: Placed, expires at T0 + 7d
  bid_B: Placed, expires at T0 + 7d
  bid_C: Placed, expires at T0 + 7d
  bid_D: Accepted (terminal)
  bid_E: Placed, expires at T0 + 7d

After 7 days + 1 second (Time T0 + 7d + 1s):
  env.ledger().timestamp() = T0 + 7d + 1s
  
Call: cleanup_expired_bids(&env, &invoice_id)
  
  Result:
    ✓ bid_A: Placed → Expired (pruned from index)
    ✓ bid_B: Placed → Expired (pruned from index)
    ✓ bid_C: Placed → Expired (pruned from index)
    ✓ bid_D: Accepted → Accepted (PRESERVED in index)
    ✓ bid_E: Placed → Expired (pruned from index)
    
  returned: 4 (cleaned)
  New invoice bid_ids = [bid_D]

Call: cleanup_expired_bids(&env, &invoice_id) [again]
  Result:
    ✓ bid_D: Accepted → Accepted (terminal, preserved)
    returned: 0 (idempotent; nothing new to clean)
```

## Configuration & Constants

```rust
pub const DEFAULT_BID_TTL_DAYS: u64 = 7;  // Bid lifetime
pub const MIN_BID_TTL_DAYS: u64 = 1;     // Minimum TTL
pub const MAX_BID_TTL_DAYS: u64 = 30;    // Maximum TTL
pub const MAX_BIDS_PER_INVOICE: u32 = 50; // Index size cap
pub const SECONDS_PER_DAY: u64 = 86400;

// Admin-configurable:
pub const DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR: u32 = 20;
```

## Integration Points

### Called During
1. **Bid Acceptance** (`accept_bid`): Cleanup before counting active bids
2. **Bid Placement** (`place_bid`): Cleanup to free slots if at MAX_BIDS
3. **Invoice Retrieval** (`get_bid_records_for_invoice`): Cleanup before returning records
4. **Off-chain Indexing** (optional): Proactive cleanup to optimize storage

### Effects
- Reduces invoice bid index size
- Updates bid statuses in storage
- Emits `BidExpired` events for monitoring
- Ensures accurate active bid counts for rate limits

## Best Practices

### For Off-Chain Indexers
```rust
// Safe to call periodically (idempotent)
loop {
    for invoice_id in get_all_invoice_ids() {
        let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
        if cleaned > 0 {
            log!("Cleaned {} expired bids from invoice", cleaned);
        }
    }
    sleep(Duration::from_secs(3600)); // Every hour
}
```

### For Contract Developers
```rust
// Always cleanup before counting active bids
pub fn place_bid(...) -> Result<BytesN<32>, Error> {
    let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    let active_count = BidStorage::get_active_bid_count(&env, &invoice_id);
    
    // Now active_count is accurate
    if active_count >= MAX_BIDS_PER_INVOICE {
        return Err(Error::MaxBidsReached);
    }
    
    // Proceed with bid placement...
}
```

## Summary

The expired bid cleanup system provides:

| Property | Guarantee |
|----------|-----------|
| **Correctness** | Only expired Placed bids are pruned; terminals preserved |
| **Efficiency** | O(N) cleanup where N ≤ MAX_BIDS_PER_INVOICE |
| **Safety** | Idempotent; safe to call multiple times |
| **DoS Resistance** | Bounded iteration; predictable gas cost |
| **Auditability** | Terminal bids always accessible for history |

This design ensures the bidding system scales safely while maintaining protocol invariants and security assumptions.
