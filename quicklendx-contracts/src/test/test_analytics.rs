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

// ============================================================================
// ANALYTICS TRENDS AND TIME PERIODS TESTS (Issue #365)
// ============================================================================

#[test]
fn test_time_period_daily_calculation() {
    let current_timestamp: u64 = 1_000_000;
    let (start, end) = AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Daily);

    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 86400); // 24 hours in seconds
    assert_eq!(end - start, 86400);
}

#[test]
fn test_time_period_weekly_calculation() {
    let current_timestamp: u64 = 1_000_000;
    let (start, end) = AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Weekly);

    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 7 * 86400); // 7 days
    assert_eq!(end - start, 7 * 86400);
}

#[test]
fn test_time_period_monthly_calculation() {
    let current_timestamp: u64 = 10_000_000;
    let (start, end) =
        AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Monthly);

    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 30 * 86400); // 30 days
    assert_eq!(end - start, 30 * 86400);
}

#[test]
fn test_time_period_quarterly_calculation() {
    let current_timestamp: u64 = 50_000_000;
    let (start, end) =
        AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Quarterly);

    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 90 * 86400); // 90 days
    assert_eq!(end - start, 90 * 86400);
}

#[test]
fn test_time_period_yearly_calculation() {
    let current_timestamp: u64 = 100_000_000;
    let (start, end) = AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::Yearly);

    assert_eq!(end, current_timestamp);
    assert_eq!(start, current_timestamp - 365 * 86400); // 365 days
    assert_eq!(end - start, 365 * 86400);
}

#[test]
fn test_time_period_all_time_starts_at_zero() {
    let current_timestamp: u64 = 500_000_000;
    let (start, end) =
        AnalyticsCalculator::get_period_dates(current_timestamp, TimePeriod::AllTime);

    assert_eq!(start, 0);
    assert_eq!(end, current_timestamp);
}

#[test]
fn test_time_period_underflow_protection() {
    // Test with timestamp smaller than period duration
    let small_timestamp: u64 = 1000; // Very small timestamp

    // Daily period should use saturating_sub to prevent underflow
    let (start, _end) = AnalyticsCalculator::get_period_dates(small_timestamp, TimePeriod::Daily);
    assert_eq!(start, 0); // Should saturate to 0, not underflow
}

#[test]
fn test_financial_metrics_daily_period() {
    let env = Env::default();
    // Set timestamp to 2 days
    env.ledger().set_timestamp(2 * 86400);
    let (client, _admin, business) = setup_contract(&env);

    // Create invoice at current timestamp
    create_invoice(&env, &client, &business, 5000, "Daily period invoice");

    let metrics = client.get_financial_metrics(&TimePeriod::Daily);
    assert_eq!(metrics.total_volume, 5000);
}

#[test]
fn test_financial_metrics_weekly_period() {
    let env = Env::default();
    env.ledger().set_timestamp(10 * 86400); // 10 days
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 3000, "Weekly period invoice");

    let metrics = client.get_financial_metrics(&TimePeriod::Weekly);
    assert_eq!(metrics.total_volume, 3000);
}

#[test]
fn test_financial_metrics_monthly_period() {
    let env = Env::default();
    env.ledger().set_timestamp(35 * 86400); // 35 days
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 7500, "Monthly period invoice");

    let metrics = client.get_financial_metrics(&TimePeriod::Monthly);
    assert_eq!(metrics.total_volume, 7500);
}

#[test]
fn test_financial_metrics_quarterly_period() {
    let env = Env::default();
    env.ledger().set_timestamp(100 * 86400); // 100 days
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 15000, "Quarterly period invoice");

    let metrics = client.get_financial_metrics(&TimePeriod::Quarterly);
    assert_eq!(metrics.total_volume, 15000);
}

#[test]
fn test_financial_metrics_yearly_period() {
    let env = Env::default();
    env.ledger().set_timestamp(400 * 86400); // 400 days
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 50000, "Yearly period invoice");

    let metrics = client.get_financial_metrics(&TimePeriod::Yearly);
    assert_eq!(metrics.total_volume, 50000);
}

#[test]
fn test_financial_metrics_empty_trends() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, _business) = setup_contract(&env);

    // No invoices created - all periods should return empty/zero metrics
    let daily = client.get_financial_metrics(&TimePeriod::Daily);
    let weekly = client.get_financial_metrics(&TimePeriod::Weekly);
    let monthly = client.get_financial_metrics(&TimePeriod::Monthly);
    let quarterly = client.get_financial_metrics(&TimePeriod::Quarterly);
    let yearly = client.get_financial_metrics(&TimePeriod::Yearly);
    let all_time = client.get_financial_metrics(&TimePeriod::AllTime);

    assert_eq!(daily.total_volume, 0);
    assert_eq!(weekly.total_volume, 0);
    assert_eq!(monthly.total_volume, 0);
    assert_eq!(quarterly.total_volume, 0);
    assert_eq!(yearly.total_volume, 0);
    assert_eq!(all_time.total_volume, 0);
}

