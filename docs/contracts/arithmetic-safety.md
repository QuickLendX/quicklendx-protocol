# Arithmetic Safety

All arithmetic in QuickLendX contracts is overflow- and underflow-safe. This document describes the approach and where it is applied.

## Approach

- **Checked / saturating operations**: Amounts, fees, percentages, timestamps, and counters use `checked_*`, `saturating_*`, or explicit checks before operations. There are no unchecked `+`, `-`, `*`, or `/` on numeric types that could overflow or underflow.
- **Release profile**: `overflow-checks = true` is enabled in `Cargo.toml` for the release profile, so the compiler enforces overflow checks in production builds.
- **Consistent patterns**: Prefer `saturating_add`, `saturating_sub`, `saturating_mul`, `checked_div` (with `unwrap_or(0)` or error handling) for amounts and fees; use `saturating_add` for counters and timestamps.

## Modules and Usage

| Module         | Usage                                                                                                                                                                                                                               |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **invoice**    | Counter increment (`saturating_add(1)`), `total_ratings`, `total_paid`, `payment_progress` (saturating/checked), rating average (checked division).                                                                                 |
| **bid**        | `default_expiration` (saturating add), `compare_bids` (saturating_sub), bid ID generation counter and timestamp mix (saturating).                                                                                                   |
| **payments**   | Escrow ID generation: counter and timestamp (saturating_add).                                                                                                                                                                       |
| **investment** | Investment ID generation: counter and timestamp (saturating); premium/coverage (saturating_mul, checked_div).                                                                                                                       |
| **fees**       | Fee and revenue calculations (saturating_mul, saturating_add, checked_div), revenue share sum (saturating_add), user volume and transaction count (saturating_add), analytics average and efficiency (checked_div, saturating_mul). |
| **profits**    | Profit and fee formulas (saturating_sub, saturating_mul, checked_div).                                                                                                                                                              |
| **settlement** | Uses fee module and payment amounts; no raw arithmetic.                                                                                                                                                                             |
| **escrow**     | Uses payments; no raw arithmetic.                                                                                                                                                                                                   |
| **backup**     | Backup ID generation: counter and timestamp (saturating_add).                                                                                                                                                                       |
| **audit**      | Audit ID generation: counter and timestamp (saturating_add).                                                                                                                                                                        |
| **storage**    | Invoice/bid/investment `next_count` (saturating_add(1)).                                                                                                                                                                            |
| **dispute**    | Query range end (saturating_add for start + limit).                                                                                                                                                                                 |
| **lib**        | `get_total_invoice_count` (saturating_add for status counts), pagination (saturating_add for start + limit).                                                                                                                        |

## Testing

- `test_overflow.rs` covers volume accumulation, revenue accumulation, fee calculation at limit, bid comparison at extreme values, and timestamp boundaries.
- Profit/fee and settlement tests validate correct amounts and no dust; overflow-safe paths are exercised by existing and overflow-focused tests.

## Invariants

- No operation on `i128` amounts or `u64` timestamps/counters is performed with plain `+`/`-`/`*`/`/` where the result could overflow or underflow.
- Division by zero is avoided by using `checked_div` with fallback or by ensuring denominator is positive (e.g. `max(amount, 1)` or guard clauses).
