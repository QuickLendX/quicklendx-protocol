# Analytics and Reporting

## Overview

The analytics and reporting module provides comprehensive metrics calculation, storage, and report generation for the QuickLendX platform. All analytics functions are implemented on-chain as Soroban smart contract functions.

## Data Structures

| Type                         | Description                                                                     |
| ---------------------------- | ------------------------------------------------------------------------------- |
| `PlatformMetrics`            | Total invoices, investments, volume, fees, success/default rates                |
| `PerformanceMetrics`         | Settlement time, verification time, dispute resolution, satisfaction score      |
| `UserBehaviorMetrics`        | Per-user invoice uploads, investments, bids, risk score                         |
| `FinancialMetrics`           | Volume, fees, profits by period & category, currency distribution               |
| `BusinessReport`             | Period-scoped report: uploaded/funded invoices, volume, success rate, ratings   |
| `InvestorReport`             | Period-scoped report: investments, returns, risk tolerance, portfolio diversity |
| `InvestorAnalytics`          | Comprehensive investor profile: tier, risk level, compliance score              |
| `InvestorPerformanceMetrics` | Platform-wide investor stats: counts by tier/risk, totals                       |
| `TimePeriod`                 | Enum: Daily, Weekly, Monthly, Quarterly, Yearly, AllTime                        |

## Public Contract API