#[test]
fn test_financial_metrics_non_empty_trends() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    // Create multiple invoices
    create_invoice(&env, &client, &business, 1000, "Invoice 1");
    create_invoice(&env, &client, &business, 2000, "Invoice 2");
    create_invoice(&env, &client, &business, 3000, "Invoice 3");

    let all_time = client.get_financial_metrics(&TimePeriod::AllTime);
    assert_eq!(all_time.total_volume, 6000);
    assert!(all_time.volume_by_category.len() > 0);
}

#[test]
fn test_business_report_daily_period() {
    let env = Env::default();
    env.ledger().set_timestamp(2 * 86400);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 1000, "Daily report invoice");

    let report = client.generate_business_report(&business, &TimePeriod::Daily);
    assert_eq!(report.period, TimePeriod::Daily);
    assert_eq!(report.invoices_uploaded, 1);
    assert_eq!(report.total_volume, 1000);
}

#[test]
fn test_business_report_weekly_period() {
    let env = Env::default();
    env.ledger().set_timestamp(10 * 86400);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 2500, "Weekly report invoice");

    let report = client.generate_business_report(&business, &TimePeriod::Weekly);
    assert_eq!(report.period, TimePeriod::Weekly);
    assert_eq!(report.invoices_uploaded, 1);
}

#[test]
fn test_business_report_monthly_period() {
    let env = Env::default();
    env.ledger().set_timestamp(35 * 86400);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 5000, "Monthly report invoice");

    let report = client.generate_business_report(&business, &TimePeriod::Monthly);
    assert_eq!(report.period, TimePeriod::Monthly);
    assert_eq!(report.invoices_uploaded, 1);
}

#[test]
fn test_business_report_quarterly_period() {
    let env = Env::default();
    env.ledger().set_timestamp(100 * 86400);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 10000, "Quarterly report invoice");

    let report = client.generate_business_report(&business, &TimePeriod::Quarterly);
    assert_eq!(report.period, TimePeriod::Quarterly);
    assert_eq!(report.invoices_uploaded, 1);
}

#[test]
fn test_business_report_yearly_period() {
    let env = Env::default();
    env.ledger().set_timestamp(400 * 86400);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 25000, "Yearly report invoice");

    let report = client.generate_business_report(&business, &TimePeriod::Yearly);
    assert_eq!(report.period, TimePeriod::Yearly);
    assert_eq!(report.invoices_uploaded, 1);
}

#[test]
fn test_investor_report_all_periods() {
    let env = Env::default();
    env.ledger().set_timestamp(500 * 86400);
    let (client, _admin, _business) = setup_contract(&env);

    let investor = Address::generate(&env);

    // Test all periods for investor report
    let daily = client.generate_investor_report(&investor, &TimePeriod::Daily);
    let weekly = client.generate_investor_report(&investor, &TimePeriod::Weekly);
    let monthly = client.generate_investor_report(&investor, &TimePeriod::Monthly);
    let quarterly = client.generate_investor_report(&investor, &TimePeriod::Quarterly);
    let yearly = client.generate_investor_report(&investor, &TimePeriod::Yearly);
    let all_time = client.generate_investor_report(&investor, &TimePeriod::AllTime);

    assert_eq!(daily.period, TimePeriod::Daily);
    assert_eq!(weekly.period, TimePeriod::Weekly);
    assert_eq!(monthly.period, TimePeriod::Monthly);
    assert_eq!(quarterly.period, TimePeriod::Quarterly);
    assert_eq!(yearly.period, TimePeriod::Yearly);
    assert_eq!(all_time.period, TimePeriod::AllTime);
}

#[test]
fn test_report_period_dates_consistency() {
    let env = Env::default();
    let current_timestamp = 100_000_000u64;
    env.ledger().set_timestamp(current_timestamp);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 1000, "Period dates test");

    let report = client.generate_business_report(&business, &TimePeriod::Daily);

    // Verify period dates match expected calculation
    assert_eq!(report.end_date, current_timestamp);
    assert_eq!(report.start_date, current_timestamp - 86400);
}

#[test]
fn test_time_period_enum_equality() {
    // Test TimePeriod enum comparisons
    assert_eq!(TimePeriod::Daily, TimePeriod::Daily);
    assert_eq!(TimePeriod::Weekly, TimePeriod::Weekly);
    assert_eq!(TimePeriod::Monthly, TimePeriod::Monthly);
    assert_eq!(TimePeriod::Quarterly, TimePeriod::Quarterly);
    assert_eq!(TimePeriod::Yearly, TimePeriod::Yearly);
    assert_eq!(TimePeriod::AllTime, TimePeriod::AllTime);

    // Test inequality
    assert_ne!(TimePeriod::Daily, TimePeriod::Weekly);
    assert_ne!(TimePeriod::Monthly, TimePeriod::Yearly);
}

