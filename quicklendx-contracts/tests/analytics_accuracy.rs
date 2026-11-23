use quicklendx_contracts::{
    analytics::AnalyticsCalculator, invoice::InvoiceCategory, types::InvoiceStatus,
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup(env: &Env) -> (QuickLendXContractClient<'_>, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);
    (client, contract_id)
}

fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    description: &str,
) -> soroban_sdk::BytesN<32> {
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
        &None,
    )
}

fn metrics(env: &Env, contract_id: &Address) -> quicklendx_contracts::analytics::PlatformMetrics {
    env.as_contract(contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(env).unwrap()
    })
}

#[test]
fn platform_metrics_empty_data_is_zeroed() {
    let env = Env::default();
    let (_client, contract_id) = setup(&env);

    let metrics = metrics(&env, &contract_id);
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
fn platform_metrics_calculate_rates_from_terminal_invoice_statuses() {
    let env = Env::default();
    let (client, contract_id) = setup(&env);
    let business = Address::generate(&env);

    let paid = create_invoice(&env, &client, &business, 1_000, "paid");
    client.update_invoice_status(&paid, &InvoiceStatus::Funded);
    client.update_invoice_status(&paid, &InvoiceStatus::Paid);

    let defaulted = create_invoice(&env, &client, &business, 2_000, "defaulted");
    client.update_invoice_status(&defaulted, &InvoiceStatus::Funded);
    client.update_invoice_status(&defaulted, &InvoiceStatus::Defaulted);

    let metrics = metrics(&env, &contract_id);
    assert_eq!(metrics.total_invoices, 2);
    assert_eq!(metrics.total_investments, 2);
    assert_eq!(metrics.total_volume, 3_000);
    assert_eq!(metrics.success_rate, 5_000);
    assert_eq!(metrics.default_rate, 5_000);
}
