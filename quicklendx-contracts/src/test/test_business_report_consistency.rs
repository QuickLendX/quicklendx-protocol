/// Business Report Generation Consistency Checks
///
/// Covers cases not already asserted in `src/test_analytics_consistency.rs`.
use crate::analytics::{AnalyticsCalculator, TimePeriod};
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);
    (client, contract_id, admin, business)
}

fn upload(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    desc: &str,
) -> soroban_sdk::BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, desc),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    )
}

#[test]
fn test_business_report_weekly_includes_invoice_in_window() {
    let env = Env::default();
    env.ledger().set_timestamp(10 * 86_400u64);
    let (client, contract_id, _admin, business) = setup(&env);

    upload(&env, &client, &business, 2_000, "weekly-win-inv");

    let report = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_business_report(&env, &business, TimePeriod::Weekly)
        })
        .unwrap();
    assert_eq!(report.invoices_uploaded, 1);
    assert_eq!(report.total_volume, 2_000);
}
