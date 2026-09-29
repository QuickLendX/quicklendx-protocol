# Testing Patterns Analysis: Bid Ranking & TTL Tests

## Overview
Analysis of `test_bid_ranking.rs` and `test_bid_ttl.rs` to identify reusable testing patterns, helper functions, and best practices for bid-related functionality testing.

---

## 1. Test Structure Patterns

### 1.1 Minimal Unit Tests (test_bid_ranking.rs)
**Characteristics:**
- Direct environment setup with `Env::default()`
- Uses `testutils::{Address, Ledger}` from soroban_sdk
- Sets ledger timestamp explicitly for reproducible time-based behavior
- Tests single BidStorage operations in isolation
- No authentication mocking (simpler scope)

```rust
#[test]
fn rank_bids_orders_by_profit_and_expected_return() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let invoice = invoice_id(&env, 1);
    // ... test logic ...
}
```

### 1.2 Integration Tests (test_bid_ttl.rs)
**Characteristics:**
- Full contract client setup with authentication mocking
- Reusable `setup()` helper returning `(Env, Client, Admin)`
- Contract registration and initialization
- KYC verification workflow
- Token creation and approval
- Multiple phases: setup → configure → execute → assert

```rust
fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}
```

---

## 2. Helper Functions Catalog

### 2.1 Bid Creation Helpers (test_bid_ranking.rs)

#### `invoice_id(env: &Env, seed: u8) -> BytesN<32>`
**Purpose:** Generate deterministic invoice IDs for testing
- Seed-based generation for reproducibility
- All zeros except first byte
- Used to create multiple distinct invoices

#### `build_bid(env, invoice_id, investor, bid_amount, expected_return, timestamp, status, id_suffix) -> Bid`
**Purpose:** Construct a Bid struct with controlled parameters
**Key Features:**
- Deterministic bid_id generation using prefix (0xB1D0) + timestamp + suffix
- Expiration calculated as `timestamp + 604_800` (7 days in seconds)
- Customizable status (Placed, Cancelled, etc.)
- Parameters allow testing different bid economics:
  - `profit = expected_return - bid_amount`
  - `expected_return` as tiebreaker
  - `timestamp` as secondary tiebreaker
  - `id_suffix` as final tiebreaker

```rust
fn build_bid(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    bid_amount: i128,
    expected_return: i128,
    timestamp: u64,
    status: BidStatus,
    id_suffix: u8,
) -> Bid {
    let mut bid_id_bytes = [0u8; 32];
    bid_id_bytes[0] = 0xB1;
    bid_id_bytes[1] = 0xD0;
    bid_id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
    bid_id_bytes[30] = id_suffix;
    bid_id_bytes[31] = id_suffix;

    Bid {
        bid_id: BytesN::from_array(env, &bid_id_bytes),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount,
        expected_return,
        timestamp,
        status,
        expiration_timestamp: timestamp.saturating_add(604800),
    }
}
```

#### `persist_bid(env: &Env, bid: &Bid)`
**Purpose:** Store bid in contract storage with bidding list linkage
```rust
fn persist_bid(env: &Env, bid: &Bid) {
    BidStorage::store_bid(env, bid);
    BidStorage::add_bid_to_invoice(env, &bid.invoice_id, &bid.bid_id);
}
```

#### `assert_best_matches_first_ranked(env: &Env, invoice: &BytesN<32>)`
**Purpose:** Verify consistency between ranking and best bid selection
**Pattern:** Reusable assertion that validates:
- Best bid matches ranking at position 0
- Handles empty ranking gracefully
- Used in multiple tests to ensure consistency across different tie-breaking scenarios

```rust
fn assert_best_matches_first_ranked(env: &Env, invoice: &BytesN<32>) {
    let ranked = BidStorage::rank_bids(env, invoice);
    let best = BidStorage::get_best_bid(env, invoice);

    if ranked.len() == 0 {
        assert!(best.is_none());
        return;
    }

    let best_bid = best.expect("best bid must exist when ranking is non-empty");
    assert_eq!(best_bid.bid_id, ranked.get(0).unwrap().bid_id);
}
```

### 2.2 Contract Setup Helpers (test_bid_ttl.rs)

#### `make_token(env, contract_id, business, investor) -> Address`
**Purpose:** Create a Stellar Asset contract with pre-minted balances
**Workflow:**
1. Register token admin and currency
2. Mint to business and investor
3. Mint minimal amount to contract
4. Approve spending for both parties

```rust
fn make_token(env: &Env, contract_id: &Address, business: &Address, investor: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env.register_stellar_asset_contract_v2(token_admin).address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &100_000i128);
    sac.mint(investor, &100_000i128);
    sac.mint(contract_id, &1i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(business, contract_id, &400_000i128, &exp);
    tok.approve(investor, contract_id, &400_000i128, &exp);
    currency
}
```