#[test]
fn test_volume_by_period_in_financial_metrics() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 5000, "Volume by period test");

    let metrics = client.get_financial_metrics(&TimePeriod::Monthly);

    // volume_by_period should contain the period with its volume
    assert!(metrics.volume_by_period.len() > 0);

    let mut found_monthly = false;
    for (period, volume) in metrics.volume_by_period.iter() {
        if period == TimePeriod::Monthly {
            assert_eq!(volume, 5000);
            found_monthly = true;
        }
    }
    assert!(found_monthly);
}

#[test]
fn test_category_breakdown_in_reports() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    // Create invoices (default category is Services)
    create_invoice(&env, &client, &business, 1000, "Category test 1");
    create_invoice(&env, &client, &business, 2000, "Category test 2");

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    // Category breakdown should have Services with count 2
    let mut services_count = 0u32;
    for (cat, count) in report.category_breakdown.iter() {
        if cat == InvoiceCategory::Services {
            services_count = count;
        }
    }
    assert_eq!(services_count, 2);
}

#[test]
fn test_multiple_invoices_different_periods() {
    let env = Env::default();
    // Start at a large timestamp to avoid underflow
    let base_timestamp = 500 * 86400u64;
    env.ledger().set_timestamp(base_timestamp);
    let (client, _admin, business) = setup_contract(&env);

    // Create invoice at current time
    create_invoice(&env, &client, &business, 1000, "Current invoice");

    // AllTime should include all invoices
    let all_time = client.get_financial_metrics(&TimePeriod::AllTime);
    assert_eq!(all_time.total_volume, 1000);

    // Daily should include recent invoice
    let daily = client.get_financial_metrics(&TimePeriod::Daily);
    assert_eq!(daily.total_volume, 1000);
}

#[test]
fn test_empty_business_report_all_periods() {
    let env = Env::default();
    env.ledger().set_timestamp(500 * 86400);
    let (client, _admin, _business) = setup_contract(&env);

    let new_business = Address::generate(&env);

    // All periods should return empty reports for new business
    let daily = client.generate_business_report(&new_business, &TimePeriod::Daily);
    let weekly = client.generate_business_report(&new_business, &TimePeriod::Weekly);
    let monthly = client.generate_business_report(&new_business, &TimePeriod::Monthly);
    let quarterly = client.generate_business_report(&new_business, &TimePeriod::Quarterly);
    let yearly = client.generate_business_report(&new_business, &TimePeriod::Yearly);
    let all_time = client.generate_business_report(&new_business, &TimePeriod::AllTime);

    assert_eq!(daily.invoices_uploaded, 0);
    assert_eq!(weekly.invoices_uploaded, 0);
    assert_eq!(monthly.invoices_uploaded, 0);
    assert_eq!(quarterly.invoices_uploaded, 0);
    assert_eq!(yearly.invoices_uploaded, 0);
    assert_eq!(all_time.invoices_uploaded, 0);
}

#[test]
fn test_report_generated_at_timestamp() {
    let env = Env::default();
    let current_timestamp = 1_500_000u64;
    env.ledger().set_timestamp(current_timestamp);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 1000, "Timestamp test");

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(report.generated_at, current_timestamp);
}

#[test]
fn test_investor_report_empty_all_periods() {
    let env = Env::default();
    env.ledger().set_timestamp(500 * 86400);
    let (client, _admin, _business) = setup_contract(&env);

    let new_investor = Address::generate(&env);

    // All periods should return empty metrics for new investor
    let daily = client.generate_investor_report(&new_investor, &TimePeriod::Daily);
    let all_time = client.generate_investor_report(&new_investor, &TimePeriod::AllTime);

    assert_eq!(daily.investments_made, 0);
    assert_eq!(daily.total_invested, 0);
    assert_eq!(all_time.investments_made, 0);
    assert_eq!(all_time.total_invested, 0);
}

#[test]
fn test_period_dates_boundary_conditions() {
    // Test exact boundary conditions
    let timestamp = 86400u64; // Exactly 1 day

    let (start, end) = AnalyticsCalculator::get_period_dates(timestamp, TimePeriod::Daily);
    assert_eq!(start, 0);
    assert_eq!(end, timestamp);

    // Weekly with exactly 7 days
    let week_timestamp = 7 * 86400u64;
    let (start, end) = AnalyticsCalculator::get_period_dates(week_timestamp, TimePeriod::Weekly);
    assert_eq!(start, 0);
    assert_eq!(end, week_timestamp);
}

#[test]
fn test_financial_metrics_currency_distribution() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 5000, "Currency distribution test");

    let metrics = client.get_financial_metrics(&TimePeriod::AllTime);

    // Should have at least one currency in distribution
    assert!(metrics.currency_distribution.len() > 0);
}

#[test]
fn test_financial_metrics_fee_breakdown() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 10000, "Fee breakdown test");

    let metrics = client.get_financial_metrics(&TimePeriod::AllTime);

    // Fee breakdown should exist
    assert!(metrics.fee_breakdown.len() > 0);
}

#[test]
fn test_financial_metrics_profit_margins() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 10000, "Profit margins test");

    let metrics = client.get_financial_metrics(&TimePeriod::AllTime);

    // Profit margins should exist
    assert!(metrics.profit_margins.len() > 0);
}
