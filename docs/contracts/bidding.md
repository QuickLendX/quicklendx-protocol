# Bidding Contract

> **Module:** `quicklendx-contracts/src/bid.rs`
> **Soroban SDK:** 25.1.1

---

## Overview

The bidding module manages the full lifecycle of investor bids on invoices. It enforces strict invariants during the cleanup maintenance pass to guarantee that only genuinely expired `Placed` bids are transitioned — `Accepted`, `Withdrawn`, and `Cancelled` bids are always preserved untouched.

---

## Bid Struct

| Field | Type | Description |
|---|---|---|
| `bid_id` | `BytesN<32>` | Unique bid identifier |
| `invoice_id` | `BytesN<32>` | Invoice this bid belongs to |
| `investor` | `Address` | Investor's Stellar address |
| `bid_amount` | `i128` | Amount offered in base token units |
| `expected_return` | `i128` | Expected repayment amount |
| `timestamp` | `u64` | Ledger timestamp at bid creation |
| `status` | `BidStatus` | Current lifecycle state |
| `expiration_timestamp` | `u64` | Deadline after which bid is eligible for expiry |

---

## BidStatus
```
Placed ──(accept)──► Accepted   [terminal]
Placed ──(withdraw)─► Withdrawn  [terminal]
Placed ──(cancel)───► Cancelled  [terminal]
Placed ──(cleanup)──► Expired    [terminal, only if now > expiration_timestamp]
```

Terminal states (`Accepted`, `Withdrawn`, `Cancelled`, `Expired`) are immutable — no further transitions are permitted.

---

## Expiry Configuration

| Parameter | Default | Bounds |
|---|---|---|
| `bid_ttl` | 7 days | 1–30 days |

Admin can update via `set_bid_ttl_days`. Expiration is computed at bid creation time using `Bid::default_expiration_with_env`.

---

## Cleanup Invariants

### Invariant 1 — Preservation
`Accepted`, `Withdrawn`, and `Cancelled` bids are **never** mutated by cleanup. These are terminal states and the cleanup function unconditionally skips them.

### Invariant 2 — Deadline
A `Placed` bid is only transitioned to `Expired` if `current_ledger_timestamp > bid.expiration_timestamp` (strict greater-than).

### Invariant 3 — Idempotency
Running `cleanup_expired_bids` multiple times at the same timestamp produces the same result as running it once. Already-`Expired` bids are silently skipped.

### Invariant 4 — Field Integrity
Only `status` is mutated. All other fields remain identical after cleanup.

### Invariant 5 — Post-condition
After a cleanup pass, no `Placed` bid in the invoice's active list has a deadline in the past. Verified by `assert_bid_invariants`.

---

## Public API

### `cleanup_expired_bids(env, invoice_id) -> u32`
Triggers a maintenance pass for all bids on the given invoice. Returns the number of bids transitioned to `Expired`. Called automatically inside `place_bid` and `accept_bid`.

### `assert_bid_invariants(env, invoice_id, current_timestamp) -> bool`
Post-condition validator. Returns `true` if all invariants hold. Use in tests and audit tooling.

### `count_bids_by_status(env, invoice_id) -> (u32, u32, u32, u32, u32)`
Returns `(placed, accepted, withdrawn, expired, cancelled)` counts for all bids on an invoice.

---

## Testing
```bash
# All tests
cargo test

# Invariant-specific tests
cargo test test_overdue_expiration
```

| File | Purpose |
|---|---|
| `src/test_overdue_expiration.rs` | Invariant tests for cleanup under all status combinations and timestamp boundaries |
| `src/test_bid.rs` | Full bid lifecycle integration tests |

---

## Security Notes

- Terminal bid check runs **before** the deadline check — no code path can reach `status = Expired` for an `Accepted`, `Withdrawn`, or `Cancelled` bid.
- All counter increments use `saturating_add` to prevent overflow.
- `place_bid` and `accept_bid` are protected by `reentrancy::with_payment_guard`.
