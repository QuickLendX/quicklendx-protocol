# Analytics

## Overview
This module provides aggregated analytics for the protocol. Analytics are derived from invoice lifecycle state and are exposed via public getters on `QuickLendXContract`.

The primary goal is to provide **correct and safe** platform-level totals, rates, and average calculations even when the system has **no data** (zero invoices) or **sparse data** (e.g., only Paid invoices).

## Metrics and Definitions

### Platform totals
- **`total_invoices`**
  - Total number of invoices across all relevant statuses.
- **`total_volume`**
  - Sum of invoice face values across invoices considered in platform totals.
- **`total_investments`**
  - Count of invoices that represent an investment lifecycle.
  - **Definition used by the calculator**:
    - `Funded` + `Paid` + `Defaulted`
- **`total_fees_collected`**
  - Aggregate platform fees collected (as recorded by the analytics calculator/storage).

### Averages
- **`average_invoice_amount`**
  - `total_volume / total_invoices`.
- **`average_investment_amount`**
  - `total_volume / total_investments` for invoices counted as investments.

### Rates (basis points)
Rates are expressed in **basis points (bps)**:
- `10_000 bps` = `100%`
- `5_000 bps` = `50%`

- **`success_rate`**
  - Intended to represent the fraction of investments that successfully completed.
  - Calculator definition:
    - `Paid / (Funded + Paid + Defaulted)` expressed in bps.
- **`default_rate`**
  - Intended to represent the fraction of investments that defaulted.
  - Calculator definition:
    - `Defaulted / (Funded + Paid + Defaulted)` expressed in bps.

## Correctness Rules (Zero and Sparse Data)

### Rule: no division by zero
If a denominator is `0`, the calculator returns `0` for that derived metric.

Examples:
- If `total_invoices == 0` then `average_invoice_amount == 0`.
- If `total_investments == 0` then `success_rate == 0` and `default_rate == 0`.

### Rule: sparse investment outcomes
Sparse datasets should still produce intuitive results:
- If there is exactly 1 investment and it is `Paid`, then `success_rate == 10_000` and `default_rate == 0`.
- If there is exactly 1 investment and it is `Defaulted`, then `success_rate == 0` and `default_rate == 10_000`.

### Rule: clamp rates to 100%
Any basis-point rate is clamped to `<= 10_000`.

## Security Assumptions and Invariants

### Assumptions
- Invoice lifecycle transitions are enforced by the contract, and invoice status is a trusted source of truth.
- Analytics getters do not mutate state.

### Invariants enforced by the calculator
- Derived metrics do not panic on empty/sparse data.
- Rates are always within `[0, 10_000]`.
- Investment counting includes all terminal investment statuses (`Paid`, `Defaulted`) in addition to `Funded`.

## Public API Notes (NatSpec-style)

- `/// @notice Returns platform metrics aggregated from current invoice state.`
- `/// @dev Rates are returned as basis points (bps). 10_000 = 100%.`
- `/// @dev Zero-denominator derived metrics return 0.`

## Testing

### Coverage focus
Tests cover:
- Zero-data behavior (all derived metrics are zero).
- Sparse-data behavior (Paid-only, Defaulted-only).
- Mixed sparse-data behavior (Paid + Defaulted).
- Investment counting correctness (`Funded + Paid + Defaulted`).

### Test locations
- Unit tests: `src/test/test_analytics.rs`
- Analytics-only runner: `tests/analytics_accuracy.rs`
