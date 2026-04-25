# Protocol Limits

## Overview

The QuickLendX protocol enforces hard limits on invoice amounts, due-date horizons,
and all user-supplied string/vector fields to prevent storage DoS and ensure
economic viability.

## Numeric Limits

| Parameter | Default | Min | Max | Error |
|-----------|---------|-----|-----|-------|
| `min_invoice_amount` | 1,000,000 (prod) / 10 (test) | 1 | i128::MAX | `InvalidAmount` |
| `max_due_date_days` | 365 | 1 | 730 | `InvoiceDueDateInvalid` |
| `grace_period_seconds` | 604,800 | 0 | 2,592,000 | `InvalidTimestamp` |
| `min_bid_amount` | 10 | 1 | — | `InvalidAmount` |
| `min_bid_bps` | 100 | 0 | 10,000 | `InvalidAmount` |
| `max_invoices_per_business` | 100 | 0 (unlimited) | u32::MAX | `MaxInvoicesPerBusinessExceeded` |

### Grace period constraint

`grace_period_seconds` must not exceed `max_due_date_days × 86,400`.
A 1-day horizon cannot have a 2-day grace period.

## String Length Limits

Defined in `src/protocol_limits.rs`, enforced before any storage write.

| Field | Constant | Max bytes | Error |
|-------|----------|-----------|-------|
| Invoice description | `MAX_DESCRIPTION_LENGTH` | 1,024 | `InvalidDescription` |
| Customer name | `MAX_NAME_LENGTH` | 150 | `InvalidDescription` |
| Customer address | `MAX_ADDRESS_LENGTH` | 300 | `InvalidDescription` |
| Tax ID | `MAX_TAX_ID_LENGTH` | 50 | `InvalidDescription` |
| Notes | `MAX_NOTES_LENGTH` | 2,000 | `InvalidDescription` |
| Tag | `MAX_TAG_LENGTH` | 50 | `InvalidTag` |
| Dispute reason | `MAX_DISPUTE_REASON_LENGTH` | 1,000 | `InvalidDisputeReason` |
| Dispute evidence | `MAX_DISPUTE_EVIDENCE_LENGTH` | 2,000 | `InvalidDisputeEvidence` |
| Dispute resolution | `MAX_DISPUTE_RESOLUTION_LENGTH` | 2,000 | `InvalidDisputeReason` |
| KYC data | `MAX_KYC_DATA_LENGTH` | 5,000 | `InvalidDescription` |
| Rejection reason | `MAX_REJECTION_REASON_LENGTH` | 500 | `InvalidDescription` |
| Feedback | `MAX_FEEDBACK_LENGTH` | 1,000 | `InvalidDescription` |
| Notification title | `MAX_NOTIFICATION_TITLE_LENGTH` | 150 | `InvalidDescription` |
| Notification message | `MAX_NOTIFICATION_MESSAGE_LENGTH` | 1,000 | `InvalidDescription` |
| Transaction ID | `MAX_TRANSACTION_ID_LENGTH` | 124 | `InvalidDescription` |

## Vector Limits

| Field | Max count | Error |
|-------|-----------|-------|
| Tags per invoice | 10 | `TagLimitExceeded` |
| Bids per invoice | 50 | `MaxBidsPerInvoiceExceeded` |
| Active invoices per business | 100 (configurable) | `MaxInvoicesPerBusinessExceeded` |
| Active bids per investor | 20 (configurable) | `OperationNotAllowed` |

Tags are also normalized (trimmed, ASCII-lowercased) before the length check.
Duplicate normalized tags are rejected with `InvalidTag`.

## Investor Exposure Caps and Active Bid Limits (Issue #782)

### Definitions

**Active Bid**: A bid in the `Placed` status that has not yet reached its expiration timestamp. Active bids represent current capital commitments and count toward both the active bid limit and portfolio exposure cap.

**Portfolio Exposure**: The total sum of all active bid amounts for a given investor, plus any already-funded investments. This represents the investor's total capital at risk in the protocol.

### Caps

| Parameter | Default | Configurable | Error |
|-----------|---------|--------------|-------|
| `max_active_bids_per_investor` | 20 | Yes (admin) | `OperationNotAllowed` |
| `portfolio_exposure_cap` | Per-investor limit | Yes (via KYC) | `InvalidAmount` |

- `max_active_bids_per_investor`: Maximum number of concurrent `Placed` bids an investor can have across all invoices. A value of 0 disables this limit (not recommended for production).
- `portfolio_exposure_cap`: Individual investor's total investment limit, set during KYC verification. Enforced via `validate_investor_investment()`.

### Bid Lifecycle

```
CREATED (Placed)
    │
    ├─→ CANCELLED (investor-initiated)
    │       └─ Terminal state (no further transitions)
    │
    ├─→ ACCEPTED (bid selected for funding)
    │       └─ Terminal state (no further transitions)
    │
    ├─→ WITHDRAWN (investor withdrawal)
    │       └─ Terminal state (no further transitions)
    │
    └─→ EXPIRED (TTL elapsed)
            └─ Terminal state (no further transitions)
```

Only `Placed` bids count toward active bid limits and portfolio exposure. Terminal states are excluded from all limit calculations.

### Security Guarantees

The protocol enforces the following security properties to prevent bid churn attacks:

1. **No Bid Churn Exploitation**
   - Exposure is recalculated from fresh storage state on every bid placement
   - Cancelled bids immediately free both active bid slots and portfolio capacity
   - Expired bids are pruned before counting (via `refresh_investor_bids()`)