#### `funded_setup(env, client, admin, amount) -> (Address, Address, BytesN<32>)`
**Purpose:** Complete setup workflow: KYC → Token → Invoice → Verification
**Workflow:**
1. Create business and investor addresses
2. Setup token with make_token()
3. Submit and verify business KYC
4. Submit and verify investor KYC (with limit)
5. Upload and verify invoice
6. Return (business, investor, invoice_id)

**Key Pattern:** Bundles all prerequisites for invoice/bid testing

---

## 3. BidStorage Testing Patterns

### 3.1 Ranking Tests
**Core Operations Tested:**
- `BidStorage::rank_bids(env, invoice_id)` → Vec<Bid>
- `BidStorage::get_best_bid(env, invoice_id)` → Option<Bid>
- `BidStorage::store_bid(env, bid)`
- `BidStorage::add_bid_to_invoice(env, invoice_id, bid_id)`

### 3.2 Ranking Validation Hierarchy
Tests validate tie-breaking order (in priority sequence):
1. **Profit** (expected_return - bid_amount): Higher is better
2. **Expected Return**: Higher is better (if profit ties)
3. **Bid Amount**: Higher is better (if expected_return ties)
4. **Timestamp**: Newer is better (if bid_amount ties)
5. **Bid ID**: Higher byte values win (final tiebreaker)

**Test Pattern:** Each tiebreaker level has dedicated test:
- `rank_bids_orders_by_profit_and_expected_return()`
- `rank_bids_prefers_newer_timestamp_on_full_tie()`
- `rank_bids_uses_bid_id_as_final_tiebreaker()`

### 3.3 Status Filtering
**Pattern:** Tests verify `Placed` status filtering
- Only `BidStatus::Placed` bids included in ranking
- `Cancelled`, `Expired`, other statuses excluded from ranking
- `get_best_bid()` respects same filtering as `rank_bids()`

**Example:**
```rust
#[test]
fn get_best_bid_aligns_with_ranking_and_filters_non_placed() {
    // ... setup bids with different statuses ...
    let ranked = BidStorage::rank_bids(&env, &invoice);
    assert_eq!(ranked.len(), 1);  // Only Placed bid
    
    let best = BidStorage::get_best_bid(&env, &invoice).unwrap();
    assert_eq!(best.bid_id, placed.bid_id);
}
```

### 3.4 Insertion Order Independence
**Pattern:** Tests verify deterministic ranking regardless of insertion order
```rust
for &order in &[0u8, 1u8] {
    // Test inserts bids in different orders
    // Validates ranking is deterministic
    assert_best_matches_first_ranked(&env, &invoice);
}
```

---

## 4. Time & Expiration Handling Patterns

### 4.1 Ledger Time Management
**Basic Pattern:**
```rust
env.ledger().with_mut(|li| li.timestamp = 1_000);
```

**Set/Update Pattern:**
```rust
env.ledger().set_timestamp(new_timestamp);
```

### 4.2 TTL (Time To Live) Configuration
**Constants Used:**
```rust
const SECONDS_PER_DAY: u64 = 86_400;
// DEFAULT_BID_TTL_DAYS = 7
// MIN_BID_TTL_DAYS = 1
// MAX_BID_TTL_DAYS = 30
```

**Expiration Calculation Pattern:**
```rust
expiration_timestamp = now + ttl_days * SECONDS_PER_DAY
```

### 4.3 Expiration Testing Patterns

#### Boundary Testing
**Not Expired (before boundary):** Test at `expiration_timestamp - 1`
**Expired (after boundary):** Test at `expiration_timestamp + 1`

```rust
// One second before expiry
env.ledger().set_timestamp(env.ledger().timestamp() + SECONDS_PER_DAY - 1);
// Bid must still be Placed

// One second after expiry
env.ledger().set_timestamp(env.ledger().timestamp() + SECONDS_PER_DAY + 1);
client.cleanup_expired_bids(&invoice_id);
// Bid must be Expired
```

#### TTL Update Semantics
**Pattern:** Verify TTL updates apply only to new bids
```rust
#[test]
fn test_existing_bid_expiration_unchanged_after_ttl_update() {
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &6_000);
    let original_expiry = client.get_bid(&bid_id).unwrap().expiration_timestamp;
    
    client.set_bid_ttl_days(&1u64);  // Change TTL
    
    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.expiration_timestamp, original_expiry);
    // Existing bid not retroactively updated
}
```

### 4.4 Expiration Trigger Pattern
**Pattern:** Use `cleanup_expired_bids()` to mark bids as expired
```rust
client.cleanup_expired_bids(&invoice_id);
let bid = client.get_bid(&bid_id).unwrap();
assert_eq!(bid.status, BidStatus::Expired);
```

---

## 5. Assertion Patterns

### 5.1 Equality Assertions
**Basic Pattern:**
```rust
assert_eq!(actual, expected);
assert_eq!(actual, expected, "message for clarity");
```

**Used for:**
- Bid count verification
- Ranking order validation
- Field value checks (timestamp, amounts)
- Status verification

