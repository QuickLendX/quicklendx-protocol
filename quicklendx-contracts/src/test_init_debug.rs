#![cfg(test)]

use super::*;
use crate::init::InitializationParams;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

fn base_params(admin: Address, treasury: Address, currencies: Vec<Address>) -> InitializationParams {
    InitializationParams {
        admin,
        treasury,
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: currencies,
    }
}

#[test]
fn test_init_rejects_admin_equals_treasury() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let params = base_params(admin.clone(), admin.clone(), Vec::new(&env));

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAddress)));
}

#[test]
fn test_init_rejects_admin_as_contract_address() {
    let (env, client) = setup();
    let admin = client.address.clone();
    let treasury = Address::generate(&env);
    let params = base_params(admin, treasury, Vec::new(&env));

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAddress)));
}

#[test]
fn test_init_rejects_treasury_as_contract_address() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = client.address.clone();
    let params = base_params(admin, treasury, Vec::new(&env));

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAddress)));
}

#[test]
fn test_init_rejects_duplicate_currencies() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let currency = Address::generate(&env);
    let currencies = Vec::from_array(&env, [currency.clone(), currency]);
    let params = base_params(admin, treasury, currencies);

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidCurrency)));
}

#[test]
fn test_init_rejects_currency_conflicts() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_currency = client.address.clone();
    let currencies = Vec::from_array(&env, [admin.clone(), treasury.clone(), contract_currency]);
    let params = base_params(admin, treasury, currencies);

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidCurrency)));
}
