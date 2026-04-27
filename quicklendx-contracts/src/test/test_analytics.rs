/// Focused analytics tests for investor report consistency.
///
/// Coverage:
/// - Investor report generation from persisted investments
/// - Persistence and retrieval round-trips
/// - Deterministic repeated generation for the same ledger snapshot
/// - Empty-history investors
/// - Period filtering
/// - Business report persistence regression
use super::*;
use crate::analytics::TimePeriod;
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup_contract(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    (env, client, admin, business, currency)
}

fn upload_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    amount: i128,
    category: InvoiceCategory,
    description: &str,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        business,
        &amount,
        currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, description),
        &category,
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

    let metrics = client.get_platform_metrics().unwrap();
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
fn test_platform_metrics_empty_summary_defaults() {
    let env = Env::default();
    let (platform, performance) = crate::get_analytics_summary(env);

    let metrics = client.get_platform_metrics().unwrap();
    assert_eq!(metrics.total_invoices, 3);
    assert_eq!(metrics.total_volume, 6000);
    assert_eq!(metrics.average_invoice_amount, 2000);
}

#[test]
fn test_platform_metrics_with_multiple_invoices() {
    let (env, client, _admin, business, currency) = setup();

    upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        1_000,
        InvoiceCategory::Services,
        "Invoice A",
    );
    upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        2_000,
        InvoiceCategory::Technology,
        "Invoice B",
    );

    let metrics = client.get_platform_metrics().unwrap();
    assert_eq!(metrics.total_invoices, 2);
    // Investments include invoices that are Funded, Paid, or Defaulted
    assert_eq!(metrics.total_investments, 2);
}

#[test]
fn test_platform_metrics_success_rate_paid_only_sparse_data() {
    let env = Env::default();
    let (client, _admin, business) = setup_contract(&env);

    let inv1 = create_invoice(&env, &client, &business, 1000, "Paid-only inv");
    client.update_invoice_status(&inv1, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv1, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv1, &InvoiceStatus::Paid);

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 1);
    assert_eq!(metrics.total_investments, 1);
    assert_eq!(metrics.success_rate, 10000);
    assert_eq!(metrics.default_rate, 0);
}

#[test]
fn test_platform_metrics_default_rate_defaulted_only_sparse_data() {
    let env = Env::default();
    let (client, _admin, business) = setup_contract(&env);

    let inv1 = create_invoice(&env, &client, &business, 1000, "Defaulted-only inv");
    client.update_invoice_status(&inv1, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv1, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv1, &InvoiceStatus::Defaulted);

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 1);
    assert_eq!(metrics.total_investments, 1);
    assert_eq!(metrics.success_rate, 0);
    assert_eq!(metrics.default_rate, 10000);
}

#[test]
fn test_platform_metrics_success_and_default_rates_mixed_sparse_data() {
    let env = Env::default();
    let (client, _admin, business) = setup_contract(&env);

    let inv_paid = create_invoice(&env, &client, &business, 1000, "Mixed paid");
    client.update_invoice_status(&inv_paid, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv_paid, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv_paid, &InvoiceStatus::Paid);

    let inv_defaulted = create_invoice(&env, &client, &business, 1000, "Mixed defaulted");
    client.update_invoice_status(&inv_defaulted, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv_defaulted, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv_defaulted, &InvoiceStatus::Defaulted);

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 2);
    assert_eq!(metrics.total_investments, 2);
    assert_eq!(metrics.success_rate, 5000);
    assert_eq!(metrics.default_rate, 5000);
}

// ============================================================================
// PERFORMANCE METRICS TESTS
// ============================================================================

