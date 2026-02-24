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

### Read-Only

| Function                                | Returns                                 | Auth |
| --------------------------------------- | --------------------------------------- | ---- |
| `get_platform_metrics()`                | `PlatformMetrics`                       | None |
| `get_performance_metrics()`             | `PerformanceMetrics`                    | None |
| `get_user_behavior_metrics(user)`       | `UserBehaviorMetrics`                   | None |
| `get_financial_metrics(period)`         | `FinancialMetrics`                      | None |
| `get_business_report(report_id)`        | `Option<BusinessReport>`                | None |
| `get_investor_report(report_id)`        | `Option<InvestorReport>`                | None |
| `get_analytics_summary()`               | `(PlatformMetrics, PerformanceMetrics)` | None |
| `get_investor_analytics_data(investor)` | `Option<InvestorAnalytics>`             | None |
| `get_investor_performance_metrics()`    | `Option<InvestorPerformanceMetrics>`    | None |

### Write (Admin Only)

| Function                                   | Effect                                      | Auth  |
| ------------------------------------------ | ------------------------------------------- | ----- |
| `update_platform_metrics()`                | Recalculates and stores platform metrics    | Admin |
| `update_performance_metrics()`             | Recalculates and stores performance metrics | Admin |
| `update_investor_analytics_data(investor)` | Recalculates and stores investor analytics  | Admin |
| `export_analytics_data(type, filters)`     | Emits export event                          | Admin |

### Write (User)

| Function                                     | Effect                                | Auth           |
| -------------------------------------------- | ------------------------------------- | -------------- |
| `generate_business_report(business, period)` | Generates and stores business report  | Business owner |
| `generate_investor_report(investor, period)` | Generates and stores investor report  | Investor       |
| `update_user_behavior_metrics(user)`         | Recalculates and stores user behavior | User           |

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

## Security Notes

- All write endpoints enforce authorization (admin or owner).
- Metrics calculations use `saturating_add`/`saturating_div` to prevent overflow.
- Rates are expressed in **basis points** (10000 = 100%).
- Report IDs are SHA-256 hashes ensuring uniqueness.
- Period boundaries use simple subtraction from the current ledger timestamp.