### 5.2 Option/Result Assertions
**Pattern:** Handle Option<T> safely
```rust
let best = BidStorage::get_best_bid(env, invoice);
let best_bid = best.expect("best bid must exist when ranking is non-empty");

if ranked.len() == 0 {
    assert!(best.is_none());
    return;
}
```

**Pattern:** Error verification
```rust
let result = client.try_set_bid_ttl_days(&0u64);
assert_eq!(
    result.unwrap_err().expect("contract error"),
    QuickLendXError::InvalidBidTtl
);
```

### 5.3 Collection Assertions
**Pattern:** Length checks
```rust
assert_eq!(ranked.len(), 3);
assert_eq!(ranked.get(0).unwrap().bid_id, expected_id);
```

**Pattern:** Iteration for exhaustive validation
```rust
for days in MIN_BID_TTL_DAYS..=MAX_BID_TTL_DAYS {
    let result = client.try_set_bid_ttl_days(&days);
    assert!(result.is_ok(), "TTL {} days must be accepted", days);
}
```

---

## 6. Key Testing Principles

### 6.1 Determinism
- Use seed-based ID generation for reproducibility
- Set explicit ledger timestamps (don't rely on elapsed time)
- Test insertion order independence
- Reuse same environment for related test variants

### 6.2 Boundary Testing
- Test minimum/maximum values (1 day TTL, 30 day TTL)
- Test one unit before/after boundaries (expiration - 1, expiration + 1)
- Test extreme values (u64::MAX) for rejection cases
- Test zero as special case

### 6.3 State Verification
- After each operation, assert expected state changed
- Verify both direct results and side effects
- Use helper assertions (`assert_best_matches_first_ranked`) for consistency checks
- Track status transitions (Placed → Expired via cleanup)

### 6.4 Error Coverage
- Test invalid inputs (out-of-range TTL values)
- Verify error types returned
- Test both rejection cases and success paths

---

## 7. Reusable Helper Function Recommendations

### For Cleanup/Expiration Tests (based on patterns observed):

```rust
// Time advancement helper
fn advance_time_seconds(env: &Env, seconds: u64) {
    env.ledger().set_timestamp(env.ledger().timestamp() + seconds);
}

// Expiration boundary helpers
fn advance_to_one_second_before_expiry(env: &Env, bid_expiry: u64) {
    env.ledger().set_timestamp(bid_expiry - 1);
}

fn advance_to_one_second_after_expiry(env: &Env, bid_expiry: u64) {
    env.ledger().set_timestamp(bid_expiry + 1);
}

// Status verification helper
fn verify_bid_expired(client: &QuickLendXContractClient, bid_id: &BytesN<32>) -> bool {
    client.get_bid(bid_id)
        .map(|bid| bid.status == BidStatus::Expired)
        .unwrap_or(false)
}

// Cleanup verification
fn cleanup_and_assert_expired(
    env: &Env,
    client: &QuickLendXContractClient,
    invoice_id: &BytesN<32>,
    bid_ids: &[BytesN<32>],
) {
    client.cleanup_expired_bids(invoice_id);
    for bid_id in bid_ids {
        assert!(verify_bid_expired(client, bid_id));
    }
}
```

---

## 8. Summary Table: When to Use Which Pattern

| Pattern | Use Case | Example |
|---------|----------|---------|
| Minimal unit test | Testing BidStorage functions directly | test_bid_ranking.rs |
| Integration test | Testing contract entry points with side effects | test_bid_ttl.rs |
| build_bid() | Creating test fixtures with controlled parameters | Any bid scenario test |
| funded_setup() | Complete invoice+bid workflow | TTL tests, cleanup tests |
| setup() | Basic contract initialization | Tests needing admin/client |
| assert_best_matches_first_ranked() | Verify ranking consistency | Tiebreaker validation tests |
| Boundary time tests | Expiration logic | test_bid_ttl.rs boundary tests |
| Parametric loops | Exhaustive value testing | TTL range validation |

---

## 9. Critical Pattern: Mock Authentication

**In integration tests:**
```rust
env.mock_all_auths();
```

This allows all address authorizations to succeed without explicit signatures, essential for testing contract logic without auth ceremony.

**Contrast with unit tests:**
- BidStorage tests don't need auth mocking (direct storage operations)
- Contract client tests require it (contract entry points check auth)

---

## 10. Next Steps for test_cleanup_expired_bids

Based on these patterns, a comprehensive cleanup test suite should:

1. **Use setup() + funded_setup()** for full workflow initialization
2. **Create multiple bids** with `build_bid()` and controlled timestamps
3. **Test boundary conditions** (1 second before/after TTL)
4. **Verify status transitions** (Placed → Expired)
5. **Test idempotency** (running cleanup twice should be safe)
6. **Test batch behavior** (cleanup with multiple expired bids)
7. **Test non-expired exclusion** (unexpired bids unaffected)
8. **Use assertion helpers** for consistency validation

