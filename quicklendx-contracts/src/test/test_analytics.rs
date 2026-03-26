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
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
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

fn seed_investment(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
    funded_at: u64,
    status: InvestmentStatus,
) -> BytesN<32> {
    let contract_id = client.address.clone();
    env.as_contract(&contract_id, || {
        let investment_id = InvestmentStorage::generate_unique_investment_id(env);
        let investment = Investment {
            investment_id: investment_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            amount,
            funded_at,
            status,
            insurance: Vec::new(env),
        };
        InvestmentStorage::store_investment(env, &investment);
        investment_id
    })
}

fn assert_investor_reports_match_except_id(
    left: &crate::analytics::InvestorReport,
    right: &crate::analytics::InvestorReport,
) {
    assert_eq!(left.investor_address, right.investor_address);
    assert_eq!(left.period, right.period);
    assert_eq!(left.start_date, right.start_date);
    assert_eq!(left.end_date, right.end_date);
    assert_eq!(left.investments_made, right.investments_made);
    assert_eq!(left.total_invested, right.total_invested);
    assert_eq!(left.total_returns, right.total_returns);
    assert_eq!(left.average_return_rate, right.average_return_rate);
    assert_eq!(left.success_rate, right.success_rate);
    assert_eq!(left.default_rate, right.default_rate);
    assert_eq!(left.preferred_categories, right.preferred_categories);
    assert_eq!(left.risk_tolerance, right.risk_tolerance);
    assert_eq!(left.portfolio_diversity, right.portfolio_diversity);
    assert_eq!(left.generated_at, right.generated_at);
}

#[test]
fn test_business_report_is_persisted_after_generation() {
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

    assert_eq!(stored.report_id, generated.report_id);
    assert_investor_reports_match_except_id(&generated, &stored);
    assert_eq!(stored.investments_made, 0);
    assert_eq!(stored.total_invested, 0);
    assert_eq!(stored.total_returns, 0);
    assert_eq!(stored.risk_tolerance, 25);
    assert_eq!(stored.portfolio_diversity, 0);
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
