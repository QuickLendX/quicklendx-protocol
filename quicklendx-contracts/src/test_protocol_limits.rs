#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::init::InitializationParams;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    (env, client, admin)
}

fn initialize_with_limits(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
) {
    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(env),
        fee_bps: 200,
        min_invoice_amount,
        max_due_date_days,
        grace_period_seconds,
        initial_currencies: Vec::new(env),
    };
    client.initialize(&params);
}

#[test]
fn test_initialization_limits_enforced_on_store_invoice() {
    let (env, client, admin) = setup();
    initialize_with_limits(&env, &client, &admin, 1_000i128, 30u64, 0u64);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let result = client.try_store_invoice(
        &business,
        &999i128,
        &currency,
        &due_date,
        &String::from_str(&env, "too small"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));

    assert!(client
        .try_store_invoice(
            &business,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(&env, "ok"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());
}

#[test]
fn test_due_date_boundary_enforced() {
    let (env, client, admin) = setup();
    initialize_with_limits(&env, &client, &admin, 10i128, 30u64, 0u64);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let now = env.ledger().timestamp();
    let at_max = now + 30 * 86_400;
    let above_max = at_max + 1;

    assert!(client
        .try_store_invoice(
            &business,
            &10i128,
            &currency,
            &at_max,
            &String::from_str(&env, "ok"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());

    let result = client.try_store_invoice(
        &business,
        &10i128,
        &currency,
        &above_max,
        &String::from_str(&env, "too far"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_limit_updates_propagate_to_validation_paths() {
    let (env, client, admin) = setup();
    client.set_admin(&admin);

    client.set_protocol_limits(&admin, &100i128, &365u64, &0u64);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    assert!(client
        .try_store_invoice(
            &business,
            &100i128,
            &currency,
            &due_date,
            &String::from_str(&env, "ok"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());

    client.update_protocol_limits(&admin, &200i128, &365u64, &0u64);

    let result = client.try_store_invoice(
        &business,
        &199i128,
        &currency,
        &due_date,
        &String::from_str(&env, "below updated min"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_set_protocol_limits_requires_admin() {
    let (env, client, admin) = setup();
    client.set_admin(&admin);

    let non_admin = Address::generate(&env);
    let result = client.try_set_protocol_limits(&non_admin, &10i128, &365u64, &0u64);
    assert_eq!(result, Err(Ok(QuickLendXError::NotAdmin)));
}

