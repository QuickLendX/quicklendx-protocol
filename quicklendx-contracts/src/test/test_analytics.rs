/// Comprehensive tests for the analytics and reporting module (Issue #266)
///
/// This test module covers:
/// - Platform metrics calculation and storage
/// - Performance metrics calculation
/// - User behavior metrics
/// - Financial metrics by period
/// - Business and investor report generation and storage
/// - Period date boundary calculations
/// - Admin-only update authorization
/// - Empty data / edge cases
use super::*;
use crate::analytics::{
    AnalyticsCalculator, AnalyticsStorage, FinancialMetrics, PlatformMetrics, TimePeriod,
};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn setup_contract(env: &Env) -> (QuickLendXContractClient, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);
    (client, admin, business)
}

fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    description: &str,
) -> soroban_sdk::BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400;
    client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, description),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

// ============================================================================
// PLATFORM METRICS TESTS
// ============================================================================

#[test]
fn test_platform_metrics_empty_data() {
    let env = Env::default();
    let (client, _admin, _business) = setup_contract(&env);

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 0);
    assert_eq!(metrics.total_investments, 0);
    assert_eq!(metrics.total_volume, 0);
    assert_eq!(metrics.total_fees_collected, 0);
    assert_eq!(metrics.average_invoice_amount, 0);
    assert_eq!(metrics.average_investment_amount, 0);
    assert_eq!(metrics.success_rate, 0);
    assert_eq!(metrics.default_rate, 0);
}

#[test]
fn test_platform_metrics_with_invoices() {
    let env = Env::default();
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 1000, "Invoice A");
    create_invoice(&env, &client, &business, 2000, "Invoice B");
    create_invoice(&env, &client, &business, 3000, "Invoice C");

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 3);
    assert_eq!(metrics.total_volume, 6000);
    assert_eq!(metrics.average_invoice_amount, 2000);
}

#[test]
fn test_platform_metrics_after_status_changes() {
    let env = Env::default();
    let (client, _admin, business) = setup_contract(&env);

    let inv1 = create_invoice(&env, &client, &business, 1000, "Status inv 1");
    let inv2 = create_invoice(&env, &client, &business, 2000, "Status inv 2");

    // Verify and fund inv1
    client.update_invoice_status(&inv1, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv1, &InvoiceStatus::Funded);

    // Mark inv2 as paid
    client.update_invoice_status(&inv2, &InvoiceStatus::Paid);

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 2);
    // Funded count = 1 (inv1 is Funded)
    assert_eq!(metrics.total_investments, 1);
}

// ============================================================================
// PERFORMANCE METRICS TESTS
// ============================================================================

#[test]
fn test_performance_metrics_empty_data() {
    let env = Env::default();
    let (client, _admin, _business) = setup_contract(&env);

    let metrics = client.get_performance_metrics();
    assert_eq!(metrics.average_settlement_time, 0);
    assert_eq!(metrics.average_verification_time, 0);
    assert_eq!(metrics.dispute_resolution_time, 0);
    assert_eq!(metrics.transaction_success_rate, 0);
    assert_eq!(metrics.error_rate, 0);
    assert_eq!(metrics.user_satisfaction_score, 0);
}

#[test]
fn test_performance_metrics_with_invoices() {
    let env = Env::default();
    let (client, _admin, business) = setup_contract(&env);

    let inv1 = create_invoice(&env, &client, &business, 1000, "Perf inv 1");
    let inv2 = create_invoice(&env, &client, &business, 2000, "Perf inv 2");

    // One paid, one defaulted
    client.update_invoice_status(&inv1, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv2, &InvoiceStatus::Defaulted);

    let metrics = client.get_performance_metrics();
    // 1 paid out of 2 total = 50% = 5000 bps
    assert_eq!(metrics.transaction_success_rate, 5000);
    // 1 defaulted out of 2 total = 50% = 5000 bps
    assert_eq!(metrics.error_rate, 5000);
}

// ============================================================================
// USER BEHAVIOR METRICS TESTS
// ============================================================================

