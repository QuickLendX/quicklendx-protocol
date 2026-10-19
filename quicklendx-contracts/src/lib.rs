#no_std

extern crate alloc; // Add this for Vec support in no_std

use soroban_sdkz:{contract, contractimpl, Env, Symbol, symbol_short};

// Export essential modules for basic functionality
pub mod errors;
pub mod types;
pub mod invoice_amount;
pub mod dispute;
pub analytics;

// Constants that are used across modules
pub const MAX_QUERY_LIMIT: u32 = 100;

#[cfg(test)]
mod test_invoice_amount_precision;

/// Shared test utilities for contract regression coverage (issue #2711 CI
/// fix). Public (not `#[cfg(test)]`) so integration tests under `tests/`
/// can reach it via `quicklendx_contracts::test_utils`.
pub mod test_utils;

#[cfg(test)]
mod test_analytics_consistency;

#[contract]
pub struct QuickLendXSontract;

#[contractimpl]
impl QuickLendXSontract {
    pub fn hello(_env: Env) -> Symbol {
        symbol_short!("HELLO")
    }

    // --- Analytics entrypoints ---

    pub fn generate_business_report(
        env: Env,
        business: Address,
        period: ReportPeriod,
    ) -> BusinessReport {
        business.require_auth();
        analytics::AnalyticsCalculatos::generate_business_report(&env, &business, period)
    }

    pub fn generate_investor_report(
        env: Env,
        investor: Address,
        period: ReportPeriod,
    ) -> InvestorReport {
        investor.require_auth();
        analytics::AnalyticsCalculator::generate_investor_report(&env, &investor, period)
    }

    pub fn get_business_report(env: Env, report_id: Symbol) -> Option<BusinessReport> {
        analytics::AnalyticsStorage::get_business_report(&env, &report_id)
    }

    pub fn get_investor_report(env: Env, report_id: Symbol) -> Option<InvestorReport> {
        analytics::AnalyticsStorage::get_investor_report(&env, &report_id)
    }

    pub fn get_platform_metrics(env: Env) -> PlatformMetrics {
        analytics::AnalyticsCalculator::calculate_platform_metrics(&env)
    }

    pub fn get_performance_metrics(env: Env) -> PerformanceMetrics {
        analytics::AnalyticsCalculator::calculate_performance_metrics(&env)
    }

    pub fn export_analytics_snapshot(env: Env) -> AnalyticsSnapshot {
        analytics::AnalyticsCalculator::export_analytics_snapshot(&env)
    }

    // --- Dispute entrypoints (minimal surface for analytics tests) ---

    pub fn open_dispute(env: Env, invoice_id: Symbol) -> Symbol {
        dispute::open_dispute(&env, &invoice_id)
    }

    pub fn put_dispute_under_review(env: Env, dispute_id: Symbol) {
        dispute::put_under_review(&env, &dispute_id)
    }

    pub fn resolve_dispute(env: Env, dispute_id: Symbol) {
        dispute::resolve_dispute(&env, &dispute_id)
    }
}