See the [complete API table](#public-contract-api-complete) in the
Business Report Consistency Checks section below.

## Access Control

- **Admin functions** require `admin.require_auth()` — the admin address must be set via `set_admin`.
- **Report generation** requires the caller to authenticate as the business owner or investor.
- **Read-only functions** are publicly accessible.

## Storage Keys

| Key                    | Type         | Data                         |
| ---------------------- | ------------ | ---------------------------- |
| `plt_met`              | Singleton    | `PlatformMetrics`            |
| `perf_met`             | Singleton    | `PerformanceMetrics`         |
| `usr_beh` + Address    | Per-user     | `UserBehaviorMetrics`        |
| `biz_rpt` + BytesN<32> | Per-report   | `BusinessReport`             |
| `inv_rpt` + BytesN<32> | Per-report   | `InvestorReport`             |
| `inv_anal` + Address   | Per-investor | `InvestorAnalytics`          |
| `inv_perf`             | Singleton    | `InvestorPerformanceMetrics` |

## Business Report Consistency Checks (Issue #598)

The following invariants are enforced by the test suite in
`src/test/test_business_report_consistency.rs` and must hold for every
`BusinessReport` produced by `generate_business_report`:

| #   | Invariant                                                                                                        | How it is tested                                               |
| --- | ---------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| 1   | **Timestamp ordering** — `start_date ≤ end_date` for all `TimePeriod` variants                                   | `test_report_start_date_strictly_before_end_date`              |
| 2   | **`end_date` correctness** — equals the ledger timestamp at generation time                                      | `test_report_end_date_equals_ledger_timestamp`                 |
| 3   | **`generated_at` correctness** — equals the ledger timestamp at generation time                                  | `test_report_timestamp_generated_at_equals_ledger_now`         |
| 4   | **Period dates match calculator** — `start_date` / `end_date` agree with `AnalyticsCalculator::get_period_dates` | `test_report_period_dates_match_analytics_calculator`          |
| 5   | **Volume correctness** — `total_volume` equals the arithmetic sum of every in-period invoice amount              | `test_report_total_volume_equals_sum_of_invoice_amounts`       |
| 6   | **Funding ratio** — `invoices_funded ≤ invoices_uploaded`                                                        | `test_report_invoices_funded_never_exceeds_invoices_uploaded`  |
| 7   | **Rate bounds** — `success_rate ∈ [0, 10000]`, `default_rate ∈ [0, 10000]`, sum ≤ 10 000 bps                     | `test_report_rates_within_bps_bounds`                          |
| 8   | **Rate formula** — verified with known invoice counts for all-paid, partial, and all-defaulted cases             | `test_report_success_rate_formula_*`                           |
| 9   | **Report immutability** — a stored report is unchanged when a newer report is generated                          | `test_stored_report_unchanged_after_new_report_generated`      |
| 10  | **Period exclusion** — invoices outside the period window are not counted                                        | `test_report_excludes_invoice_outside_period_window`           |
| 11  | **Multi-business isolation** — report for business A does not count business B's invoices                        | `test_report_does_not_count_other_business_invoices`           |
| 12  | **Report-ID uniqueness** — two reports generated at different ledger timestamps have different IDs               | `test_reports_generated_at_different_times_have_different_ids` |
| 13  | **Stored-vs-live field equality** — every field of the stored report matches the live return value               | `test_all_report_fields_identical_in_stored_copy`              |
| 14  | **Category breakdown sum** — sum of `category_breakdown` counts equals `invoices_uploaded`                       | `test_category_breakdown_sum_equals_invoices_uploaded`         |
| 15  | **Idempotence** — re-generating a report at the same ledger state yields identical computed summaries            | `test_report_regeneration_produces_same_summary_values`        |

### Basis-Point Rate Convention

All rates (`success_rate`, `default_rate`) are expressed in **basis points**
(10 000 bps = 100 %). The formula is:

```
success_rate  = (successful_invoices  * 10_000) / invoices_uploaded
default_rate  = (defaulted_invoices   * 10_000) / invoices_uploaded
```

Both are computed with `saturating_mul` / `saturating_div` to prevent
overflow. When `invoices_uploaded == 0` both rates are 0.

### Period Window Boundaries

| Period    | `start_date`                    | `end_date`        |
| --------- | ------------------------------- | ----------------- |
| Daily     | `now − 86 400 s` (sat. sub)     | `now` (ledger ts) |
| Weekly    | `now − 604 800 s` (sat. sub)    | `now`             |
| Monthly   | `now − 2 592 000 s` (sat. sub)  | `now`             |
| Quarterly | `now − 7 776 000 s` (sat. sub)  | `now`             |
| Yearly    | `now − 31 536 000 s` (sat. sub) | `now`             |
| AllTime   | `0`                             | `now`             |

An invoice is counted in a report if and only if
`invoice.created_at >= start_date && invoice.created_at <= end_date`.

## Public Contract API (complete)

### Read-Only

| Function                                     | Returns                                 | Auth |
| -------------------------------------------- | --------------------------------------- | ---- |
| `get_platform_metrics()`                     | `Option<PlatformMetrics>`               | None |
| `get_performance_metrics()`                  | `Option<PerformanceMetrics>`            | None |
| `get_user_behavior_metrics(user)`            | `UserBehaviorMetrics`                   | None |
| `get_financial_metrics(period)`              | `FinancialMetrics`                      | None |
| `get_business_report(report_id)`             | `Option<BusinessReport>`                | None |
| `get_investor_report(report_id)`             | `Option<InvestorReport>`                | None |
| `get_analytics_summary()`                    | `(PlatformMetrics, PerformanceMetrics)` | None |
| `get_investor_analytics_data(investor)`      | `Option<InvestorAnalytics>`             | None |
| `get_investor_performance_metrics()`         | `Option<InvestorPerformanceMetrics>`    | None |
| `query_analytics_data(type, filters, limit)` | `Vec<String>`                           | None |

### Write (Admin Only)

| Function                                   | Effect                                          | Auth  |
| ------------------------------------------ | ----------------------------------------------- | ----- |
| `update_platform_metrics()`                | Recalculates and stores platform metrics        | Admin |
| `update_performance_metrics()`             | Recalculates and stores performance metrics     | Admin |
| `update_investor_analytics_data(investor)` | Recalculates and stores investor analytics      | Admin |
| `update_investor_performance_data()`       | Recalculates and stores investor perf metrics   | Admin |
| `export_analytics_data(type, filters)`     | Emits export event, returns confirmation string | Admin |

### Write (User)

| Function                                               | Effect                                        | Auth           |
| ------------------------------------------------------ | --------------------------------------------- | -------------- |
| `generate_business_report(business, period)`           | Generates, stores and returns business report | Business owner |
| `generate_investor_report(investor, period)`           | Generates, stores and returns investor report | Investor       |
| `update_user_behavior_metrics(user)`                   | Recalculates and stores user behavior         | User           |
| `calculate_investor_analytics(investor)`               | Calculates, stores and returns analytics      | Investor       |
| `update_investor_analytics(investor, amount, success)` | Records investment outcome                    | Investor       |
| `calc_investor_perf_metrics()`                         | Calculates, stores and returns perf metrics   | Any            |

## Security Notes

- All write endpoints enforce authorization (admin or owner).
- Metrics calculations use `saturating_add`/`saturating_div` to prevent overflow.
- Rates are expressed in **basis points** (10000 = 100%).
- Report IDs are SHA-256 hashes ensuring uniqueness.
- Period boundaries use saturating subtraction from the current ledger timestamp to prevent unsigned-integer underflow.
- Reports are immutable once stored: `get_business_report` always returns the snapshot captured at generation time regardless of subsequent state changes.
