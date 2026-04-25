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
