#![cfg(test)]

use super::*;
use crate::init::InitializationParams;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec, IntoVal};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

#[test]
fn test_successful_initialization() {
    let (env, client) = setup();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let initial_currencies = Vec::from_array(&env, [Address::generate(&env)]);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: initial_currencies.clone(),
    };

    client.initialize(&params);

    assert!(client.is_initialized());
}

#[test]
fn test_double_initialization_fails() {
    let (env, client) = setup();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    client.initialize(&params);

    // Second initialization should fail
    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::OperationNotAllowed)));
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_initialization_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    // No mock_all_auths()
    let admin = Address::generate(&env);
    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    client.initialize(&params);
}

#[test]
fn test_validation_invalid_fees() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 1001, // 10.01% > 10%
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidFeeBasisPoints)));
}

#[test]
fn test_validation_invalid_amount() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 0, // Should be > 0
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_validation_invalid_due_date() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 731, // Max 730
        grace_period_seconds: 604800,
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_validation_invalid_grace_period() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 2_592_001, // Max 30 days
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidTimestamp)));
}