#[test]
fn test_performance_metrics_empty_data() {
    let env = Env::default();
    let (client, _admin, _business) = setup_contract(&env);

    let metrics = client.get_performance_metrics().unwrap();
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

    let metrics = client.get_performance_metrics().unwrap();
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
fn test_user_behavior_metrics_tracks_uploaded_invoices() {
    let (env, client, _admin, business, currency) = setup();

    upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        1_000,
        InvoiceCategory::Services,
        "Behavior invoice 1",
    );
    upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        2_500,
        InvoiceCategory::Consulting,
        "Behavior invoice 2",
    );

    let metrics = crate::get_user_behavior_metrics(env.clone(), business.clone());
    assert_eq!(metrics.user_address, business);
    assert_eq!(metrics.total_invoices_uploaded, 2);
    assert_eq!(metrics.total_investments_made, 0);
    assert_eq!(metrics.risk_score, 25);
    assert!(metrics.last_activity > 0);
}

#[test]
fn test_financial_metrics_respects_period_filter_and_categories() {
    let (env, client, _admin, business, currency) = setup();

    let old_invoice = upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        1_000,
        InvoiceCategory::Services,
        "Old invoice",
    );
    let mut old = InvoiceStorage::get_invoice(&env, &old_invoice).unwrap();
    old.created_at = env.ledger().timestamp() - (31 * 24 * 60 * 60);
    InvoiceStorage::store_invoice(&env, &old);

    upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        2_500,
        InvoiceCategory::Technology,
        "Recent invoice",
    );

    let monthly = crate::get_financial_metrics(env.clone(), TimePeriod::Monthly);
    assert_eq!(monthly.total_volume, 2_500);

    let mut technology_volume = 0i128;
    for (category, volume) in monthly.volume_by_category.iter() {
        if category == InvoiceCategory::Technology {
            technology_volume = volume;
        }
    }
    assert_eq!(technology_volume, 2_500);

    let all_time = crate::get_financial_metrics(env, TimePeriod::AllTime);
    assert_eq!(all_time.total_volume, 3_500);
}

#[test]
fn test_performance_metrics_reflect_paid_and_defaulted_invoices() {
    let (env, client, _admin, business, currency) = setup();

    let paid_invoice = upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        1_000,
        InvoiceCategory::Services,
        "Paid invoice",
    );
    let defaulted_invoice = upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        2_000,
        InvoiceCategory::Services,
        "Defaulted invoice",
    );

    client.update_invoice_status(&paid_invoice, &InvoiceStatus::Paid);
    client.update_invoice_status(&defaulted_invoice, &InvoiceStatus::Defaulted);

    let metrics = AnalyticsCalculator::calculate_performance_metrics(&env).unwrap();
    assert_eq!(metrics.transaction_success_rate, 5_000);
    assert_eq!(metrics.error_rate, 5_000);
}

#[test]
fn test_business_report_generation_matches_invoice_state() {
    let (env, client, _admin, business, currency) = setup();

    let funded = upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        1_000,
        InvoiceCategory::Services,
        "Funded invoice",
    );
    client.update_invoice_status(&funded, &InvoiceStatus::Funded);

    let paid = upload_invoice(
        &env,
        &client,
        &business,
        &currency,
        2_000,
        InvoiceCategory::Technology,
        "Paid invoice",
    );
    client.update_invoice_status(&paid, &InvoiceStatus::Paid);

    let report =
        crate::generate_business_report(env.clone(), business.clone(), TimePeriod::AllTime)
            .unwrap();

    assert_eq!(report.business_address, business);
    assert_eq!(report.invoices_uploaded, 2);
    assert_eq!(report.invoices_funded, 1);
    assert_eq!(report.total_volume, 3000);
}

#[test]
fn test_business_report_stored_and_retrievable() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, business) = setup_contract(&env);

    create_invoice(&env, &client, &business, 5_000, "Business report invoice");

    let generated = client.generate_business_report(&business, &TimePeriod::AllTime);
    let stored = client
        .get_business_report(&generated.report_id)
        .expect("generated business report must be stored");

    assert_eq!(stored.report_id, generated.report_id);
    assert_eq!(stored.business_address, generated.business_address);
    assert_eq!(stored.invoices_uploaded, 1);
    assert_eq!(stored.total_volume, 5_000);
}

