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

### Withdrawal Guard

`withdraw_bid` is only valid for bids that are still in the `Placed` state.
Before checking the status, the contract runs an expiry refresh for the parent
invoice so a stale bid whose deadline has already passed is first converted to
`Expired`. This guarantees deterministic rejection semantics:

- `Placed` and not expired: withdrawal succeeds and status becomes `Withdrawn`
- `Accepted`: withdrawal returns `OperationNotAllowed`
- `Expired`: withdrawal returns `OperationNotAllowed`
- `Withdrawn` or `Cancelled`: withdrawal returns `OperationNotAllowed`

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

### Invariant 6 — Investor Pruning
When an investor's bid count is queried or when a new bid is placed, the protocol automatically prunes `Expired` bids from the investor's global index. This maintains $O(\text{active\_bids})$ performance for rate-limiting and ensures storage consistency.

---

## Public API

### `cleanup_expired_bids(env, invoice_id) -> u32`
Triggers a maintenance pass for all bids on the given invoice. Returns the number of bids transitioned to `Expired`. Called automatically inside `place_bid` and `accept_bid`.

### `assert_bid_invariants(env, invoice_id, current_timestamp) -> bool`
Post-condition validator. Returns `true` if all invariants hold. Use in tests and audit tooling.

### `count_active_placed_bids_for_investor(env, investor) -> u32`
Returns the number of active `Placed` bids for a given investor across all invoices. Triggers `refresh_investor_bids` to ensure accuracy and prune stale index entries.

### `refresh_investor_bids(env, investor) -> u32`
Internal maintenance helper that scans an investor's bid index and prunes `Expired` entries.

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
| `src/test_overdue_expiration.rs` | Invariant and **load tests** for cleanup under high bid pressure (50+ items) |
| `src/test_bid.rs` | Full bid lifecycle integration tests |

---

## Security Notes

Withdrawal rejection is status-gated and deterministic. Refreshing expiry
inside `withdraw_bid` prevents a time-expired bid from slipping through as a
stale `Placed` record, which would otherwise allow an investor to withdraw a
bid that should already be terminal. The module therefore preserves these
assumptions:

1. Accepted bids remain immutable after funding.
2. Expired bids cannot be revived through withdrawal.
3. Rejected withdrawals surface `OperationNotAllowed` consistently for all
   terminal bid states.

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
