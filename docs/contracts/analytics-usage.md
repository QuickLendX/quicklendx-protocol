# Analytics Module: Features & Usage

## Overview
The analytics module provides comprehensive metrics and reporting for the QuickLendX protocol. It enables platform administrators, businesses, and investors to access actionable insights about protocol activity, performance, and user behavior.

## Features Implemented

### 1. Platform Metrics
- **get_platform_metrics**: Retrieve overall protocol statistics (total invoices, investments, volume, fees, averages, success/default rates).
- **update_platform_metrics (admin)**: Admin-only function to recalculate and store platform metrics.

### 2. Performance Metrics
- **get_performance_metrics**: Get protocol performance indicators (settlement time, verification time, dispute resolution, transaction success/error rates, satisfaction score).
- **update_performance_metrics (admin)**: Admin-only function to recalculate and store performance metrics.

### 3. User Behavior Metrics
- **get_user_behavior_metrics**: Retrieve behavioral metrics for a specific user (invoices uploaded, investments made, bids placed, last activity, risk score).
- **update_user_behavior_metrics**: Update and store user behavior metrics (requires user authentication).

### 4. Financial Metrics
- **get_financial_metrics (period)**: Get financial statistics for a specified period (total volume, fees, profits, average return rate, volume by category).

### 5. Business & Investor Reports
- **generate_business_report (auth)**: Authenticated businesses can generate detailed reports for a given period (invoices, funding, volume, category breakdown, ratings, etc.).
- **generate_investor_report (auth)**: Authenticated investors can generate reports for a given period (investments, returns, success/default rates, risk tolerance, portfolio diversity).
- **get_business_report / get_investor_report**: Retrieve stored reports by ID.

### 6. Analytics Summary
- **get_analytics_summary**: Returns a tuple of platform and performance metrics for quick overview.

### 7. Export Analytics Data
- **export_analytics_data (admin)**: Admin-only function to export analytics data for external analysis (supports filters and export types).

## Usage Examples

### Platform Metrics
```rust
let metrics = contract.get_platform_metrics();
```

### Update Platform Metrics (Admin)
```rust
contract.update_platform_metrics(); // Requires admin auth
```

### Performance Metrics
```rust
let perf = contract.get_performance_metrics();
```

### User Behavior Metrics
```rust
let behavior = contract.get_user_behavior_metrics(&user_address);
contract.update_user_behavior_metrics(&user_address); // Requires user auth
```

### Financial Metrics (Period)
```rust
let metrics = contract.get_financial_metrics(&TimePeriod::Monthly);
```

### Business Report
```rust
let report = contract.generate_business_report(&business_address, &TimePeriod::Quarterly); // Requires business auth
let stored = contract.get_business_report(&report.report_id);
```

### Investor Report
```rust
let report = contract.generate_investor_report(&investor_address, &TimePeriod::Yearly); // Requires investor auth
let stored = contract.get_investor_report(&report.report_id);
```

### Analytics Summary
```rust
let (platform, performance) = contract.get_analytics_summary();
```

### Export Analytics Data (Admin)
```rust
contract.export_analytics_data("csv", filters); // Requires admin auth
```

## TimePeriod Edge-Case & Boundary Semantics

### Period Date Calculation

`get_period_dates(current_timestamp, period)` uses **`saturating_sub`** for all
arithmetic, so results are always valid `u64` pairs with `start ≤ end`.

| Period    | Window formula                          |
|-----------|-----------------------------------------|
| Daily     | `[ts − 86_400, ts]`                     |
| Weekly    | `[ts − 604_800, ts]`                    |
| Monthly   | `[ts − 2_592_000, ts]`                  |
| Quarterly | `[ts − 7_776_000, ts]`                  |
| Yearly    | `[ts − 31_536_000, ts]`                 |
| AllTime   | `[0, ts]` — entire history              |

### Boundary Inclusion

Both edges are **inclusive**:

```
invoice.created_at >= start_date && invoice.created_at <= end_date
```

An invoice created at exactly `start_date` or exactly `end_date` is always
counted. An invoice one second before `start_date` is always excluded.

### Near-Zero Timestamps (Saturating Underflow)

When `current_timestamp` is smaller than the period's nominal duration the
subtraction saturates to `0` rather than wrapping:

```
// ts = 1, Daily (86_400 s): 1.saturating_sub(86_400) = 0
// window becomes [0, 1] — a one-second window, not a panic or overflow
```

At `current_timestamp = 0` every variant returns `(0, 0)` — a degenerate
zero-length window. All query functions return zero/empty results in this state
without panicking.

### Zero-Length Ranges

A zero-length range (`start == end`) is valid. When no invoices have
`created_at == start == end`, every aggregated counter is 0. The contract never
returns an error for degenerate windows.

### Empty Range Guarantees

Regardless of period or timestamp, if no data matches the window:

- `total_volume`, `total_fees`, `total_profits` → `0`
- `total_invoices`, `invoices_uploaded`, `investments_made` → `0`
- `success_rate`, `default_rate`, `average_return_rate` → `0`
- No panics, no unrecoverable errors.

### Growth Stability

- Stored reports are **immutable snapshots**. Adding invoices after a report is
  generated never alters the stored copy.
- `total_invoices` in `PlatformMetrics` grows monotonically: each successful
  `store_invoice` call increases the count by exactly 1.
- `AllTime` always captures every invoice ever stored, regardless of when
  subsequent reports are generated.

## Security Assumptions

1. **No cross-business data leakage** — `generate_business_report` filters by
   `business_address`; Business A's report never includes Business B's invoices.
2. **Read-only analytics** — `get_platform_metrics`, `get_performance_metrics`,
   `get_financial_metrics`, and `get_analytics_summary` require no auth and
   expose only aggregated totals, not individual user records.
3. **Admin-gated writes** — `update_platform_metrics` and
   `update_performance_metrics` require admin authorization; unauthenticated
   callers receive an error.
4. **No sensitive data in reports** — Business and investor reports contain
   aggregated statistics only; private invoice details and raw investment amounts
   are not surfaced.

## Notes
- Admin-only and authenticated functions require proper authorization.
- All metrics and reports are available via the contract client interface.
- See `quicklendx-contracts/src/test/test_analytics.rs` for boundary/edge tests
  and `src/test_analytics_consistency.rs` for business-report invariant tests.
