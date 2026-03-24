use super::*;
use crate::analytics::{AnalyticsCalculator, AnalyticsStorage, TimePeriod};
use crate::invoice::{InvoiceCategory, InvoiceStatus, InvoiceStorage};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

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
    client.upload_invoice(
        business,
        &amount,
        currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, description),
        &category,
        &Vec::new(env),
    )
}

#[test]
fn test_platform_metrics_empty_summary_defaults() {
    let env = Env::default();
    let (platform, performance) = crate::get_analytics_summary(env);

    assert_eq!(platform.total_invoices, 0);
    assert_eq!(platform.total_volume, 0);
    assert_eq!(platform.success_rate, 0);
    assert_eq!(performance.transaction_success_rate, 0);
    assert_eq!(performance.error_rate, 0);
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

    let metrics = AnalyticsCalculator::calculate_platform_metrics(&env).unwrap();
    assert_eq!(metrics.total_invoices, 2);
    assert_eq!(metrics.total_volume, 3_000);
    assert_eq!(metrics.average_invoice_amount, 1_500);
    assert_eq!(metrics.verified_businesses, 1);
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
    assert_eq!(report.invoices_funded, 2);
    assert_eq!(report.total_volume, 3_000);
    assert_eq!(report.success_rate, 5_000);
    assert_eq!(report.default_rate, 0);

    AnalyticsStorage::store_business_report(&env, &report);
    let stored = crate::get_business_report(env, report.report_id.clone()).unwrap();
    assert_eq!(stored.report_id, report.report_id);
    assert_eq!(stored.total_volume, report.total_volume);
}

#[test]
fn test_investor_report_round_trip_storage() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let investor = Address::generate(&env);
    let report =
        crate::generate_investor_report(env.clone(), investor.clone(), TimePeriod::AllTime)
            .unwrap();

    assert_eq!(report.investor_address, investor);
    assert_eq!(report.investments_made, 0);
    assert_eq!(report.total_invested, 0);
    assert_eq!(report.total_returns, 0);

    AnalyticsStorage::store_investor_report(&env, &report);
    let stored = crate::get_investor_report(env, report.report_id.clone()).unwrap();
    assert_eq!(stored.report_id, report.report_id);
    assert_eq!(stored.investor_address, report.investor_address);
}
