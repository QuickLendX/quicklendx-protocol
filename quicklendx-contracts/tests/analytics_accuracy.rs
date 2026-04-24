#![cfg(feature = "legacy-tests")]

#![cfg(feature = "legacy-tests")]

use quicklendx_contracts::{
    InvoiceCategory, InvoiceStatus, QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup_contract(env: &Env) -> (QuickLendXContractClient<'static>, Address, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    let currency = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);

    // store_invoice requires protocol limits validation and a whitelisted currency.
    let _ = client.try_initialize_protocol_limits(&admin, &1i128, &365u64, &86400u64);
    client.add_currency(&admin, &currency);

    (client, admin, business, currency)
}

fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    amount: i128,
    description: &str,
) -> soroban_sdk::BytesN<32> {
    let due_date = env.ledger().timestamp() + 86400;
    client.store_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, description),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

#[test]
fn platform_metrics_empty_data_is_zeroed() {
    let env = Env::default();
    let (client, _admin, _business, _currency) = setup_contract(&env);

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
fn platform_metrics_paid_only_sparse_data_has_100pct_success() {
    let env = Env::default();
    let (client, _admin, business, currency) = setup_contract(&env);

    let inv = create_invoice(&env, &client, &business, &currency, 1000, "paid-only");
    client.update_invoice_status(&inv, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv, &InvoiceStatus::Paid);

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 1);
    assert_eq!(metrics.total_investments, 1);
    assert_eq!(metrics.success_rate, 10_000);
    assert_eq!(metrics.default_rate, 0);
}

#[test]
fn platform_metrics_defaulted_only_sparse_data_has_100pct_default() {
    let env = Env::default();
    let (client, _admin, business, currency) = setup_contract(&env);

    let inv = create_invoice(&env, &client, &business, &currency, 1000, "defaulted-only");
    client.update_invoice_status(&inv, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv, &InvoiceStatus::Defaulted);

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 1);
    assert_eq!(metrics.total_investments, 1);
    assert_eq!(metrics.success_rate, 0);
    assert_eq!(metrics.default_rate, 10_000);
}

#[test]
fn platform_metrics_mixed_sparse_data_has_expected_rates() {
    let env = Env::default();
    let (client, _admin, business, currency) = setup_contract(&env);

    let paid = create_invoice(&env, &client, &business, &currency, 1000, "paid");
    client.update_invoice_status(&paid, &InvoiceStatus::Verified);
    client.update_invoice_status(&paid, &InvoiceStatus::Funded);
    client.update_invoice_status(&paid, &InvoiceStatus::Paid);

    let defaulted = create_invoice(&env, &client, &business, &currency, 2000, "defaulted");
    client.update_invoice_status(&defaulted, &InvoiceStatus::Verified);
    client.update_invoice_status(&defaulted, &InvoiceStatus::Funded);
    client.update_invoice_status(&defaulted, &InvoiceStatus::Defaulted);

    let metrics = client.get_platform_metrics();
    assert_eq!(metrics.total_invoices, 2);
    assert_eq!(metrics.total_investments, 2);
    assert_eq!(metrics.success_rate, 5_000);
    assert_eq!(metrics.default_rate, 5_000);
}
