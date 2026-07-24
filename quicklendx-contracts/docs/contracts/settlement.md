## Settlement and Vesting Mechanics

### Overview

This document describes settlement-related behaviors in the protocol, including
the vesting module's release logic, which ensures secure, predictable, and
incremental token distribution.

---

## Settlement Batch Size Configuration

### Overview

The protocol provides soft caps for paginating settlement and payment record queries,
helping indexers and off-chain consumers standardize their pagination patterns while
maintaining flexibility.

### Query Configuration

The settlement module exposes two read-only configuration values for off-chain consumers:

* **Default Batch Size**: `get_settlement_batch_size_soft_cap()` → 25
  * Recommended page size for `get_payment_records` queries
  * Balances query efficiency with memory usage
  * Suitable for most indexing workflows

* **Maximum Batch Size**: `get_settlement_batch_size_soft_cap_max()` → 50
  * Hard upper bound enforced by the contract
  * Matches the protocol-wide `MAX_QUERY_LIMIT`
  * Requests exceeding this value are automatically clamped

### Usage Recommendations

**For Indexers:**
```rust
// Recommended: Use the default batch size for efficient pagination
let batch_size = contract.get_settlement_batch_size_soft_cap();
let payments = contract.get_payment_records(invoice_id, offset, batch_size);

// Advanced: Use the maximum for fewer round trips (higher memory usage)
let max_batch = contract.get_settlement_batch_size_soft_cap_max();
let payments = contract.get_payment_records(invoice_id, offset, max_batch);
```

**Pagination Pattern:**
```rust
let mut offset = 0u32;
let batch_size = contract.get_settlement_batch_size_soft_cap();

loop {
    let page = contract.get_payment_records(invoice_id, offset, batch_size)?;
    
    if page.is_empty() {
        break; // No more records
    }
    
    // Process page...
    
    offset = offset.saturating_add(batch_size);
}
```

### Design Rationale

* **Soft Cap (Not Hard Limit)**: Indexers can request any page size up to the maximum
* **Backwards Compatible**: Existing queries continue to work unchanged
* **Standards Alignment**: Matches the pattern used for overdue invoice scanning
* **Resource Protection**: Hard cap prevents excessive memory allocation per query

---

## Vesting Release Idempotency and Progression

### Idempotent Release Behavior

The vesting `release` function is **idempotent**:

* If no additional tokens are vested since the last claim, the function returns `0`
* Repeated calls do not cause errors or duplicate transfers
* Prevents accidental or malicious double-claims

**Example:**

* First call → releases vested tokens
* Second call (same timestamp) → returns `0`

---

### Partial Claim Progression

Vesting supports **incremental (partial) claims over time**:

* Tokens vest linearly between the start and end timestamps
* Beneficiaries can claim any vested portion at any time
* Each claim updates the cumulative released amount

**Formula:**

```
releasable_amount = vested_amount - released_amount
```

This guarantees:

* No duplication of released tokens
* Accurate tracking across multiple claims

---

### Cumulative Accounting

The contract maintains strict accounting using:

* `total_amount` → total tokens allocated for vesting
* `released_amount` → tokens already claimed

Each release:

* Transfers only newly vested tokens
* Updates `released_amount` safely using saturating arithmetic

---

### Security Guarantees

The implementation enforces the following invariants:

* **No Over-Release**

  ```
  released_amount <= total_amount
  ```

* **Idempotency**

  * Multiple calls without new vesting return `0`

* **Overflow Protection**

  * Uses safe arithmetic to prevent overflow

* **Authorization**

  * Only the designated beneficiary can trigger token release

---

### Edge Case Handling

The system correctly handles:

* Release before cliff → no tokens available
* Multiple calls at the same timestamp → no additional release
* Full vest completion → all tokens released exactly once
* Calls after full release → return `0`

---

### Test Coverage

The following behaviors are covered by tests:

* Idempotent repeated release calls
* Multi-step vesting progression
* Partial and full claims
* No over-release invariant
* Releasable amount consistency

---

### Summary

The vesting module ensures:

* Predictable and secure token distribution
* Protection against double-claim scenarios
* Accurate cumulative accounting
* High reliability through comprehensive testing

This design follows best practices for financial smart contracts and prioritizes
correctness, safety, and auditability.