2. **No Race Condition Bypass**
   - Active bid count is checked atomically before bid placement
   - Portfolio exposure calculation uses saturating arithmetic to prevent overflow
   - All limit checks occur before any storage writes (fail-fast)

3. **No State Desync Risk**
   - `count_active_placed_bids_for_investor()` calls `refresh_investor_bids()` before counting
   - `get_active_bid_amount_sum_for_investor()` reads directly from current storage
   - State consistency tests verify system counts match actual stored state

### Attack Scenarios and Mitigations

#### Attack 1: Cancel + Re-bid Spam
**Scenario**: Investor rapidly cancels and re-places bids to manipulate market dynamics or bypass limits.

**Mitigation**:
- Exposure is recalculated from fresh state on every bid placement
- No cached counts or temporary gaps are exploitable
- Cancellation immediately frees capacity; new bids must pass full validation

#### Attack 2: Expire/Recreate Cycles
**Scenario**: Investor lets bids expire to free up slots, then immediately places new bids.

**Mitigation**:
- `refresh_investor_bids()` is called before counting active bids
- Expired bids are pruned from the investor's bid index
- New bids cannot be placed until the cleanup is triggered

#### Attack 3: Partial Fill Manipulation
**Scenario**: Investor attempts to use partial fills to exceed portfolio exposure cap.

**Mitigation**:
- Only `Placed` bids count toward exposure (not partially filled bids)
- Portfolio exposure includes both active bids and funded investments
- Saturating arithmetic prevents overflow attacks

#### Attack 4: Concurrent Submission Race
**Scenario**: Multiple concurrent bid submissions attempt to exceed limits simultaneously.

**Mitigation**:
- Active bid count check uses `>=` comparison (not `>`) to prevent off-by-one errors
- Check occurs before any storage writes
- Soroban's atomic transaction model ensures consistency

### Validation Flow

```
place_bid
  └─ validate_bid
       ├─ Basic amount validation
       ├─ Invoice state check
       ├─ Ownership check
       ├─ validate_investor_investment (centralized enforcement)
       │    ├─ Verification status check
       │    ├─ Active bid count check (max_active_bids_per_investor)
       │    │    └─ count_active_placed_bids_for_investor
       │    │         └─ refresh_investor_bids (prunes expired bids)
       │    └─ Portfolio exposure check (investment_limit)
       │         └─ get_active_bid_amount_sum_for_investor
       └─ Existing bid protection
```

### Test Coverage (Issue #782)

`src/test_bid.rs` — 11 comprehensive security tests across 5 groups:

| Group | Tests | Coverage |
|-------|-------|----------|
| A: Active Bid Limit Enforcement | 3 | Limit enforcement, cancellation, expiration |
| B: Portfolio Exposure Cap | 2 | Cap enforcement, bid churn prevention |
| C: Concurrent/Race Conditions | 2 | Concurrent submissions, cancel/create |
| D: State Consistency | 2 | Count accuracy, exposure accuracy |
| E: Edge Cases | 2 | Zero-value, malformed bids, idempotency |

Run with:

```bash
cd quicklendx-contracts
cargo test test_investor_cannot_exceed_max_active_bids
cargo test test_bid_cancellation_frees_slot_for_new_bid
cargo test test_expired_bids_do_not_allow_limit_bypass
cargo test test_investor_cannot_exceed_portfolio_cap
cargo test test_bid_churn_cannot_increase_exposure
cargo test test_concurrent_bid_submissions_respect_limits
cargo test test_concurrent_cancel_and_create_does_not_bypass_limits
cargo test test_active_bid_count_matches_actual_state
cargo test test_portfolio_exposure_matches_sum_of_active_bids
cargo test test_zero_value_bids_do_not_break_limits
cargo test test_malformed_or_partial_bids_are_rejected
cargo test test_repeated_cancel_operations_are_idempotent
cargo test test_limit_disabled_when_set_to_zero
```

## Validation Flow

```
store_invoice / upload_invoice
  └─ amount > 0                          → InvalidAmount
  └─ due_date > now                      → InvoiceDueDateInvalid
  └─ ProtocolLimitsContract::validate_invoice
       └─ amount >= min_invoice_amount   → InvalidAmount
       └─ due_date <= now + max_days×86400 → InvoiceDueDateInvalid
  └─ validate_invoice_tags
       └─ count <= 10                    → TagLimitExceeded
       └─ each tag 1–50 bytes            → InvalidTag
       └─ no duplicates                  → InvalidTag
```

## Security Notes

- All limits are checked **before** any storage write (fail-fast).
- Limits are configurable by admin only; non-admin calls return `NotAdmin`.
- The grace-period/horizon constraint prevents impossible configurations.
- String limits prevent storage DoS from oversized payloads.

## Test Coverage (Issue #826)

`src/test_protocol_limits_boundary.rs` — 35 tests across 10 groups:

| Group | Tests |
|-------|-------|
| Invoice amount bounds | 6 |
| Due-date horizon bounds | 5 |
| Protocol limits parameter bounds | 9 |
| Description string limits | 2 |
| Tag vector and string limits | 7 |
| KYC data string limits | 3 |
| Rejection reason string limits | 2 |
| Dispute string limits | 6 |
| check_string_length unit tests | 3 |
| Consistency across store/upload | 3 |

Run with:

```bash
cd quicklendx-contracts
cargo test test_protocol_limits_boundary
```
