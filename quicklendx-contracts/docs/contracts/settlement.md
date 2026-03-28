## Settlement and Vesting Mechanics

### Overview

This document describes settlement-related behaviors in the protocol, including
the vesting module's release logic, which ensures secure, predictable, and
incremental token distribution.

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