#[test]
fn test_user_behavior_new_user() {
    let env = Env::default();
    let (client, _admin, _business) = setup_contract(&env);

    let new_user = Address::generate(&env);
    let behavior = client.get_user_behavior_metrics(&new_user);

    assert_eq!(behavior.user_address, new_user);
    assert_eq!(behavior.total_invoices_uploaded, 0);
    assert_eq!(behavior.total_investments_made, 0);
    assert_eq!(behavior.total_bids_placed, 0);
    assert_eq!(behavior.last_activity, 0);
    assert_eq!(behavior.risk_score, 25); // low default risk
}

#[test]
fn test_user_behavior_with_invoices() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 1000, "Behavior inv 1");
    create_invoice(&env, &client, &business, 2000, "Behavior inv 2");

    let behavior = client.get_user_behavior_metrics(&business);
    assert_eq!(behavior.total_invoices_uploaded, 2);
    assert!(behavior.last_activity > 0);
}

// ============================================================================
// FINANCIAL METRICS TESTS
// ============================================================================

#[test]
fn test_financial_metrics_empty_data() {
    let env = Env::default();
    let (client, _admin, _business) = setup_contract(&env);

    let metrics = client.get_financial_metrics(&TimePeriod::AllTime);
    assert_eq!(metrics.total_volume, 0);
    assert_eq!(metrics.total_fees, 0);
    assert_eq!(metrics.total_profits, 0);
    assert_eq!(metrics.average_return_rate, 0);
}

#[test]
fn test_financial_metrics_with_invoices_all_time() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 5000, "Financial inv 1");
    create_invoice(&env, &client, &business, 3000, "Financial inv 2");

    let metrics = client.get_financial_metrics(&TimePeriod::AllTime);
    assert_eq!(metrics.total_volume, 8000);
    // Volume by category should have Services category with 8000
    let mut services_volume = 0i128;
    for (cat, vol) in metrics.volume_by_category.iter() {
        if cat == InvoiceCategory::Services {
            services_volume = vol;
        }
    }
    assert_eq!(services_volume, 8000);
}

#[test]
fn test_financial_metrics_period_boundary() {
    let env = Env::default();
    // Set timestamp to 2 days in
    env.ledger().set_timestamp(2 * 86400);
    let (client, _admin, business) = setup_contract(&env);

    // Create invoice — its created_at will be the current timestamp (2 days)
    create_invoice(&env, &client, &business, 1000, "Period boundary");

    // Daily period looks at last 24h → should include (since created_at == now, AllTime includes now)
    let daily = client.get_financial_metrics(&TimePeriod::Daily);
    // The invoice is at timestamp 2*86400, daily start = 2*86400 - 86400 = 86400
    // Invoice created_at (2*86400) >= start (86400) && <= end (2*86400) → included
    assert_eq!(daily.total_volume, 1000);

    // AllTime always includes everything
    let all_time = client.get_financial_metrics(&TimePeriod::AllTime);
    assert_eq!(all_time.total_volume, 1000);
}

// ============================================================================
// BUSINESS REPORT TESTS
// ============================================================================

#[test]
fn test_business_report_empty() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(report.business_address, business);
    assert_eq!(report.invoices_uploaded, 0);
    assert_eq!(report.invoices_funded, 0);
    assert_eq!(report.total_volume, 0);
    assert_eq!(report.success_rate, 0);
    assert_eq!(report.default_rate, 0);
    assert!(report.rating_average.is_none());
    assert_eq!(report.period, TimePeriod::AllTime);
}

#[test]
fn test_business_report_with_invoices() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    let inv1 = create_invoice(&env, &client, &business, 1000, "Biz report inv 1");
    let _inv2 = create_invoice(&env, &client, &business, 2000, "Biz report inv 2");

    // Fund one invoice
    client.update_invoice_status(&inv1, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv1, &InvoiceStatus::Funded);

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(report.invoices_uploaded, 2);
    assert_eq!(report.invoices_funded, 1);
    assert_eq!(report.total_volume, 3000);
}

#[test]
fn test_business_report_stored_and_retrievable() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 1000, "Stored report inv");

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);
    let report_id = report.report_id.clone();

    // Retrieve stored report
    let stored = client.get_business_report(&report_id);
    assert!(stored.is_some());
    let stored = stored.unwrap();
    assert_eq!(stored.business_address, business);
    assert_eq!(stored.invoices_uploaded, report.invoices_uploaded);
}

