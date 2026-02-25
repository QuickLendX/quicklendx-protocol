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

## Notes
- Admin-only and authenticated functions require proper authorization.
- All metrics and reports are available via the contract client interface.
- See test_analytics.rs for comprehensive test coverage and usage patterns.
