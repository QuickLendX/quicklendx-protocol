# Protocol Limits

## Overview

The QuickLendX protocol enforces hard limits on invoice amounts, due-date horizons,
and all user-supplied string/vector fields to prevent storage DoS and ensure
economic viability.

## Numeric Limits

| Parameter | Default constant | Default value | Min | Max | Error |
|-----------|-----------------|---------------|-----|-----|-------|
| `min_invoice_amount` | — | 1,000,000 (prod) / 10 (test) | 1 | i128::MAX | `InvalidAmount` |
| `max_due_date_days` | — | 365 | 1 | 730 | `InvoiceDueDateInvalid` |
| `grace_period_seconds` | — | 604,800 | 0 | 2,592,000 | `InvalidTimestamp` |
| `min_bid_amount` | `DEFAULT_MIN_BID_AMOUNT` = 10 | 10 | 1 | — | `InvalidAmount` |
| `min_bid_bps` | `DEFAULT_MIN_BID_BPS` = 100 | 100 (1 %) | 0 | 10,000 | `InvalidAmount` |
| `max_invoices_per_business` | `DEFAULT_MAX_INVOICES_PER_BUSINESS` = 100 | 100 | 0 (unlimited) | u32::MAX | `MaxInvoicesPerBusinessExceeded` |

The constants `DEFAULT_MIN_BID_AMOUNT`, `DEFAULT_MIN_BID_BPS`, and
`DEFAULT_MAX_INVOICES_PER_BUSINESS` are defined in `src/protocol_limits.rs` and
re-used everywhere defaults are applied, so there is a single source of truth.

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

Tags are also normalized (trimmed, ASCII-lowercased) before the length check.
Duplicate normalized tags are rejected with `InvalidTag`.

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

## Admin API for Setting Limits

### `set_protocol_limits_full` (preferred)

Sets **all six** configurable protocol limits in a single transaction.  This is
the recommended entrypoint for operators and admin dashboards that need to
configure `min_bid_amount` or `min_bid_bps`.

```
set_protocol_limits_full(
    admin: Address,
    min_invoice_amount: i128,
    min_bid_amount: i128,          // ← was previously hardcoded
    min_bid_bps: u32,              // ← was previously hardcoded
    max_due_date_days: u64,
    grace_period_seconds: u64,
    max_invoices_per_business: u32,
) -> Result<(), QuickLendXError>
```

### Narrow helpers (backwards-compatible)

The older, narrower helpers (`set_protocol_limits`, `update_protocol_limits`,
`update_limits_max_invoices`, `initialize_protocol_limits`) **preserve** the
currently-stored `min_bid_amount`, `min_bid_bps` (and where applicable
`max_invoices_per_business`) rather than overwriting them with hardcoded
defaults.  Existing callers are unaffected.

### Bid-limit config

| Entrypoint | Description |
|-----------|-------------|
| `get_bid_limit_config()` | Returns [`BidLimitConfig`] snapshot: active limit, compile-time default, `is_disabled`, `is_custom`. |
| `set_max_active_bids_per_investor(limit)` | Set per-investor concurrent-bid cap. Pass `0` to disable. |
| `reset_max_active_bids_per_investor()` | Reset to compile-time default (20) and clear `is_custom` flag. |
| `get_bid_ttl_config()` | Returns [`BidTtlConfig`] snapshot including `is_custom` flag. |
| `set_bid_ttl_days(days)` | Set bid TTL in days (1–30). |
| `reset_bid_ttl_to_default()` | Reset to compile-time default (7 days). |

All admin-mutating entrypoints require the caller to be the current admin
(`AdminStorage::require_admin` is enforced inside the implementation).

## Test Coverage

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

`src/test_protocol_limits.rs` also covers `set_protocol_limits_full`,
`get_bid_limit_config`, and `reset_max_active_bids_per_investor`:

| Test | What it verifies |
|------|-----------------|
| `test_set_protocol_limits_full_round_trips_all_fields` | All 6 fields written and read back correctly. |
| `test_set_protocol_limits_full_non_admin_rejected` | Non-admin call returns `NotAdmin`. |
| `test_set_protocol_limits_full_rejects_zero_min_bid_amount` | `min_bid_amount = 0` → `InvalidAmount`. |
| `test_set_protocol_limits_full_rejects_min_bid_bps_above_10000` | `min_bid_bps > 10000` → `InvalidAmount`. |
| `test_narrow_set_protocol_limits_preserves_bid_fields` | `set_protocol_limits` does not clobber previously-set `min_bid_amount`/`min_bid_bps`. |
| `test_update_protocol_limits_preserves_bid_fields` | `update_protocol_limits` does not clobber bid fields. |
| `test_get_bid_limit_config_returns_defaults_before_any_admin_set` | Default snapshot is correct before any override. |
| `test_set_and_get_bid_limit_config_round_trip` | Custom limit written and read back with `is_custom = true`. |
| `test_set_bid_limit_to_zero_marks_disabled` | `limit = 0` sets `is_disabled = true`. |
| `test_reset_max_active_bids_per_investor_clears_custom_flag` | Reset restores default and clears `is_custom`. |

Run with:

```bash
cd quicklendx-contracts
cargo test test_protocol_limits
```