// ============================================================================
// INVESTOR REPORT TESTS
// ============================================================================

#[test]
fn test_investor_report_empty() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, _business) = setup_contract(&env);

    let investor = Address::generate(&env);
    let report = client.generate_investor_report(&investor, &TimePeriod::AllTime);

    assert_eq!(report.investor_address, investor);
    assert_eq!(report.investments_made, 0);
    assert_eq!(report.total_invested, 0);
    assert_eq!(report.total_returns, 0);
    assert_eq!(report.success_rate, 0);
    assert_eq!(report.default_rate, 0);
}

#[test]
fn test_investor_report_stored_and_retrievable() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, _business) = setup_contract(&env);

    let investor = Address::generate(&env);
    let report = client.generate_investor_report(&investor, &TimePeriod::AllTime);
    let report_id = report.report_id.clone();

    let stored = client.get_investor_report(&report_id);
    assert!(stored.is_some());
    let stored = stored.unwrap();
    assert_eq!(stored.investor_address, investor);
}

// ============================================================================
// STORAGE ROUND-TRIP TESTS
// ============================================================================

#[test]
fn test_platform_metrics_storage_round_trip() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.set_admin(&admin);

    // Before update, get_analytics_summary still works (calculates on the fly)
    let summary = client.get_analytics_summary();
    assert_eq!(summary.0.total_invoices, 0);

    // Admin updates platform metrics — stores them
    client.update_platform_metrics();

    // Retrieve should now return stored value
    let summary2 = client.get_analytics_summary();
    assert_eq!(summary2.0.total_invoices, 0);
}

#[test]
fn test_performance_metrics_storage_round_trip() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.set_admin(&admin);

    // Admin updates performance metrics
    client.update_performance_metrics();

    // Result should be retrievable via summary
    let summary = client.get_analytics_summary();
    assert_eq!(summary.1.average_settlement_time, 0);
    assert_eq!(summary.1.user_satisfaction_score, 0);
}

// ============================================================================
// ADMIN-ONLY UPDATE TESTS
// ============================================================================

#[test]
fn test_update_platform_metrics_requires_admin() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // No admin set — should fail
    let result = client.try_update_platform_metrics();
    assert!(result.is_err());
}

#[test]
fn test_update_performance_metrics_requires_admin() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // No admin set — should fail
    let result = client.try_update_performance_metrics();
    assert!(result.is_err());
}

// ============================================================================
// PERIOD DATE CALCULATION TESTS
// ============================================================================

#[test]
fn test_period_dates_all_periods() {
    // Use a timestamp large enough that yearly subtraction won't underflow
    let current_timestamp: u64 = 100_000_000;

    let (start, end) = AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Daily);
    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 86400);

    let (start, end) = AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Weekly);
    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 7 * 86400);

    let (start, end) =
        AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Monthly);
    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 30 * 86400);

    let (start, end) =
        AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Quarterly);
    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 90 * 86400);

    let (start, end) = AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Yearly);
    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 365 * 86400);
}

#[test]
fn test_period_dates_all_time() {
    let current_timestamp: u64 = 500_000;

    let (start, end) =
        AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::AllTime);
    assert_eq!(start, 0);
    assert_eq!(end, current_timestamp);
}

// ============================================================================
// ANALYTICS SUMMARY TEST
// ============================================================================

#[test]
fn test_analytics_summary_returns_tuple() {
    let env = Env::default();
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 1000, "Summary inv");

    let (platform, performance) = client.get_analytics_summary();
    assert_eq!(platform.total_invoices, 1);
    assert_eq!(platform.total_volume, 1000);
    // Performance should still be default / calculated
    assert_eq!(performance.average_settlement_time, 0);
}

// ============================================================================
// USER BEHAVIOR UPDATE AND STORAGE TEST
// ============================================================================

#[test]
fn test_update_user_behavior_metrics() {
    let env = Env::default();
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 1000, "Update behavior inv");

    // Update stores the behavior
    client.update_user_behavior_metrics(&business);

    // Subsequent get should reflect stored data
    let behavior = client.get_user_behavior_metrics(&business);
    assert_eq!(behavior.total_invoices_uploaded, 1);
}