#[test]
fn test_investor_report_empty_history_is_stored_and_retrievable() {
    let env = Env::default();
    env.ledger().set_timestamp(2_000_000);
    let (client, _admin, _business) = setup_contract(&env);

    let investor = Address::generate(&env);
    let generated = client.generate_investor_report(&investor, &TimePeriod::Monthly);
    let stored = client
        .get_investor_report(&generated.report_id)
        .expect("empty-history investor report must be stored");

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

    // No admin set - should fail
    let result = client.try_update_platform_metrics();
    assert!(result.is_err());
}

#[test]
fn test_update_performance_metrics_requires_admin() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // No admin set - should fail
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
// PLATFORM METRICS
// ============================================================================

#[test]
fn test_platform_metrics_empty_state() {
    let env = Env::default();
    let (client, _, _) = setup_contract(&env);

    let metrics = client.get_platform_metrics().unwrap();

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
fn test_platform_metrics_update() {
    let env = Env::default();
    let (client, admin, _) = setup_contract(&env);

    env.mock_all_auths();
    client.set_admin(&admin);

    client.update_platform_metrics();

    let metrics = client.get_platform_metrics().unwrap();

    assert!(metrics.total_invoices >= 0);
    assert!(metrics.total_volume >= 0);
}

// ============================================================================
// PERFORMANCE METRICS
// ============================================================================

#[test]
fn test_performance_metrics_empty_state() {
    let env = Env::default();
    let (client, _, _) = setup_contract(&env);

    let metrics = client.get_performance_metrics().unwrap();

    assert_eq!(metrics.average_settlement_time, 0);
    assert_eq!(metrics.average_verification_time, 0);
    assert_eq!(metrics.dispute_resolution_time, 0);
    assert_eq!(metrics.transaction_success_rate, 0);
    assert_eq!(metrics.error_rate, 0);
    assert_eq!(metrics.user_satisfaction_score, 0);
}

#[test]
fn test_performance_metrics_update() {
    let env = Env::default();
    let (client, admin, _) = setup_contract(&env);

    env.mock_all_auths();
    client.set_admin(&admin);

    client.update_performance_metrics();

    let metrics = client.get_performance_metrics().unwrap();

    assert!(metrics.transaction_success_rate >= 0);
    assert!(metrics.error_rate >= 0);
}

// ============================================================================
// ANALYTICS SUMMARY (ISSUE#600)
// ============================================================================

#[test]
fn test_analytics_summary_empty_state_fallback() {
    let env = Env::default();
    let (client, _, _) = setup_contract(&env);

    let (platform, performance) = client.get_analytics_summary();

    assert_eq!(platform.total_invoices, 0);
    assert_eq!(platform.total_investments, 0);
    assert_eq!(platform.total_volume, 0);
    assert_eq!(platform.total_fees_collected, 0);
    assert_eq!(platform.average_invoice_amount, 0);
    assert_eq!(platform.average_investment_amount, 0);
    assert_eq!(platform.success_rate, 0);
    assert_eq!(platform.default_rate, 0);

    assert_eq!(performance.average_settlement_time, 0);
    assert_eq!(performance.average_verification_time, 0);
    assert_eq!(performance.dispute_resolution_time, 0);
    assert_eq!(performance.transaction_success_rate, 0);
    assert_eq!(performance.error_rate, 0);
    assert_eq!(performance.user_satisfaction_score, 0);
}

#[test]
fn test_analytics_summary_after_updates() {
    let env = Env::default();
    let (client, admin, _) = setup_contract(&env);

    env.mock_all_auths();
    client.set_admin(&admin);

    client.update_platform_metrics();
    client.update_performance_metrics();

    let (platform, performance) = client.get_analytics_summary();

    assert!(platform.total_invoices >= 0);
    assert!(performance.transaction_success_rate >= 0);
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
fn test_investor_report_generation_is_consistent_for_same_snapshot() {
    let env = Env::default();
    env.ledger().set_timestamp(3_000_000);
    let (client, _admin, business) = setup_contract(&env);

    let investor = Address::generate(&env);
    let invoice_id = create_invoice(&env, &client, &business, 10_000, "Consistent report invoice");
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Paid);
    seed_investment(
        &env,
        &client,
        &investor,
        &invoice_id,
        8_000,
        env.ledger().timestamp(),
        InvestmentStatus::Completed,
    );

    let first = client.generate_investor_report(&investor, &TimePeriod::AllTime);
    let second = client.generate_investor_report(&investor, &TimePeriod::AllTime);

    assert_ne!(first.report_id, second.report_id);
    assert_investor_reports_match_except_id(&first, &second);
}

#[test]
fn test_investor_report_persistence_matches_generated_snapshot() {
    let env = Env::default();
    env.ledger().set_timestamp(4_000_000);
    let (client, _admin, business) = setup_contract(&env);

    let investor = Address::generate(&env);

    let paid_invoice = create_invoice(&env, &client, &business, 20_000, "Paid investment");
    client.update_invoice_status(&paid_invoice, &InvoiceStatus::Paid);
    seed_investment(
        &env,
        &client,
        &investor,
        &paid_invoice,
        15_000,
        env.ledger().timestamp(),
        InvestmentStatus::Completed,
    );

    let defaulted_invoice = create_invoice(&env, &client, &business, 12_000, "Defaulted investment");
    client.update_invoice_status(&defaulted_invoice, &InvoiceStatus::Defaulted);
    seed_investment(
        &env,
        &client,
        &investor,
        &defaulted_invoice,
        9_000,
        env.ledger().timestamp(),
        InvestmentStatus::Defaulted,
    );

    let generated = client.generate_investor_report(&investor, &TimePeriod::AllTime);
    let stored = client
        .get_investor_report(&generated.report_id)
        .expect("generated report must be persisted");

    assert_eq!(stored.report_id, generated.report_id);
    assert_investor_reports_match_except_id(&generated, &stored);
    assert_eq!(stored.investments_made, 2);
    assert_eq!(stored.total_invested, 24_000);
    assert_eq!(stored.success_rate, 5_000);
    assert_eq!(stored.default_rate, 5_000);
}

#[test]
fn test_investor_report_retrieval_is_deterministic() {
    let env = Env::default();
    env.ledger().set_timestamp(5_000_000);
    let (client, _admin, _business) = setup_contract(&env);

    let investor = Address::generate(&env);
    let generated = client.generate_investor_report(&investor, &TimePeriod::AllTime);

    let first = client
        .get_investor_report(&generated.report_id)
        .expect("stored report must exist");
    let second = client
        .get_investor_report(&generated.report_id)
        .expect("stored report must remain stable");

    assert_eq!(first.report_id, second.report_id);
    assert_investor_reports_match_except_id(&first, &second);
}

#[test]
fn test_investor_report_period_filter_excludes_out_of_range_history() {
    let env = Env::default();
    env.ledger().set_timestamp(6_000_000);
    let (client, _admin, business) = setup_contract(&env);

    let investor = Address::generate(&env);

    let within_period = create_invoice(&env, &client, &business, 9_000, "Recent investment");
    client.update_invoice_status(&within_period, &InvoiceStatus::Paid);
    seed_investment(
        &env,
        &client,
        &investor,
        &within_period,
        7_000,
        env.ledger().timestamp(),
        InvestmentStatus::Completed,
    );

    let older_invoice = create_invoice(&env, &client, &business, 11_000, "Older investment");
    client.update_invoice_status(&older_invoice, &InvoiceStatus::Paid);
    seed_investment(
        &env,
        &client,
        &investor,
        &older_invoice,
        8_000,
        env.ledger().timestamp().saturating_sub(40 * 86_400),
        InvestmentStatus::Completed,
    );

    let report = client.generate_investor_report(&investor, &TimePeriod::Monthly);

    assert_eq!(report.investments_made, 1);
    assert_eq!(report.total_invested, 7_000);
    assert_eq!(report.success_rate, 10_000);
    assert_eq!(report.default_rate, 0);
}

#[test]
fn test_get_investor_report_returns_none_for_unknown_id() {
    let env = Env::default();
    let (client, _admin, _business) = setup_contract(&env);

    let missing_id = BytesN::from_array(&env, &[0u8; 32]);
    assert!(client.get_investor_report(&missing_id).is_none());
}
